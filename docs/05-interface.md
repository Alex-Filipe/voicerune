# Interface

## Framework (decidido: Slint)

| Opção | Por que sim / por que não |
|---|---|
| **Slint** ✅ | Declarativo, leve, estilos Fluent (Win) e Cupertino (Mac), animações |
| Iced | Leve, mas exige mais trabalho para chegar no visual |
| egui | Cara de ferramenta de dev |
| Tauri | WebView2 pesado — conflita com leveza |
| WinUI 3 | Só Windows; bindings Rust trabalhosos |

Extras: `window-vibrancy` (Mica/Acrylic no Win, vibrancy no Mac), `tray-icon` (bandeja / barra de menu).

## Princípios de UX
- Tela principal mínima: mic → **Cancelamento: Desligado / Leve / Forte** → saída; medidor de nível; mute.
- **Presets** ("Reunião", "Stream", "Jogo") em vez de botões técnicos.
- Avançado (gate, EQ, roteamento) numa aba escondida.
- Configuração automática de dispositivo e buffer; nunca mostrar "WDM/KS/MME".
- **Flyout na bandeja** (estilo controle de volume do Windows) para o uso diário.
- Botão **"Ouvir antes/depois"**.
- Primeiro uso em 3 passos: escolher mic → testar voz → onde usar. No Mac, inclui a permissão de microfone.
- Animações curtas e sutis (não pesar).

## Mockup aprovado em estrutura (Fase 3)

Direções testadas em HTML (`docs/mockups/direcoes.html`): A · Fluent compacto, B · botão central,
C · faixa de canal. **Escolhida a A com barra lateral de ícones** (`docs/mockups/a-lateral.html`),
para crescer sem poluir a tela.

| Página | Conteúdo |
|---|---|
| Início | Microfone + medidor, Cancelamento (Desligado / Leve / Forte), Volume de saída (−12 a +24 dB), Enviar para, linha de status |
| Efeitos | Ícone apagado ("em breve") até existir |
| Configurações (embaixo) | Baixa latência, Iniciar com o sistema, Tema, Desempenho (atraso, CPU, falhas) |
| Sobre (embaixo) | Versão, sem rede/telemetria, licença, modelo, selo do Slint (exigido pela licença) |
| Flyout da bandeja | Liga/desliga, modo, medidor, rota e "Abrir ›" |

### Implementação em Slint (`crates/ui`)
- `ui/telas.slint`: `JanelaPrincipal` e `Flyout`; estado no global `Dados` (o Rust preenche).
- Estilo Fluent; cores próprias claro/escuro seguindo o tema do sistema e a **cor de destaque do
  Windows**. Tema pode ser forçado (Configurações → Tema).
- Leveza: `.slint` compilado para Rust no build; sem features padrão do Slint (só `backend-winit`
  + `renderer-software`: sem OpenGL); só a página visível é instanciada (`if`).
- Prévia: `mic-limpo-ui-previa [--flyout] [--pagina N] [--claro|--escuro] [--medidor-parado]
  [--medir] [--sair-apos-ms N]`. Capturas em `docs/mockups/capturas/`.

### Medições da prévia (2026-10-20, release, Windows 11, 12 threads)

| Caso | RAM (working set) | RAM privada | CPU |
|---|---|---|---|
| Janela parada | 20,0 MB | 4,0 MB | 0,00 % |
| Janela, medidor a 30 Hz | 20,0 MB | 4,0 MB | 1,6 % de 1 núcleo (0,14 % do total) |
| Flyout, medidor a 30 Hz | 18,2 MB | 3,0 MB | 1,5 % de 1 núcleo (0,12 % do total) |

- Até o laço de eventos rodar: 30–106 ms (5 execuções). Abrir + fechar o processo inteiro,
  medido pelo PowerShell: 270–730 ms, com alta variação; o tempo de fechar não foi medido à parte.
- Binário 12,9 MB (sem LTO nem `opt-level = "s"`; dá para reduzir).
- Rede: `cargo tree` sem crates de rede (`url`/`data-url` só no compilador do Slint, em tempo de
  build; `scoped-tls-hkt` é thread-local). O binário não importa `ws2_32`/`winhttp`/`wininet`.

### App ligado ao motor (Fase 4, bloco 2)
- `mic-limpo` abre a `JanelaPrincipal` com dados reais: listas de dispositivos de `audio::listar`
  (na partida; ainda não atualizam ao plugar/desplugar), modo, ganho, baixa latência, status,
  atraso, CPU, falhas e modelo. Cada controle chama um callback próprio no `Dados`
  (`modo-escolhido`, `ganho-mudou`, ...): o mic continua "o padrão do sistema" até ser escolhido.
- Medidor: pico da entrada (antes da IA) em dB, de −60 a 0 dBFS, caindo ~1,8 dB por tick.
- "CPU" = só o processamento da IA, em % de um núcleo, no último segundo. "Atraso do app" =
  estimativa do app (soma dos atrasos médios), **sem** o VB-Cable (~55–60 ms a mais).
- Troca que falha (ex.: "CABLE In 16ch" recusa o formato, `AUDCLNT_E_UNSUPPORTED_FORMAT`): o motor
  antigo continua, a janela volta ao valor anterior e o status diz "Não deu para trocar (...)".
- Busca de dispositivo: nome exato primeiro, depois "contém".
- Captura: `docs/mockups/capturas/app-inicio.png`.

**Medições (2026-10-20, release; entrada padrão = Voicemeeter Out B1 em silêncio, saída CABLE
Input; 10 s cada pelo `TotalProcessorTime` do processo):**

| Caso | RAM (working set) | RAM privada | CPU do processo |
|---|---|---|---|
| Leve, janela visível, medidor parado | 23,8 MB | 5,4 MB | 0,6–0,9 % de 1 núcleo |
| Leve, minimizada | 23,8 MB | 5,4 MB | 0,5–1,1 % de 1 núcleo |
| Forte, visível | 48,0 MB | 18,1 MB | 0,6 % (silêncio digital: o DFN quase não trabalha) |

- Janela aparece em 150–190 ms (uma vez 440 ms); fechar pelo "X" encerra em 24–32 ms.
- **Não medido:** CPU com o medidor animando e Forte com voz. O mic estava mudo (gate do Windows:
  p90 −84 dBFS) e o Discord aberto impediu injetar sinal no VB-Cable. Hipótese: +~1,5 ponto com
  o medidor animado (prévia da Fase 3) e ~8 % no Forte com voz (Fase 2).
- **Não testado:** reabrir depois de um aviso fatal (precisa desconectar um dispositivo).

### Para a Fase 4
- Fechar a janela deve **destruir** o componente (não só esconder) e manter só a bandeja: a RAM
  volta ao mínimo; reabrir custa ~0,1 s.
- O medidor só deve animar com a janela/flyout visível (parado = 0 % de CPU). Bloco 2: o timer
  segue acordando com a janela minimizada (sem redesenhar); some quando o componente for
  destruído (bloco 3).
- Flyout: posicionar junto da bandeja, cantos arredondados (DWM) e fechar ao perder o foco.
- Avaliar o `system-tray` do próprio Slint 1.18 em vez do crate `tray-icon`.
- Pendências visuais: barra de título nativa não segue o tema forçado; Mica/Acrylic
  (`window-vibrancy`) não testado. Ícone próprio da janela: `crates/ui/ui/icone.svg`.
- Armadilha resolvida: **nunca** pôr `Palette.color-scheme = ColorScheme.unknown`. Os widgets do
  Fluent e as nossas cores passam a divergir (texto branco em fundo branco: caixas de seleção
  "somem"). "Do sistema" volta para o esquema que o sistema tinha antes do 1º ajuste.
