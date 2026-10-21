//! `rodar` e `listar`: o app pela linha de comando (sem janela), sobre o mesmo `Controlador` do
//! `mic-limpo`. Para testar trocas e medir sem a UI.

use std::io::BufRead;
use std::time::Duration;

use mic_limpo::controlador::{Ajustes, Controlador, Modo, Troca};
use mic_limpo_audio as audio;
use mic_limpo_core::modelo::Tipo;

use crate::Res;

pub const AJUDA: &str = "  mic-limpo-dev listar
      lista dispositivos de áudio (* = padrão do sistema)
  mic-limpo-dev rodar [--entrada T] [--saida T] [--sem-ia] [--modelo M] [--limite-db N]
                      [--ganho-db N] [--folga-ms N] [--teto-ms N] [--baixa-latencia]
                      [--buffer N] [--segundos N]
      o app sem janela: microfone → IA → saída (padrão: mic do sistema → CABLE Input).
      Rodando (digite + Enter): (vazio) liga/desliga a IA · l Leve · f Forte ·
      b alterna baixa latência · g N ganho em dB · p pausa/retoma · q encerra com o resumo";

/// Lê as opções do `rodar` e roda.
pub fn rodar(args: &[String]) -> Res<()> {
    let mut aj = Ajustes::default();
    let (mut sem_ia, mut segundos) = (false, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut valor = || it.next().cloned().ok_or_else(|| format!("{a} precisa de um valor"));
        match a.as_str() {
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
            _ => return Err(format!("opção desconhecida: {a} (veja --ajuda)").into()),
        }
    }
    executar(aj, sem_ia, segundos)
}

pub fn mostrar_lista() -> Res<()> {
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

fn executar(aj: Ajustes, sem_ia: bool, segundos: Option<u64>) -> Res<()> {
    let mut c = Controlador::novo(None);
    let troca = c.aplicar(aj)?;
    mostrar(&c, troca);
    if sem_ia {
        mudar(&mut c, |a| a.ia = false)?;
    }
    if let Some(seg) = segundos {
        println!("Rodando por {seg} s...");
        std::thread::sleep(Duration::from_secs(seg));
    } else {
        println!("Comandos: (vazio) IA, l, f, b, g N, p, q — veja mic-limpo-dev --ajuda");
        for linha in std::io::stdin().lock().lines() {
            let linha = linha?;
            let mut partes = linha.split_whitespace();
            let r = match partes.next() {
                None => mudar(&mut c, |a| a.ia = !a.ia),
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
        "Modo: {:?}{} (modelo {}) | ganho {:+.1} dB | limite: {}",
        aj.modo,
        if aj.ia { "" } else { ", IA desligada" },
        a.modelo,
        aj.ganho_db,
        aj.limite_db.map_or("nenhum".into(), |db| format!("{db} dB"))
    );
}
