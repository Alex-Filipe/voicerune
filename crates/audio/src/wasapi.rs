//! Backend WASAPI de baixa latência (só Windows): modo compartilhado com IAudioClient3 no menor
//! período que o driver aceita (~3 ms no mic e no VB-Cable desta máquina, contra 10 ms do cpal) e
//! thread com MMCSS "Pro Audio". Entrega os mesmos `Pacote`s do backend cpal.
//!
//! Cada fluxo tem uma thread própria, que cria e usa os objetos COM do início ao fim.

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use windows::core::{w, GUID};
use windows::Win32::Devices::Properties::DEVPKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, HANDLE, PROPERTYKEY, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    STGM_READ,
};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, WaitForSingleObject,
};

use crate::{avisar, escolher, Aviso, Pacote, Res};

const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const SUBTIPO_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

pub(crate) enum Callback {
    Entrada(Box<dyn FnMut(&[f32], Pacote) + Send>),
    Saida(Box<dyn FnMut(&mut [f32], Pacote) + Send>),
}

pub(crate) struct Aberto {
    pub nome: String,
    pub canais: usize,
    pub periodo: u32,
}

/// Fluxo rodando numa thread própria. Parar = soltar (`drop`).
pub(crate) struct Fluxo {
    iniciar: Option<Sender<()>>,
    parar: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub periodo: u32,
}

impl Fluxo {
    pub fn iniciar(&self) -> Res<()> {
        self.iniciar.as_ref().ok_or("fluxo já encerrado")?.send(()).map_err(|_| "a thread de áudio terminou".into())
    }
}

impl Drop for Fluxo {
    fn drop(&mut self) {
        self.parar.store(true, Relaxed);
        self.iniciar.take(); // acorda a thread se ela ainda esperava o início
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub(crate) fn abrir(trecho: Option<String>, taxa: u32, cb: Callback, avisos: Option<Sender<Aviso>>) -> Res<(Fluxo, Aberto)> {
    let (tx_pronto, rx_pronto) = mpsc::sync_channel::<Result<Aberto, String>>(1);
    let (tx_iniciar, rx_iniciar) = mpsc::channel::<()>();
    let parar = Arc::new(AtomicBool::new(false));
    let p = parar.clone();
    let thread = std::thread::Builder::new()
        .name("mic-limpo-wasapi".into())
        .spawn(move || unsafe { thread_fluxo(trecho, taxa, cb, tx_pronto, rx_iniciar, p, avisos) })?;
    match rx_pronto.recv() {
        Ok(Ok(aberto)) => {
            let fluxo = Fluxo { iniciar: Some(tx_iniciar), parar, thread: Some(thread), periodo: aberto.periodo };
            Ok((fluxo, aberto))
        }
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e.into())
        }
        Err(_) => Err("a thread de áudio terminou antes de abrir o dispositivo".into()),
    }
}

/// Tudo o que a thread precisa depois de abrir o dispositivo.
struct Sessao {
    nome: String,
    cliente: IAudioClient3,
    evento: HANDLE,
    canais: usize,
    taxa: u32,
    buffer: u32,
}

unsafe fn thread_fluxo(
    trecho: Option<String>,
    taxa: u32,
    mut cb: Callback,
    pronto: mpsc::SyncSender<Result<Aberto, String>>,
    iniciar: Receiver<()>,
    parar: Arc<AtomicBool>,
    avisos: Option<Sender<Aviso>>,
) {
    if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
        let _ = pronto.send(Err("CoInitializeEx falhou".into()));
        return;
    }
    let entrada = matches!(cb, Callback::Entrada(_));
    let sessao = match abrir_dispositivo(trecho.as_deref(), entrada, taxa, &avisos) {
        Ok((s, aberto)) => {
            let _ = pronto.send(Ok(aberto));
            s
        }
        Err(e) => {
            let _ = pronto.send(Err(e.to_string()));
            CoUninitialize();
            return;
        }
    };
    // Espera o `iniciar()`; canal fechado = o fluxo foi solto antes de começar.
    if iniciar.recv().is_ok() {
        let mut idx = 0u32;
        let mmcss = AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut idx);
        if let Err(e) = rodar(&sessao, &mut cb, &parar) {
            avisar(&avisos, Aviso { entrada, fatal: true, texto: format!("{}: {e}", sessao.nome) });
        }
        if let Ok(h) = mmcss {
            let _ = AvRevertMmThreadCharacteristics(h);
        }
    }
    let _ = sessao.cliente.Stop();
    let _ = CloseHandle(sessao.evento);
    drop(sessao);
    CoUninitialize();
}

unsafe fn abrir_dispositivo(
    trecho: Option<&str>,
    entrada: bool,
    taxa: u32,
    avisos: &Option<Sender<Aviso>>,
) -> Res<(Sessao, Aberto)> {
    let fluxo = if entrada { eCapture } else { eRender };
    let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let dev = match trecho {
        None => en.GetDefaultAudioEndpoint(fluxo, eConsole)?,
        Some(t) => {
            let col = en.EnumAudioEndpoints(fluxo, DEVICE_STATE_ACTIVE)?;
            let devs = (0..col.GetCount()?).map(|i| col.Item(i)).collect::<Result<Vec<_>, _>>()?;
            let nomes: Vec<String> = devs.iter().map(|d| nome(d).unwrap_or_default()).collect();
            let i = escolher(&nomes, t).ok_or_else(|| format!("nenhum dispositivo contém \"{t}\" (veja listar)"))?;
            devs.into_iter().nth(i).expect("índice veio da lista")
        }
    };
    let nome = nome(&dev).unwrap_or_else(|_| "?".into());
    let cliente: IAudioClient3 = dev.Activate(CLSCTX_ALL, None)?;
    let fmt = cliente.GetMixFormat()?;
    let resultado = configurar(&cliente, fmt, taxa, entrada, avisos);
    let canais = (*fmt).nChannels as usize;
    CoTaskMemFree(Some(fmt as _));
    let (periodo, buffer, evento) = resultado.map_err(|e| format!("{nome}: {e}"))?;
    let sessao = Sessao { nome: nome.clone(), cliente, evento, canais, taxa, buffer };
    Ok((sessao, Aberto { nome, canais, periodo }))
}

/// Confere o formato do mixer e inicializa no menor período possível. Devolve (período, buffer, evento).
unsafe fn configurar(
    c: &IAudioClient3,
    fmt: *mut WAVEFORMATEX,
    taxa: u32,
    entrada: bool,
    avisos: &Option<Sender<Aviso>>,
) -> Res<(u32, u32, HANDLE)> {
    let f = *fmt;
    let subtipo = || std::ptr::addr_of!((*(fmt as *const WAVEFORMATEXTENSIBLE)).SubFormat).read_unaligned();
    let float = f.wFormatTag == WAVE_FORMAT_IEEE_FLOAT || (f.wFormatTag == WAVE_FORMAT_EXTENSIBLE && subtipo() == SUBTIPO_FLOAT);
    if !float || f.wBitsPerSample != 32 || f.nSamplesPerSec != taxa {
        return Err(format!(
            "mixer em {} Hz/{} bits (precisa {taxa} Hz em float 32); use o modo padrão",
            { f.nSamplesPerSec },
            { f.wBitsPerSample }
        )
        .into());
    }
    let (mut padrao, mut passo, mut minimo, mut maximo) = (0u32, 0u32, 0u32, 0u32);
    c.GetSharedModeEnginePeriod(fmt, &mut padrao, &mut passo, &mut minimo, &mut maximo)?;
    // Se outro app já abriu o dispositivo, o Windows trava o período no dele: usa o atual.
    let periodo = match c.InitializeSharedAudioStream(AUDCLNT_STREAMFLAGS_EVENTCALLBACK, minimo, fmt, None) {
        Ok(()) => minimo,
        Err(e) => {
            let mut atual_fmt = std::ptr::null_mut();
            let mut atual = 0u32;
            c.GetCurrentSharedModeEnginePeriod(&mut atual_fmt, &mut atual)?;
            CoTaskMemFree(Some(atual_fmt as _));
            let texto = format!(
                "período de {minimo} quadros recusado ({e}); usando o atual, {atual} quadros{}",
                if entrada { " (outro app usando o mic?)" } else { "" }
            );
            avisar(avisos, Aviso { entrada, fatal: false, texto });
            c.InitializeSharedAudioStream(AUDCLNT_STREAMFLAGS_EVENTCALLBACK, atual, fmt, None)?;
            atual
        }
    };
    let evento = CreateEventW(None, false, false, None)?;
    c.SetEventHandle(evento)?;
    Ok((periodo, c.GetBufferSize()?, evento))
}

unsafe fn nome(dev: &IMMDevice) -> Res<String> {
    let props = dev.OpenPropertyStore(STGM_READ)?;
    let valor = props.GetValue(&DEVPKEY_Device_FriendlyName as *const _ as *const PROPERTYKEY)?;
    let texto = PropVariantToStringAlloc(&valor)?;
    let s = texto.to_string().unwrap_or_default();
    CoTaskMemFree(Some(texto.0 as _));
    Ok(s)
}

/// Relógio QPC em ns: o mesmo dos timestamps do WASAPI (unidades de 100 ns).
struct Relogio {
    freq: u128,
}

impl Relogio {
    unsafe fn novo() -> Res<Self> {
        let mut f = 0i64;
        QueryPerformanceFrequency(&mut f)?;
        Ok(Self { freq: f as u128 })
    }
    unsafe fn agora_ns(&self) -> u64 {
        let mut c = 0i64;
        let _ = QueryPerformanceCounter(&mut c);
        (c as u128 * 1_000_000_000 / self.freq) as u64
    }
}

unsafe fn rodar(s: &Sessao, cb: &mut Callback, parar: &AtomicBool) -> Res<()> {
    let relogio = Relogio::novo()?;
    match cb {
        Callback::Entrada(cb) => {
            let cap: IAudioCaptureClient = s.cliente.GetService()?;
            // Pacote marcado como silêncio vem com o ponteiro indefinido: entrega zeros.
            let zeros = vec![0.0f32; s.buffer as usize * s.canais];
            s.cliente.Start()?;
            while !parar.load(Relaxed) {
                if WaitForSingleObject(s.evento, 100) != WAIT_OBJECT_0 {
                    continue;
                }
                while cap.GetNextPacketSize()? > 0 {
                    let (mut ptr, mut quadros, mut flags, mut qpc) = (std::ptr::null_mut(), 0u32, 0u32, 0u64);
                    cap.GetBuffer(&mut ptr, &mut quadros, &mut flags, None, Some(&mut qpc))?;
                    let n = (quadros as usize * s.canais).min(zeros.len());
                    let dados = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || ptr.is_null() {
                        &zeros[..n]
                    } else {
                        std::slice::from_raw_parts(ptr as *const f32, n)
                    };
                    let pacote = Pacote { canais: s.canais, callback_ns: relogio.agora_ns(), dispositivo_ns: qpc * 100 };
                    cb(dados, pacote);
                    cap.ReleaseBuffer(quadros)?;
                }
            }
        }
        Callback::Saida(cb) => {
            let ren: IAudioRenderClient = s.cliente.GetService()?;
            // Começa com o buffer cheio de silêncio, para o primeiro ciclo não faltar áudio.
            ren.GetBuffer(s.buffer)?;
            ren.ReleaseBuffer(s.buffer, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;
            s.cliente.Start()?;
            while !parar.load(Relaxed) {
                if WaitForSingleObject(s.evento, 100) != WAIT_OBJECT_0 {
                    continue;
                }
                let fila = s.cliente.GetCurrentPadding()?;
                let livre = s.buffer - fila;
                if livre == 0 {
                    continue;
                }
                let ptr = ren.GetBuffer(livre)?;
                let dados = std::slice::from_raw_parts_mut(ptr as *mut f32, livre as usize * s.canais);
                let agora = relogio.agora_ns();
                // A primeira amostra escrita agora toca depois do que já está na fila.
                let reproducao = agora + fila as u64 * 1_000_000_000 / s.taxa as u64;
                cb(dados, Pacote { canais: s.canais, callback_ns: agora, dispositivo_ns: reproducao });
                ren.ReleaseBuffer(livre, 0)?;
            }
        }
    }
    Ok(())
}
