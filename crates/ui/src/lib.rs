//! Telas do VoiceRune (Slint). As janelas trocam dados pelo global `Dados`.

slint::include_modules!();

use slint::{Model, VecModel};

/// Barras da onda do Início (uma por tick do medidor: ~1,3 s a 30 Hz).
pub const BARRAS_ONDA: usize = 40;

/// Onda vazia (silêncio), para o `Dados.onda` de uma janela.
pub fn onda_vazia() -> VecModel<Barra> {
    VecModel::from(vec![Barra::default(); BARRAS_ONDA])
}

/// Anda a onda uma barra para a esquerda e põe `nova` no fim. Muda as linhas no lugar (não
/// recria as barras na tela).
pub fn empurrar_onda(onda: &VecModel<Barra>, nova: Barra) {
    let n = onda.row_count();
    for i in 1..n {
        if let Some(b) = onda.row_data(i) {
            onda.set_row_data(i - 1, b);
        }
    }
    if n > 0 {
        onda.set_row_data(n - 1, nova);
    }
}

/// Zera a onda (a janela abre sem o histórico da vez anterior).
pub fn limpar_onda(onda: &VecModel<Barra>) {
    for i in 0..onda.row_count() {
        onda.set_row_data(i, Barra::default());
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn onda_anda_para_a_esquerda() {
        let onda = onda_vazia();
        for i in 1..=3 {
            empurrar_onda(&onda, Barra { mic: i as f32, limpo: 0.0 });
        }
        let mic: Vec<f32> = onda.iter().map(|b| b.mic).collect();
        assert_eq!(mic.len(), BARRAS_ONDA);
        assert_eq!(&mic[BARRAS_ONDA - 3..], &[1.0, 2.0, 3.0]);
        assert!(mic[..BARRAS_ONDA - 3].iter().all(|&m| m == 0.0));
        limpar_onda(&onda);
        assert!(onda.iter().all(|b| b == Barra::default()));
    }
}
