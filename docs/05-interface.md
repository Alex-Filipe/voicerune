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
| Teste (fone) | Gravar 5 s e ouvir com A/B Original ↔ Limpo (Fase 4, pedido do Alex) |
| Teste (fone) | Gravar 5 s e ouvir com A/B Original ↔ Limpo (Fase 4, pedido do Alex) |
| Efeitos | Ícone apagado ("em breve") até existir |
| Configurações (embaixo) | Baixa latência, Iniciar com o sistema, Tema, Desempenho (atraso, CPU, falhas) |
| Sobre (embaixo) | Versão, sem rede/telemetria, licença, modelo, selo do Slint (exigido pela licença) |
| Flyout da bandeja | Liga/desliga, modo, medidor, rota e "Abrir ›" |

### Implementação em Slint (`crates/ui`)
- `ui/telas.slint`: `JanelaPrincipal`, `Flyout` e `Bandeja`; estado no global `Dados` (o Rust
  preenche; cada tela tem o seu, o app repõe o estado em cada tela nova).
- Estilo Fluent; cores próprias claro/escuro seguindo o tema do sistema e a **cor de destaque do
  Windows**. Tema pode ser forçado (Configurações → Tema).
- Leveza: `.slint` compilado para Rust no build; sem features padrão do Slint (só `backend-winit`
  + `renderer-software`: sem OpenGL; desde o bloco 3 da Fase 4 também `system-tray` e
  `unstable-winit-030`, sem crate novo no Windows); só a página visível é instanciada (`if`).
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

### Bandeja e flyout (Fase 4, bloco 3)
- Ícone: componente `Bandeja` (`SystemTrayIcon` do Slint com `icone.svg`, dica "mic-limpo").
  Clique abre ou fecha o flyout; botão direito abre o menu **Abrir / Sair**. No Windows 11 ele
  nasce no grupo de ícones ocultos ("^"); o Alex pode arrastá-lo para a barra.
- Flyout: criado a cada clique e destruído ao perder o foco. A altura do layout é lida antes de
  mostrar e a posição vem do cursor + área útil do monitor (encostado na barra de tarefas, em
  qualquer lado, margem de 12 px). Fica fora da barra de tarefas e tem cantos arredondados (DWM)
  pelo gancho de atributos do winit. Um clique no ícone até 400 ms depois de fechar por perder o
  foco é o mesmo clique e não reabre.
- O que muda no flyout aparece na janela aberta, e vice-versa. Nome longo da rota é cortado com
  "…" (antes empurrava o "Abrir ›" para fora).
- Janela: o "X" esconde e destrói o componente logo depois, e o app fica na bandeja. Reabrir
  volta na mesma posição e tamanho (se não estava minimizada nem maximizada) e com o tema escolhido.
- Teste automático: script PowerShell manda as mensagens da bandeja (`WM_APP+1`) direto para a
  janela oculta `SlintSystemTrayWindow` do processo e só manda teclas se o app estiver em primeiro
  plano. Captura: `docs/mockups/capturas/app-flyout.png`.

**Medições (2026-10-20, release; Leve; entrada padrão = Voicemeeter Out B1 em silêncio, saída
CABLE Input; Discord fechado; 10 s cada pelo `TotalProcessorTime` do processo; 2 rodadas):**

| Caso | RAM (working set) | RAM privada | CPU do processo |
|---|---|---|---|
| Janela visível, medidor parado | 24,8 MB | 5,4–5,5 MB | 0,47–0,94 % de 1 núcleo |
| **Só bandeja** | **24,0 MB** | **4,5 MB** | **0,31–0,47 % de 1 núcleo** |
| Só bandeja, depois de 10 abre/fecha | 27,4–27,5 MB | 5,1–5,2 MB | 0,78–1,40 % de 1 núcleo (ruído, ver abaixo) |
| Flyout aberto (1 rodada) | 27,7 MB | 5,3 MB | 0,78 % de 1 núcleo |

- Thread da UI por `QueryThreadCycleTime`: 40 Mciclos em 10 s com a janela (timer a 30 Hz) e
  **0** só com a bandeja, ou seja, o timer some de fato. O `TotalProcessorTime` da thread dá 0 nos
  dois casos (granularidade de ~15,6 ms), não serve para isso.
- Só com a bandeja a CPU é a do áudio (RNNoise ~0,4 %). Fechar a janela pela 1ª vez solta ~1 MB
  privada; o resto (~24 MB de working set) é áudio + runtime, então o "mínimo" já é quase isso.
- Reabrir (Abrir do menu → janela visível, debug): 37–46 ms (3×); "Abrir ›" do flyout: 75 ms.
  Sair: processo acaba em 48 ms (debug).
- **Abre/fecha repetido não vaza** (30 ciclos, release, contando todas as janelas do processo,
  inclusive ocultas e message-only):

| Depois de | Working set | Privada | Threads | Handles | Janela principal (mesmo oculta) |
|---|---|---|---|---|---|
| 0 ciclos (só bandeja) | 24,0 MB | 4,5 MB | 16 | 313 | 0 |
| 10 ciclos | 27,5 MB | 5,1 MB | 16 | 342 | 0 |
| 20 ciclos | 27,9 MB | 5,6 MB | 16 | 342 | 0 |
| 30 ciclos | 27,7 MB | 5,4 MB | 12 | 336 | 0 |

  O aumento (~+3,5 MB de working set, ~+1 MB privada) vem uma vez só nos primeiros ciclos e
  estabiliza: cache (DLLs do IME/TSF, alocador, fontes), não vazamento. Cada janela é destruída
  de fato; só aparece uma vez uma `SystemUserAdapterWindowClass` do sistema. CPU por thread
  (`QueryThreadCycleTime`, 10 s, só bandeja): só `cpal_wasapi_in` (~176 Mciclos) e
  `cpal_wasapi_out` (~38) trabalham, iguais antes e depois dos 30 ciclos; a da UI, 0. A diferença de
  CPU do processo na tabela acima (0,3–1,4 %) é ruído do `TotalProcessorTime` em 10 s.
- **Não testado:** clique real no ícone (os testes mandam a mensagem direto) e a nitidez do ícone
  de 64 px reduzido pelo shell.

### Configuração salva (Fase 4, bloco 4)
- `%APPDATA%\mic-limpo\config.ini` (módulo `salvo` na lib do app): ligado, modo, volume, mic,
  saída, baixa latência, tema e posição/tamanho da janela. Lido na partida; gravado 0,5 s depois da
  última mudança, ao fechar a janela e no Sair (temporário + troca de nome). Apagar o arquivo
  volta aos padrões.
- Tolerante: chave ausente ou valor inválido fica no padrão, chave desconhecida é ignorada, BOM do
  Bloco de Notas é aceito, volume limitado a −12…+24 dB.
- Mic ou saída salvos que não estão conectados: volta ao padrão (mic do Windows; saída `CABLE
  Input`) e o áudio abre. **Limitação:** a próxima gravação guarda o padrão no lugar do nome salvo
  (plugar o mic de novo pede escolher outra vez).
- Janela salva cuja barra de título não cai em nenhum monitor (monitor desligado) é ignorada.
- Testado (debug, 2026-10-20): Desligado pelo flyout → arquivo 0,5 s depois; janela movida → posição
  salva no Sair; reabrir volta na posição e no modo (o motor também). Arquivo editado com mic
  inexistente + tema claro + 6 dB → abre com o mic padrão, tema claro e +6 dB. +2 testes de
  unidade (ida e volta, arquivo estranho).

### Página Teste (Fase 4, pedido do Alex)
- **Gravar:** liga a cópia no motor (`Controle::escutar`): a cada quadro, original e limpo vão
  intercalados para um ring buffer à parte (0,5 s, ~190 KB fixos por motor); com a escuta
  desligada, o custo na thread de áudio é ler um atômico. A janela junta tudo a cada tick do
  medidor (~33 ms) até 5 s, tira o atraso do modelo do limpo e guarda a gravação (~1,9 MB).
  O áudio continua indo normalmente para o CABLE.
- **Ouvir:** a thread de controle abre um fluxo de saída à parte (padrão do Windows ou o escolhido
  em "Ouvir em") com o `Tocador` (`core::escuta`): troca Original ↔ Limpo na mesma posição, com
  rampa de 5 ms; aplica o volume de saída atual aos dois (comparação justa). No fim o fluxo fecha.
- Trocar de modo ou reabrir o áudio no meio cancela a gravação. Fechar a janela cancela o teste e
  solta a gravação (o tick só existe com a janela).
- Testado (2026-10-21): cliques simulados em debug (gravar, ouvir, A/B, parar, fim sozinho) e
  escuta do Alex ("ficou bom"). Medido (release, Leve): página parada 0,9 % de um núcleo com o
  medidor; +3,7 MB privada depois de gravar, por realocação do `Vec` (corrigido reservando a folga;
  esperado ~+1,9 MB, **não medido de novo**). +1 thread depois da 1ª reprodução: **não verificado**
  se acumula a cada reprodução. +5 testes de unidade.
- Alex achou o Forte ruim no teste ("às vezes corta"): ver `04-ia-ruido.md` e o próximo passo.

### Página Teste (Fase 4, pedido do Alex)
- **Gravar:** liga a cópia no motor (`Controle::escutar`): a cada quadro, original e limpo vão
  intercalados para um ring buffer à parte (0,5 s, ~190 KB fixos por motor); com a escuta
  desligada, o custo na thread de áudio é ler um atômico. A janela junta tudo a cada tick do
  medidor (~33 ms) até 5 s, tira o atraso do modelo do limpo e guarda a gravação (~1,9 MB).
  O áudio continua indo normalmente para o CABLE.
- **Ouvir:** a thread de controle abre um fluxo de saída à parte (padrão do Windows ou o escolhido
  em "Ouvir em") com o `Tocador` (`core::escuta`): troca Original ↔ Limpo na mesma posição, com
  rampa de 5 ms; aplica o volume de saída atual aos dois (comparação justa). No fim o fluxo fecha.
- Trocar de modo ou reabrir o áudio no meio cancela a gravação. Fechar a janela cancela o teste e
  solta a gravação (o tick só existe com a janela).
- Testado (2026-10-21): cliques simulados em debug (gravar, ouvir, A/B, parar, fim sozinho) e
  escuta do Alex ("ficou bom"). Medido (release, Leve): página parada 0,9 % de um núcleo com o
  medidor; +3,7 MB privada depois de gravar, por realocação do `Vec` (corrigido reservando a folga;
  esperado ~+1,9 MB, **não medido de novo**). +1 thread depois da 1ª reprodução: **não verificado**
  se acumula a cada reprodução. +5 testes de unidade.
- Alex achou o Forte ruim no teste ("às vezes corta"): ver `04-ia-ruido.md` e o próximo passo.

### Medidor de duas camadas (Fase 4, pedido do Alex, 2026-10-21)
- Em cinza o que o mic capta (pico da entrada); em cores por cima o que sai da IA (pico na saída do
  modelo, **antes** do volume de saída, para a comparação ser justa). O cinza que aparece é o que
  a IA tirou. Mesma escala (−60 a 0 dBFS) e queda nos dois; vale para Início, Teste e flyout.
- Custo: um `fetch_max` atômico por chamada do modelo (`Controle::tomar_pico_limpo`). +1 teste.
- Conferido na prévia (claro e escuro). **Não medido** ao vivo nem aprovado pelo Alex ("se não
  gostar, retiramos").

### Pendências de UI
- O timer do medidor ainda acorda (sem redesenhar) com a janela minimizada.
- Pendências visuais: barra de título nativa não segue o tema forçado; Mica/Acrylic
  (`window-vibrancy`) não testado. Ícone próprio da janela: `crates/ui/ui/icone.svg`.
- Armadilha resolvida: **nunca** pôr `Palette.color-scheme = ColorScheme.unknown`. Os widgets do
  Fluent e as nossas cores passam a divergir (texto branco em fundo branco: caixas de seleção
  "somem"). "Do sistema" volta para o esquema que o sistema tinha antes do 1º ajuste.
