//! Controlador do áudio: guarda os ajustes do usuário e mantém motor e fluxos de acordo com eles.
//! IA ligada e ganho mudam na hora (atômicos). Modelo, dispositivos e backend pedem motor e fluxos
//! novos: o novo é montado com o antigo ainda tocando, e o antigo só sai quando o áudio novo já
//! chegou ao dispositivo, para a troca não ter corte.

use std::error::Error;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mic_limpo_audio::{self as audio, Aviso, Backend, Fluxo, Pacote, Pedido};
use mic_limpo_core::denoise::SAMPLE_RATE;
use mic_limpo_core::modelo::Tipo;
use mic_limpo_core::motor::{self, Config, Controle, Tempos};

pub type Res<T> = Result<T, Box<dyn Error>>;

/// Quanto esperar o motor novo começar a tocar antes de soltar o antigo mesmo assim.
const ESPERA_MAX: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Modo {
    Desligado,
    #[default]
    Leve,
    Forte,
}

impl Modo {
    /// Modelo do modo; `Desligado` não pede nenhum (fica o que já estiver carregado).
    fn modelo(self) -> Option<Tipo> {
        match self {
            Modo::Desligado => None,
            Modo::Leve => Some(Tipo::Rnnoise),
            Modo::Forte => Some(Tipo::Dfn),
        }
    }
}

/// O que o usuário escolhe.
#[derive(Clone, Debug, PartialEq)]
pub struct Ajustes {
    /// Desligado = sem fluxos abertos (nada roda).
    pub ligado: bool,
    pub modo: Modo,
    pub ganho_db: f32,
    /// Parte do nome do microfone; `None` = o padrão do sistema.
    pub entrada: Option<String>,
    /// Parte do nome da saída; `None` = a padrão do sistema.
    pub saida: Option<String>,
    pub baixa_latencia: bool,
    pub limite_db: Option<f32>,
    // Ajustes de desenvolvimento (só pela linha de comando).
    pub folga_ms: u32,
    pub teto_ms: u32,
    pub buffer: Option<u32>,
}

impl Default for Ajustes {
    fn default() -> Self {
        let m = Config::default();
        Self {
            ligado: true,
            modo: Modo::Leve,
            ganho_db: 0.0,
            entrada: None,
            saida: Some("CABLE Input".into()),
            baixa_latencia: false,
            limite_db: None,
            folga_ms: m.folga_ms,
            teto_ms: m.teto_ms,
            buffer: None,
        }
    }
}

impl Ajustes {
    fn backend(&self) -> Backend {
        if self.baixa_latencia { Backend::BaixaLatencia } else { Backend::Padrao }
    }
    /// Se nada que exige motor e fluxos novos mudou (fora o modelo).
    fn mesmos_fluxos(&self, o: &Ajustes) -> bool {
        (&self.entrada, &self.saida, self.baixa_latencia, self.limite_db, self.folga_ms, self.teto_ms, self.buffer)
            == (&o.entrada, &o.saida, o.baixa_latencia, o.limite_db, o.folga_ms, o.teto_ms, o.buffer)
    }
}

/// Motor e fluxos rodando.
pub struct Ativo {
    pub ctl: Arc<Controle>,
    pub modelo: Tipo,
    pub nome_entrada: String,
    pub nome_saida: String,
    /// Períodos dos dispositivos em quadros (entrada, saída).
    pub periodos: (u32, u32),
    pub inicio: Instant,
    _entrada: Fluxo,
    _saida: Fluxo,
}

/// O que `aplicar` precisou fazer.
#[derive(Debug)]
pub enum Troca {
    /// Só atômicos (IA, ganho) ou nada.
    NaHora,
    /// Fluxos fechados.
    Parado,
    /// Motor e fluxos novos. Tempos de cada etapa; `espera` = até soltar o motor antigo (zero se
    /// não havia); `sem_corte` = falso se o novo não começou a tocar a tempo.
    Aberto { motor: Duration, fluxos: Duration, espera: Duration, sem_corte: bool },
}

pub struct Controlador {
    ajustes: Ajustes,
    ativo: Option<Ativo>,
    avisos: Option<Sender<Aviso>>,
}

impl Controlador {
    /// Começa parado; `aplicar` abre. `avisos` = `None` imprime os avisos dos fluxos no stderr.
    pub fn novo(avisos: Option<Sender<Aviso>>) -> Self {
        Self { ajustes: Ajustes { ligado: false, ..Ajustes::default() }, ativo: None, avisos }
    }

    pub fn ajustes(&self) -> &Ajustes {
        &self.ajustes
    }

    pub fn ativo(&self) -> Option<&Ativo> {
        self.ativo.as_ref()
    }

    /// Passa a usar `novos`. Se algo falhar, o que estava rodando continua e os ajustes antigos
    /// ficam valendo.
    pub fn aplicar(&mut self, novos: Ajustes) -> Res<Troca> {
        if !novos.ligado {
            self.ajustes = novos;
            return Ok(if self.ativo.take().is_some() { Troca::Parado } else { Troca::NaHora });
        }
        let carregado = self.ativo.as_ref().map(|a| a.modelo);
        let modelo = novos.modo.modelo().or(carregado).unwrap_or_default();
        let reabrir = match &self.ativo {
            None => true,
            Some(a) => a.modelo != modelo || !self.ajustes.mesmos_fluxos(&novos),
        };
        if !reabrir {
            let ctl = &self.ativo.as_ref().expect("checado acima").ctl;
            ctl.set_ia(novos.modo != Modo::Desligado);
            ctl.set_ganho_db(novos.ganho_db);
            self.ajustes = novos;
            return Ok(Troca::NaHora);
        }
        let troca = self.reabrir_com(&novos, modelo)?;
        self.ajustes = novos;
        Ok(troca)
    }

    fn reabrir_com(&mut self, a: &Ajustes, modelo: Tipo) -> Res<Troca> {
        let t0 = Instant::now();
        let cfg = Config {
            folga_ms: a.folga_ms,
            teto_ms: a.teto_ms,
            ia: a.modo != Modo::Desligado,
            modelo,
            limite_db: a.limite_db,
            ganho_db: a.ganho_db,
        };
        let (ctl, mut entrada, mut saida) = motor::criar(cfg)?;
        let t_motor = t0.elapsed();

        let pedido = |trecho: &Option<String>| Pedido {
            backend: a.backend(),
            trecho: trecho.clone(),
            taxa: SAMPLE_RATE,
            buffer: a.buffer,
            avisos: self.avisos.clone(),
        };
        let fin = audio::abrir_entrada(&pedido(&a.entrada), move |d, p| entrada.processar(d, p.canais, tempos(p)))?;
        let fout = audio::abrir_saida(&pedido(&a.saida), move |d, p| saida.preencher(d, p.canais, tempos(p)))?;
        fin.iniciar()?;
        fout.iniciar()?;
        let periodos = (fin.periodo()?, fout.periodo()?);
        let t_fluxos = t0.elapsed() - t_motor;

        // Com um motor antigo tocando, só o solta quando o áudio novo já chegou ao dispositivo:
        // espera a saída nova sair da espera inicial e mais o que o buffer dela tem na frente.
        let mut sem_corte = true;
        if self.ativo.is_some() {
            let t1 = Instant::now();
            while !ctl.tocando() && t1.elapsed() < ESPERA_MAX {
                std::thread::sleep(Duration::from_millis(2));
            }
            sem_corte = ctl.tocando();
            if sem_corte {
                std::thread::sleep(Duration::from_micros(ctl.metricas.atraso_saida_us.valor() as u64));
            }
        }
        let t_espera = t0.elapsed() - t_motor - t_fluxos;
        // Solta o motor antigo (se havia) aqui.
        self.ativo = Some(Ativo {
            ctl,
            modelo,
            nome_entrada: fin.nome.clone(),
            nome_saida: fout.nome.clone(),
            periodos,
            inicio: Instant::now(),
            _entrada: fin,
            _saida: fout,
        });
        Ok(Troca::Aberto { motor: t_motor, fluxos: t_fluxos, espera: t_espera, sem_corte })
    }
}

fn tempos(p: Pacote) -> Tempos {
    Tempos { callback_ns: p.callback_ns, dispositivo_ns: p.dispositivo_ns }
}
