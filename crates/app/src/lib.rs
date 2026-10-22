//! mic-limpo como biblioteca: o `Controlador` (ajustes do usuário → motor + fluxos), usado pela
//! janela (`mic-limpo`) e pela linha de comando de desenvolvimento (`mic-limpo-dev rodar`), e a
//! última configuração salva (`salvo`) e o iniciar com o sistema (`partida`).

pub mod controlador;
pub mod partida;
pub mod salvo;
