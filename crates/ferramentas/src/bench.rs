//! Custo e atraso algorítmico do RNNoise com uma "voz" sintética (harmônicos com pitch
//! variando) + ruído, sem dispositivos.

use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use mic_limpo_core::denoise::{Denoiser, FRAME, SAMPLE_RATE};
use mic_limpo_core::metricas::amostras_ms;

use crate::Res;

pub fn bench(segundos: u64) -> Res<()> {
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
