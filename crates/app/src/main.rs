//! mic-limpo: microfone → IA (RNNoise ou DeepFilterNet) → saída (por padrão o VB-Cable).
//! Hoje é uma CLI sobre o `Controlador`; a interface (Slint) e a bandeja entram na fase 4.

use std::error::Error;
use std::io::BufRead;
use std::time::Duration;

use mic_limpo_audio as audio;
use mic_limpo_core::modelo::Tipo;

mod controlador;

use controlador::{Ajustes, Controlador, Modo, Troca};

type Res<T> = Result<T, Box<dyn Error>>;

const AJUDA: &str = "mic-limpo

  mic-limpo [opções]   microfone → IA → saída
  mic-limpo --listar   lista dispositivos de áudio

Opções:
  --entrada TRECHO   parte do nome do microfone (padrão: o do sistema)
  --saida TRECHO     parte do nome da saída (padrão: \"CABLE Input\")
  --sem-ia           começa com a IA desligada
  --modelo M         rnnoise (modo Leve, padrão) ou dfn (modo Forte: DeepFilterNet3, +20 ms,
                     ~8 % de um núcleo; use com --baixa-latencia)
  --limite-db N      reduz o ruído no máximo N dB (padrão: sem limite)
  --ganho-db N       ganho de saída em dB (padrão: 0)
  --folga-ms N       margem do buffer entre entrada e saída (padrão: 10)
  --teto-ms N        acima de folga + N ms, descarta o excesso (padrão: 20)
  --baixa-latencia   WASAPI direto no menor período do driver, com MMCSS (Windows)
  --buffer N         buffer do dispositivo em quadros (padrão: o do sistema)
  --segundos N       roda N segundos e mostra o resumo

Rodando (digite + Enter):
  (vazio) liga/desliga a IA   l  modo Leve      f  modo Forte     b  alterna baixa latência
  g N     ganho de saída em dB p  pausa/retoma   q  encerra e mostra o resumo
Ferramentas de medição: mic-limpo-dev --ajuda";

fn main() {
    if let Err(e) = executar() {
        eprintln!("erro: {e}");
        std::process::exit(1);
    }
}

fn executar() -> Res<()> {
    let mut aj = Ajustes::default();
    let (mut listar, mut sem_ia, mut segundos) = (false, false, None);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut valor = || it.next().cloned().ok_or_else(|| format!("{a} precisa de um valor"));
        match a.as_str() {
            "--listar" => listar = true,
            "--entrada" => aj.entrada = Some(valor()?),
            "--saida" => aj.saida = Some(valor()?),
            "--sem-ia" => sem_ia = true,
            "--modelo" => {
                aj.modo = match valor()?.parse()? {
                    Tipo::Rnnoise => Modo::Leve,
                    Tipo::Dfn => Modo::Forte,
                }
            }
            "--limite-db" => aj.limite_db = Some(valor()?.parse()?),
            "--ganho-db" => aj.ganho_db = valor()?.parse()?,
            "--folga-ms" => aj.folga_ms = valor()?.parse()?,
            "--teto-ms" => aj.teto_ms = valor()?.parse()?,
            "--baixa-latencia" => aj.baixa_latencia = true,
            "--buffer" => aj.buffer = Some(valor()?.parse()?),
            "--segundos" => segundos = Some(valor()?.parse()?),
            "-h" | "--ajuda" => {
                println!("{AJUDA}");
                return Ok(());
            }
            _ => return Err(format!("opção desconhecida: {a} (veja --ajuda)").into()),
        }
    }
    if listar { mostrar_lista() } else { rodar(aj, sem_ia, segundos) }
}

fn mostrar_lista() -> Res<()> {
    let l = audio::listar()?;
    println!("Host: {}", l.host);
    for (titulo, lista) in [("Entradas", &l.entradas), ("Saídas", &l.saidas)] {
        println!("
{titulo}:");
        for d in lista {
            println!(" {} {} — {}", if d.padrao { "*" } else { " " }, d.nome, d.formato);
        }
    }
    println!("
* = padrão do sistema");
    Ok(())
}

fn rodar(aj: Ajustes, sem_ia: bool, segundos: Option<u64>) -> Res<()> {
    let mut c = Controlador::novo(None);
    let modo = aj.modo;
    let troca = c.aplicar(aj)?;
    mostrar(&c, troca);
    if sem_ia {
        mudar(&mut c, |a| a.modo = Modo::Desligado)?;
    }
    if let Some(seg) = segundos {
        println!("Rodando por {seg} s...");
        std::thread::sleep(Duration::from_secs(seg));
    } else {
        println!("Comandos: (vazio) IA, l, f, b, g N, p, q — veja --ajuda");
        let mut ultimo = modo;
        for linha in std::io::stdin().lock().lines() {
            let linha = linha?;
            let mut partes = linha.split_whitespace();
            let r = match partes.next() {
                None => mudar(&mut c, |a| {
                    if a.modo == Modo::Desligado {
                        a.modo = ultimo;
                    } else {
                        ultimo = a.modo;
                        a.modo = Modo::Desligado;
                    }
                }),
                Some("q") => break,
                Some("l") => mudar(&mut c, |a| a.modo = Modo::Leve),
                Some("f") => mudar(&mut c, |a| a.modo = Modo::Forte),
                Some("b") => mudar(&mut c, |a| a.baixa_latencia = !a.baixa_latencia),
                Some("p") => mudar(&mut c, |a| a.ligado = !a.ligado),
                Some("g") => match partes.next().map(str::parse::<f32>) {
                    Some(Ok(db)) => mudar(&mut c, |a| a.ganho_db = db),
                    _ => Err("use: g N (dB)".into()),
                },
                Some(x) => Err(format!("comando desconhecido: {x}").into()),
            };
            if let Err(e) = r {
                println!("erro: {e}");
            }
        }
    }
    if let Some(a) = c.ativo() {
        println!("
{} (desde a última troca)", a.ctl.metricas.resumo(a.inicio.elapsed(), a.ctl.ia(), a.modelo));
    }
    Ok(())
}

fn mudar(c: &mut Controlador, f: impl FnOnce(&mut Ajustes)) -> Res<()> {
    let mut a = c.ajustes().clone();
    f(&mut a);
    let troca = c.aplicar(a)?;
    mostrar(c, troca);
    Ok(())
}

fn mostrar(c: &Controlador, troca: Troca) {
    let aj = c.ajustes();
    let Some(a) = c.ativo() else {
        println!("Pausado (fluxos fechados).");
        return;
    };
    if let Troca::Aberto { motor, fluxos, espera, sem_corte } = troca {
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        println!("Entrada: {} | Saída: {}", a.nome_entrada, a.nome_saida);
        println!(
            "Backend: {:?} | períodos: entrada {} / saída {} quadros | folga {} ms | teto {} ms | MMCSS: {}",
            if aj.baixa_latencia { audio::Backend::BaixaLatencia } else { audio::Backend::Padrao },
            a.periodos.0,
            a.periodos.1,
            aj.folga_ms,
            aj.teto_ms,
            if audio::MMCSS || aj.baixa_latencia { "sim" } else { "não" }
        );
        println!(
            "Abertura: motor {:.0} ms + fluxos {:.0} ms + espera {:.0} ms{}",
            ms(motor),
            ms(fluxos),
            ms(espera),
            if sem_corte { "" } else { " (o novo não tocou a tempo: houve corte)" }
        );
    }
    println!(
        "Modo: {:?} (modelo {}) | ganho {:+.1} dB | limite: {}",
        aj.modo,
        a.modelo,
        aj.ganho_db,
        aj.limite_db.map_or("nenhum".into(), |db| format!("{db} dB"))
    );
}
