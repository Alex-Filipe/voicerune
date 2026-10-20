//! mic-limpo: a janela (Slint) ligada ao motor pelo `Controlador`, com ícone na bandeja.
//!
//! Threads: a da janela só lê e mostra; a de controle (`mic-limpo-controle`) é dona do
//! `Controlador` e aplica os ajustes (abrir fluxos e carregar o DFN leva até ~0,3 s); a de avisos
//! repassa os avisos dos fluxos para a de controle. Nenhuma acorda se nada mudar, fora o medidor
//! (~30 Hz) com a janela ou o flyout abertos. Fechar a janela destrói o componente e o app fica
//! só na bandeja; Sair (menu do ícone) encerra. Para usar sem janela: `mic-limpo-dev rodar`.

// Sem console no app de verdade; em debug ele fica para ver os `eprintln!`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::{Cell, RefCell};
use std::error::Error;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mic_limpo::controlador::{Ajustes, Controlador, Modo};
use mic_limpo::salvo::Salvo;
use mic_limpo_audio::{self as audio, Aviso};
use mic_limpo_core::denoise::FRAME;
use mic_limpo_core::metricas::amostras_ms;
use mic_limpo_core::modelo::Tipo;
use mic_limpo_core::motor::Controle;
use mic_limpo_ui::{Bandeja, Dados, Flyout, JanelaPrincipal};
use slint::winit_030::winit::{event::WindowEvent, window::WindowAttributes};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

/// Intervalo do medidor; os números (atraso, CPU, falhas) mudam a cada `TICKS_NUMEROS` ticks.
const MEDIDOR: Duration = Duration::from_millis(33);
const TICKS_NUMEROS: u32 = 30;
/// O medidor vai de −60 dBFS (vazio) a 0 dBFS e cai no máximo isto por tick (~1,8 dB).
const FAIXA_DB: f32 = 60.0;
const QUEDA: f32 = 0.03;
/// Espera antes de reabrir depois de um aviso fatal (o Windows troca o dispositivo padrão).
const ESPERA_REABRIR: Duration = Duration::from_millis(300);
/// O flyout perde o foco no botão para baixo do clique no ícone e o clique chega no botão para
/// cima: um clique logo depois de ele fechar assim é o mesmo clique e não o reabre.
const CLIQUE_FECHOU: Duration = Duration::from_millis(400);
/// Distância do flyout até a barra de tarefas e a borda da tela (px lógicos).
const MARGEM_FLYOUT: f32 = 12.0;
/// A configuração é gravada este tempo depois da última mudança (arrastar o volume = uma gravação).
const GRAVAR_APOS: Duration = Duration::from_millis(500);

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

/// Textos que vêm do controle e do medidor, guardados para preencher uma tela nova.
struct Mostrado {
    erro: SharedString,
    rota: SharedString,
    modelo: SharedString,
    atraso: SharedString,
    cpu: SharedString,
    falhas: i32,
}

#[derive(Clone, Copy)]
enum Tela {
    Janela,
    Flyout,
}

/// Estado da UI; só existe na thread do Slint. A janela e o flyout são criados e destruídos
/// conforme o uso, e tudo o que mostram vem daqui.
struct Ui {
    janela: Option<JanelaPrincipal>,
    flyout: Option<Flyout>,
    /// Quando o flyout fechou por perder o foco (ver `CLIQUE_FECHOU`).
    flyout_fechou: Option<Instant>,
    /// Só existe com a janela ou o flyout abertos.
    medidor: Option<Timer>,
    tx: Sender<Msg>,
    /// Último pedido mandado para o controle.
    pedido: Ajustes,
    disp: Dispositivos,
    motor: Option<(Arc<Controle>, Tipo)>,
    mostrado: Mostrado,
    /// Tema de Configurações (0 = do sistema) da última janela; vale também para o flyout.
    tema: i32,
    /// Posição e tamanho da última janela, para reabrir no mesmo lugar.
    lugar: Option<(slint::PhysicalPosition, slint::PhysicalSize)>,
    /// Gravação adiada da configuração (ver `GRAVAR_APOS`).
    gravacao: Timer,
    nivel: f32,
    tick: u32,
    /// (dsp_ns, quadros) na última conta da CPU, para mostrar a do último segundo.
    dsp_anterior: (u64, u64),
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
    /// Ligado só enquanto o flyout é criado: o gancho do winit (`atributos`) vale só para ele.
    static CRIANDO_FLYOUT: Cell<bool> = const { Cell::new(false) };
}

fn com_ui<R>(f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    UI.with_borrow_mut(|ui| ui.as_mut().map(f))
}

/// Roda `f` na próxima volta do laço de eventos (fora do callback de quem pediu: uma tela não é
/// destruída dentro do próprio evento).
fn depois(f: impl FnOnce() + 'static) {
    Timer::single_shot(Duration::ZERO, f);
}

fn main() -> Result<(), Box<dyn Error>> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(atributos)
        .select()?;

    let lista = audio::listar()?;
    let nomes = |l: &[audio::Info]| l.iter().map(|d| d.nome.clone()).collect::<Vec<_>>();
    let disp = Dispositivos {
        entradas: nomes(&lista.entradas),
        saidas: nomes(&lista.saidas),
        padrao_entrada: lista.entradas.iter().position(|d| d.padrao),
        padrao_saida: lista.saidas.iter().position(|d| d.padrao),
    };
    // Última configuração (ou os padrões na 1ª vez).
    let salvo = Salvo::carregar();
    let inicial = disponivel(salvo.ajustes, &disp);

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
        janela: None,
        flyout: None,
        flyout_fechou: None,
        medidor: None,
        tx: tx.clone(),
        pedido: inicial,
        disp,
        motor: None,
        mostrado: Mostrado {
            erro: SharedString::new(),
            rota: SharedString::new(),
            modelo: "—".into(),
            atraso: "—".into(),
            cpu: "—".into(),
            falhas: 0,
        },
        tema: salvo.tema,
        lugar: salvo
            .janela
            .filter(|&j| na_tela(j))
            .map(|(x, y, l, h)| (slint::PhysicalPosition::new(x, y), slint::PhysicalSize::new(l, h))),
        gravacao: Timer::default(),
        nivel: 0.0,
        tick: 0,
        dsp_anterior: (0, 0),
    }));

    // A bandeja visível mantém o laço de eventos vivo depois que a janela fecha.
    let bandeja = Bandeja::new()?;
    bandeja.on_clicado(clicou_icone);
    bandeja.on_abrir(abrir_janela);
    bandeja.on_sair(|| {
        let _ = slint::quit_event_loop();
    });
    bandeja.show()?;
    abrir_janela();

    slint::run_event_loop()?;

    // Sair: grava o estado final, destrói as telas e o ícone e para o áudio antes de sair.
    let janela = com_ui(|ui| {
        let j = ui.janela.take();
        if let Some(j) = &j {
            guardar_janela(ui, j);
        }
        gravar(ui);
        j
    });
    drop(janela);
    drop(UI.take());
    drop(bandeja);
    let _ = tx.send(Msg::Sair);
    let _ = controle.join();
    Ok(())
}

/// Gancho do winit antes de criar cada janela: o flyout sai da barra de tarefas e ganha os cantos
/// arredondados do Windows 11 (DWM).
fn atributos(a: WindowAttributes) -> WindowAttributes {
    #[cfg(windows)]
    if CRIANDO_FLYOUT.get() {
        use slint::winit_030::winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
        return a.with_skip_taskbar(true).with_corner_preference(CornerPreference::Round);
    }
    a
}

impl Ui {
    /// Aplica `f` ao `Dados` de cada tela aberta.
    fn em_cada(&self, mut f: impl FnMut(&Dados)) {
        if let Some(j) = &self.janela {
            f(&j.global::<Dados>());
        }
        if let Some(fl) = &self.flyout {
            f(&fl.global::<Dados>());
        }
    }

    /// Preenche uma tela recém-criada com o estado atual.
    fn preencher(&self, d: &Dados) {
        d.set_versao(env!("CARGO_PKG_VERSION").into());
        d.set_entradas(modelo_de(&self.disp.entradas));
        d.set_saidas(modelo_de(&self.disp.saidas));
        d.set_tema(self.tema_atual());
        mostrar_ajustes(d, &self.pedido, &self.disp);
        self.mostrar(d);
        d.set_nivel(self.nivel);
    }

    fn mostrar(&self, d: &Dados) {
        let m = &self.mostrado;
        d.set_erro(m.erro.clone());
        d.set_resumo_rota(m.rota.clone());
        d.set_modelo(m.modelo.clone());
        d.set_atraso(m.atraso.clone());
        d.set_cpu(m.cpu.clone());
        d.set_falhas(m.falhas);
    }

    fn tema_atual(&self) -> i32 {
        self.janela.as_ref().map_or(self.tema, |j| j.global::<Dados>().get_tema())
    }

    /// Liga o medidor quando a primeira tela abre e o solta quando a última fecha.
    fn ajustar_medidor(&mut self) {
        let aberta = self.janela.is_some() || self.flyout.is_some();
        if aberta && self.medidor.is_none() {
            let t = Timer::default();
            t.start(TimerMode::Repeated, MEDIDOR, || {
                com_ui(tick);
            });
            self.medidor = Some(t);
        } else if !aberta {
            self.medidor = None;
            self.nivel = 0.0;
        }
    }
}

/// Liga os callbacks do `Dados` de uma tela nova. Cada controle muda só o seu campo do pedido (o
/// mic padrão continua "o do sistema" até o usuário escolher outro).
fn ligar<C>(c: &C, tela: Tela)
where
    C: ComponentHandle + 'static,
    for<'a> Dados<'a>: slint::Global<'a, C>,
{
    let d = c.global::<Dados>();
    let ao_mudar = |f: fn(&mut Ajustes, &Dados, &Dispositivos)| {
        let w = c.as_weak();
        move || {
            if let Some(c) = w.upgrade() {
                pedir(tela, &c.global::<Dados>(), f);
            }
        }
    };
    d.on_modo_escolhido(ao_mudar(|a, d, _| a.modo = MODOS[d.get_modo().clamp(0, 2) as usize]));
    d.on_ganho_mudou(ao_mudar(|a, d, _| a.ganho_db = d.get_ganho_db().round()));
    d.on_entrada_escolhida(ao_mudar(|a, d, disp| a.entrada = disp.entradas.get(d.get_entrada() as usize).cloned()));
    d.on_saida_escolhida(ao_mudar(|a, d, disp| a.saida = disp.saidas.get(d.get_saida() as usize).cloned()));
    d.on_baixa_latencia_mudou(ao_mudar(|a, d, _| a.baixa_latencia = d.get_baixa_latencia()));
    d.on_ligado_mudou(ao_mudar(|a, d, _| a.ligado = d.get_ligado()));
    d.on_tema_escolhido(|| {
        com_ui(agendar_gravacao);
    });
    d.on_abrir_janela(|| {
        depois(|| {
            fechar_flyout(false);
            abrir_janela();
        })
    });
}

/// Um controle mudou: `f` atualiza o pedido lendo o `Dados` de quem mudou, o pedido vai para o
/// controle e a outra tela aberta acompanha.
fn pedir(origem: Tela, d: &Dados, f: fn(&mut Ajustes, &Dados, &Dispositivos)) {
    com_ui(|ui| {
        f(&mut ui.pedido, d, &ui.disp);
        let _ = ui.tx.send(Msg::Ajustes(ui.pedido.clone()));
        agendar_gravacao(ui);
        match origem {
            Tela::Janela => {
                if let Some(fl) = &ui.flyout {
                    mostrar_ajustes(&fl.global::<Dados>(), &ui.pedido, &ui.disp);
                }
            }
            Tela::Flyout => {
                if let Some(j) = &ui.janela {
                    mostrar_ajustes(&j.global::<Dados>(), &ui.pedido, &ui.disp);
                }
            }
        }
    });
}

/// Abrir (menu do ícone ou "Abrir ›" do flyout): cria a janela, ou traz para a frente a que já
/// está aberta.
fn abrir_janela() {
    if let Some(Some(j)) = com_ui(|ui| ui.janela.as_ref().map(|j| j.clone_strong())) {
        j.window().set_minimized(false);
        j.window().with_winit_window(|w| w.focus_window());
        return;
    }
    let j = match JanelaPrincipal::new() {
        Ok(j) => j,
        Err(e) => return eprintln!("janela: {e}"),
    };
    ligar(&j, Tela::Janela);
    let tema = com_ui(|ui| {
        ui.preencher(&j.global::<Dados>());
        ui.tema_atual()
    });
    if let Some(t @ 1..) = tema {
        j.invoke_aplicar_tema(t);
    }
    if let Some(Some((pos, tam))) = com_ui(|ui| ui.lugar) {
        j.window().set_position(pos);
        j.window().set_size(tam);
    }
    // "X": o Slint esconde a janela e o componente é destruído logo depois (fica a bandeja).
    j.window().on_close_requested(|| {
        depois(fechar_janela);
        CloseRequestResponse::HideWindow
    });
    com_ui(|ui| {
        ui.janela = Some(j.clone_strong());
        ui.ajustar_medidor();
    });
    if let Err(e) = j.show() {
        eprintln!("janela: {e}");
    }
}

fn fechar_janela() {
    let j = com_ui(|ui| {
        let j = ui.janela.take();
        if let Some(j) = &j {
            guardar_janela(ui, j);
            gravar(ui);
        }
        ui.ajustar_medidor();
        j
    });
    // Fora do `UI`: destruir o componente solta a janela do winit.
    drop(j);
}

/// Guarda o tema e o lugar da janela que vai fechar.
fn guardar_janela(ui: &mut Ui, j: &JanelaPrincipal) {
    ui.tema = j.global::<Dados>().get_tema();
    // Minimizada (posição −32000) ou maximizada: fica o lugar anterior.
    let w = j.window();
    if !w.is_minimized() && !w.is_maximized() && w.position().x > -30000 {
        ui.lugar = Some((w.position(), w.size()));
    }
}

fn agendar_gravacao(ui: &mut Ui) {
    ui.gravacao.start(TimerMode::SingleShot, GRAVAR_APOS, || {
        com_ui(|ui| gravar(ui));
    });
}

fn gravar(ui: &Ui) {
    ui.gravacao.stop();
    let salvo = Salvo {
        ajustes: ui.pedido.clone(),
        tema: ui.tema_atual(),
        janela: ui.lugar.map(|(p, t)| (p.x, p.y, t.width, t.height)),
    };
    if let Err(e) = salvo.gravar() {
        eprintln!("gravar a configuração: {e}");
    }
}

/// Dispositivo salvo que não está conectado agora volta ao padrão (o mic do Windows; a saída
/// padrão do app). Limitação: a próxima gravação já guarda o padrão no lugar dele.
fn disponivel(mut a: Ajustes, disp: &Dispositivos) -> Ajustes {
    if a.entrada.as_deref().is_some_and(|n| audio::escolher(&disp.entradas, n).is_none()) {
        eprintln!("mic salvo não encontrado: {:?}; usando o padrão", a.entrada);
        a.entrada = None;
    }
    if a.saida.as_deref().is_some_and(|n| audio::escolher(&disp.saidas, n).is_none()) {
        eprintln!("saída salva não encontrada: {:?}; usando a padrão", a.saida);
        a.saida = Ajustes::default().saida;
    }
    a
}

/// A barra de título da janela salva cai em algum monitor? (Um monitor pode ter saído.)
#[cfg(windows)]
fn na_tela((x, y, l, _): (i32, i32, u32, u32)) -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{MonitorFromRect, MONITOR_DEFAULTTONULL};
    let titulo = RECT { left: x, top: y, right: x.saturating_add(l as i32), bottom: y.saturating_add(32) };
    // SAFETY: só lê o retângulo local.
    !unsafe { MonitorFromRect(&titulo, MONITOR_DEFAULTTONULL) }.is_invalid()
}

#[cfg(not(windows))]
fn na_tela(_: (i32, i32, u32, u32)) -> bool {
    true
}

/// Clique no ícone: abre o flyout, ou fecha o que está aberto.
fn clicou_icone() {
    let (aberto, mesmo_clique) = com_ui(|ui| {
        (ui.flyout.is_some(), ui.flyout_fechou.is_some_and(|t| t.elapsed() < CLIQUE_FECHOU))
    })
    .unwrap_or_default();
    if aberto {
        fechar_flyout(false);
    } else if !mesmo_clique {
        abrir_flyout();
    }
}

fn abrir_flyout() {
    CRIANDO_FLYOUT.set(true);
    let f = Flyout::new();
    CRIANDO_FLYOUT.set(false);
    let f = match f {
        Ok(f) => f,
        Err(e) => return eprintln!("flyout: {e}"),
    };
    ligar(&f, Tela::Flyout);
    let tema = com_ui(|ui| {
        ui.preencher(&f.global::<Dados>());
        ui.tema_atual()
    });
    if let Some(t @ 1..) = tema {
        f.invoke_aplicar_tema(t);
    }
    if let Some(p) = posicao_flyout(f.get_largura(), f.get_altura()) {
        f.window().set_position(p);
    }
    f.window().on_winit_window_event(|_, e| {
        if let WindowEvent::Focused(false) = e {
            depois(|| fechar_flyout(true));
        }
        EventResult::Propagate
    });
    com_ui(|ui| {
        ui.flyout = Some(f.clone_strong());
        ui.ajustar_medidor();
    });
    if let Err(e) = f.show() {
        eprintln!("flyout: {e}");
    }
    f.window().with_winit_window(|w| w.focus_window());
}

fn fechar_flyout(perdeu_foco: bool) {
    let f = com_ui(|ui| {
        let f = ui.flyout.take();
        if f.is_some() && perdeu_foco {
            ui.flyout_fechou = Some(Instant::now());
        }
        ui.ajustar_medidor();
        f
    })
    .flatten();
    if let Some(f) = f {
        let _ = f.hide();
    }
}

/// Onde pôr o flyout: encostado na barra de tarefas, perto do cursor (que está no ícone).
#[cfg(windows)]
fn posicao_flyout(largura: f32, altura: f32) -> Option<slint::PhysicalPosition> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut cursor = POINT::default();
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    let (mut dpi, mut dpi_y) = (96u32, 96u32);
    // SAFETY: só leituras para variáveis locais, que vivem até o fim das chamadas.
    unsafe {
        GetCursorPos(&mut cursor).ok()?;
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y).ok()?;
    }
    let px = |v: f32| (v * dpi as f32 / 96.0).round() as i32;
    let (w, h, m) = (px(largura), px(altura), px(MARGEM_FLYOUT));
    let (tela, area) = (info.rcMonitor, info.rcWork);
    // Perto do cursor sem sair da área útil; depois, colado no lado da barra de tarefas (onde a
    // área útil é menor que o monitor; embaixo se a barra se esconde sozinha).
    let x = (cursor.x - w / 2).min(area.right - w - m).max(area.left + m);
    let y = (cursor.y - h / 2).min(area.bottom - h - m).max(area.top + m);
    let (x, y) = if area.top > tela.top {
        (x, area.top + m)
    } else if area.left > tela.left {
        (area.left + m, y)
    } else if area.right < tela.right {
        (area.right - w - m, y)
    } else {
        (x, area.bottom - h - m)
    };
    Some(slint::PhysicalPosition::new(x, y))
}

#[cfg(not(windows))]
fn posicao_flyout(_largura: f32, _altura: f32) -> Option<slint::PhysicalPosition> {
    None
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
        let _ = slint::invoke_from_event_loop(move || {
            com_ui(|ui| receber(ui, estado));
        });
    }
}

fn receber(ui: &mut Ui, e: Estado) {
    ui.mostrado.erro = e.erro.into();
    ui.mostrado.rota = e.rota.into();
    ui.mostrado.modelo = match e.motor.as_ref().map(|m| m.1) {
        Some(Tipo::Rnnoise) => "RNNoise",
        Some(Tipo::Dfn) => "DeepFilterNet3",
        None => "—",
    }
    .into();
    if let Some(a) = e.reverter {
        ui.pedido = a;
        ui.em_cada(|d| mostrar_ajustes(d, &ui.pedido, &ui.disp));
        agendar_gravacao(ui);
    }
    let mesmo = match (&ui.motor, &e.motor) {
        (Some((a, _)), Some((b, _))) => Arc::ptr_eq(a, b),
        _ => false,
    };
    if !mesmo {
        ui.dsp_anterior = (0, 0);
    }
    ui.motor = e.motor;
    atualizar_numeros(ui);
    ui.em_cada(|d| ui.mostrar(d));
}

/// Medidor (~30 Hz) e, a cada segundo, os números. Não faz nada com a janela minimizada e sem flyout.
fn tick(ui: &mut Ui) {
    let minimizada = ui.janela.as_ref().is_none_or(|j| j.window().is_minimized());
    if ui.flyout.is_none() && minimizada {
        return;
    }
    let pico = ui.motor.as_ref().map_or(0.0, |(c, _)| c.tomar_pico());
    let alvo = if pico > 0.0 { ((20.0 * pico.log10() + FAIXA_DB) / FAIXA_DB).clamp(0.0, 1.0) } else { 0.0 };
    ui.nivel = alvo.max(ui.nivel - QUEDA);
    ui.em_cada(|d| d.set_nivel(ui.nivel));
    ui.tick += 1;
    if ui.tick.is_multiple_of(TICKS_NUMEROS) {
        atualizar_numeros(ui);
        ui.em_cada(|d| ui.mostrar(d));
    }
}

/// Atualiza atraso, CPU e falhas em `ui.mostrado` (quem chama mostra nas telas).
fn atualizar_numeros(ui: &mut Ui) {
    let m = &mut ui.mostrado;
    let Some((c, modelo)) = &ui.motor else {
        (m.atraso, m.cpu, m.falhas) = ("—".into(), "—".into(), 0);
        return;
    };
    let met = &c.metricas;
    if met.atraso_saida_us.n() > 0 {
        m.atraso = format!("~{:.0} ms", met.latencia_ms(c.ia(), *modelo)).into();
    }
    // CPU da IA no último intervalo, em % de um núcleo.
    let agora = (met.dsp_ns.load(Relaxed), met.quadros.load(Relaxed));
    let (ns, quadros) = (agora.0.saturating_sub(ui.dsp_anterior.0), agora.1.saturating_sub(ui.dsp_anterior.1));
    ui.dsp_anterior = agora;
    if quadros > 0 {
        let pct = ns as f64 / quadros as f64 / 1e6 / amostras_ms(FRAME as f64) * 100.0;
        m.cpu = format!("{pct:.1} %").replace('.', ",").into();
    }
    m.falhas = met.falhas() as i32;
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
