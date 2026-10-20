//! Motor de áudio: entrada (mono → modelo → ring buffer) e saída (ring buffer → dispositivo).
//! Não conhece backend nem SO: o backend só chama `Entrada::processar` e `Saida::preencher` na
//! thread de áudio. Nenhuma das duas aloca nem trava.
//!
//! Modelos que alocam por quadro (DeepFilterNet) rodam numa thread própria: a entrada só empurra
//! as amostras num ring buffer e acorda a thread (`unpark`), que processa e empurra direto no
//! ring buffer da saída.

use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering::{Acquire, Relaxed, Release}};
use std::sync::Arc;
use std::thread::{self, Thread};
use std::time::Instant;

use crate::denoise::{Denoiser, FRAME, SAMPLE_RATE};
use crate::metricas::Metricas;
use crate::modelo::{self, Modelo, Rnnoise, Tipo};

/// Capacidade do ring buffer: 200 ms, bem acima da folga, só para absorver soluços.
const CAPACIDADE: usize = SAMPLE_RATE as usize / 5;
/// Maior pacote de entrada processado de uma vez (1 s); o excesso é ignorado.
const MAX_PACOTE: usize = SAMPLE_RATE as usize;

pub type Erro = Box<dyn Error>;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Margem mantida no ring buffer depois de cada callback de saída.
    pub folga_ms: u32,
    /// Acima de folga + teto, o excesso é descartado (deriva de relógio entre dispositivos).
    pub teto_ms: u32,
    pub ia: bool,
    pub modelo: Tipo,
    /// Atenuação máxima do ruído em dB (`None` = sem limite).
    pub limite_db: Option<f32>,
}

impl Default for Config {
    fn default() -> Self {
        Self { folga_ms: 10, teto_ms: 20, ia: true, modelo: Tipo::Rnnoise, limite_db: None }
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
/// Carrega o modelo aqui (fora da thread de áudio); leva ~0,2 s com o DeepFilterNet.
pub fn criar(cfg: Config) -> Result<(Arc<Controle>, Entrada, Saida), Erro> {
    let ctl = Arc::new(Controle::default());
    ctl.set_ia(cfg.ia);
    let (prod, cons) = rtrb::RingBuffer::new(CAPACIDADE);
    let amostras = |ms: u32| (SAMPLE_RATE * ms / 1000) as usize;
    let caminho = if cfg.modelo.thread_propria() {
        Caminho::Thread(Trabalhador::iniciar(cfg, prod, ctl.clone())?)
    } else {
        Caminho::Local { den: Box::new(Denoiser::com(Rnnoise::new(cfg.limite_db))), prod }
    };
    let entrada = Entrada { caminho, mono: vec![0.0; MAX_PACOTE], ctl: ctl.clone() };
    let saida = Saida {
        cons,
        folga: amostras(cfg.folga_ms),
        teto: amostras(cfg.teto_ms),
        esperando: true,
        ctl: ctl.clone(),
    };
    Ok((ctl, entrada, saida))
}

/// Aplica o modelo por quadro e empurra o resultado para a saída, medindo o tempo gasto.
/// Roda na thread de áudio (RNNoise) ou na thread própria do modelo (DeepFilterNet).
fn aplicar<M: Modelo>(den: &mut Denoiser<M>, amostras: &[f32], prod: &mut rtrb::Producer<f32>, ctl: &Controle) {
    let m = &ctl.metricas;
    let t0 = Instant::now();
    let mut quadros = 0;
    den.push(amostras, ctl.ia(), |q, _vad| {
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
}

enum Caminho {
    /// O modelo roda na própria thread de áudio (só o RNNoise).
    Local { den: Box<Denoiser<Rnnoise>>, prod: rtrb::Producer<f32> },
    /// O modelo roda na thread própria; a entrada só repassa as amostras.
    Thread(Trabalhador),
}

/// Thread própria do modelo. Para quando a `Entrada` é descartada.
struct Trabalhador {
    para_ia: rtrb::Producer<f32>,
    thread: Thread,
    parar: Arc<AtomicBool>,
}

impl Trabalhador {
    /// Cria o modelo dentro da thread (o DeepFilterNet não é `Send`) e espera a carga terminar.
    fn iniciar(cfg: Config, mut prod: rtrb::Producer<f32>, ctl: Arc<Controle>) -> Result<Self, Erro> {
        let (para_ia, mut cons) = rtrb::RingBuffer::new(CAPACIDADE);
        let parar = Arc::new(AtomicBool::new(false));
        let p = parar.clone();
        let (pronto_tx, pronto_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let h = thread::Builder::new().name("mic-limpo-ia".into()).spawn(move || {
            let mut den = match modelo::criar(cfg.modelo, cfg.limite_db) {
                Ok(m) => {
                    let _ = pronto_tx.send(Ok(()));
                    Denoiser::com(m)
                }
                Err(e) => {
                    let _ = pronto_tx.send(Err(e.to_string()));
                    return;
                }
            };
            let mut quadro = [0.0f32; FRAME];
            while !p.load(Acquire) {
                let Ok(c) = cons.read_chunk(FRAME) else {
                    // Sem quadro completo: dorme até a entrada empurrar mais (ou pedir parada).
                    thread::park();
                    continue;
                };
                let (a, b) = c.as_slices();
                quadro[..a.len()].copy_from_slice(a);
                quadro[a.len()..].copy_from_slice(b);
                c.commit_all();
                aplicar(&mut den, &quadro, &mut prod, &ctl);
            }
        })?;
        pronto_rx.recv().map_err(|_| "a thread do modelo terminou sem avisar")??;
        Ok(Self { para_ia, thread: h.thread().clone(), parar })
    }
}

impl Drop for Trabalhador {
    fn drop(&mut self) {
        self.parar.store(true, Release);
        self.thread.unpark();
    }
}

pub struct Entrada {
    caminho: Caminho,
    mono: Vec<f32>,
    ctl: Arc<Controle>,
}

impl Entrada {
    /// Recebe um pacote intercalado em `canais` canais: mistura para mono e aplica o modelo por
    /// quadro (aqui ou na thread própria); o resultado vai para o ring buffer da saída.
    pub fn processar(&mut self, dados: &[f32], canais: usize, t: Tempos) {
        let Self { caminho, mono, ctl } = self;
        let m = &ctl.metricas;
        m.atraso_entrada_us.add(t.callback_ns.saturating_sub(t.dispositivo_ns) / 1000);
        let canais = canais.max(1);
        let n = (dados.len() / canais).min(mono.len());
        for (s, f) in mono[..n].iter_mut().zip(dados.chunks_exact(canais)) {
            *s = f.iter().sum::<f32>() / canais as f32;
        }
        match caminho {
            Caminho::Local { den, prod } => {
                aplicar(den, &mono[..n], prod, ctl);
                m.acumulo.add(den.pendentes() as u64);
            }
            Caminho::Thread(tr) => {
                for &x in &mono[..n] {
                    if tr.para_ia.push(x).is_err() {
                        m.perdidas.fetch_add(1, Relaxed);
                    }
                }
                tr.thread.unpark();
                m.acumulo.add((CAPACIDADE - tr.para_ia.slots()) as u64);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sem_ia(folga_ms: u32, teto_ms: u32) -> (Arc<Controle>, Entrada, Saida) {
        criar(Config { folga_ms, teto_ms, ia: false, ..Config::default() }).unwrap()
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

    #[cfg(feature = "dfn")]
    #[test]
    fn dfn_na_thread_propria_processa_todos_os_quadros() {
        let cfg = Config { folga_ms: 0, teto_ms: 1000, modelo: Tipo::Dfn, ..Config::default() };
        let (ctl, mut ent, mut sai) = criar(cfg).unwrap();
        let sinal: Vec<f32> = (0..48_000).map(|i| 0.3 * (i as f32 * 0.03).sin()).collect();
        for bloco in sinal.chunks_exact(441) {
            ent.processar(bloco, 1, Tempos::default());
            // Dá tempo à thread do modelo, como o relógio do dispositivo daria.
            std::thread::sleep(std::time::Duration::from_millis(3));
            let mut buf = vec![0.0; 441];
            sai.preencher(&mut buf, 1, Tempos::default());
        }
        assert_eq!(ctl.metricas.perdidas.load(Relaxed), 0);
        assert!(ctl.metricas.quadros.load(Relaxed) >= (48_000 / FRAME - 2) as u64);
    }
}
