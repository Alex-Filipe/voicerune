//! Núcleo portável: DSP, IA, motor de áudio e métricas. Sem dependências de SO:
//! testável sem microfone e igual no Windows e no macOS.

pub mod denoise;
pub mod escuta;
pub mod metricas;
pub mod modelo;
pub mod motor;
