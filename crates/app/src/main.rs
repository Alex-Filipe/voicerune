//! VoiceRune: a janela (Slint) ligada ao motor pelo `Controlador`, com ícone na bandeja.
//!
//! Threads: a da janela só lê e mostra; a de controle (`voicerune-controle`) é dona do
//! `Controlador` e aplica os ajustes (abrir fluxos e carregar o DFN leva até ~0,3 s); a de avisos
//! repassa os avisos dos fluxos para a de controle. Nenhuma acorda se nada mudar, fora o medidor
//! (~30 Hz) com a janela ou o flyout abertos. Fechar a janela destrói o componente e o app fica
//! só na bandeja; Sair (menu do ícone) encerra. Para usar sem janela: `voicerune-dev rodar`.
//! Com `--bandeja` (a linha registrada por "iniciar com o sistema", ver `partida`) abre só o ícone.
//!
//! Página Teste: a janela liga a cópia do motor (`Controle::escutar`) e junta original e limpo a
//! cada tick do medidor; a reprodução é um fluxo de saída à parte, aberto pela thread de controle.

// Sem console no app de verdade; em debug ele fica para ver os `eprintln!`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::{Cell, RefCell};
use std::error::Error;
use std::rc::Rc;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::winit_030::winit::{
    event::WindowEvent,
    window::{Theme, WindowAttributes},
};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};
use voicerune::controlador::{Ajustes, Controlador, Modo};
use voicerune::partida;
use voicerune::salvo::Salvo;
use voicerune_audio::{self as audio, Aviso, Fluxo, Pedido};
use voicerune_core::escuta::{self, Gravacao, Reproducao, Tocador};
use voicerune_core::modelo::Tipo;
use voicerune_core::motor::Controle;
#[cfg(not(target_os = "macos"))]
use voicerune_ui::Bandeja;
#[cfg(target_os = "macos")]
use voicerune_ui::BandejaSemMenu;
use voicerune_ui::{empurrar_onda, limpar_onda, onda_vazia, Barra, Dados, Flyout, JanelaPrincipal};

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
#[cfg(windows)]
const MARGEM_FLYOUT: f32 = 12.0;
/// A configuração é gravada este tempo depois da última mudança (arrastar o volume = uma gravação).
const GRAVAR_APOS: Duration = Duration::from_millis(500);

/// Na ordem do seletor; o 3º (Personalizado) ainda não existe e fica desabilitado na tela.
const MODOS: [Modo; 2] = [Modo::Leve, Modo::Forte];

enum Msg {
    Ajustes(Ajustes),
    Aviso(Aviso),
    /// Tocar a gravação do teste em `saida` (parte do nome; `None` = padrão do sistema).
    Ouvir { grav: Arc<Gravacao>, rep: Arc<Reproducao>, saida: Option<String>, ganho_db: f32 },
    PararOuvir,
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

/// Página Teste. Gravando, tocando e gravada valem nessa ordem.
struct Teste {
    gravando: Option<Gravando>,
    gravada: Option<Arc<Gravacao>>,
    tocando: Option<Arc<Reproducao>>,
    info: SharedString,
    /// 0 Original, 1 Limpo.
    versao: i32,
    /// Onde ouvir (índice em `Dispositivos::saidas`).
    saida: i32,
}

struct Gravando {
    /// Motor que está copiando; se o áudio for reaberto (ou o modo mudar), a gravação é cancelada.
    ctl: Arc<Controle>,
    /// `None` = cancelamento desligado.
    modo: Option<Modo>,
    /// Atraso do modelo em amostras (tirado do limpo no fim).
    atraso: usize,
    grav: Gravacao,
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
    /// Iniciar com o sistema, como está no registro (ver `ler_partida`).
    iniciar: bool,
    /// Só na bandeja: o do registro se registrado; senão a preferência do `config.ini`.
    so_bandeja: bool,
    /// Gravação adiada da configuração (ver `GRAVAR_APOS`).
    gravacao: Timer,
    nivel: f32,
    /// Medidor do que sai da IA (mesma escala e queda que `nivel`).
    nivel_limpo: f32,
    /// Histórico do medidor desenhado no Início (só anda com a janela visível).
    onda: Rc<VecModel<Barra>>,
    tick: u32,
    /// (tempo de CPU do processo, relógio) na última conta da CPU, para mostrar a do último segundo.
    cpu_anterior: Option<(Duration, Instant)>,
    teste: Teste,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
    /// Ligado só enquanto o flyout é criado: o gancho do winit (`atributos`) vale só para ele.
    static CRIANDO_FLYOUT: Cell<bool> = const { Cell::new(false) };
    /// Tema da próxima janela: o winit a cria depois, no laço de eventos, mas lê os atributos no `new`.
    static TEMA_NOVA: Cell<i32> = const { Cell::new(0) };
}

fn com_ui<R>(f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    UI.with_borrow_mut(|ui| ui.as_mut().map(f))
}

/// Roda `f` na próxima volta do laço de eventos (fora do callback de quem pediu: uma tela não é
/// destruída dentro do próprio evento).
fn sair() {
    let _ = slint::quit_event_loop();
}

fn depois(f: impl FnOnce() + 'static) {
    Timer::single_shot(Duration::ZERO, f);
}

fn main() -> Result<(), Box<dyn Error>> {
    // Antes de tudo: a 2ª cópia só avisa a 1ª e sai, sem abrir janela nem áudio.
    let Some(instancia) = primeira_instancia() else {
        return Ok(());
    };
    // Iniciado pelo sistema com "só na bandeja": sem janela, só o ícone.
    let so_icone = std::env::args().skip(1).any(|a| a == partida::ARG_BANDEJA);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(atributos)
        .select()?;
    // Depois do backend: o aviso da 2ª cópia chega pelo laço de eventos do winit.
    instancia.esperar();

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
    std::thread::Builder::new().name("voicerune-avisos".into()).spawn(move || {
        for a in rx_aviso {
            if repasse.send(Msg::Aviso(a)).is_err() {
                break;
            }
        }
    })?;
    let controle = std::thread::Builder::new()
        .name("voicerune-controle".into())
        .spawn(move || controlar(rx, tx_aviso))?;
    tx.send(Msg::Ajustes(inicial.clone()))?;

    let saida_teste = disp.padrao_saida.unwrap_or(0) as i32;
    if let Err(e) = partida::migrar() {
        eprintln!("iniciar com o sistema (nome antigo): {e}");
    }
    let registrado = partida::ler();
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
        iniciar: registrado.is_some(),
        so_bandeja: registrado.unwrap_or(salvo.so_bandeja),
        gravacao: Timer::default(),
        nivel: 0.0,
        nivel_limpo: 0.0,
        onda: Rc::new(onda_vazia()),
        tick: 0,
        cpu_anterior: None,
        teste: Teste {
            gravando: None,
            gravada: None,
            tocando: None,
            info: SharedString::new(),
            versao: 1,
            saida: saida_teste,
        },
    }));

    // A bandeja visível mantém o laço de eventos vivo depois que a janela fecha.
    #[cfg(not(target_os = "macos"))]
    let bandeja = Bandeja::new()?;
    #[cfg(not(target_os = "macos"))]
    {
        bandeja.on_abrir(abrir_janela);
        bandeja.on_sair(sair);
    }
    // Mac: ícone sem menu (Abrir e Sair ficam no flyout).
    #[cfg(target_os = "macos")]
    let bandeja = BandejaSemMenu::new()?;
    bandeja.on_clicado(clicou_icone);
    bandeja.show()?;
    if !so_icone {
        abrir_janela();
    }

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

/// Instância única. A 1ª cópia cria um evento nomeado (por sessão do Windows); abrir o app de novo
/// acha o evento, sinaliza e sai, e a 1ª abre a janela (ou a traz para a frente).
#[cfg(windows)]
struct Instancia(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
fn primeira_instancia() -> Option<Instancia> {
    use windows::core::w;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::{CreateEventW, SetEvent};
    use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};

    // SAFETY: sem atributos de segurança (padrão); o handle fica aberto até o processo sair.
    let evento = match unsafe { CreateEventW(None, false, false, w!("Local\\VoiceRune-abrir")) } {
        Ok(e) => e,
        // Sem o evento o app abre normalmente (pode haver duas cópias).
        Err(e) => {
            eprintln!("instância única: {e}");
            return Some(Instancia(Default::default()));
        }
    };
    // SAFETY: lê o erro deixado pelo `CreateEventW` logo acima.
    if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        return Some(Instancia(evento));
    }
    // Quem o usuário acabou de abrir pode trazer janelas para a frente; repassa isso à 1ª cópia.
    // SAFETY: handle válido, recém-aberto.
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        let _ = SetEvent(evento);
    }
    None
}

#[cfg(windows)]
impl Instancia {
    /// Thread que dorme no evento e abre a janela a cada nova tentativa de abrir o app.
    fn esperar(self) {
        use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
        use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
        if self.0.is_invalid() {
            return;
        }
        // O `HANDLE` não é `Send`; o número dele é.
        let h = self.0 .0 as usize;
        let r = std::thread::Builder::new().name("voicerune-instancia".into()).spawn(move || {
            // SAFETY: o evento nunca é fechado (vive com o processo).
            while unsafe { WaitForSingleObject(HANDLE(h as *mut _), INFINITE) } == WAIT_OBJECT_0 {
                if slint::invoke_from_event_loop(abrir_janela).is_err() {
                    break;
                }
            }
        });
        if let Err(e) = r {
            eprintln!("instância única: {e}");
        }
    }
}

/// No macOS o próprio sistema já abre um app `.app` uma vez só (Fase 5).
#[cfg(not(windows))]
struct Instancia;

#[cfg(not(windows))]
fn primeira_instancia() -> Option<Instancia> {
    Some(Instancia)
}

#[cfg(not(windows))]
impl Instancia {
    fn esperar(self) {}
}

/// Gancho do winit antes de criar cada janela: tema do app; no Mac, janela transparente sobre o
/// desfoque do sistema; no Windows o flyout sai da barra de tarefas e ganha os cantos arredondados
/// do Windows 11 (DWM).
fn atributos(a: WindowAttributes) -> WindowAttributes {
    let a = a.with_theme(tema_winit(TEMA_NOVA.get()));
    #[cfg(windows)]
    if CRIANDO_FLYOUT.get() {
        use slint::winit_030::winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
        return a.with_skip_taskbar(true).with_corner_preference(CornerPreference::Round);
    }
    #[cfg(target_os = "macos")]
    if vidro() {
        // O vidro vai até o topo: barra de título transparente, sem o texto (o cabeçalho já tem o
        // nome), só com os três botões; o conteúdo desce `Dados.topo` para não ficar embaixo deles.
        use slint::winit_030::winit::platform::macos::WindowAttributesExtMacOS;
        let a = a
            .with_transparent(true)
            .with_titlebar_transparent(true)
            .with_title_hidden(true)
            .with_fullsize_content_view(true);
        // Flyout: a moldura fica só pelo vidro e pelos cantos arredondados (sem moldura o vidro não
        // aparece); sem botões. A altura tem folga no Mac (ver `Flyout` em telas.slint).
        if CRIANDO_FLYOUT.get() {
            return a.with_titlebar_buttons_hidden(true).with_movable_by_window_background(false);
        }
        return a;
    }
    a
}

/// Tema da janela do sistema (0 = do sistema, 1 claro, 2 escuro): a barra de título e o desfoque
/// atrás dela seguem este tema, não o `Palette` do Slint, que não o repassa.
fn tema_winit(tema: i32) -> Option<Theme> {
    match tema {
        1 => Some(Theme::Light),
        2 => Some(Theme::Dark),
        _ => None,
    }
}

/// Tema do sistema, lido direto do SO (o `Palette` do Slint mostra o da janela, que pode estar forçado).
/// `AppleInterfaceStyle` = "Dark" no escuro; ausente no claro. Fora do Mac: não sabe (vale o `Palette`).
#[cfg(target_os = "macos")]
fn sistema_escuro() -> Option<bool> {
    use objc2_foundation::{ns_string, NSUserDefaults};
    let estilo = NSUserDefaults::standardUserDefaults().stringForKey(ns_string!("AppleInterfaceStyle"));
    Some(estilo.is_some_and(|e| e.to_string().eq_ignore_ascii_case("dark")))
}

#[cfg(not(target_os = "macos"))]
fn sistema_escuro() -> Option<bool> {
    None
}

/// Desfoque do sistema atrás das janelas: só no Mac (no Windows o Alex escolheu o fundo sólido).
fn vidro() -> bool {
    cfg!(target_os = "macos")
}

/// Põe o vidro do sistema (Liquid Glass ou `NSVisualEffectView`) atrás da janela. O winit só cria a janela no
/// laço de eventos, depois do `show`: tenta de novo a cada 10 ms até ela existir (no máximo 1 s).
/// Estado "ativo": o desfoque não some quando a janela perde o foco. Sem ele fica só o véu.
#[cfg(target_os = "macos")]
fn desfocar<C: ComponentHandle + 'static>(c: &C, cantos: f64) {
    fn tentar<C: ComponentHandle + 'static>(w: slint::Weak<C>, cantos: f64, resta: u32) {
        let Some(c) = w.upgrade() else { return };
        match c.window().with_winit_window(|j| moldura(j).map(|v| desfocar_vista(v, cantos))) {
            Some(Some(Err(e))) => eprintln!("desfoque: {e}"),
            Some(None) => eprintln!("desfoque: janela sem moldura"),
            Some(Some(Ok(()))) => {}
            None if resta > 0 => Timer::single_shot(Duration::from_millis(10), move || tentar(w, cantos, resta - 1)),
            None => eprintln!("desfoque: a janela não apareceu"),
        }
    }
    tentar(c.as_weak(), cantos, 100);
}

/// View da moldura (`NSThemeFrame`): a mãe da view do winit. O `window-vibrancy` põe o vidro
/// como filha da view que recebe, atrás das outras filhas; na view do winit ele ficaria por cima do
/// desenho OpenGL (que é da própria view), então vai na moldura, atrás da view do winit.
#[cfg(target_os = "macos")]
fn moldura(j: &slint::winit_030::winit::window::Window) -> Option<std::ptr::NonNull<std::ffi::c_void>> {
    use objc2::{msg_send, runtime::AnyObject};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let RawWindowHandle::AppKit(h) = j.window_handle().ok()?.as_raw() else { return None };
    let vista = h.ns_view.as_ptr().cast::<AnyObject>();
    // SAFETY: `ns_view` é a NSView viva da janela (o winit a mantém); `superview` só lê.
    let mae: *mut AnyObject = unsafe { msg_send![vista, superview] };
    let mae = std::ptr::NonNull::new(mae)?;
    // Sem moldura (flyout) a moldura (`NSNextStepFrame`) não tem camada e o vidro não aparece; com
    // moldura (`NSThemeFrame`) já tem.
    // SAFETY: `mae` é a view viva da moldura; `setWantsLayer:` é chamada na thread principal.
    let _: () = unsafe { msg_send![mae.as_ptr(), setWantsLayer: true] };
    Some(mae.cast())
}

#[cfg(target_os = "macos")]
fn desfocar_vista(vista: std::ptr::NonNull<std::ffi::c_void>, cantos: f64) -> Result<(), window_vibrancy::Error> {
    use raw_window_handle::{AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
    use window_vibrancy::{
        apply_liquid_glass, apply_vibrancy, LiquidGlassOptions, NSGlassEffectViewStyle, NSVisualEffectMaterial,
        NSVisualEffectState,
    };
    struct Vista(std::ptr::NonNull<std::ffi::c_void>);
    impl HasWindowHandle for Vista {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            // SAFETY: a moldura vive enquanto a janela existir, e `Vista` só existe nesta chamada.
            Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::AppKit(AppKitWindowHandle::new(self.0))) })
        }
    }
    // Liquid Glass (macOS 26+), o mesmo vidro dos widgets do sistema: pega a cor do que está atrás.
    // Antes do 26 o crate devolve erro e fica o desfoque clássico (`HudWindow`, o mais translúcido).
    // `cantos` arredonda o vidro (0 = só os cantos da janela, que o sistema já arredonda).
    let raio = (cantos > 0.0).then_some(cantos);
    let mut o = LiquidGlassOptions::new(NSGlassEffectViewStyle::Regular);
    if let Some(r) = raio {
        o = o.radius(r);
    }
    apply_liquid_glass(Vista(vista), o).or_else(|_| {
        apply_vibrancy(Vista(vista), NSVisualEffectMaterial::HudWindow, Some(NSVisualEffectState::Active), raio)
    })
}

#[cfg(not(target_os = "macos"))]
fn desfocar<C: ComponentHandle + 'static>(_: &C, _: f64) {}

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
        d.set_sistema(match sistema_escuro() {
            Some(true) => 2,
            Some(false) => 1,
            None => 0,
        });
        d.set_vidro(vidro());
        d.set_sair_no_flyout(cfg!(target_os = "macos"));
        // Altura da barra de título do Mac (o conteúdo vai até o topo da janela, ver `atributos`).
        d.set_topo(if vidro() { 28.0 } else { 0.0 });
        d.set_iniciar_disponivel(partida::DISPONIVEL);
        self.mostrar_partida(d);
        mostrar_ajustes(d, &self.pedido, &self.disp);
        self.mostrar(d);
        self.mostrar_teste(d);
        d.set_nivel(self.nivel);
        d.set_nivel_limpo(self.nivel_limpo);
        d.set_onda(ModelRc::from(self.onda.clone()));
    }

    fn mostrar_teste(&self, d: &Dados) {
        let t = &self.teste;
        let (estado, progresso) = if let Some(g) = &t.gravando {
            (1, g.grav.len() as f32 / (escuta::AMOSTRAS + g.atraso) as f32)
        } else if let (Some(rep), Some(grav)) = (&t.tocando, &t.gravada) {
            (3, rep.pos.load(Relaxed) as f32 / grav.len().max(1) as f32)
        } else {
            (if t.gravada.is_some() { 2 } else { 0 }, 0.0)
        };
        d.set_teste_estado(estado);
        d.set_teste_progresso(progresso.min(1.0));
        d.set_teste_info(t.info.clone());
        d.set_teste_versao(t.versao);
        d.set_teste_saida(t.saida);
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

    fn mostrar_partida(&self, d: &Dados) {
        d.set_iniciar_com_sistema(self.iniciar);
        d.set_iniciar_so_bandeja(self.so_bandeja);
    }

    /// Relê do registro: o Gerenciador de Tarefas (ou outro exe) pode ter mudado a partida.
    fn ler_partida(&mut self) {
        let registrado = partida::ler();
        self.iniciar = registrado.is_some();
        self.so_bandeja = registrado.unwrap_or(self.so_bandeja);
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
            self.nivel_limpo = 0.0;
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
    d.on_modo_escolhido(ao_mudar(|a, d, _| a.modo = MODOS[d.get_modo().clamp(0, 1) as usize]));
    d.on_ganho_mudou(ao_mudar(|a, d, _| a.ganho_db = d.get_ganho_db().round()));
    d.on_entrada_escolhida(ao_mudar(|a, d, disp| a.entrada = disp.entradas.get(d.get_entrada() as usize).cloned()));
    d.on_saida_escolhida(ao_mudar(|a, d, disp| a.saida = disp.saidas.get(d.get_saida() as usize).cloned()));
    d.on_baixa_latencia_mudou(ao_mudar(|a, d, _| a.baixa_latencia = d.get_baixa_latencia()));
    d.on_cancelamento_mudou(ao_mudar(|a, d, _| a.ia = d.get_cancelamento()));
    let w = c.as_weak();
    d.on_tema_escolhido(move || {
        if let Some(c) = w.upgrade() {
            let t = c.global::<Dados>().get_tema();
            c.window().with_winit_window(|w| w.set_theme(tema_winit(t)));
        }
        com_ui(agendar_gravacao);
    });
    let w = c.as_weak();
    d.on_iniciar_mudou(move || {
        if let Some(c) = w.upgrade() {
            let d = c.global::<Dados>();
            com_ui(|ui| mudar_partida(ui, d.get_iniciar_com_sistema(), d.get_iniciar_so_bandeja()));
        }
    });
    let no_teste = |f: fn(&mut Ui, &Dados)| {
        let w = c.as_weak();
        move || {
            if let Some(c) = w.upgrade() {
                com_ui(|ui| {
                    f(ui, &c.global::<Dados>());
                    ui.em_cada(|d| ui.mostrar_teste(d));
                });
            }
        }
    };
    d.on_teste_gravar(no_teste(|ui, _| teste_gravar(ui)));
    d.on_teste_ouvir(no_teste(|ui, d| {
        ui.teste.saida = d.get_teste_saida();
        teste_ouvir(ui);
    }));
    d.on_teste_parar(no_teste(|ui, _| teste_parar(ui)));
    d.on_teste_versao_mudou(no_teste(|ui, d| {
        ui.teste.versao = d.get_teste_versao();
        if let Some(rep) = &ui.teste.tocando {
            rep.limpo.store(ui.teste.versao == 1, Relaxed);
        }
    }));
    d.on_sair(sair);
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

/// Abrir (menu do ícone, "Abrir ›" do flyout ou o app aberto de novo): cria a janela, ou traz
/// para a frente a que já está aberta.
fn abrir_janela() {
    if let Some(Some(j)) = com_ui(|ui| ui.janela.as_ref().map(|j| j.clone_strong())) {
        j.window().set_minimized(false);
        j.window().with_winit_window(|w| w.focus_window());
        return;
    }
    TEMA_NOVA.set(com_ui(|ui| ui.tema_atual()).unwrap_or(0));
    let j = match JanelaPrincipal::new() {
        Ok(j) => j,
        Err(e) => return eprintln!("janela: {e}"),
    };
    ligar(&j, Tela::Janela);
    let tema = com_ui(|ui| {
        ui.ler_partida();
        // A onda recomeça vazia (sem o histórico da vez anterior).
        limpar_onda(&ui.onda);
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
    // Contorno do Slint 1.18 (renderizador de software no Windows): minimizada, a janela às vezes
    // é desenhada inteira mesmo invisível (ex.: ao perder o foco) e, ao voltar, só o que mudou é
    // redesenhado — fica vazia, só com o medidor. Ao voltar, `repintar` força a janela inteira.
    let minimizada = Cell::new(false);
    let w = j.as_weak();
    j.window().on_winit_window_event(move |_, e| {
        if let WindowEvent::Resized(t) = e {
            if t.width == 0 || t.height == 0 {
                minimizada.set(true);
            } else if minimizada.replace(false) {
                let w = w.clone();
                depois(move || {
                    if let Some(j) = w.upgrade() {
                        j.set_repintar(j.get_repintar().wrapping_add(1));
                    }
                });
            }
        }
        EventResult::Propagate
    });
    com_ui(|ui| {
        ui.janela = Some(j.clone_strong());
        ui.ajustar_medidor();
    });
    if let Err(e) = j.show() {
        eprintln!("janela: {e}");
    }
    desfocar(&j, 0.0);
}

fn fechar_janela() {
    let j = com_ui(|ui| {
        let j = ui.janela.take();
        if let Some(j) = &j {
            guardar_janela(ui, j);
            gravar(ui);
        }
        // O teste só anda com a janela aberta (o tick para com ela); a gravação (~2 MB) sai junto.
        teste_cancelar(ui);
        teste_parar(ui);
        ui.teste.gravada = None;
        ui.teste.info = SharedString::new();
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

/// Chaves de "iniciar com o sistema": grava no registro e mostra o que ficou lá de verdade.
fn mudar_partida(ui: &mut Ui, ligado: bool, bandeja: bool) {
    ui.so_bandeja = bandeja;
    if let Err(e) = partida::gravar(ligado, bandeja) {
        eprintln!("iniciar com o sistema: {e}");
        ui.mostrado.erro = format!("Não consegui mudar o iniciar com o sistema: {e}").into();
    }
    ui.ler_partida();
    ui.em_cada(|d| {
        ui.mostrar_partida(d);
        ui.mostrar(d);
    });
    agendar_gravacao(ui);
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
        so_bandeja: ui.so_bandeja,
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
    TEMA_NOVA.set(com_ui(|ui| ui.tema_atual()).unwrap_or(0));
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
    // Fecha ao perder o foco, mas só depois de tê-lo ganhado: no Mac o winit manda um
    // `Focused(false)` antes do `Focused(true)` quando a janela nasce.
    let ganhou_foco = Cell::new(false);
    f.window().on_winit_window_event(move |_, e| {
        match e {
            WindowEvent::Focused(true) => ganhou_foco.set(true),
            WindowEvent::Focused(false) if ganhou_foco.get() => depois(|| fechar_flyout(true)),
            _ => {}
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
    desfocar(&f, 0.0);
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
fn posicao_flyout(largura: f32, altura: f32) -> Option<slint::WindowPosition> {
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
    Some(slint::PhysicalPosition::new(x, y).into())
}

/// Mac: embaixo do ícone da barra de menus, centrado no mouse (que está no ícone), sem sair da tela.
/// O AppKit conta y de baixo para cima a partir da tela principal; o winit, de cima para baixo.
#[cfg(target_os = "macos")]
fn posicao_flyout(largura: f32, _altura: f32) -> Option<slint::WindowPosition> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSScreen};
    use objc2_foundation::NSPointInRect;
    const MARGEM: f64 = 6.0;
    let mtm = MainThreadMarker::new()?;
    let mouse = NSEvent::mouseLocation();
    let telas = NSScreen::screens(mtm);
    let altura_principal = telas.firstObject()?.frame().size.height;
    let tela = telas.iter().find(|t| NSPointInRect(mouse, t.frame()))?;
    // Área útil: abaixo da barra de menus (e fora do Dock).
    let util = tela.visibleFrame();
    let w = largura as f64;
    let x = (mouse.x - w / 2.0).min(util.origin.x + util.size.width - w - MARGEM).max(util.origin.x + MARGEM);
    let topo = util.origin.y + util.size.height - MARGEM;
    Some(slint::LogicalPosition::new(x as f32, (altura_principal - topo) as f32).into())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn posicao_flyout(_largura: f32, _altura: f32) -> Option<slint::WindowPosition> {
    None
}

/// Thread de controle: aplica os pedidos e reage aos avisos. Dorme no `recv` se nada muda.
fn controlar(rx: Receiver<Msg>, avisos: Sender<Aviso>) {
    let mut c = Controlador::novo(Some(avisos));
    // Reprodução do teste (fluxo de saída à parte do motor); guardar o `Fluxo` o mantém tocando.
    let mut _tocando: Option<Fluxo> = None;
    while let Ok(primeira) = rx.recv() {
        // Junta o que chegou de uma vez (ex.: arrastando o ganho): só o último pedido vale.
        let (mut pedido, mut fatal) = (None, None);
        for m in std::iter::once(primeira).chain(rx.try_iter()) {
            match m {
                Msg::Ajustes(a) => pedido = Some(a),
                Msg::Aviso(a) if a.fatal => fatal = Some(a),
                Msg::Aviso(a) => eprintln!("aviso ({}): {}", if a.entrada { "entrada" } else { "saída" }, a.texto),
                Msg::Ouvir { grav, rep, saida, ganho_db } => {
                    _tocando = None;
                    match abrir_tocador(grav, rep, saida, ganho_db) {
                        Ok(f) => _tocando = Some(f),
                        Err(e) => {
                            let texto = format!("Não consegui tocar: {e}");
                            let _ = slint::invoke_from_event_loop(move || {
                                com_ui(|ui| teste_falhou(ui, texto));
                            });
                        }
                    }
                }
                Msg::PararOuvir => _tocando = None,
                Msg::Sair => return,
            }
        }
        if pedido.is_none() && fatal.is_none() {
            continue;
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

fn abrir_tocador(grav: Arc<Gravacao>, rep: Arc<Reproducao>, saida: Option<String>, ganho_db: f32) -> Result<Fluxo, Box<dyn Error>> {
    let mut t = Tocador::novo(grav, rep, ganho_db);
    let f = audio::abrir_saida(&Pedido { trecho: saida, ..Pedido::default() }, move |d, p| t.preencher(d, p.canais))?;
    f.iniciar()?;
    Ok(f)
}

/// "Gravar": liga a cópia no motor atual; o tick junta as amostras (`teste_tick`).
fn teste_gravar(ui: &mut Ui) {
    teste_parar(ui);
    let Some((ctl, modelo)) = &ui.motor else {
        ui.teste.info = "Ligue o áudio para gravar.".into();
        return;
    };
    let atraso = if ctl.ia() { modelo.atraso_amostras() } else { 0 };
    ctl.escutar(true);
    ui.teste.gravando = Some(Gravando { ctl: ctl.clone(), modo: modo_ativo(&ui.pedido), atraso, grav: Gravacao::nova() });
    ui.teste.info = "Fale agora…".into();
}

fn teste_cancelar(ui: &mut Ui) {
    if let Some(g) = ui.teste.gravando.take() {
        g.ctl.escutar(false);
    }
}

fn teste_ouvir(ui: &mut Ui) {
    let Some(grav) = ui.teste.gravada.clone() else { return };
    let rep = Arc::new(Reproducao::default());
    rep.limpo.store(ui.teste.versao == 1, Relaxed);
    let saida = ui.disp.saidas.get(ui.teste.saida as usize).cloned();
    let _ = ui.tx.send(Msg::Ouvir { grav, rep: rep.clone(), saida, ganho_db: ui.pedido.ganho_db });
    ui.teste.tocando = Some(rep);
}

fn teste_parar(ui: &mut Ui) {
    if ui.teste.tocando.take().is_some() {
        let _ = ui.tx.send(Msg::PararOuvir);
    }
}

fn teste_falhou(ui: &mut Ui, texto: String) {
    ui.teste.tocando = None;
    ui.teste.info = texto.into();
    ui.em_cada(|d| ui.mostrar_teste(d));
}

/// A cada tick: junta o que o motor copiou e fecha a gravação no tamanho; solta a reprodução
/// que chegou ao fim.
fn teste_tick(ui: &mut Ui) {
    if ui.teste.gravando.is_none() && ui.teste.tocando.is_none() {
        return;
    }
    if let Some(g) = &mut ui.teste.gravando {
        let mesmo = ui.motor.as_ref().is_some_and(|(c, _)| Arc::ptr_eq(c, &g.ctl)) && modo_ativo(&ui.pedido) == g.modo;
        if !mesmo {
            teste_cancelar(ui);
            ui.teste.info = "Gravação cancelada: o modo ou o áudio mudou no meio.".into();
        } else {
            g.ctl.colher_escuta(&mut g.grav);
            if g.grav.len() >= escuta::AMOSTRAS + g.atraso {
                let mut g = ui.teste.gravando.take().expect("checado acima");
                g.ctl.escutar(false);
                g.grav.alinhar(g.atraso);
                let modo = match g.modo {
                    None => "sem cancelamento",
                    Some(Modo::Leve) => "no modo Leve",
                    Some(Modo::Forte) => "no modo Forte",
                };
                ui.teste.info = format!("Gravado {modo}. Troque entre Original e Limpo enquanto ouve.").into();
                ui.teste.gravada = Some(Arc::new(g.grav));
            }
        }
    }
    if ui.teste.tocando.as_ref().is_some_and(|r| r.fim.load(Relaxed)) {
        teste_parar(ui);
    }
    ui.em_cada(|d| ui.mostrar_teste(d));
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
    ui.motor = e.motor;
    atualizar_numeros(ui);
    ui.em_cada(|d| ui.mostrar(d));
}

/// Medidor e onda (~30 Hz) e, a cada segundo, os números. Não faz nada com a janela minimizada e sem flyout.
fn tick(ui: &mut Ui) {
    teste_tick(ui);
    let minimizada = ui.janela.as_ref().is_none_or(|j| j.window().is_minimized());
    if ui.flyout.is_none() && minimizada {
        return;
    }
    let (pico, pico_limpo) = ui.motor.as_ref().map_or((0.0, 0.0), |(c, _)| (c.tomar_pico(), c.tomar_pico_limpo()));
    let alvo = |p: f32| if p > 0.0 { ((20.0 * p.log10() + FAIXA_DB) / FAIXA_DB).clamp(0.0, 1.0) } else { 0.0 };
    let (mic, limpo) = (alvo(pico), alvo(pico_limpo));
    ui.nivel = mic.max(ui.nivel - QUEDA);
    ui.nivel_limpo = limpo.max(ui.nivel_limpo - QUEDA);
    ui.em_cada(|d| {
        d.set_nivel(ui.nivel);
        d.set_nivel_limpo(ui.nivel_limpo);
    });
    // A onda leva o pico de cada tick, sem a queda do medidor (as palavras não se borram).
    if !minimizada {
        empurrar_onda(&ui.onda, Barra { mic, limpo });
    }
    ui.tick += 1;
    if ui.tick.is_multiple_of(TICKS_NUMEROS) {
        atualizar_numeros(ui);
        ui.em_cada(|d| ui.mostrar(d));
    }
}

/// Atualiza atraso, CPU e falhas em `ui.mostrado` (quem chama mostra nas telas).
fn atualizar_numeros(ui: &mut Ui) {
    let m = &mut ui.mostrado;
    // CPU do processo inteiro (IA + áudio + janela) em % do PC, na escala do Gerenciador de Tarefas.
    if let Some(cpu) = cpu_do_processo() {
        let agora = Instant::now();
        match ui.cpu_anterior {
            // Chamada fora do tique (troca de motor): espera o intervalo inteiro.
            Some((_, t)) if agora - t < Duration::from_millis(500) => {}
            // Depois de minimizada a média seria de minutos; recomeça a conta.
            Some((c, t)) if agora - t <= Duration::from_secs(3) => {
                let nucleos = std::thread::available_parallelism().map_or(1, |n| n.get());
                let pct = (cpu - c).as_secs_f64() / (agora - t).as_secs_f64() / nucleos as f64 * 100.0;
                m.cpu = if pct < 0.1 { "< 0,1 %".into() } else { format!("{pct:.1} %").replace('.', ",").into() };
                ui.cpu_anterior = Some((cpu, agora));
            }
            _ => ui.cpu_anterior = Some((cpu, agora)),
        }
    }
    let Some((c, modelo)) = &ui.motor else {
        (m.atraso, m.falhas) = ("—".into(), 0);
        return;
    };
    let met = &c.metricas;
    if met.atraso_saida_us.n() > 0 {
        m.atraso = format!("~{:.0} ms", met.latencia_ms(c.ia(), *modelo)).into();
    }
    m.falhas = met.falhas() as i32;
}

/// Tempo de CPU (núcleo + usuário, todas as threads) gasto pelo processo até agora.
#[cfg(windows)]
fn cpu_do_processo() -> Option<Duration> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let [mut criacao, mut fim, mut nucleo, mut usuario] = [FILETIME::default(); 4];
    // SAFETY: o pseudo-handle do próprio processo não precisa ser fechado; só escreve nas variáveis locais.
    unsafe { GetProcessTimes(GetCurrentProcess(), &mut criacao, &mut fim, &mut nucleo, &mut usuario) }.ok()?;
    let unidades = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
    Some(Duration::from_nanos((unidades(nucleo) + unidades(usuario)) * 100))
}

/// Mesma conta no macOS/Linux: `getrusage` do próprio processo (todas as threads).
#[cfg(unix)]
fn cpu_do_processo() -> Option<Duration> {
    // SAFETY: `rusage` é só números (zerado é válido); `getrusage` só escreve na variável local.
    let mut r: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut r) } != 0 {
        return None;
    }
    let t = |v: libc::timeval| Duration::from_secs(v.tv_sec as u64) + Duration::from_micros(v.tv_usec as u64);
    Some(t(r.ru_utime) + t(r.ru_stime))
}

/// O que a IA está fazendo: `None` com o cancelamento desligado.
fn modo_ativo(a: &Ajustes) -> Option<Modo> {
    a.ia.then_some(a.modo)
}

fn mostrar_ajustes(d: &Dados, a: &Ajustes, disp: &Dispositivos) {
    let indice = |nomes: &[String], escolhido: &Option<String>, padrao: Option<usize>| {
        escolhido.as_deref().map_or(padrao, |t| audio::escolher(nomes, t)).map_or(-1, |i| i as i32)
    };
    d.set_cancelamento(a.ia);
    d.set_modo(MODOS.iter().position(|m| *m == a.modo).unwrap_or(0) as i32);
    d.set_ganho_db(a.ganho_db);
    d.set_baixa_latencia(a.baixa_latencia);
    d.set_entrada(indice(&disp.entradas, &a.entrada, disp.padrao_entrada));
    d.set_saida(indice(&disp.saidas, &a.saida, disp.padrao_saida));
}

fn modelo_de(nomes: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(nomes.iter().map(SharedString::from).collect::<Vec<_>>()))
}
