//! Entrada e saída de áudio atrás de callbacks simples, para o resto do projeto não depender de
//! cpal nem de API do SO. Backends: `cpal` (padrão: WASAPI compartilhado no Windows, CoreAudio no
//! macOS) e WASAPI de baixa latência (só Windows). Backend novo = módulo novo aqui.

#[cfg(windows)]
mod wasapi;

use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, ErrorKind, Host, StreamConfig};

pub type Res<T> = Result<T, Box<dyn Error>>;

/// Se as threads de áudio pedem prioridade de tempo real (feature `realtime`).
pub const MMCSS: bool = cfg!(feature = "realtime");

/// Um pacote entregue ao callback: áudio f32 intercalado em `canais` canais.
#[derive(Clone, Copy, Debug)]
pub struct Pacote {
    pub canais: usize,
    /// Instante do callback, em ns (relógio QPC no Windows).
    pub callback_ns: u64,
    /// Captura (entrada) ou reprodução prevista (saída) da primeira amostra, em ns.
    pub dispositivo_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    /// cpal: WASAPI compartilhado com período de 10 ms (Windows) ou CoreAudio (macOS).
    #[default]
    Padrao,
    /// WASAPI direto com IAudioClient3 no menor período do driver e MMCSS (só Windows).
    BaixaLatencia,
}

/// Qual dispositivo abrir e como.
#[derive(Clone, Debug)]
pub struct Pedido {
    pub backend: Backend,
    /// Parte do nome do dispositivo; `None` = padrão do sistema.
    pub trecho: Option<String>,
    pub taxa: u32,
    /// Buffer pedido em quadros; `None` = o do sistema.
    pub buffer: Option<u32>,
    /// Para onde vão os avisos do fluxo; `None` = imprime no stderr.
    pub avisos: Option<Sender<Aviso>>,
}

impl Default for Pedido {
    fn default() -> Self {
        Self { backend: Backend::Padrao, trecho: None, taxa: 48_000, buffer: None, avisos: None }
    }
}

/// Algo que aconteceu com um fluxo depois de pedido (ex.: dispositivo desconectado).
#[derive(Clone, Debug)]
pub struct Aviso {
    pub entrada: bool,
    /// O fluxo parou de entregar pacotes e precisa ser reaberto.
    pub fatal: bool,
    pub texto: String,
}

pub(crate) fn avisar(destino: &Option<Sender<Aviso>>, a: Aviso) {
    match destino {
        Some(tx) => {
            let _ = tx.send(a);
        }
        None => eprintln!("{} ({}): {}", if a.fatal { "erro" } else { "aviso" }, if a.entrada { "entrada" } else { "saída" }, a.texto),
    }
}

/// Erros do cpal depois dos quais o fluxo não volta sozinho.
fn fatal(k: ErrorKind) -> bool {
    matches!(
        k,
        ErrorKind::DeviceNotAvailable | ErrorKind::StreamInvalidated | ErrorKind::HostUnavailable | ErrorKind::PermissionDenied
    )
}

/// Fluxo aberto e ainda parado. Parar = soltar (`drop`).
pub struct Fluxo {
    interno: Interno,
    pub nome: String,
    pub canais: usize,
}

enum Interno {
    Cpal(cpal::Stream),
    #[cfg(windows)]
    Wasapi(wasapi::Fluxo),
}

impl Fluxo {
    pub fn iniciar(&self) -> Res<()> {
        match &self.interno {
            Interno::Cpal(s) => Ok(s.play()?),
            #[cfg(windows)]
            Interno::Wasapi(f) => f.iniciar(),
        }
    }
    /// Período do dispositivo em quadros (tamanho típico de cada callback).
    pub fn periodo(&self) -> Res<u32> {
        match &self.interno {
            Interno::Cpal(s) => Ok(s.buffer_size()?),
            #[cfg(windows)]
            Interno::Wasapi(f) => Ok(f.periodo),
        }
    }
}

#[cfg(windows)]
fn abrir_wasapi(p: &Pedido, cb: wasapi::Callback) -> Res<Fluxo> {
    let (f, a) = wasapi::abrir(p.trecho.clone(), p.taxa, cb, p.avisos.clone())?;
    Ok(Fluxo { interno: Interno::Wasapi(f), nome: a.nome, canais: a.canais })
}

/// Desvia para o backend de baixa latência quando pedido (ou falha fora do Windows).
macro_rules! baixa_latencia {
    ($p:expr, $variante:ident, $cb:expr) => {
        if $p.backend == Backend::BaixaLatencia {
            #[cfg(windows)]
            return abrir_wasapi($p, wasapi::Callback::$variante(Box::new($cb)));
            #[cfg(not(windows))]
            return Err("modo de baixa latência só existe no Windows".into());
        }
    };
}

pub struct Info {
    pub nome: String,
    pub padrao: bool,
    pub formato: String,
}

pub struct Lista {
    pub host: String,
    pub entradas: Vec<Info>,
    pub saidas: Vec<Info>,
}

pub fn listar() -> Res<Lista> {
    let host = cpal::default_host();
    let padrao_in = host.default_input_device().map(|d| nome(&d));
    let padrao_out = host.default_output_device().map(|d| nome(&d));
    let info = |d: Device, entrada: bool, padrao: &Option<String>| {
        let cfg = if entrada { d.default_input_config() } else { d.default_output_config() };
        let formato = cfg
            .map(|c| format!("{} Hz, {} canais, {:?}", c.sample_rate(), c.channels(), c.sample_format()))
            .unwrap_or_else(|e| format!("({e})"));
        let nome = nome(&d);
        Info { padrao: padrao.as_ref() == Some(&nome), nome, formato }
    };
    Ok(Lista {
        host: host.id().name().to_string(),
        entradas: host.input_devices()?.map(|d| info(d, true, &padrao_in)).collect(),
        saidas: host.output_devices()?.map(|d| info(d, false, &padrao_out)).collect(),
    })
}

pub fn abrir_entrada(p: &Pedido, mut cb: impl FnMut(&[f32], Pacote) + Send + 'static) -> Res<Fluxo> {
    baixa_latencia!(p, Entrada, cb);
    let host = cpal::default_host();
    let dev = achar(&host, true, p.trecho.as_deref())?;
    let cfg = config(&dev, true, p)?;
    let (nome, canais) = (nome(&dev), cfg.channels as usize);
    // Com o dispositivo já rodando (outro fluxo aberto nele), o cpal acusa um Xrun no 1º pacote,
    // cuja flag de descontinuidade é indefinida: medido em 2026-10-20. Só conta Xrun depois dele.
    let entregou = Arc::new(AtomicBool::new(false));
    let e = entregou.clone();
    let mut erro = erro_cpal(p, true, &nome);
    let stream = dev.build_input_stream(
        cfg,
        move |dados: &[f32], info: &cpal::InputCallbackInfo| {
            let ts = info.timestamp();
            let callback_ns = ts.callback.as_nanos() as u64;
            cb(dados, Pacote { canais, callback_ns, dispositivo_ns: ts.capture.as_nanos() as u64 });
            e.store(true, Relaxed);
        },
        move |x: cpal::Error| {
            if x.kind() != ErrorKind::Xrun || entregou.load(Relaxed) {
                erro(x)
            }
        },
        None,
    )?;
    Ok(Fluxo { interno: Interno::Cpal(stream), nome, canais })
}

pub fn abrir_saida(p: &Pedido, mut cb: impl FnMut(&mut [f32], Pacote) + Send + 'static) -> Res<Fluxo> {
    baixa_latencia!(p, Saida, cb);
    let host = cpal::default_host();
    let dev = achar(&host, false, p.trecho.as_deref())?;
    let cfg = config(&dev, false, p)?;
    let (nome, canais) = (nome(&dev), cfg.channels as usize);
    let stream = dev.build_output_stream(
        cfg,
        move |dados: &mut [f32], info: &cpal::OutputCallbackInfo| {
            let ts = info.timestamp();
            let callback_ns = ts.callback.as_nanos() as u64;
            cb(dados, Pacote { canais, callback_ns, dispositivo_ns: ts.playback.as_nanos() as u64 });
        },
        erro_cpal(p, false, &nome),
        None,
    )?;
    Ok(Fluxo { interno: Interno::Cpal(stream), nome, canais })
}

fn erro_cpal(p: &Pedido, entrada: bool, nome: &str) -> impl FnMut(cpal::Error) + Send + 'static {
    let (destino, nome) = (p.avisos.clone(), nome.to_string());
    move |e| avisar(&destino, Aviso { entrada, fatal: fatal(e.kind()), texto: format!("{nome}: {e}") })
}

fn nome(d: &Device) -> String {
    d.description().map(|x| x.name().to_string()).unwrap_or_else(|_| "?".into())
}

fn achar(host: &Host, entrada: bool, trecho: Option<&str>) -> Res<Device> {
    let Some(t) = trecho else {
        let d = if entrada { host.default_input_device() } else { host.default_output_device() };
        return d.ok_or_else(|| "sem dispositivo padrão".into());
    };
    let t = t.to_lowercase();
    let mut devs = if entrada { host.input_devices()? } else { host.output_devices()? };
    devs.find(|d| nome(d).to_lowercase().contains(&t))
        .ok_or_else(|| format!("nenhum dispositivo contém \"{t}\" (veja --listar)").into())
}

/// Mantém os canais do dispositivo e pede a taxa do motor em f32 (o WASAPI converte na saída).
fn config(d: &Device, entrada: bool, p: &Pedido) -> Res<StreamConfig> {
    let padrao = if entrada { d.default_input_config()? } else { d.default_output_config()? };
    Ok(StreamConfig {
        channels: padrao.channels(),
        sample_rate: p.taxa,
        buffer_size: p.buffer.map_or(BufferSize::Default, BufferSize::Fixed),
    })
}
