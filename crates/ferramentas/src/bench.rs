//! Custo e atraso algorítmico de cada modelo com uma "voz" sintética (harmônicos com pitch
//! variando) + ruído, sem dispositivos. Mesma entrada e mesmos ajustes para todos os modelos.

use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use mic_limpo_core::denoise::{FRAME, SAMPLE_RATE};
use mic_limpo_core::metricas::amostras_ms;
use mic_limpo_core::modelo::{self, Tipo};

use crate::Res;

fn sinal_sintetico(total: usize) -> Vec<f32> {
    let mut rng = 0x1234_5678u32;
    let mut fase = 0.0f32;
    (0..total)
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
        .collect()
}

/// Deslocamento (em amostras, até `max`) com maior correlação entre entrada e saída.
pub fn atraso(entrada: &[f32], saida: &[f32], max: usize) -> usize {
    let n = entrada.len().min(saida.len());
    let corr = |lag: usize| -> f64 {
        entrada[..n - lag].iter().zip(&saida[lag..n]).map(|(a, b)| (*a * *b) as f64).sum()
    };
    (0..=max).max_by(|&a, &b| corr(a).total_cmp(&corr(b))).unwrap_or(0)
}

pub fn bench(segundos: u64, modelos: &[Tipo], limite_db: Option<f32>) -> Res<()> {
    let total = segundos as usize * SAMPLE_RATE as usize / FRAME * FRAME;
    let sinal = sinal_sintetico(total);
    println!(
        "{segundos} s de voz sintética + ruído ({} quadros de 10 ms), 1 thread, limite: {}",
        total / FRAME,
        limite_db.map_or("nenhum".into(), |db| format!("{db} dB"))
    );
    for &tipo in modelos {
        let t0 = Instant::now();
        let mut m = modelo::criar(tipo, limite_db)?;
        let carga = t0.elapsed();
        let mut saida = vec![0.0f32; total];
        let mut tempos = Vec::with_capacity(total / FRAME);
        for (e, s) in sinal.as_chunks::<FRAME>().0.iter().zip(saida.as_chunks_mut::<FRAME>().0) {
            let t0 = Instant::now();
            m.processar(e, s);
            tempos.push(t0.elapsed());
        }
        tempos.sort();
        let n = tempos.len();
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let media = ms(tempos.iter().sum::<Duration>()) / n as f64;
        let lag = atraso(&sinal, &saida, 4 * FRAME);

        println!("\n{tipo} (carga do modelo {:.0} ms)", ms(carga));
        println!(
            "  por quadro: média {media:.3} ms | p99 {:.3} ms | máx {:.3} ms",
            ms(tempos[n * 99 / 100]),
            ms(tempos[n - 1])
        );
        println!("  custo médio: {:.1}% de um núcleo", media / amostras_ms(FRAME as f64) * 100.0);
        println!("  atraso algorítmico (pico da correlação): {lag} amostras = {:.1} ms", amostras_ms(lag as f64));
    }
    Ok(())
}
