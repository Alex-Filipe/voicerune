//! Protótipo da fase 1: microfone → RNNoise → saída (por padrão o VB-Cable).

#[cfg(windows)]
mod sonda;

use std::error::Error;
use std::f32::consts::TAU;
use std::io::BufRead;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, Host, StreamConfig};
use mic_limpo_core::denoise::{Denoiser, FRAME, SAMPLE_RATE};

type Res<T> = Result<T, Box<dyn Error>>;

const SAIDA_PADRAO: &str = "CABLE Input";
const RETORNO_PADRAO: &str = "CABLE Output";
/// Intervalo entre cliques na medição de latência (500 ms).
const PERIODO_CLIQUE: u64 = SAMPLE_RATE as u64 / 2;

const AJUDA: &str = "\
mic-limpo (protótipo da fase 1)

  mic-limpo [opções]        microfone → RNNoise → saída
  mic-limpo --listar        lista dispositivos de áudio
  mic-limpo --bench         mede o custo do RNNoise sem dispositivos
  mic-limpo --sondar        mostra períodos e latências que o WASAPI permite (Windows)

Opções:
  --entrada TRECHO   parte do nome do microfone (padrão: o do sistema)
  --saida TRECHO     parte do nome da saída (padrão: \"CABLE Input\")
  --sem-ia           começa com a IA desligada
  --folga-ms N       margem do buffer entre entrada e saída (padrão: 10)
  --teto-ms N        acima de folga + N ms, descarta o excesso (padrão: 20)
  --buffer N         buffer do WASAPI em quadros, nos dois lados (padrão: o do sistema)
  --medir-latencia   troca o mic por cliques e mede quando chegam em --retorno (IA desligada)
  --retorno TRECHO   onde escutar os cliques (padrão: \"CABLE Output\")
  --segundos N       roda N segundos e mostra o resumo (bench: padrão 20)

Rodando: Enter liga/desliga a IA; q + Enter encerra e mostra o resumo.";

enum Modo {
    Rodar,
    Listar,
    Bench,
    Sondar,
}

struct Opcoes {
    modo: Modo,
    entrada: Option<String>,
    saida: String,
    ia: bool,
    folga_ms: u32,
    teto_ms: u32,
    buffer: Option<u32>,
    medir: bool,
    retorno: String,
    segundos: Option<u64>,
}

fn main() {
    if let Err(e) = executar() {
        eprintln!("erro: {e}");
        std::process::exit(1);
    }
}

fn executar() -> Res<()> {
    let mut op = Opcoes {
        modo: Modo::Rodar,
        entrada: None,
        saida: SAIDA_PADRAO.into(),
        ia: true,
        folga_ms: 10,
        teto_ms: 20,
        buffer: None,
        medir: false,
        retorno: RETORNO_PADRAO.into(),
        segundos: None,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut valor = || it.next().cloned().ok_or_else(|| format!("{a} precisa de um valor"));
        match a.as_str() {
            "--listar" => op.modo = Modo::Listar,
            "--bench" => op.modo = Modo::Bench,
            "--sondar" => op.modo = Modo::Sondar,
            "--entrada" => op.entrada = Some(valor()?),
            "--saida" => op.saida = valor()?,
            "--sem-ia" => op.ia = false,
            "--folga-ms" => op.folga_ms = valor()?.parse()?,
            "--teto-ms" => op.teto_ms = valor()?.parse()?,
            "--buffer" => op.buffer = Some(valor()?.parse()?),
            "--medir-latencia" => op.medir = true,
            "--retorno" => op.retorno = valor()?,
            "--segundos" => op.segundos = Some(valor()?.parse()?),
            "-h" | "--ajuda" => {
                println!("{AJUDA}");
                return Ok(());
            }
            _ => return Err(format!("opção desconhecida: {a} (veja --ajuda)").into()),
        }
    }
    match op.modo {
        Modo::Listar => listar(),
        Modo::Bench => bench(op.segundos.unwrap_or(20)),
        #[cfg(windows)]
        Modo::Sondar => Ok(sonda::sondar(&["Microphone", SAIDA_PADRAO, RETORNO_PADRAO])?),
        #[cfg(not(windows))]
        Modo::Sondar => Err("--sondar só existe no Windows".into()),
        Modo::Rodar => rodar(op),
    }
}

fn nome(d: &Device) -> String {
    d.description()
        .map(|x| x.name().to_string())
        .unwrap_or_else(|_| "?".into())
}

fn listar() -> Res<()> {
    let host = cpal::default_host();
    let padrao_in = host.default_input_device().map(|d| nome(&d));
    let padrao_out = host.default_output_device().map(|d| nome(&d));
    println!("Host: {}", host.id().name());

    println!("\nEntradas:");
    for d in host.input_devices()? {
        let n = nome(&d);
        let cfg = d
            .default_input_config()
            .map(|c| format!("{} Hz, {} canais, {:?}", c.sample_rate(), c.channels(), c.sample_format()))
            .unwrap_or_else(|e| format!("({e})"));
        let marca = if padrao_in.as_ref() == Some(&n) { "*" } else { " " };
        println!(" {marca} {n} — {cfg}");
    }

    println!("\nSaídas:");
    for d in host.output_devices()? {
        let n = nome(&d);
        let cfg = d
            .default_output_config()
            .map(|c| format!("{} Hz, {} canais, {:?}", c.sample_rate(), c.channels(), c.sample_format()))
            .unwrap_or_else(|e| format!("({e})"));
        let marca = if padrao_out.as_ref() == Some(&n) { "*" } else { " " };
        println!(" {marca} {n} — {cfg}");
    }
    println!("\n* = padrão do sistema");
    Ok(())
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

/// Mantém os canais do dispositivo e pede 48 kHz em f32 (o WASAPI converte se preciso).
fn config_48k(d: &Device, entrada: bool, buffer: Option<u32>) -> Res<StreamConfig> {
    let padrao = if entrada { d.default_input_config()? } else { d.default_output_config()? };
    Ok(StreamConfig {
        channels: padrao.channels(),
        sample_rate: SAMPLE_RATE,
        buffer_size: buffer.map_or(BufferSize::Default, BufferSize::Fixed),
    })
}

/// Média acumulada sem locks (pode ser atualizada na thread de áudio).
#[derive(Default)]
struct Media {
    soma: AtomicU64,
    n: AtomicU64,
}

impl Media {
    fn add(&self, v: u64) {
        self.soma.fetch_add(v, Relaxed);
        self.n.fetch_add(1, Relaxed);
    }
    fn valor(&self) -> f64 {
        let n = self.n.load(Relaxed);
        if n == 0 { 0.0 } else { self.soma.load(Relaxed) as f64 / n as f64 }
    }
}

#[derive(Default)]
struct Stats {
    quadros: AtomicU64,
    dsp_ns: AtomicU64,
    dsp_max_ns: AtomicU64,
    atraso_entrada_us: Media,
    atraso_saida_us: Media,
    acumulo: Media,
    fila: Media,
    underruns: AtomicU64,
    descartes: AtomicU64,
    descartadas: AtomicU64,
    perdidas: AtomicU64,
    // Medição por cliques: instante (ns, relógio do WASAPI) do último clique injetado.
    ultimo_clique_ns: AtomicU64,
    // Instante previsto (pelo cpal) em que o clique sai para o dispositivo de saída.
    clique_saida_ns: AtomicU64,
    cliques: AtomicU64,
    cabo_us: Media,
    latencia_us: Media,
    latencia_min_us: AtomicU64,
    latencia_max_us: AtomicU64,
}

fn amostras_ms(n: f64) -> f64 {
    n * 1000.0 / SAMPLE_RATE as f64
}

fn rodar(op: Opcoes) -> Res<()> {
    let host = cpal::default_host();
    let dev_in = achar(&host, true, op.entrada.as_deref())?;
    let dev_out = achar(&host, false, Some(&op.saida))?;
    let cfg_in = config_48k(&dev_in, true, op.buffer)?;
    let cfg_out = config_48k(&dev_out, false, op.buffer)?;
    println!("Entrada: {} ({} canais)", nome(&dev_in), cfg_in.channels);
    println!("Saída:   {} ({} canais)", nome(&dev_out), cfg_out.channels);
    println!(
        "Buffer pedido: {} | folga {} ms | teto {} ms | MMCSS: {}",
        op.buffer.map_or("padrão".into(), |b| format!("{b} quadros")),
        op.folga_ms,
        op.teto_ms,
        if cfg!(feature = "realtime") { "sim" } else { "não" }
    );

    let stats = Arc::new(Stats { latencia_min_us: AtomicU64::new(u64::MAX), ..Default::default() });
    let medir = op.medir;
    let ia = Arc::new(AtomicBool::new(op.ia && !medir));
    let folga = (SAMPLE_RATE * op.folga_ms / 1000) as usize;
    let teto = (SAMPLE_RATE * op.teto_ms / 1000) as usize;
    // 200 ms de capacidade: bem acima da folga, só para absorver soluços.
    let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(SAMPLE_RATE as usize / 5);

    // Entrada: mistura para mono, aplica o RNNoise por quadro e empurra para o ring buffer.
    let ch_in = cfg_in.channels as usize;
    let mut den = Denoiser::new();
    let mut mono = vec![0.0f32; SAMPLE_RATE as usize];
    let mut contador = 0u64;
    let (s, ia_in) = (stats.clone(), ia.clone());
    let stream_in = dev_in.build_input_stream(
        cfg_in,
        move |data: &[f32], info: &cpal::InputCallbackInfo| {
            let ts = info.timestamp();
            s.atraso_entrada_us.add(ts.callback.duration_since(ts.capture).as_micros() as u64);
            let n = (data.len() / ch_in).min(mono.len());
            if medir {
                // Silêncio com um clique a cada PERIODO_CLIQUE; guarda quando ele foi "capturado".
                for (i, m) in mono[..n].iter_mut().enumerate() {
                    *m = 0.0;
                    if contador % PERIODO_CLIQUE == 0 {
                        *m = 0.9;
                        let t = ts.capture.as_nanos() as u64 + i as u64 * 1_000_000_000 / SAMPLE_RATE as u64;
                        s.ultimo_clique_ns.store(t, Relaxed);
                        s.cliques.fetch_add(1, Relaxed);
                    }
                    contador += 1;
                }
            } else {
                for (m, f) in mono[..n].iter_mut().zip(data.chunks_exact(ch_in)) {
                    *m = f.iter().sum::<f32>() / ch_in as f32;
                }
            }
            let t0 = Instant::now();
            let mut quadros = 0;
            den.push(&mono[..n], ia_in.load(Relaxed), |q, _vad| {
                quadros += 1;
                for &x in q {
                    if prod.push(x).is_err() {
                        s.perdidas.fetch_add(1, Relaxed);
                    }
                }
            });
            if quadros > 0 {
                let ns = t0.elapsed().as_nanos() as u64;
                s.quadros.fetch_add(quadros, Relaxed);
                s.dsp_ns.fetch_add(ns, Relaxed);
                s.dsp_max_ns.fetch_max(ns, Relaxed);
            }
            s.acumulo.add(den.pendentes() as u64);
        },
        |e| eprintln!("erro na entrada: {e}"),
        None,
    )?;

    // Saída: mantém `folga` amostras de margem; descarta o excesso se os relógios divergirem.
    let ch_out = cfg_out.channels as usize;
    let s = stats.clone();
    let mut esperando = true;
    let stream_out = dev_out.build_output_stream(
        cfg_out,
        move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
            let ts = info.timestamp();
            s.atraso_saida_us.add(ts.playback.duration_since(ts.callback).as_micros() as u64);
            let n = data.len() / ch_out;
            let fila = cons.slots();
            s.fila.add(fila.saturating_sub(n / 2) as u64);
            if esperando && fila < n + folga {
                data.fill(0.0);
                return;
            }
            esperando = false;
            if fila > n + folga + teto {
                let excesso = fila - n - folga;
                if let Ok(c) = cons.read_chunk(excesso) {
                    c.commit_all();
                }
                s.descartes.fetch_add(1, Relaxed);
                s.descartadas.fetch_add(excesso as u64, Relaxed);
            }
            for (i, f) in data.chunks_mut(ch_out).enumerate() {
                match cons.pop() {
                    Ok(x) => {
                        f.fill(x);
                        if medir && x > 0.5 {
                            let t = ts.playback.as_nanos() as u64 + i as u64 * 1_000_000_000 / SAMPLE_RATE as u64;
                            s.clique_saida_ns.store(t, Relaxed);
                        }
                    }
                    Err(_) => {
                        f.fill(0.0);
                        if !esperando {
                            esperando = true;
                            s.underruns.fetch_add(1, Relaxed);
                        }
                    }
                }
            }
        },
        |e| eprintln!("erro na saída: {e}"),
        None,
    )?;

    // Retorno (só na medição): detecta o clique depois de passar pelo dispositivo virtual.
    let stream_ret = if medir {
        let dev_ret = achar(&host, true, Some(&op.retorno))?;
        let cfg_ret = config_48k(&dev_ret, true, op.buffer)?;
        let ch_ret = cfg_ret.channels as usize;
        println!("Retorno: {} ({} canais)", nome(&dev_ret), ch_ret);
        let s = stats.clone();
        let mut ultima_deteccao = 0u64;
        Some(dev_ret.build_input_stream(
            cfg_ret,
            move |data: &[f32], info: &cpal::InputCallbackInfo| {
                let base = info.timestamp().capture.as_nanos() as u64;
                for (i, f) in data.chunks_exact(ch_ret).enumerate() {
                    if f[0].abs() < 0.05 {
                        continue;
                    }
                    let t = base + i as u64 * 1_000_000_000 / SAMPLE_RATE as u64;
                    if t.saturating_sub(ultima_deteccao) < 250_000_000 {
                        continue;
                    }
                    ultima_deteccao = t;
                    let clique = s.ultimo_clique_ns.load(Relaxed);
                    if clique > 0 && t > clique && t - clique < 400_000_000 {
                        let us = (t - clique) / 1000;
                        s.latencia_us.add(us);
                        s.latencia_min_us.fetch_min(us, Relaxed);
                        s.latencia_max_us.fetch_max(us, Relaxed);
                        let saida = s.clique_saida_ns.load(Relaxed);
                        if saida > clique && t > saida {
                            s.cabo_us.add((t - saida) / 1000);
                        }
                    }
                }
            },
            |e| eprintln!("erro no retorno: {e}"),
            None,
        )?)
    } else {
        None
    };

    stream_in.play()?;
    stream_out.play()?;
    if let Some(r) = &stream_ret {
        r.play()?;
    }
    println!(
        "Período do WASAPI: entrada {} quadros, saída {} quadros",
        stream_in.buffer_size()?,
        stream_out.buffer_size()?
    );
    let inicio = Instant::now();

    if let Some(seg) = op.segundos {
        println!("Rodando por {seg} s (IA {})...", if ia.load(Relaxed) { "ligada" } else { "desligada" });
        std::thread::sleep(Duration::from_secs(seg));
    } else {
        println!("IA ligada: {}. Enter alterna, q + Enter encerra.", op.ia);
        for linha in std::io::stdin().lock().lines() {
            if linha?.trim() == "q" {
                break;
            }
            let novo = !ia.load(Relaxed);
            ia.store(novo, Relaxed);
            println!("IA {}", if novo { "ligada" } else { "desligada" });
        }
    }
    drop(stream_ret);
    drop(stream_in);
    drop(stream_out);
    resumo(&stats, inicio.elapsed(), ia.load(Relaxed));
    if medir {
        resumo_medicao(&stats);
    }
    Ok(())
}

fn resumo(s: &Stats, duracao: Duration, ia: bool) {
    let quadros = s.quadros.load(Relaxed).max(1) as f64;
    let dsp_ms = s.dsp_ns.load(Relaxed) as f64 / quadros / 1e6;
    let quadro_ms = amostras_ms(FRAME as f64);
    let ent = s.atraso_entrada_us.valor() / 1000.0;
    let sai = s.atraso_saida_us.valor() / 1000.0;
    let acumulo = amostras_ms(s.acumulo.valor());
    let fila = amostras_ms(s.fila.valor());
    let algoritmo = if ia { quadro_ms } else { 0.0 };

    println!("\nResumo ({:.1} s, IA {} ao final):", duracao.as_secs_f64(), if ia { "ligada" } else { "desligada" });
    println!(
        "  DSP por quadro de 10 ms: média {dsp_ms:.3} ms ({:.1}% de um núcleo), pior callback {:.3} ms",
        dsp_ms / quadro_ms * 100.0,
        s.dsp_max_ns.load(Relaxed) as f64 / 1e6
    );
    println!(
        "  underruns: {} | descartes: {} ({} amostras) | perdidas: {}",
        s.underruns.load(Relaxed),
        s.descartes.load(Relaxed),
        s.descartadas.load(Relaxed),
        s.perdidas.load(Relaxed)
    );
    println!("  atrasos médios (ms): driver entrada {ent:.1} | acúmulo de quadro {acumulo:.1} | RNNoise {algoritmo:.1} | fila {fila:.1} | driver saída {sai:.1}");
    println!("  latência estimada do app: {:.1} ms (soma acima; estimativa, não medição acústica)", ent + acumulo + algoritmo + fila + sai);
}

fn resumo_medicao(s: &Stats) {
    let n = s.latencia_us.n.load(Relaxed);
    println!("\nMedição por cliques ({} injetados, {n} detectados):", s.cliques.load(Relaxed));
    if n == 0 {
        println!("  nenhum clique detectado no retorno");
        return;
    }
    let ms = |us: u64| us as f64 / 1000.0;
    let media = s.latencia_us.valor() / 1000.0;
    let quadro_ms = amostras_ms(FRAME as f64);
    println!(
        "  captura no mic → captura no retorno: média {media:.1} ms (mín {:.1}, máx {:.1})",
        ms(s.latencia_min_us.load(Relaxed)),
        ms(s.latencia_max_us.load(Relaxed))
    );
    let cabo = s.cabo_us.valor() / 1000.0;
    println!(
        "  divisão: app até a saída prevista {:.1} ms | saída prevista → retorno (VB-Cable) {cabo:.1} ms",
        media - cabo
    );
    println!("  com o RNNoise ligado: + {quadro_ms:.1} ms (atraso algorítmico) = {:.1} ms", media + quadro_ms);
}

/// Mede o custo do RNNoise com uma "voz" sintética (harmônicos com pitch variando) + ruído.
fn bench(segundos: u64) -> Res<()> {
    let total = segundos as usize * SAMPLE_RATE as usize / FRAME * FRAME;
    let mut rng = 0x1234_5678u32;
    let mut fase = 0.0f32;
    let sinal: Vec<f32> = (0..total)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let f0 = 150.0 + 50.0 * (TAU * 0.7 * t).sin();
            fase = (fase + TAU * f0 / SAMPLE_RATE as f32) % TAU;
            let silabas = (0.5 + 0.5 * (TAU * 3.0 * t).sin()).powi(2);
            let voz: f32 = (1..=10).map(|h| (fase * h as f32).sin() / h as f32).sum();
            rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let ruido = (rng >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
            0.1 * silabas * voz + 0.05 * ruido
        })
        .collect();

    let mut den = Denoiser::new();
    let mut saida = vec![0.0f32; total];
    let mut tempos = Vec::with_capacity(total / FRAME);
    for (q_in, q_out) in sinal.chunks_exact(FRAME).zip(saida.chunks_exact_mut(FRAME)) {
        let t0 = Instant::now();
        den.push(q_in, true, |o, _| q_out.copy_from_slice(o));
        tempos.push(t0.elapsed());
    }
    tempos.sort();
    let n = tempos.len();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let media = ms(tempos.iter().sum::<Duration>()) / n as f64;
    let quadro_ms = amostras_ms(FRAME as f64);

    // Atraso algorítmico: deslocamento com maior correlação entre entrada e saída.
    let corr = |lag: usize| -> f64 {
        sinal[..total - lag].iter().zip(&saida[lag..]).map(|(a, b)| (*a * *b) as f64).sum()
    };
    let corrs: Vec<f64> = (0..=2 * FRAME).map(corr).collect();
    let lag = (0..corrs.len()).max_by(|&a, &b| corrs[a].total_cmp(&corrs[b])).unwrap_or(0);

    println!("RNNoise: {segundos} s de áudio ({n} quadros de 10 ms), 1 thread");
    println!(
        "  por quadro: média {media:.3} ms | p99 {:.3} ms | máx {:.3} ms",
        ms(tempos[n * 99 / 100]),
        ms(tempos[n - 1])
    );
    println!("  custo médio: {:.1}% de um núcleo", media / quadro_ms * 100.0);
    println!("  atraso algorítmico (pico da correlação): {lag} amostras = {:.1} ms", amostras_ms(lag as f64));
    Ok(())
}
