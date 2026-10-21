//! Teste de escuta: o motor copia o original e o limpo de cada quadro (ver
//! `Controle::escutar`), a gravação junta os dois lado a lado e o `Tocador` toca de volta,
//! trocando entre original e limpo na mesma posição, com uma rampa curta (sem estalo).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;

use crate::denoise::SAMPLE_RATE;

/// Duração do teste.
pub const SEGUNDOS: usize = 5;
pub const AMOSTRAS: usize = SEGUNDOS * SAMPLE_RATE as usize;
/// Folga da gravação: o atraso do modelo (até 30 ms) e o que chega entre um tick e outro, para o
/// `Vec` não realocar (dobraria a memória).
const FOLGA: usize = SAMPLE_RATE as usize / 5;
/// A troca original ↔ limpo leva isto (~5 ms).
const RAMPA: f32 = 1.0 / 240.0;

/// Original (antes da IA) e limpo (depois), alinhados amostra a amostra, mono a 48 kHz.
#[derive(Clone, Debug, Default)]
pub struct Gravacao {
    pub original: Vec<f32>,
    pub limpo: Vec<f32>,
}

impl Gravacao {
    pub fn nova() -> Self {
        Self { original: Vec::with_capacity(AMOSTRAS + FOLGA), limpo: Vec::with_capacity(AMOSTRAS + FOLGA) }
    }

    pub fn len(&self) -> usize {
        self.original.len().min(self.limpo.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Tira o atraso do modelo (o limpo sai `atraso` amostras depois do original) e corta no
    /// tamanho do teste.
    pub fn alinhar(&mut self, atraso: usize) {
        self.limpo.drain(..atraso.min(self.limpo.len()));
        let n = self.len().min(AMOSTRAS);
        self.original.truncate(n);
        self.limpo.truncate(n);
    }
}

/// O que o app lê e muda enquanto o `Tocador` toca.
#[derive(Default)]
pub struct Reproducao {
    /// Toca o limpo (senão, o original).
    pub limpo: AtomicBool,
    /// Amostra atual.
    pub pos: AtomicUsize,
    /// Chegou ao fim (o fluxo pode ser fechado).
    pub fim: AtomicBool,
}

/// Vai para o callback de saída. Não aloca nem trava.
pub struct Tocador {
    grav: Arc<Gravacao>,
    rep: Arc<Reproducao>,
    ganho: f32,
    pos: usize,
    /// 0 = original, 1 = limpo; segue `rep.limpo` em rampa.
    mistura: f32,
}

impl Tocador {
    pub fn novo(grav: Arc<Gravacao>, rep: Arc<Reproducao>, ganho_db: f32) -> Self {
        let mistura = if rep.limpo.load(Relaxed) { 1.0 } else { 0.0 };
        Self { grav, rep, ganho: 10f32.powf(ganho_db / 20.0), pos: 0, mistura }
    }

    /// Preenche um pacote intercalado em `canais` canais (mono copiado em todos), com o ganho
    /// aplicado e limitado a [-1, 1]; silêncio depois do fim.
    pub fn preencher(&mut self, dados: &mut [f32], canais: usize) {
        let alvo = if self.rep.limpo.load(Relaxed) { 1.0 } else { 0.0 };
        let n = self.grav.len();
        for f in dados.chunks_mut(canais.max(1)) {
            if self.pos >= n {
                f.fill(0.0);
                continue;
            }
            self.mistura += (alvo - self.mistura).clamp(-RAMPA, RAMPA);
            let (o, l) = (self.grav.original[self.pos], self.grav.limpo[self.pos]);
            f.fill(((o + (l - o) * self.mistura) * self.ganho).clamp(-1.0, 1.0));
            self.pos += 1;
        }
        self.rep.pos.store(self.pos, Relaxed);
        if self.pos >= n {
            self.rep.fim.store(true, Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gravacao() -> Arc<Gravacao> {
        Arc::new(Gravacao { original: vec![0.2; 1000], limpo: vec![0.1; 1000] })
    }

    #[test]
    fn alinhar_tira_o_atraso_do_limpo_e_corta_no_tamanho() {
        let mut g = Gravacao { original: (0..10).map(|i| i as f32).collect(), limpo: (0..10).map(|i| i as f32 - 3.0).collect() };
        g.alinhar(3);
        assert_eq!(g.len(), 7);
        assert_eq!(g.original, g.limpo);
    }

    #[test]
    fn toca_ate_o_fim_e_depois_silencio() {
        let rep = Arc::new(Reproducao::default());
        let mut t = Tocador::novo(gravacao(), rep.clone(), 0.0);
        let mut buf = vec![1.0; 1200 * 2];
        t.preencher(&mut buf, 2);
        assert!(buf[..2000].iter().all(|&x| (x - 0.2).abs() < 1e-6));
        assert!(buf[2000..].iter().all(|&x| x == 0.0));
        assert_eq!(rep.pos.load(Relaxed), 1000);
        assert!(rep.fim.load(Relaxed));
    }

    #[test]
    fn troca_para_o_limpo_em_rampa_na_mesma_posicao() {
        let rep = Arc::new(Reproducao::default());
        let mut t = Tocador::novo(gravacao(), rep.clone(), 0.0);
        let mut buf = vec![0.0; 100];
        t.preencher(&mut buf, 1);
        rep.limpo.store(true, Relaxed);
        let mut buf = vec![0.0; 400];
        t.preencher(&mut buf, 1);
        // Sai do original (0,2) e chega ao limpo (0,1) em ~240 amostras, sem pular.
        assert!(buf[0] > 0.199 && buf[0] < 0.2);
        assert!(buf.windows(2).all(|w| w[1] <= w[0] && w[0] - w[1] < 1e-3));
        assert!((buf[399] - 0.1).abs() < 1e-6);
        assert_eq!(rep.pos.load(Relaxed), 500);
    }

    #[test]
    fn ganho_vale_para_os_dois_e_satura() {
        let rep = Arc::new(Reproducao::default());
        let mut t = Tocador::novo(gravacao(), rep, 20.0 * 10f32.log10()); // ×10
        let mut buf = vec![0.0; 10];
        t.preencher(&mut buf, 1);
        assert!(buf.iter().all(|&x| x == 1.0));
    }
}
