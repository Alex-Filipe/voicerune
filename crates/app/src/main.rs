//! mic-limpo: a janela (Slint) ligada ao motor pelo `Controlador`.
//!
//! Threads: a da janela só lê e mostra; a de controle (`mic-limpo-controle`) é dona do
//! `Controlador` e aplica os ajustes (abrir fluxos e carregar o DFN leva até ~0,3 s); a de avisos
//! repassa os avisos dos fluxos para a de controle. Nenhuma acorda se nada mudar, fora o medidor
//! (~30 Hz) com a janela aberta. Para usar sem janela: `mic-limpo-dev rodar`.

// Sem console no app de verdade; em debug ele fica para ver os `eprintln!`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::error::Error;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use mic_limpo::controlador::{Ajustes, Controlador, Modo};
use mic_limpo_audio::{self as audio, Aviso};
use mic_limpo_core::denoise::FRAME;
use mic_limpo_core::metricas::amostras_ms;
use mic_limpo_core::modelo::Tipo;
use mic_limpo_core::motor::Controle;
use mic_limpo_ui::{Dados, JanelaPrincipal};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};

/// Intervalo do medidor; os números (atraso, CPU, falhas) mudam a cada `TICKS_NUMEROS` ticks.
const MEDIDOR: Duration = Duration::from_millis(33);
const TICKS_NUMEROS: u32 = 30;
/// O medidor vai de −60 dBFS (vazio) a 0 dBFS e cai no máximo isto por tick (~1,8 dB).
const FAIXA_DB: f32 = 60.0;
const QUEDA: f32 = 0.03;
/// Espera antes de reabrir depois de um aviso fatal (o Windows troca o dispositivo padrão).
const ESPERA_REABRIR: Duration = Duration::from_millis(300);

const MODOS: [Modo; 3] = [Modo::Desligado, Modo::Leve, Modo::Forte];

enum Msg {
    Ajustes(Ajustes),
    Aviso(Aviso),
    Sair,
}

/// O que a thread de controle manda para a janela depois de cada mudança.
struct Estado {
    motor: Option<(Arc<Controle>, Tipo)>,
    rota: String,
    erro: String,
    /// Ajustes em vigor quando o pedido da janela falhou e o motor antigo continuou.
    reverter: Option<Ajustes>,
}

struct Dispositivos {
    entradas: Vec<String>,
    saidas: Vec<String>,
    padrao_entrada: Option<usize>,
    padrao_saida: Option<usize>,
}

/// Estado da janela; só existe na thread do Slint.
struct Ui {
    janela: Weak<JanelaPrincipal>,
    tx: Sender<Msg>,
    /// Último pedido mandado para o controle.
    pedido: Ajustes,
    disp: Dispositivos,
    motor: Option<(Arc<Controle>, Tipo)>,
    nivel: f32,
    tick: u32,
    /// (dsp_ns, quadros) na última conta da CPU, para mostrar a do último segundo.
    dsp_anterior: (u64, u64),
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

fn com_ui(f: impl FnOnce(&mut Ui, &Dados)) {
    UI.with_borrow_mut(|ui| {
        if let Some(ui) = ui {
            if let Some(j) = ui.janela.upgrade() {
                f(ui, &j.global::<Dados>());
            }
        }
    });
}

fn main() -> Result<(), Box<dyn Error>> {
    let lista = audio::listar()?;
    let nomes = |l: &[audio::Info]| l.iter().map(|d| d.nome.clone()).collect::<Vec<_>>();
    let disp = Dispositivos {
        entradas: nomes(&lista.entradas),
        saidas: nomes(&lista.saidas),
        padrao_entrada: lista.entradas.iter().position(|d| d.padrao),
        padrao_saida: lista.saidas.iter().position(|d| d.padrao),
    };
    let inicial = Ajustes::default();

    let janela = JanelaPrincipal::new()?;
    let d = janela.global::<Dados>();
    d.set_versao(env!("CARGO_PKG_VERSION").into());
    d.set_entradas(modelo_de(&disp.entradas));
    d.set_saidas(modelo_de(&disp.saidas));
    d.set_modelo("—".into());
    d.set_atraso("—".into());
    d.set_cpu("—".into());
    mostrar_ajustes(&d, &inicial, &disp);

    // Cada controle muda só o seu campo do pedido (o mic padrão continua "o do sistema" até o
    // usuário escolher outro).
    d.on_modo_escolhido(|| pedir(|a, d, _| a.modo = MODOS[d.get_modo().clamp(0, 2) as usize]));
    d.on_ganho_mudou(|| pedir(|a, d, _| a.ganho_db = d.get_ganho_db().round()));
    d.on_entrada_escolhida(|| pedir(|a, d, disp| a.entrada = disp.entradas.get(d.get_entrada() as usize).cloned()));
    d.on_saida_escolhida(|| pedir(|a, d, disp| a.saida = disp.saidas.get(d.get_saida() as usize).cloned()));
    d.on_baixa_latencia_mudou(|| pedir(|a, d, _| a.baixa_latencia = d.get_baixa_latencia()));
    d.on_ligado_mudou(|| pedir(|a, d, _| a.ligado = d.get_ligado()));

    let (tx, rx) = mpsc::channel::<Msg>();
    let (tx_aviso, rx_aviso) = mpsc::channel::<Aviso>();
    let repasse = tx.clone();
    std::thread::Builder::new().name("mic-limpo-avisos".into()).spawn(move || {
        for a in rx_aviso {
            if repasse.send(Msg::Aviso(a)).is_err() {
                break;
            }
        }
    })?;
    let controle = std::thread::Builder::new()
        .name("mic-limpo-controle".into())
        .spawn(move || controlar(rx, tx_aviso))?;
    tx.send(Msg::Ajustes(inicial.clone()))?;

    UI.set(Some(Ui {
        janela: janela.as_weak(),
        tx: tx.clone(),
        pedido: inicial,
        disp,
        motor: None,
        nivel: 0.0,
        tick: 0,
        dsp_anterior: (0, 0),
    }));
    let medidor = Timer::default();
    medidor.start(TimerMode::Repeated, MEDIDOR, || com_ui(tick));

    janela.run()?;

    // Janela fechada (até a bandeja existir, isso encerra o app): para o áudio antes de sair.
    drop(medidor);
    UI.set(None);
    let _ = tx.send(Msg::Sair);
    let _ = controle.join();
    Ok(())
}

/// Um controle mudou: `f` atualiza o pedido lendo o `Dados`, e o pedido vai para o controle.
fn pedir(f: impl FnOnce(&mut Ajustes, &Dados, &Dispositivos)) {
    com_ui(|ui, d| {
        f(&mut ui.pedido, d, &ui.disp);
        let _ = ui.tx.send(Msg::Ajustes(ui.pedido.clone()));
    });
}

/// Thread de controle: aplica os pedidos e reage aos avisos. Dorme no `recv` se nada muda.
fn controlar(rx: Receiver<Msg>, avisos: Sender<Aviso>) {
    let mut c = Controlador::novo(Some(avisos));
    while let Ok(primeira) = rx.recv() {
        // Junta o que chegou de uma vez (ex.: arrastando o ganho): só o último pedido vale.
        let (mut pedido, mut fatal) = (None, None);
        for m in std::iter::once(primeira).chain(rx.try_iter()) {
            match m {
                Msg::Ajustes(a) => pedido = Some(a),
                Msg::Aviso(a) if a.fatal => fatal = Some(a),
                Msg::Aviso(a) => eprintln!("aviso ({}): {}", if a.entrada { "entrada" } else { "saída" }, a.texto),
                Msg::Sair => return,
            }
        }
        let (mut erro, mut reverter) = (String::new(), None);
        if let Some(a) = pedido {
            if let Err(e) = c.aplicar(a.clone()) {
                // Com um motor rodando, ele continua com os ajustes antigos (a janela volta a eles).
                erro = if c.ativo().is_some() {
                    reverter = Some(c.ajustes().clone());
                    format!("Não deu para trocar ({e}). Continua como estava.")
                } else {
                    format!("Não consegui abrir o áudio: {e}")
                };
            }
        }
        if let Some(av) = fatal {
            eprintln!("erro no fluxo: {}", av.texto);
            std::thread::sleep(ESPERA_REABRIR);
            if let Err(e) = c.reabrir() {
                eprintln!("reabrir: {e}");
                let qual = if av.entrada { "O microfone" } else { "A saída" };
                erro = format!("{qual} parou de responder. Reconecte ou escolha outro.");
            }
        }
        let estado = Estado {
            motor: c.ativo().map(|a| (a.ctl.clone(), a.modelo)),
            rota: c.ativo().map_or(String::new(), |a| format!("{} → {}", a.nome_entrada, a.nome_saida)),
            erro,
            reverter,
        };
        let _ = slint::invoke_from_event_loop(move || com_ui(|ui, d| receber(ui, d, estado)));
    }
}

fn receber(ui: &mut Ui, d: &Dados, e: Estado) {
    d.set_erro(e.erro.into());
    d.set_resumo_rota(e.rota.into());
    d.set_modelo(
        match e.motor.as_ref().map(|m| m.1) {
            Some(Tipo::Rnnoise) => "RNNoise",
            Some(Tipo::Dfn) => "DeepFilterNet3",
            None => "—",
        }
        .into(),
    );
    if let Some(a) = e.reverter {
        mostrar_ajustes(d, &a, &ui.disp);
        ui.pedido = a;
    }
    let mesmo = match (&ui.motor, &e.motor) {
        (Some((a, _)), Some((b, _))) => Arc::ptr_eq(a, b),
        _ => false,
    };
    if !mesmo {
        ui.dsp_anterior = (0, 0);
    }
    ui.motor = e.motor;
    atualizar_numeros(ui, d);
}

/// Medidor (~30 Hz) e, a cada segundo, os números. Parado com a janela minimizada.
fn tick(ui: &mut Ui, d: &Dados) {
    if ui.janela.upgrade().is_some_and(|j| j.window().is_minimized()) {
        return;
    }
    let pico = ui.motor.as_ref().map_or(0.0, |(c, _)| c.tomar_pico());
    let alvo = if pico > 0.0 { ((20.0 * pico.log10() + FAIXA_DB) / FAIXA_DB).clamp(0.0, 1.0) } else { 0.0 };
    ui.nivel = alvo.max(ui.nivel - QUEDA);
    d.set_nivel(ui.nivel);
    ui.tick += 1;
    if ui.tick.is_multiple_of(TICKS_NUMEROS) {
        atualizar_numeros(ui, d);
    }
}

fn atualizar_numeros(ui: &mut Ui, d: &Dados) {
    let Some((c, modelo)) = &ui.motor else {
        d.set_atraso("—".into());
        d.set_cpu("—".into());
        d.set_falhas(0);
        return;
    };
    let m = &c.metricas;
    if m.atraso_saida_us.n() > 0 {
        d.set_atraso(format!("~{:.0} ms", m.latencia_ms(c.ia(), *modelo)).into());
    }
    // CPU da IA no último intervalo, em % de um núcleo.
    let agora = (m.dsp_ns.load(Relaxed), m.quadros.load(Relaxed));
    let (ns, quadros) = (agora.0.saturating_sub(ui.dsp_anterior.0), agora.1.saturating_sub(ui.dsp_anterior.1));
    ui.dsp_anterior = agora;
    if quadros > 0 {
        let pct = ns as f64 / quadros as f64 / 1e6 / amostras_ms(FRAME as f64) * 100.0;
        d.set_cpu(format!("{pct:.1} %").replace('.', ",").into());
    }
    d.set_falhas(m.falhas() as i32);
}

fn mostrar_ajustes(d: &Dados, a: &Ajustes, disp: &Dispositivos) {
    let indice = |nomes: &[String], escolhido: &Option<String>, padrao: Option<usize>| {
        escolhido.as_deref().map_or(padrao, |t| audio::escolher(nomes, t)).map_or(-1, |i| i as i32)
    };
    d.set_ligado(a.ligado);
    d.set_modo(MODOS.iter().position(|m| *m == a.modo).unwrap_or(1) as i32);
    d.set_ganho_db(a.ganho_db);
    d.set_baixa_latencia(a.baixa_latencia);
    d.set_entrada(indice(&disp.entradas, &a.entrada, disp.padrao_entrada));
    d.set_saida(indice(&disp.saidas, &a.saida, disp.padrao_saida));
}

fn modelo_de(nomes: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(nomes.iter().map(SharedString::from).collect::<Vec<_>>()))
}
