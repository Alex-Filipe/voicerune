//! Supressão de ruído com RNNoise (`nnnoiseless`): 48 kHz, mono, quadros de 10 ms.

use nnnoiseless::DenoiseState;

/// Amostras por quadro (10 ms a 48 kHz).
pub const FRAME: usize = DenoiseState::FRAME_SIZE;
pub const SAMPLE_RATE: u32 = 48_000;

/// O RNNoise espera amostras na faixa do i16, não em [-1, 1].
const SCALE: f32 = 32768.0;

/// Junta blocos de tamanho qualquer em quadros de `FRAME` e aplica o RNNoise.
/// Não aloca depois de criado, então pode rodar na thread de áudio.
pub struct Denoiser {
    state: Box<DenoiseState<'static>>,
    entrada: [f32; FRAME],
    saida: [f32; FRAME],
    cheio: usize,
}

impl Denoiser {
    pub fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            entrada: [0.0; FRAME],
            saida: [0.0; FRAME],
            cheio: 0,
        }
    }

    /// Amostras esperando completar o próximo quadro.
    pub fn pendentes(&self) -> usize {
        self.cheio
    }

    /// Recebe amostras mono em [-1, 1]. A cada quadro completo chama `emitir` com o
    /// quadro processado (ou só copiado, se `ativo` for falso) e a probabilidade de voz.
    pub fn push(&mut self, amostras: &[f32], ativo: bool, mut emitir: impl FnMut(&[f32], f32)) {
        for &s in amostras {
            self.entrada[self.cheio] = s * SCALE;
            self.cheio += 1;
            if self.cheio == FRAME {
                self.cheio = 0;
                let vad = if ativo {
                    self.state.process_frame(&mut self.saida, &self.entrada)
                } else {
                    self.saida.copy_from_slice(&self.entrada);
                    1.0
                };
                for x in &mut self.saida {
                    *x = (*x / SCALE).clamp(-1.0, 1.0);
                }
                emitir(&self.saida, vad);
            }
        }
    }
}

impl Default for Denoiser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emite_um_quadro_a_cada_frame_amostras() {
        let mut d = Denoiser::new();
        let mut quadros = 0;
        // Blocos de tamanho que não divide FRAME, como os callbacks de áudio reais.
        for _ in 0..10 {
            d.push(&[0.0; 441], true, |q, _| {
                assert_eq!(q.len(), FRAME);
                quadros += 1;
            });
        }
        assert_eq!(quadros, 4410 / FRAME);
    }

    #[test]
    fn bypass_preserva_o_sinal() {
        let mut d = Denoiser::new();
        let entrada: Vec<f32> = (0..FRAME).map(|i| (i as f32 / FRAME as f32) - 0.5).collect();
        d.push(&entrada, false, |q, _| {
            for (a, b) in q.iter().zip(&entrada) {
                assert!((a - b).abs() < 1e-6);
            }
        });
    }
}
