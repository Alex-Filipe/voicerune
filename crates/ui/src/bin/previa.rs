//! Prévia das telas com dados estáticos (sem áudio), para ajustar o visual e medir leveza.

use std::time::{Duration, Instant};

use mic_limpo_ui::{Dados, Flyout, JanelaPrincipal};
use slint::{ComponentHandle, Timer, TimerMode};

const AJUDA: &str = "\
mic-limpo-ui-previa [opções]

  --flyout           mostra o flyout da bandeja em vez da janela principal
  --pagina N         0 Início, 1 Configurações, 2 Sobre
  --claro | --escuro força o tema (padrão: o do sistema)
  --medidor-parado   não anima o medidor (medir a janela parada)
  --medir            mostra o tempo até o laço de eventos rodar
  --sair-apos-ms N   fecha sozinho depois de N ms (medir abrir + fechar)";

struct Opcoes {
    flyout: bool,
    pagina: i32,
    tema: i32,
    medidor_parado: bool,
    medir: bool,
    sair_apos: Option<Duration>,
}

fn ler_opcoes() -> Opcoes {
    let mut o = Opcoes { flyout: false, pagina: 0, tema: 0, medidor_parado: false, medir: false, sair_apos: None };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--flyout" => o.flyout = true,
            "--pagina" => o.pagina = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--claro" => o.tema = 1,
            "--escuro" => o.tema = 2,
            "--medidor-parado" => o.medidor_parado = true,
            "--medir" => o.medir = true,
            "--sair-apos-ms" => o.sair_apos = args.next().and_then(|v| v.parse().ok()).map(Duration::from_millis),
            _ => {
                println!("{AJUDA}");
                std::process::exit(if a == "--ajuda" { 0 } else { 2 });
            }
        }
    }
    o
}

/// Nível de microfone fictício: passeio aleatório entre 5 % e 90 % (xorshift, sem dependências).
fn animar_medidor(dados: impl Fn(f32) + 'static) -> Timer {
    let timer = Timer::default();
    let (mut estado, mut nivel) = (0x9e37_79b9_u32, 0.3_f32);
    timer.start(TimerMode::Repeated, Duration::from_millis(33), move || {
        estado ^= estado << 13;
        estado ^= estado >> 17;
        estado ^= estado << 5;
        let passo = (estado as f32 / u32::MAX as f32 - 0.5) * 0.2;
        nivel = (nivel + passo).clamp(0.05, 0.9);
        dados(nivel);
    });
    timer
}

macro_rules! preparar {
    ($janela:expr, $o:expr) => {{
        let d = $janela.global::<Dados>();
        d.set_pagina($o.pagina);
        d.set_tema($o.tema);
        if $o.tema != 0 {
            $janela.invoke_aplicar_tema($o.tema);
        }
        let fraca = $janela.as_weak();
        (!$o.medidor_parado).then(|| {
            animar_medidor(move |n| {
                if let Some(j) = fraca.upgrade() {
                    j.global::<Dados>().set_nivel(n);
                }
            })
        })
    }};
}

fn main() -> Result<(), slint::PlatformError> {
    let t0 = Instant::now();
    let o = ler_opcoes();

    // Guardados até o fim: soltar a janela ou o timer antes do laço os destruiria.
    let (_flyout, _janela, _medidor);
    if o.flyout {
        let f = Flyout::new()?;
        _medidor = preparar!(f, o);
        f.show()?;
        (_flyout, _janela) = (Some(f), None);
    } else {
        let j = JanelaPrincipal::new()?;
        _medidor = preparar!(j, o);
        j.show()?;
        (_flyout, _janela) = (None, Some(j));
    }

    if o.medir {
        Timer::single_shot(Duration::ZERO, move || eprintln!("laço de eventos rodando: {:.1} ms", t0.elapsed().as_secs_f64() * 1e3));
    }
    if let Some(d) = o.sair_apos {
        Timer::single_shot(d, || {
            let _ = slint::quit_event_loop();
        });
    }
    slint::run_event_loop()?;
    if o.medir {
        eprintln!("fechou: {:.1} ms", t0.elapsed().as_secs_f64() * 1e3);
    }
    Ok(())
}
