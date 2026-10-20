//! Qualidade dos modelos, fora do tempo real:
//! - `gravar`: grava o mic cru (sem IA) num WAV mono de 48 kHz;
//! - `processar`: passa um WAV por cada modelo (sem e com limite de redução) e escreve um WAV por
//!   variante, alinhados com a entrada (atraso algorítmico descontado), para escuta A/B.
//!
//! Mesma entrada e mesmos ajustes para todos. Métricas:
//! - sempre: nível da voz (p90 do RMS por quadro) e piso de ruído (p10), em dBFS;
//! - com `--ruido`: a voz é misturada ao ruído num SNR conhecido e a voz limpa vira referência
//!   para o SI-SDR (dB, maior = mais perto da voz limpa).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mic_limpo_audio::{self as audio, Pedido};
use mic_limpo_core::denoise::{FRAME, SAMPLE_RATE};
use mic_limpo_core::modelo::{self, Tipo};

use crate::Res;

pub fn gravar(arquivo: &Path, trecho: Option<String>, segundos: u64) -> Res<()> {
    let amostras = Arc::new(Mutex::new(Vec::with_capacity((segundos as usize + 1) * SAMPLE_RATE as usize)));
    let a = amostras.clone();
    let pedido = Pedido { backend: Default::default(), trecho, taxa: SAMPLE_RATE, buffer: None };
    let fluxo = audio::abrir_entrada(&pedido, move |dados, p| {
        // Ferramenta, não o motor: um lock aqui é aceitável (o main só lê depois de parar).
        if let Ok(mut a) = a.try_lock() {
            a.extend(dados.chunks_exact(p.canais).map(|f| f.iter().sum::<f32>() / p.canais as f32));
        }
    })?;
    println!("Gravando {segundos} s de {} (mic cru, sem IA)...", fluxo.nome);
    fluxo.iniciar()?;
    std::thread::sleep(Duration::from_secs(segundos));
    drop(fluxo);
    let a = amostras.lock().map_err(|_| "gravação corrompida")?;
    escrever(arquivo, &a)?;
    let (voz, piso) = niveis(&a);
    println!("{}: {:.1} s | voz {voz:.1} dBFS | piso {piso:.1} dBFS", arquivo.display(), a.len() as f64 / SAMPLE_RATE as f64);
    Ok(())
}

pub struct Opcoes {
    pub ruido: Option<PathBuf>,
    pub snr_db: f32,
    pub limite_db: f32,
}

pub fn processar(arquivo: &Path, op: &Opcoes) -> Res<()> {
    let voz = ler(arquivo)?;
    let base = arquivo.with_extension("");
    let (entrada, referencia) = match &op.ruido {
        Some(r) => {
            let mistura = misturar(&voz, &ler(r)?, op.snr_db);
            let nome = PathBuf::from(format!("{}.mistura-{}db.wav", base.display(), op.snr_db));
            escrever(&nome, &mistura)?;
            println!("Mistura (SNR {} dB): {}", op.snr_db, nome.display());
            (mistura, Some(voz))
        }
        None => (voz, None),
    };
    println!("{:<26} {:>8} {:>8} {:>9} {:>11}", "variante", "voz dB", "piso dB", "SI-SDR", "ms/quadro");
    linha("entrada", &entrada, referencia.as_deref(), None);

    for tipo in Tipo::TODOS {
        for limite in [None, Some(op.limite_db)] {
            let nome = match limite {
                None => tipo.nome().to_string(),
                Some(db) => format!("{tipo}-lim{db}"),
            };
            let mut m = match modelo::criar(tipo, limite) {
                Ok(m) => m,
                Err(e) => {
                    println!("{nome:<26} pulado: {e}");
                    continue;
                }
            };
            let atraso = tipo.atraso_amostras();
            // Completa com zeros para que a última amostra útil também saia do modelo.
            let n = (entrada.len() + atraso).div_ceil(FRAME) * FRAME;
            let mut ent = entrada.clone();
            ent.resize(n, 0.0);
            let mut sai = vec![0.0f32; n];
            let t0 = Instant::now();
            for (e, s) in ent.as_chunks::<FRAME>().0.iter().zip(sai.as_chunks_mut::<FRAME>().0) {
                m.processar(e, s);
            }
            let ms = t0.elapsed().as_secs_f64() * 1000.0 / (n / FRAME) as f64;
            let alinhada = &sai[atraso..atraso + entrada.len()];
            escrever(&PathBuf::from(format!("{}.{nome}.wav", base.display())), alinhada)?;
            linha(&nome, alinhada, referencia.as_deref(), Some(ms));
        }
    }
    println!("\nWAVs em {}.*.wav, alinhados com a entrada (troque entre eles no player).", base.display());
    Ok(())
}

fn linha(nome: &str, x: &[f32], referencia: Option<&[f32]>, ms: Option<f64>) {
    let (voz, piso) = niveis(x);
    let sdr = referencia.map_or("—".into(), |r| format!("{:.1}", si_sdr(x, r)));
    let ms = ms.map_or("—".into(), |v| format!("{v:.3}"));
    println!("{nome:<26} {voz:>8.1} {piso:>8.1} {sdr:>9} {ms:>11}");
}

fn ler(arquivo: &Path) -> Res<Vec<f32>> {
    let mut r = hound::WavReader::open(arquivo).map_err(|e| format!("{}: {e}", arquivo.display()))?;
    let spec = r.spec();
    if spec.sample_rate != SAMPLE_RATE {
        return Err(format!("{}: {} Hz; os modelos pedem 48000 Hz", arquivo.display(), spec.sample_rate).into());
    }
    let amostras: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let escala = (1u64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.map(|v| v as f32 / escala)).collect::<Result<_, _>>()?
        }
    };
    let c = spec.channels.max(1) as usize;
    Ok(amostras.chunks_exact(c).map(|f| f.iter().sum::<f32>() / c as f32).collect())
}

fn escrever(arquivo: &Path, x: &[f32]) -> Res<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(arquivo, spec)?;
    for &s in x {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    w.finalize()?;
    Ok(())
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn db(v: f64) -> f64 {
    20.0 * v.max(1e-10).log10()
}

/// (voz, piso): p90 e p10 do RMS por quadro de 10 ms, em dBFS.
fn niveis(x: &[f32]) -> (f64, f64) {
    let mut q: Vec<f64> = x.as_chunks::<FRAME>().0.iter().map(|c| rms(c)).collect();
    if q.is_empty() {
        return (db(0.0), db(0.0));
    }
    q.sort_by(f64::total_cmp);
    (db(q[q.len() * 9 / 10]), db(q[q.len() / 10]))
}

/// Voz + ruído (repetido ou cortado no tamanho da voz) com o SNR pedido.
fn misturar(voz: &[f32], ruido: &[f32], snr_db: f32) -> Vec<f32> {
    let ruido: Vec<f32> = ruido.iter().copied().cycle().take(voz.len()).collect();
    let g = rms(voz) / rms(&ruido).max(1e-10) / 10f64.powf(snr_db as f64 / 20.0);
    voz.iter().zip(&ruido).map(|(v, r)| v + g as f32 * r).collect()
}

/// SI-SDR (dB) da estimativa em relação à referência (mesmo tamanho, já alinhadas).
fn si_sdr(est: &[f32], referencia: &[f32]) -> f64 {
    let dot: f64 = est.iter().zip(referencia).map(|(e, r)| *e as f64 * *r as f64).sum();
    let energia: f64 = referencia.iter().map(|&r| (r as f64).powi(2)).sum();
    let a = dot / energia.max(1e-20);
    let (mut alvo, mut erro) = (0.0, 0.0);
    for (e, r) in est.iter().zip(referencia) {
        let s = a * *r as f64;
        alvo += s * s;
        erro += (*e as f64 - s).powi(2);
    }
    10.0 * (alvo / erro.max(1e-20)).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn si_sdr_de_sinal_igual_e_alto_e_de_mistura_bate_com_o_snr() {
        let voz: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.01).sin()).collect();
        assert!(si_sdr(&voz, &voz) > 100.0);
        let ruido: Vec<f32> = (0..48_000).map(|i| ((i * 7919) % 1000) as f32 / 500.0 - 1.0).collect();
        let m = misturar(&voz, &ruido, 10.0);
        assert!((si_sdr(&m, &voz) - 10.0).abs() < 0.5);
    }
}
