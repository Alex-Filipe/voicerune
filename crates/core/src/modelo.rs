//! Modelos de supressão de ruído atrás de uma interface só: um quadro de 10 ms (480 amostras
//! mono a 48 kHz, em [-1, 1]) entra, um quadro sai, com atraso fixo.
//!
//! - RNNoise (`nnnoiseless`): não aloca por quadro, roda na thread de áudio.
//! - DeepFilterNet3 (`deep_filter` + tract, feature `dfn`): aloca por quadro e não é `Send`
//!   (tract usa `Rc`), então o motor o cria e roda numa thread própria (`Tipo::thread_propria`).

use std::error::Error;
use std::fmt;
use std::str::FromStr;

use nnnoiseless::DenoiseState;

use crate::denoise::FRAME;

/// O RNNoise espera amostras na faixa do i16, não em [-1, 1].
const SCALE: f32 = 32768.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tipo {
    #[default]
    Rnnoise,
    Dfn,
}

impl Tipo {
    pub const TODOS: [Tipo; 2] = [Tipo::Rnnoise, Tipo::Dfn];

    /// Atraso algorítmico em amostras (medido pelo pico da correlação no `bench`).
    pub fn atraso_amostras(self) -> usize {
        match self {
            Tipo::Rnnoise => FRAME,
            // FFT de 960 − hop de 480 + 2 quadros de lookahead.
            Tipo::Dfn => 3 * FRAME,
        }
    }

    /// O modelo aloca por quadro e não pode rodar na thread de áudio.
    pub fn thread_propria(self) -> bool {
        self == Tipo::Dfn
    }

    pub fn nome(self) -> &'static str {
        match self {
            Tipo::Rnnoise => "rnnoise",
            Tipo::Dfn => "dfn",
        }
    }
}

impl fmt::Display for Tipo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.nome())
    }
}

impl FromStr for Tipo {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "rnnoise" => Ok(Tipo::Rnnoise),
            "dfn" => Ok(Tipo::Dfn),
            _ => Err(format!("modelo desconhecido: {s} (use rnnoise ou dfn)")),
        }
    }
}

/// Um quadro de `FRAME` amostras entra, um sai. Devolve a probabilidade de voz (0 a 1) quando o
/// modelo a estima; senão 1.
pub trait Modelo {
    fn processar(&mut self, entrada: &[f32; FRAME], saida: &mut [f32; FRAME]) -> f32;
}

impl<M: Modelo + ?Sized> Modelo for Box<M> {
    fn processar(&mut self, entrada: &[f32; FRAME], saida: &mut [f32; FRAME]) -> f32 {
        (**self).processar(entrada, saida)
    }
}

/// Cria o modelo. `limite_db`: atenuação máxima do ruído (ex.: 20 → no máximo −20 dB);
/// `None` = sem limite. Pode demorar (o DeepFilterNet carrega o modelo em ~0,2 s).
pub fn criar(tipo: Tipo, limite_db: Option<f32>) -> Result<Box<dyn Modelo>, Box<dyn Error>> {
    match tipo {
        Tipo::Rnnoise => Ok(Box::new(Rnnoise::new(limite_db))),
        #[cfg(feature = "dfn")]
        Tipo::Dfn => Ok(Box::new(dfn::Dfn::new(limite_db)?)),
        #[cfg(not(feature = "dfn"))]
        Tipo::Dfn => Err("este build não inclui o DeepFilterNet (feature `dfn`)".into()),
    }
}

/// Fração do sinal original misturada de volta para limitar a atenuação a `db`.
fn mistura(limite_db: Option<f32>) -> f32 {
    limite_db.map_or(0.0, |db| 10f32.powf(-db.abs() / 20.0))
}

pub struct Rnnoise {
    state: Box<DenoiseState<'static>>,
    entrada: [f32; FRAME],
    /// Quadro anterior, alinhado com a saída (o RNNoise atrasa um quadro): base do limite.
    anterior: [f32; FRAME],
    mistura: f32,
}

impl Rnnoise {
    pub fn new(limite_db: Option<f32>) -> Self {
        Self {
            state: DenoiseState::new(),
            entrada: [0.0; FRAME],
            anterior: [0.0; FRAME],
            mistura: mistura(limite_db),
        }
    }
}

impl Modelo for Rnnoise {
    fn processar(&mut self, entrada: &[f32; FRAME], saida: &mut [f32; FRAME]) -> f32 {
        for (e, &x) in self.entrada.iter_mut().zip(entrada) {
            *e = x * SCALE;
        }
        let vad = self.state.process_frame(saida, &self.entrada);
        let g = self.mistura;
        for (s, &orig) in saida.iter_mut().zip(&self.anterior) {
            *s = ((1.0 - g) * *s / SCALE + g * orig).clamp(-1.0, 1.0);
        }
        self.anterior.copy_from_slice(entrada);
        vad
    }
}

#[cfg(feature = "dfn")]
mod dfn {
    use super::*;
    use df::tract::{DfParams, DfTract, RuntimeParams};
    use ndarray::{ArrayView2, ArrayViewMut2};

    /// DeepFilterNet3 com o modelo padrão embutido no binário (commit fixado no Cargo.lock).
    pub struct Dfn {
        df: DfTract,
    }

    impl Dfn {
        pub fn new(limite_db: Option<f32>) -> Result<Self, Box<dyn Error>> {
            // 100 dB = sem limite, na convenção da libDF.
            // Limiares do binário oficial `deep-filter` (configuração avaliada no artigo), não os
            // do `default_with_ch` (plugin LADSPA), que pulam o 2º estágio já acima de 20 dB.
            let rp = RuntimeParams::default_with_ch(1)
                .with_atten_lim(limite_db.map_or(100.0, f32::abs))
                .with_thresholds(-15.0, 35.0, 35.0);
            let df = DfTract::new(DfParams::default(), &rp).map_err(|e| e.to_string())?;
            if df.hop_size != FRAME || df.sr != 48_000 {
                return Err(format!("modelo inesperado: hop {} a {} Hz", df.hop_size, df.sr).into());
            }
            Ok(Self { df })
        }
    }

    impl Modelo for Dfn {
        fn processar(&mut self, entrada: &[f32; FRAME], saida: &mut [f32; FRAME]) -> f32 {
            let ent = ArrayView2::from_shape((1, FRAME), &entrada[..]).expect("forma fixa");
            let sai = ArrayViewMut2::from_shape((1, FRAME), &mut saida[..]).expect("forma fixa");
            match self.df.process(ent, sai) {
                // SNR local estimado (−15 a 35 dB) → 0 a 1, só para métricas.
                Ok(lsnr) => ((lsnr + 15.0) / 50.0).clamp(0.0, 1.0),
                Err(_) => {
                    saida.copy_from_slice(entrada);
                    1.0
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Voz" sintética (harmônicos de ~150 Hz em sílabas de 3 Hz) + ruído branco: um tom puro
    /// seria tratado como ruído pelo DeepFilterNet.
    fn voz_e_ruido(n: usize) -> Vec<f32> {
        let mut rng = 1u32;
        (0..n)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                let silabas = (0.5 + 0.5 * (std::f32::consts::TAU * 3.0 * t).sin()).powi(2);
                let voz: f32 = (1..=10).map(|h| (std::f32::consts::TAU * 150.0 * h as f32 * t).sin() / h as f32).sum();
                rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let ruido = (rng >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
                0.1 * silabas * voz + 0.05 * ruido
            })
            .collect()
    }

    fn rodar(m: &mut dyn Modelo, sinal: &[f32]) -> Vec<f32> {
        let mut saida = vec![0.0; sinal.len()];
        for (e, s) in sinal.as_chunks::<FRAME>().0.iter().zip(saida.as_chunks_mut::<FRAME>().0) {
            m.processar(e, s);
        }
        saida
    }

    #[test]
    fn limite_zero_db_devolve_o_original_atrasado() {
        let sinal = voz_e_ruido(FRAME * 20);
        let saida = rodar(&mut Rnnoise::new(Some(0.0)), &sinal);
        for (a, b) in saida[FRAME..].iter().zip(&sinal) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn tipo_le_e_escreve_o_nome() {
        for t in Tipo::TODOS {
            assert_eq!(t.nome().parse::<Tipo>().unwrap(), t);
        }
        assert!("xyz".parse::<Tipo>().is_err());
    }

    #[cfg(feature = "dfn")]
    #[test]
    fn dfn_tem_o_atraso_previsto() {
        let sinal = voz_e_ruido(48_000);
        let mut m = criar(Tipo::Dfn, None).unwrap();
        let saida = rodar(m.as_mut(), &sinal);
        let corr = |lag: usize| -> f64 {
            sinal[..sinal.len() - lag].iter().zip(&saida[lag..]).map(|(a, b)| (a * b) as f64).sum()
        };
        let lag = (0..=4 * FRAME).max_by(|&a, &b| corr(a).total_cmp(&corr(b))).unwrap();
        assert_eq!(lag, Tipo::Dfn.atraso_amostras());
    }
}
