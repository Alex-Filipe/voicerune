//! mic-limpo: microfone → IA (RNNoise) → saída (por padrão o VB-Cable).
//! Hoje é uma CLI; a interface (Slint) e a bandeja entram na fase 4.

use std::error::Error;
use std::io::BufRead;
use std::time::{Duration, Instant};

use mic_limpo_audio::{self as audio, Pacote, Pedido};
use mic_limpo_core::denoise::SAMPLE_RATE;
use mic_limpo_core::motor::{self, Config, Tempos};

type Res<T> = Result<T, Box<dyn Error>>;

const SAIDA_PADRAO: &str = "CABLE Input";

const AJUDA: &str = "\
mic-limpo

  mic-limpo [opções]   microfone → IA → saída
  mic-limpo --listar   lista dispositivos de áudio

Opções:
  --entrada TRECHO   parte do nome do microfone (padrão: o do sistema)
  --saida TRECHO     parte do nome da saída (padrão: \"CABLE Input\")
  --sem-ia           começa com a IA desligada
  --folga-ms N       margem do buffer entre entrada e saída (padrão: 10)
  --teto-ms N        acima de folga + N ms, descarta o excesso (padrão: 20)
  --buffer N         buffer do dispositivo em quadros (padrão: o do sistema)
  --segundos N       roda N segundos e mostra o resumo

Rodando: Enter liga/desliga a IA; q + Enter encerra e mostra o resumo.
Ferramentas de medição: mic-limpo-dev --ajuda";

struct Opcoes {
    listar: bool,
    entrada: Option<String>,
    saida: String,
    motor: Config,
    buffer: Option<u32>,
    segundos: Option<u64>,
}

fn main() {
    if let Err(e) = executar() {
        eprintln!("erro: {e}");
        std::process::exit(1);
    }
}

fn executar() -> Res<()> {
    let mut op = Opcoes {
        listar: false,
        entrada: None,
        saida: SAIDA_PADRAO.into(),
        motor: Config::default(),
        buffer: None,
        segundos: None,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut valor = || it.next().cloned().ok_or_else(|| format!("{a} precisa de um valor"));
        match a.as_str() {
            "--listar" => op.listar = true,
            "--entrada" => op.entrada = Some(valor()?),
            "--saida" => op.saida = valor()?,
            "--sem-ia" => op.motor.ia = false,
            "--folga-ms" => op.motor.folga_ms = valor()?.parse()?,
            "--teto-ms" => op.motor.teto_ms = valor()?.parse()?,
            "--buffer" => op.buffer = Some(valor()?.parse()?),
            "--segundos" => op.segundos = Some(valor()?.parse()?),
            "-h" | "--ajuda" => {
                println!("{AJUDA}");
                return Ok(());
            }
            _ => return Err(format!("opção desconhecida: {a} (veja --ajuda)").into()),
        }
    }
    if op.listar { listar() } else { rodar(op) }
}

fn listar() -> Res<()> {
    let l = audio::listar()?;
    println!("Host: {}", l.host);
    for (titulo, lista) in [("Entradas", &l.entradas), ("Saídas", &l.saidas)] {
        println!("\n{titulo}:");
        for d in lista {
            println!(" {} {} — {}", if d.padrao { "*" } else { " " }, d.nome, d.formato);
        }
    }
    println!("\n* = padrão do sistema");
    Ok(())
}

fn tempos(p: Pacote) -> Tempos {
    Tempos { callback_ns: p.callback_ns, dispositivo_ns: p.dispositivo_ns }
}

fn rodar(op: Opcoes) -> Res<()> {
    let (ctl, mut entrada, mut saida) = motor::criar(op.motor);
    let pedido = |trecho: Option<String>| Pedido { trecho, taxa: SAMPLE_RATE, buffer: op.buffer };
    let fin = audio::abrir_entrada(&pedido(op.entrada.clone()), move |d, p| {
        entrada.processar(d, p.canais, tempos(p))
    })?;
    let fout = audio::abrir_saida(&pedido(Some(op.saida.clone())), move |d, p| {
        saida.preencher(d, p.canais, tempos(p))
    })?;
    println!("Entrada: {} ({} canais)", fin.nome, fin.canais);
    println!("Saída:   {} ({} canais)", fout.nome, fout.canais);
    println!(
        "Buffer pedido: {} | folga {} ms | teto {} ms | MMCSS: {}",
        op.buffer.map_or("padrão".into(), |b| format!("{b} quadros")),
        op.motor.folga_ms,
        op.motor.teto_ms,
        if audio::MMCSS { "sim" } else { "não" }
    );

    fin.iniciar()?;
    fout.iniciar()?;
    println!("Período do dispositivo: entrada {} quadros, saída {} quadros", fin.periodo()?, fout.periodo()?);
    let inicio = Instant::now();

    let estado = |ligada: bool| if ligada { "ligada" } else { "desligada" };
    if let Some(seg) = op.segundos {
        println!("Rodando por {seg} s (IA {})...", estado(ctl.ia()));
        std::thread::sleep(Duration::from_secs(seg));
    } else {
        println!("IA {}. Enter alterna, q + Enter encerra.", estado(ctl.ia()));
        for linha in std::io::stdin().lock().lines() {
            if linha?.trim() == "q" {
                break;
            }
            ctl.set_ia(!ctl.ia());
            println!("IA {}", estado(ctl.ia()));
        }
    }
    drop(fin);
    drop(fout);
    println!("\n{}", ctl.metricas.resumo(inicio.elapsed(), ctl.ia()));
    Ok(())
}
