//! Motor de áudio: entrada (mono → RNNoise → ring buffer) e saída (ring buffer → dispositivo).
//! Não conhece backend nem SO: o backend só chama `Entrada::processar` e `Saida::preencher` na
//! thread de áudio. Nenhuma das duas aloca nem trava.

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Instant;

use crate::denoise::{Denoiser, FRAME, SAMPLE_RATE};
use crate::metricas::Metricas;

/// Capacidade do ring buffer: 200 ms, bem acima da folga, só para absorver soluços.
const CAPACIDADE: usize = SAMPLE_RATE as usize / 5;
/// Maior pacote de entrada processado de uma vez (1 s); o excesso é ignorado.
const MAX_PACOTE: usize = SAMPLE_RATE as usize;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Margem mantida no ring buffer depois de cada callback de saída.
    pub folga_ms: u32,
    /// Acima de folga + teto, o excesso é descartado (deriva de relógio entre dispositivos).
    pub teto_ms: u32,
    pub ia: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { folga_ms: 10, teto_ms: 20, ia: true }
    }
}

/// Instantes de um pacote, em ns no relógio do backend (QPC no Windows).
#[derive(Clone, Copy, Debug, Default)]
pub struct Tempos {
    pub callback_ns: u64,
    /// Captura (entrada) ou reprodução prevista (saída) da primeira amostra do pacote.
    pub dispositivo_ns: u64,
}

/// Estado compartilhado entre as threads de áudio e o resto do app.
#[derive(Default)]
pub struct Controle {
    ia: AtomicBool,
    pub metricas: Metricas,
}

impl Controle {
    pub fn ia(&self) -> bool {
        self.ia.load(Relaxed)
    }
    pub fn set_ia(&self, ligada: bool) {
        self.ia.store(ligada, Relaxed);
    }
}

/// Cria o motor: o `Controle` fica com quem controla; `Entrada` e `Saida` vão para os callbacks.
pub fn criar(cfg: Config) -> (Arc<Controle>, Entrada, Saida) {
    let ctl = Arc::new(Controle::default());
    ctl.set_ia(cfg.ia);
    let (prod, cons) = rtrb::RingBuffer::new(CAPACIDADE);
    let amostras = |ms: u32| (SAMPLE_RATE * ms / 1000) as usize;
    let entrada = Entrada { den: Denoiser::new(), mono: vec![0.0; MAX_PACOTE], prod, ctl: ctl.clone() };
    let saida = Saida {
        cons,
        folga: amostras(cfg.folga_ms),
        teto: amostras(cfg.teto_ms),
        esperando: true,
        ctl: ctl.clone(),
    };
    (ctl, entrada, saida)
}

pub struct Entrada {
    den: Denoiser,
    mono: Vec<f32>,
    prod: rtrb::Producer<f32>,
    ctl: Arc<Controle>,
}

impl Entrada {
    /// Recebe um pacote intercalado em `canais` canais: mistura para mono, aplica o RNNoise por
    /// quadro e empurra para o ring buffer.
    pub fn processar(&mut self, dados: &[f32], canais: usize, t: Tempos) {
        let Self { den, mono, prod, ctl } = self;
        let m = &ctl.metricas;
        m.atraso_entrada_us.add(t.callback_ns.saturating_sub(t.dispositivo_ns) / 1000);
        let canais = canais.max(1);
        let n = (dados.len() / canais).min(mono.len());
        for (s, f) in mono[..n].iter_mut().zip(dados.chunks_exact(canais)) {
            *s = f.iter().sum::<f32>() / canais as f32;
        }
        let t0 = Instant::now();
        let mut quadros = 0;
        den.push(&mono[..n], ctl.ia(), |q, _vad| {
            quadros += 1;
            for &x in q {
                if prod.push(x).is_err() {
                    m.perdidas.fetch_add(1, Relaxed);
                }
            }
        });
        if quadros > 0 {
            let ns = t0.elapsed().as_nanos() as u64;
            m.quadros.fetch_add(quadros, Relaxed);
            m.dsp_ns.fetch_add(ns, Relaxed);
            m.dsp_max_ns.fetch_max(ns, Relaxed);
        }
        m.acumulo.add(den.pendentes() as u64);
    }
}

pub struct Saida {
    cons: rtrb::Consumer<f32>,
    folga: usize,
    teto: usize,
    esperando: bool,
    ctl: Arc<Controle>,
}

impl Saida {
    /// Preenche um pacote intercalado em `canais` canais com o áudio processado (mono copiado em
    /// todos os canais). Começa só com `folga` de margem; depois de um underrun, espera reencher.
    pub fn preencher(&mut self, dados: &mut [f32], canais: usize, t: Tempos) {
        let m = &self.ctl.metricas;
        m.atraso_saida_us.add(t.dispositivo_ns.saturating_sub(t.callback_ns) / 1000);
        let canais = canais.max(1);
        let n = dados.len() / canais;
        let fila = self.cons.slots();
        m.fila.add(fila.saturating_sub(n / 2) as u64);
        if self.esperando && fila < n + self.folga {
            dados.fill(0.0);
            return;
        }
        self.esperando = false;
        if fila > n + self.folga + self.teto {
            let excesso = fila - n - self.folga;
            if let Ok(c) = self.cons.read_chunk(excesso) {
                c.commit_all();
            }
            m.descartes.fetch_add(1, Relaxed);
            m.descartadas.fetch_add(excesso as u64, Relaxed);
        }
        for f in dados.chunks_mut(canais) {
            match self.cons.pop() {
                Ok(x) => f.fill(x),
                Err(_) => {
                    f.fill(0.0);
                    if !self.esperando {
                        self.esperando = true;
                        m.underruns.fetch_add(1, Relaxed);
                    }
                }
            }
        }
    }
}

/// Atraso algorítmico do RNNoise (um quadro), para quem soma latências.
pub const ATRASO_IA_AMOSTRAS: usize = FRAME;

#[cfg(test)]
mod tests {
    use super::*;

    fn sem_ia(folga_ms: u32, teto_ms: u32) -> (Arc<Controle>, Entrada, Saida) {
        criar(Config { folga_ms, teto_ms, ia: false })
    }

    #[test]
    fn sem_ia_entrega_o_mesmo_sinal_atrasado_e_sem_underrun() {
        let (ctl, mut ent, mut sai) = sem_ia(10, 20);
        let sinal: Vec<f32> = (0..48_000).map(|i| (i % 1000) as f32 / 1000.0 - 0.5).collect();
        let mut saida = Vec::new();
        // Entrada e saída estéreo em pacotes de 441 quadros, que não batem com os 480 do RNNoise.
        for bloco in sinal.chunks_exact(441) {
            let estereo: Vec<f32> = bloco.iter().flat_map(|&x| [x, x]).collect();
            ent.processar(&estereo, 2, Tempos::default());
            let mut buf = vec![0.0; 441 * 2];
            sai.preencher(&mut buf, 2, Tempos::default());
            saida.extend(buf.chunks(2).map(|f| f[0]));
        }
        assert_eq!(ctl.metricas.underruns.load(Relaxed), 0);
        assert_eq!(ctl.metricas.descartes.load(Relaxed), 0);
        let atraso = saida.iter().position(|&x| x != 0.0).expect("a saída nunca começou");
        assert!(saida.len() - atraso > 40_000);
        for (a, b) in saida[atraso..].iter().zip(&sinal) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn espera_inicial_nao_conta_underrun_mas_falta_de_entrada_conta() {
        let (ctl, mut ent, mut sai) = sem_ia(0, 20);
        let mut buf = vec![0.0; 480];
        sai.preencher(&mut buf, 1, Tempos::default());
        assert_eq!(ctl.metricas.underruns.load(Relaxed), 0);
        ent.processar(&[0.1; 480], 1, Tempos::default());
        sai.preencher(&mut buf, 1, Tempos::default());
        assert!(buf.iter().all(|&x| (x - 0.1).abs() < 1e-6));
        sai.preencher(&mut buf, 1, Tempos::default());
        assert_eq!(ctl.metricas.underruns.load(Relaxed), 1);
    }

    #[test]
    fn excesso_acima_do_teto_e_descartado() {
        let (ctl, mut ent, mut sai) = sem_ia(0, 10);
        ent.processar(&[0.1; 4800], 1, Tempos::default());
        let mut buf = vec![0.0; 480];
        sai.preencher(&mut buf, 1, Tempos::default());
        assert_eq!(ctl.metricas.descartes.load(Relaxed), 1);
        assert_eq!(ctl.metricas.descartadas.load(Relaxed), 4800 - 480);
    }
}
