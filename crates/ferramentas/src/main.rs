//! mic-limpo-dev: ferramentas de medição do mic-limpo (fora do app do usuário).

mod bench;
mod comparar;
mod latencia;
mod processar;
#[cfg(windows)]
mod sonda;

use std::error::Error;
use std::path::PathBuf;

use mic_limpo_core::modelo::Tipo;

type Res<T> = Result<T, Box<dyn Error>>;

const AJUDA: &str = "\
mic-limpo-dev (ferramentas de medição)

  mic-limpo-dev bench [--modelo M] [--limite-db N] [--segundos N]
      custo e atraso algorítmico dos modelos, sem dispositivos (padrão: todos, 20 s)
  mic-limpo-dev latencia [--entrada T] [--saida T] [--retorno T] [--folga-ms N] [--teto-ms N]
                         [--buffer N] [--baixa-latencia] [--modelo M] [--segundos N]
      troca o mic por cliques e mede quando chegam no retorno; escreve na saída, então feche
      o Discord antes (padrão: CABLE Input → CABLE Output, 8 s)
  mic-limpo-dev gravar ARQ.wav [--entrada T] [--segundos N]
      grava o mic cru (sem IA), mono 48 kHz (padrão: Microphone, 20 s)
  mic-limpo-dev processar ARQ.wav [--ruido R.wav] [--snr N] [--limite-db N]
      um WAV por modelo (sem e com limite; padrão: 20 dB), alinhados, para escuta A/B, com
      nível da voz e piso de ruído; com --ruido, mistura no SNR N (padrão: 5) e mede o SI-SDR
  mic-limpo-dev comparar [--ref T] [--alvo T1,T2] [--segundos N]
      atraso de cada alvo em relação ao mic pelo mesmo som; só lê, mas precisa de som (fale)
      (padrão: Microphone × Voicemeeter Out B1, 10 s)
  mic-limpo-dev sondar
      períodos e latências que o WASAPI permite no mic e no VB-Cable (Windows)

T = parte do nome do dispositivo (veja mic-limpo --listar). M = rnnoise ou dfn.";

fn main() {
    if let Err(e) = executar() {
        eprintln!("erro: {e}");
        std::process::exit(1);
    }
}

fn executar() -> Res<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((comando, resto)) = args.split_first() else {
        println!("{AJUDA}");
        return Ok(());
    };
    let mut lat = latencia::Opcoes::default();
    let (mut referencia, mut alvo) = ("Microphone".to_string(), "Voicemeeter Out B1".to_string());
    let mut segundos = None;
    let (mut modelo, mut limite_db) = (None::<Tipo>, None::<f32>);
    let mut proc = processar::Opcoes { ruido: None, snr_db: 5.0, limite_db: 20.0 };
    let mut arquivo = None::<PathBuf>;
    let mut it = resto.iter();
    while let Some(a) = it.next() {
        let mut valor = || it.next().cloned().ok_or_else(|| format!("{a} precisa de um valor"));
        match a.as_str() {
            "--segundos" => segundos = Some(valor()?.parse()?),
            "--entrada" => lat.entrada = Some(valor()?),
            "--saida" => lat.saida = valor()?,
            "--retorno" => lat.retorno = valor()?,
            "--folga-ms" => lat.motor.folga_ms = valor()?.parse()?,
            "--teto-ms" => lat.motor.teto_ms = valor()?.parse()?,
            "--buffer" => lat.buffer = Some(valor()?.parse()?),
            "--baixa-latencia" => lat.backend = mic_limpo_audio::Backend::BaixaLatencia,
            "--ref" => referencia = valor()?,
            "--alvo" => alvo = valor()?,
            "--modelo" => modelo = Some(valor()?.parse()?),
            "--limite-db" => limite_db = Some(valor()?.parse()?),
            "--ruido" => proc.ruido = Some(valor()?.into()),
            "--snr" => proc.snr_db = valor()?.parse()?,
            _ if !a.starts_with('-') && arquivo.is_none() => arquivo = Some(a.into()),
            _ => return Err(format!("opção desconhecida: {a} (veja --ajuda)").into()),
        }
    }
    lat.motor.modelo = modelo.unwrap_or_default();
    lat.motor.limite_db = limite_db;
    let arquivo = || arquivo.clone().ok_or_else(|| format!("{comando} precisa de um arquivo .wav"));
    match comando.as_str() {
        "bench" => {
            let modelos = modelo.map_or(Tipo::TODOS.to_vec(), |m| vec![m]);
            bench::bench(segundos.unwrap_or(20), &modelos, limite_db)
        }
        "latencia" => latencia::medir(&lat, segundos.unwrap_or(8)),
        "gravar" => processar::gravar(&arquivo()?, lat.entrada.clone(), segundos.unwrap_or(20)),
        "processar" => {
            proc.limite_db = limite_db.unwrap_or(proc.limite_db);
            processar::processar(&arquivo()?, &proc)
        }
        "comparar" => comparar::comparar(&referencia, &alvo, segundos.unwrap_or(10)),
        #[cfg(windows)]
        "sondar" => Ok(sonda::sondar(&["Microphone", "CABLE Input", "CABLE Output"])?),
        #[cfg(not(windows))]
        "sondar" => Err("sondar só existe no Windows".into()),
        "-h" | "--ajuda" | "ajuda" => {
            println!("{AJUDA}");
            Ok(())
        }
        _ => Err(format!("comando desconhecido: {comando} (veja --ajuda)").into()),
    }
}
