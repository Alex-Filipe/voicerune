//! Mede o atraso entre dois caminhos de captura do mesmo som, por correlação cruzada.
//! Ex.: microfone direto × "Voicemeeter Out B1" (o que o Discord recebe pelo Voicemeeter).
//! Só lê dispositivos: não escreve em nenhuma saída.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use mic_limpo_core::denoise::SAMPLE_RATE;

use super::{achar, config_48k, nome, Res};

/// Decimação da busca grossa (48 kHz → 12 kHz).
const DECIM: usize = 4;
/// Pacotes ignorados no começo (~0,5 s): os primeiros do WASAPI podem vir com timestamp ruim.
const PACOTES_INICIAIS: u64 = 50;

/// Sinal mono contínuo (contagem de amostras) e, por pacote, o índice da primeira amostra com
/// o timestamp de captura. Os timestamps de dispositivos virtuais (ex.: Voicemeeter) oscilam
/// muito; a reta ajustada sobre todos os pacotes dá um t0 estável.
#[derive(Default)]
struct Linha {
    amostras: Vec<f32>,
    usadas: usize,
    pacotes: Vec<(f64, f64)>,
}

/// Reta ns = a + b·índice por mínimos quadrados; devolve (a, b, desvio-padrão dos resíduos em ms).
fn ajustar(pts: &[(f64, f64)]) -> (f64, f64, f64) {
    let n = pts.len() as f64;
    let (mx, my) = (pts.iter().map(|p| p.0).sum::<f64>() / n, pts.iter().map(|p| p.1).sum::<f64>() / n);
    let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let sxy: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let b = sxy / sxx;
    let a = my - b * mx;
    let var = pts.iter().map(|p| (p.1 - a - b * p.0).powi(2)).sum::<f64>() / n;
    (a, b, var.sqrt() / 1e6)
}

struct Gravacao {
    nome: String,
    stream: cpal::Stream,
    linha: Arc<Mutex<Linha>>,
}

fn gravar(host: &cpal::Host, trecho: &str, segundos: u64) -> Res<Gravacao> {
    let dev = achar(host, true, Some(trecho))?;
    let cfg = config_48k(&dev, true, None)?;
    let ch = cfg.channels as usize;
    let linha = Arc::new(Mutex::new(Linha {
        amostras: vec![0.0; (segundos as usize + 1) * SAMPLE_RATE as usize],
        pacotes: Vec::with_capacity(segundos as usize * 200),
        ..Default::default()
    }));
    let l = linha.clone();
    let mut contador = 0u64;
    let rotulo = nome(&dev);
    let stream = dev.build_input_stream(
        cfg,
        move |data: &[f32], info: &cpal::InputCallbackInfo| {
            contador += 1;
            if contador <= PACOTES_INICIAIS {
                return;
            }
            // Medição, não o pipeline: um lock aqui é aceitável (o main só lê depois de parar).
            let Ok(mut l) = l.try_lock() else { return };
            let pos = l.usadas;
            let ns = info.timestamp().capture.as_nanos() as f64;
            l.pacotes.push((pos as f64, ns));
            let fim = (pos + data.len() / ch).min(l.amostras.len());
            for (a, f) in l.amostras[pos..fim].iter_mut().zip(data.chunks_exact(ch)) {
                *a = f.iter().sum::<f32>() / ch as f32;
            }
            l.usadas = fim;
        },
        move |e| eprintln!("erro na captura ({rotulo}): {e}"),
        None,
    )?;
    Ok(Gravacao { nome: nome(&dev), stream, linha })
}

fn dbfs(x: &[f32]) -> f64 {
    let rms = (x.iter().map(|&v| (v * v) as f64).sum::<f64>() / x.len().max(1) as f64).sqrt();
    20.0 * rms.max(1e-10).log10()
}

/// Correlação normalizada de r[i] com g[i + desloc] na sobreposição.
fn corr(r: &[f32], g: &[f32], desloc: i64) -> f64 {
    let ini = 0i64.max(-desloc) as usize;
    let fim = (r.len() as i64).min(g.len() as i64 - desloc).max(0) as usize;
    if fim <= ini {
        return 0.0;
    }
    let (mut s, mut er, mut eg) = (0.0f64, 0.0f64, 0.0f64);
    for i in ini..fim {
        let (a, b) = (r[i] as f64, g[(i as i64 + desloc) as usize] as f64);
        s += a * b;
        er += a * a;
        eg += b * b;
    }
    if er == 0.0 || eg == 0.0 { 0.0 } else { s / (er * eg).sqrt() }
}

fn decimar(x: &[f32]) -> Vec<f32> {
    x.chunks_exact(DECIM).map(|c| c.iter().sum::<f32>() / DECIM as f32).collect()
}

/// Sinal já pronto para a correlação: branqueado, decimado e com o instante da amostra 0.
struct Preparado {
    nome: String,
    dbfs: f64,
    oscilacao_ms: f64,
    deriva_ppm: f64,
    t0_ns: f64,
    cheio: Vec<f32>,
    decimado: Vec<f32>,
}

fn preparar(nome: String, linha: &Mutex<Linha>) -> Res<Preparado> {
    let l = linha.lock().unwrap();
    if l.pacotes.len() < 10 {
        return Err(format!("{nome}: gravação curta demais").into());
    }
    let x = &l.amostras[..l.usadas];
    let (a, b, oscilacao_ms) = ajustar(&l.pacotes);
    // Pré-branqueamento (diferença de 1ª ordem): ruído grave alarga o pico da correlação.
    let cheio: Vec<f32> = x.windows(2).map(|w| w[1] - w[0]).collect();
    Ok(Preparado {
        dbfs: dbfs(x),
        oscilacao_ms,
        // Taxa efetiva em relação ao relógio QPC: 1e9/b amostras por segundo.
        deriva_ppm: (1e9 / b / SAMPLE_RATE as f64 - 1.0) * 1e6,
        t0_ns: a,
        decimado: decimar(&cheio),
        cheio,
        nome,
    })
}

/// Atraso de `g` em relação a `r`: (ms, correlação, razão entre o pico e o segundo maior).
fn atraso(r: &Preparado, g: &Preparado) -> (f64, f64, f64) {
    let fs = SAMPLE_RATE as f64;
    // g[j] e r[i] são o mesmo instante quando j = i + d.
    let d = ((r.t0_ns - g.t0_ns) * fs / 1e9).round() as i64;
    // Busca grossa a 12 kHz de -20 a +400 ms, depois fina a 48 kHz.
    let (lmin, lmax) = (-(fs * 0.020) as i64 / DECIM as i64, (fs * 0.400) as i64 / DECIM as i64);
    let grossa: Vec<(i64, f64)> =
        (lmin..=lmax).map(|l| (l, corr(&r.decimado, &g.decimado, d / DECIM as i64 + l))).collect();
    let &(melhor, pico) = grossa.iter().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).unwrap();
    // Segundo maior pico fora de ±2 ms do melhor: mede se o pico é inequívoco.
    let viz = (fs * 0.002) as i64 / DECIM as i64;
    let segundo = grossa
        .iter()
        .filter(|(l, _)| (l - melhor).abs() > viz)
        .map(|(_, c)| c.abs())
        .fold(0.0f64, f64::max);
    let (lag, c) = (melhor * DECIM as i64 - 8..=melhor * DECIM as i64 + 8)
        .map(|l| (l, corr(&r.cheio, &g.cheio, d + l)))
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .unwrap();
    (lag as f64 * 1000.0 / fs, c, pico.abs() / segundo.max(1e-12))
}

/// `alvos` aceita vários dispositivos separados por vírgula, gravados ao mesmo tempo.
pub fn comparar(referencia: &str, alvos: &str, segundos: u64) -> Res<()> {
    let host = cpal::default_host();
    let r = gravar(&host, referencia, segundos)?;
    let gs = alvos.split(',').map(|a| gravar(&host, a.trim(), segundos)).collect::<Res<Vec<_>>>()?;
    r.stream.play()?;
    for g in &gs {
        g.stream.play()?;
    }
    println!("Gravando {segundos} s (fale, estale os dedos ou bata palmas perto do mic)...");
    std::thread::sleep(Duration::from_secs(segundos));
    drop(r.stream);
    let gs: Vec<_> = gs
        .into_iter()
        .map(|g| {
            drop(g.stream);
            (g.nome, g.linha)
        })
        .collect();

    let r = preparar(r.nome, &r.linha)?;
    println!(
        "Referência: {} | {:.1} dBFS | oscilação {:.2} ms | deriva {:+.0} ppm",
        r.nome, r.dbfs, r.oscilacao_ms, r.deriva_ppm
    );
    if r.dbfs < -130.0 {
        println!("  silêncio digital: mic mudo ou gate do driver fechado (falar perto do mic)");
    }
    for (nome, linha) in gs {
        let g = preparar(nome, &linha)?;
        let (ms, c, razao) = atraso(&r, &g);
        println!(
            "Alvo: {} | {:.1} dBFS | oscilação {:.2} ms | deriva {:+.0} ppm\n  atraso: {ms:.1} ms (correlação {c:.2}, pico {razao:.1}× o segundo maior)",
            g.nome, g.dbfs, g.oscilacao_ms, g.deriva_ppm
        );
        if razao < 1.5 || c.abs() < 0.2 {
            println!("  pico fraco: inconclusivo (pouco som em comum, ou processamento forte no alvo)");
        }
    }
    Ok(())
}
