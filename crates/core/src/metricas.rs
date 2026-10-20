//! Métricas locais do motor (nunca saem da máquina). Atômicas: a thread de áudio atualiza sem
//! locks e qualquer outra thread lê para mostrar um resumo.

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;

use crate::denoise::{FRAME, SAMPLE_RATE};
use crate::modelo::Tipo;

pub fn amostras_ms(n: f64) -> f64 {
    n * 1000.0 / SAMPLE_RATE as f64
}

/// Média acumulada sem locks.
#[derive(Default)]
pub struct Media {
    soma: AtomicU64,
    n: AtomicU64,
}

impl Media {
    pub fn add(&self, v: u64) {
        self.soma.fetch_add(v, Relaxed);
        self.n.fetch_add(1, Relaxed);
    }
    pub fn n(&self) -> u64 {
        self.n.load(Relaxed)
    }
    pub fn valor(&self) -> f64 {
        let n = self.n();
        if n == 0 { 0.0 } else { self.soma.load(Relaxed) as f64 / n as f64 }
    }
}

#[derive(Default)]
pub struct Metricas {
    pub quadros: AtomicU64,
    pub dsp_ns: AtomicU64,
    /// Pior tempo de processamento de um pacote de entrada.
    pub dsp_max_ns: AtomicU64,
    /// Callback − captura, por pacote de entrada.
    pub atraso_entrada_us: Media,
    /// Reprodução prevista − callback, por pacote de saída.
    pub atraso_saida_us: Media,
    /// Amostras esperando completar um quadro de 480.
    pub acumulo: Media,
    /// Amostras no ring buffer (média entre antes e depois de cada callback de saída).
    pub fila: Media,
    pub underruns: AtomicU64,
    pub descartes: AtomicU64,
    pub descartadas: AtomicU64,
    /// Amostras perdidas com o ring buffer cheio.
    pub perdidas: AtomicU64,
}

impl Metricas {
    /// Tempo médio de DSP por quadro de 10 ms, em ms.
    pub fn dsp_ms(&self) -> f64 {
        self.dsp_ns.load(Relaxed) as f64 / self.quadros.load(Relaxed).max(1) as f64 / 1e6
    }

    /// Uso médio de um núcleo pelo processamento, em %.
    pub fn cpu_pct(&self) -> f64 {
        self.dsp_ms() / amostras_ms(FRAME as f64) * 100.0
    }

    /// Atrasos médios em ms: (driver entrada, acúmulo de quadro, modelo, fila, driver saída).
    fn atrasos(&self, ia: bool, modelo: Tipo) -> [f64; 5] {
        [
            self.atraso_entrada_us.valor() / 1000.0,
            amostras_ms(self.acumulo.valor()),
            if ia { amostras_ms(modelo.atraso_amostras() as f64) } else { 0.0 },
            amostras_ms(self.fila.valor()),
            self.atraso_saida_us.valor() / 1000.0,
        ]
    }

    /// Latência estimada do app em ms (soma dos atrasos médios; não é medição ponta a ponta).
    pub fn latencia_ms(&self, ia: bool, modelo: Tipo) -> f64 {
        self.atrasos(ia, modelo).iter().sum()
    }

    /// Falhas audíveis: underruns + descartes por deriva.
    pub fn falhas(&self) -> u64 {
        self.underruns.load(Relaxed) + self.descartes.load(Relaxed)
    }

    pub fn resumo(&self, duracao: Duration, ia: bool, modelo: Tipo) -> String {
        let dsp_ms = self.dsp_ms();
        let [ent, acumulo, algoritmo, fila, sai] = self.atrasos(ia, modelo);
        format!(
            "Resumo ({:.1} s, IA {} ao final):\n\
             \x20 DSP por quadro de 10 ms: média {dsp_ms:.3} ms ({:.1}% de um núcleo), pior callback {:.3} ms\n\
             \x20 underruns: {} | descartes: {} ({} amostras) | perdidas: {}\n\
             \x20 atrasos médios (ms): driver entrada {ent:.1} | acúmulo de quadro {acumulo:.1} | {modelo} {algoritmo:.1} | fila {fila:.1} | driver saída {sai:.1}\n\
             \x20 latência estimada do app: {:.1} ms (soma acima; estimativa, não medição ponta a ponta)",
            duracao.as_secs_f64(),
            if ia { "ligada" } else { "desligada" },
            self.cpu_pct(),
            self.dsp_max_ns.load(Relaxed) as f64 / 1e6,
            self.underruns.load(Relaxed),
            self.descartes.load(Relaxed),
            self.descartadas.load(Relaxed),
            self.perdidas.load(Relaxed),
            self.latencia_ms(ia, modelo)
        )
    }
}
