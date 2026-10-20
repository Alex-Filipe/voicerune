//! Latência ponta a ponta por cliques: troca o mic por silêncio com um clique a cada 500 ms
//! antes do motor e detecta quando ele chega no dispositivo de retorno (ex.: CABLE Output),
//! comparando timestamps de captura no mesmo relógio. IA desligada: o clique não sobrevive ao
//! RNNoise, então o atraso algorítmico dele é somado à parte.

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mic_limpo_audio::{self as audio, Backend, Pacote, Pedido};
use mic_limpo_core::denoise::SAMPLE_RATE;
use mic_limpo_core::metricas::{amostras_ms, Media};
use mic_limpo_core::motor::{self, Config, Tempos, ATRASO_IA_AMOSTRAS};

use crate::Res;

/// Intervalo entre cliques (500 ms).
const PERIODO_CLIQUE: u64 = SAMPLE_RATE as u64 / 2;

pub struct Opcoes {
    pub entrada: Option<String>,
    pub saida: String,
    pub retorno: String,
    pub motor: Config,
    pub backend: Backend,
    pub buffer: Option<u32>,
}

impl Default for Opcoes {
    fn default() -> Self {
        Self {
            entrada: Some("Microphone".into()),
            saida: "CABLE Input".into(),
            retorno: "CABLE Output".into(),
            motor: Config::default(),
            backend: Backend::Padrao,
            buffer: None,
        }
    }
}

#[derive(Default)]
struct Cliques {
    /// Instante (ns) em que o último clique foi "capturado".
    injetado_ns: AtomicU64,
    /// Instante previsto (pelo backend) em que o clique sai para o dispositivo de saída.
    saida_ns: AtomicU64,
    injetados: AtomicU64,
    latencia_us: Media,
    min_us: AtomicU64,
    max_us: AtomicU64,
    /// Da saída prevista até o retorno: o trecho dentro do dispositivo virtual.
    cabo_us: Media,
}

fn ns_da_amostra(i: usize) -> u64 {
    i as u64 * 1_000_000_000 / SAMPLE_RATE as u64
}

fn tempos(p: Pacote) -> Tempos {
    Tempos { callback_ns: p.callback_ns, dispositivo_ns: p.dispositivo_ns }
}

pub fn medir(op: &Opcoes, segundos: u64) -> Res<()> {
    let (ctl, mut entrada, mut saida) = motor::criar(Config { ia: false, ..op.motor });
    let ck = Arc::new(Cliques { min_us: AtomicU64::new(u64::MAX), ..Default::default() });
    let backend = op.backend;
    let pedido = |trecho: Option<String>| Pedido { backend, trecho, taxa: SAMPLE_RATE, buffer: op.buffer };

    // Entrada: ignora o mic e entrega silêncio com um clique a cada PERIODO_CLIQUE.
    let (c, mut cliques, mut contador) = (ck.clone(), vec![0.0f32; SAMPLE_RATE as usize], 0u64);
    let fin = audio::abrir_entrada(&pedido(op.entrada.clone()), move |d, p| {
        let n = (d.len() / p.canais).min(cliques.len());
        for (i, s) in cliques[..n].iter_mut().enumerate() {
            *s = 0.0;
            if contador % PERIODO_CLIQUE == 0 {
                *s = 0.9;
                c.injetado_ns.store(p.dispositivo_ns + ns_da_amostra(i), Relaxed);
                c.injetados.fetch_add(1, Relaxed);
            }
            contador += 1;
        }
        entrada.processar(&cliques[..n], 1, tempos(p));
    })?;

    let c = ck.clone();
    let fout = audio::abrir_saida(&pedido(Some(op.saida.clone())), move |d, p| {
        saida.preencher(d, p.canais, tempos(p));
        for (i, f) in d.chunks(p.canais).enumerate() {
            if f[0] > 0.5 {
                c.saida_ns.store(p.dispositivo_ns + ns_da_amostra(i), Relaxed);
            }
        }
    })?;

    // Retorno: primeiro pico depois de 250 ms de calma é o clique. Sempre no backend padrão,
    // como faria o Discord; muda só o caminho do app.
    let (c, mut ultima) = (ck.clone(), 0u64);
    let retorno = Pedido { backend: Backend::Padrao, ..pedido(Some(op.retorno.clone())) };
    let fret = audio::abrir_entrada(&retorno, move |d, p| {
        for (i, f) in d.chunks_exact(p.canais).enumerate() {
            let t = p.dispositivo_ns + ns_da_amostra(i);
            if f[0].abs() < 0.05 || t.saturating_sub(ultima) < 250_000_000 {
                continue;
            }
            ultima = t;
            let clique = c.injetado_ns.load(Relaxed);
            if clique > 0 && t > clique && t - clique < 400_000_000 {
                let us = (t - clique) / 1000;
                c.latencia_us.add(us);
                c.min_us.fetch_min(us, Relaxed);
                c.max_us.fetch_max(us, Relaxed);
                let saida = c.saida_ns.load(Relaxed);
                if saida > clique && t > saida {
                    c.cabo_us.add((t - saida) / 1000);
                }
            }
        }
    })?;

    println!("Entrada: {} | Saída: {} | Retorno: {}", fin.nome, fout.nome, fret.nome);
    println!(
        "Backend: {backend:?} | buffer pedido: {} | folga {} ms | teto {} ms | MMCSS: {}",
        op.buffer.map_or("padrão".into(), |b| format!("{b} quadros")),
        op.motor.folga_ms,
        op.motor.teto_ms,
        if audio::MMCSS || backend == Backend::BaixaLatencia { "sim" } else { "não" }
    );
    fin.iniciar()?;
    fout.iniciar()?;
    fret.iniciar()?;
    println!("Período do dispositivo: entrada {} quadros, saída {} quadros", fin.periodo()?, fout.periodo()?);
    println!("Medindo por {segundos} s...");
    let inicio = Instant::now();
    std::thread::sleep(Duration::from_secs(segundos));
    drop(fret);
    drop(fin);
    drop(fout);
    println!("\n{}", ctl.metricas.resumo(inicio.elapsed(), false));

    let n = ck.latencia_us.n();
    println!("\nMedição por cliques ({} injetados, {n} detectados):", ck.injetados.load(Relaxed));
    if n == 0 {
        println!("  nenhum clique detectado no retorno");
        return Ok(());
    }
    let ms = |us: u64| us as f64 / 1000.0;
    let media = ck.latencia_us.valor() / 1000.0;
    let cabo = ck.cabo_us.valor() / 1000.0;
    let ia_ms = amostras_ms(ATRASO_IA_AMOSTRAS as f64);
    println!(
        "  captura no mic → captura no retorno: média {media:.1} ms (mín {:.1}, máx {:.1})",
        ms(ck.min_us.load(Relaxed)),
        ms(ck.max_us.load(Relaxed))
    );
    println!(
        "  divisão: app até a saída prevista {:.1} ms | saída prevista → retorno (VB-Cable) {cabo:.1} ms",
        media - cabo
    );
    println!("  com o RNNoise ligado: + {ia_ms:.1} ms (atraso algorítmico) = {:.1} ms", media + ia_ms);
    Ok(())
}
