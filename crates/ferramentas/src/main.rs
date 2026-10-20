//! mic-limpo-dev: ferramentas de medição do mic-limpo (fora do app do usuário).

mod bench;
mod comparar;
mod latencia;
#[cfg(windows)]
mod sonda;

use std::error::Error;

type Res<T> = Result<T, Box<dyn Error>>;

const AJUDA: &str = "\
mic-limpo-dev (ferramentas de medição)

  mic-limpo-dev bench [--segundos N]
      custo e atraso algorítmico do RNNoise, sem dispositivos (padrão: 20 s)
  mic-limpo-dev latencia [--entrada T] [--saida T] [--retorno T] [--folga-ms N] [--teto-ms N]
                         [--buffer N] [--baixa-latencia] [--segundos N]
      troca o mic por cliques e mede quando chegam no retorno; escreve na saída, então feche
      o Discord antes (padrão: CABLE Input → CABLE Output, 8 s)
  mic-limpo-dev comparar [--ref T] [--alvo T1,T2] [--segundos N]
      atraso de cada alvo em relação ao mic pelo mesmo som; só lê, mas precisa de som (fale)
      (padrão: Microphone × Voicemeeter Out B1, 10 s)
  mic-limpo-dev sondar
      períodos e latências que o WASAPI permite no mic e no VB-Cable (Windows)

T = parte do nome do dispositivo (veja mic-limpo --listar).";

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
            _ => return Err(format!("opção desconhecida: {a} (veja --ajuda)").into()),
        }
    }
    match comando.as_str() {
        "bench" => bench::bench(segundos.unwrap_or(20)),
        "latencia" => latencia::medir(&lat, segundos.unwrap_or(8)),
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
