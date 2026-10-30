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
- Tela principal mínima: mic → **Cancelamento: chave + Leve / Forte (/ Personalizado)** → saída; medidor de nível; mute.
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

**2026-10-23:** o Alex aprovou um visual novo, a direção B "vidro" (`docs/mockups/b-vidro.html`): painel
único (sem barra lateral; Teste/Configurações/Sobre pelos ícones do rodapé), botão redondo do
cancelamento, onda ao vivo (cinza = mic, acento = limpo), volume em número grande com − / +, ladrilhos
Leve/Forte/Personalizado, dispositivos em lista agrupada, pílulas de recursos. Fundo sólido primeiro;
desfoque real só depois de medir o `window-vibrancy`. **Feito com fundo sólido em 2026-10-23** (ver
"Visual vidro" abaixo); a onda a 30 Hz foi medida. **No Windows fica o fundo sólido** (Alex, 2026-10-23);
desfoque só no Mac (Fase 5; ver "Desfoque: só no Mac" abaixo).

| Painel | Conteúdo |
|---|---|
| Início | Cabeçalho (nome, status, botão redondo = chave do cancelamento), onda ao vivo, volume grande (− / + e faixa de −12 a +24 dB), Cancelamento em ladrilhos (+ "?"), Dispositivos em lista agrupada, Recursos (Baixa latência, Testar voz, Iniciar com o sistema), rodapé ("CPU do app · sem rede"; lua = claro/escuro, Teste, Configurações, Sobre) |
| Teste (fone) | Gravar 5 s e ouvir com A/B Original ↔ Limpo; "‹" volta ao Início |
| Configurações | Baixa latência, Iniciar com o sistema + Só na bandeja, Tema (Do sistema / Claro / Escuro), Desempenho (atraso, CPU do app, falhas) |
| Sobre | Versão, sem rede/telemetria, licença, modelo, selo do Slint (exigido pela licença) |
| Flyout da bandeja | Status, botão redondo, medidor, Leve / Forte / Personalizado, volume, rota e "Abrir ›" |

O ícone "Efeitos (em breve)" da barra lateral saiu com ela; o ladrilho Personalizado (desabilitado)
ocupa esse lugar.

### Implementação em Slint (`crates/ui`)
- `ui/telas.slint`: `JanelaPrincipal`, `Flyout` e `Bandeja`; estado no global `Dados` (o Rust
  preenche; cada tela tem o seu, o app repõe o estado em cada tela nova).
- Desde o vidro (2026-10-23): cores próprias claro/escuro (fundo sólido quente, acento azul fixo do
  mockup, não o do Windows) e componentes próprios (botão redondo, ladrilhos, pílulas, faixa, chave,
  botões). Do estilo Fluent sobram o `ComboBox` (invisível por cima das linhas de dispositivo: a lista
  aberta é a do Fluent) e o `AboutSlint`. Tema segue o sistema e pode ser forçado (Configurações →
  Tema, ou a lua do rodapé).
- Leveza: `.slint` compilado para Rust no build; sem features padrão do Slint (só `backend-winit`
  + `renderer-software`: sem OpenGL; desde o bloco 3 da Fase 4 também `system-tray` e
  `unstable-winit-030`, sem crate novo no Windows); só a página visível é instanciada (`if`).
- Prévia: `mic-limpo-ui-previa [--flyout] [--pagina N] [--claro|--escuro] [--medidor-parado]
  [--medir] [--sair-apos-ms N]` (`--pagina` 0 Início, 1 Configurações, 2 Sobre, 3 Teste; anima
  medidor e onda com fala fictícia). Capturas em `docs/mockups/capturas/` (as do vidro: `vidro-*`).

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
- "CPU do app" = processo inteiro (IA + áudio + janela) em % do PC, como no Gerenciador de Tarefas:
  delta de núcleo + usuário do `GetProcessTimes` ÷ tempo de relógio ÷ threads lógicas, a cada
  segundo e só com janela/flyout abertos; abaixo de 0,1 mostra "< 0,1 %"; fora do Windows "—".
  Até 2026-10-21 era só a IA em % de **um** núcleo (Forte "11,7 %" assustava). "Atraso do app" =
  estimativa do app (soma dos atrasos médios), **sem** o VB-Cable (~55–60 ms a mais).
- Conferido (2026-10-21, release, 12 threads, mic do Alex, página Configurações aberta, 10 s pelo
  `TotalProcessorTime` × cartão a cada segundo): **Forte** 1,16 % e 1,04 % × cartão 0,9–1,3 %;
  **Leve** 0,05 % × cartão "< 0,1 %" (picos 0,1–0,3 %).
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
- `%APPDATA%\mic-limpo\config.ini` (módulo `salvo` na lib do app): cancelamento, modo, volume, mic,
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

- **Divergência não reproduzida (2026-10-21):** o app do Alex aberto às 20:08:55 mostrava Leve, 0 dB
  e tema claro (Windows escuro) enquanto o `config.ini` (gravado 20:05:37) dizia Forte, +5 dB, `tema =
  sistema`; Alex não mexeu em nada. Depois: fechar a janela regravou o arquivo e a cópia seguinte
  abriu em Forte, +5 dB, escuro (certo). Matar o processo (como nos testes) não grava: normal. Se
  voltar a acontecer, anotar o que aparece ao abrir e o conteúdo do arquivo antes de fechar o app.

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
- Conferido na prévia (claro e escuro) e **aprovado pelo Alex** ao vivo ("gostei, ficou bom").

### Chave do cancelamento + "?" (Fase 4, pedido do Alex, 2026-10-21)
- O antigo modo "Desligado" virou uma **chave** no título do cartão "Cancelamento de ruído"; embaixo,
  **Leve | Forte | Personalizado** (este apagado e sem clique: ainda não existe). Desligada, a voz
  passa sem filtro, os modos ficam apagados e o escolhido continua marcado (religar volta igual).
- **"?"** ao lado do título abre uma janelinha (`PopupWindow`) explicando chave, Leve, Forte e
  Personalizado; clicar em qualquer lugar fecha. Sem rótulo extra na chave: o título já diz o quê.
- **Flyout:** a chave do topo agora é a do cancelamento (antes **pausava o áudio todo**, e o Discord
  ficava sem mic). A pausa saiu da janela; parar tudo = "Sair" da bandeja. `ligado` continua no
  `Controlador` (o `mic-limpo-dev rodar` usa no `p`), o app sempre abre ligado.
- `config.ini`: `cancelamento = sim/nao` + `modo = leve/forte`. Arquivo antigo: `modo = desligado`
  vira a chave desligada e `ligado` é ignorado (+1 teste). Com a chave desligada o modelo do modo
  fica carregado (o Forte ocupa a RAM do DFN), para religar sem espera.
- Conferido em release (2026-10-21, cliques simulados): janelinha cabe na janela; desligar →
  status "sem cancelamento", clique no Leve ignorado, `cancelamento = nao` gravado; religar volta ao
  Forte. Captura: `docs/mockups/capturas/app-inicio.png`. Flyout com as três opções (360 px)
  conferido pelo Alex: "ficou bom".

### Instância única (Fase 4, bloco 4, 2026-10-21)
- A 1ª cópia cria o evento nomeado `Local\mic-limpo-abrir` (por sessão do Windows) e uma thread
  dorme nele. Abrir o app de novo (atalho, exe) acha o evento, chama `AllowSetForegroundWindow` +
  `SetEvent` e sai antes do backend do Slint e do áudio; a 1ª abre a janela (ou restaura e foca a
  aberta). Sem crate novo (só a feature `Win32_Security` do `windows`). Fora do Windows: nada (o
  macOS já abre um `.app` uma vez só; conferir na Fase 5).
- Conferido em debug (2026-10-21, PowerShell): a 2ª cópia sai com código 0 em ~0,6 s e fica 1
  processo; só bandeja → a janela volta; minimizada → restaurada. **Não verificado:** a janela vir
  para a frente. Lançada pelo PowerShell do teste ela não veio (quem lança não tinha o primeiro
  plano); pelo atalho, clicado pelo Alex, deve vir. Falta o Alex conferir.
- **Bug achado pelo Alex (corrigido):** minimizar e clicar no atalho duas vezes deixava a janela
  vazia, só com o medidor (fechar no X não tinha o problema: a janela é recriada). Causa no Slint
  1.18 com `renderer-software` no Windows: minimizada, a janela às vezes é desenhada inteira mesmo
  invisível (consome o "redesenhar tudo" que o Slint marca ao minimizar) e o softbuffer valida a
  janela toda; ao voltar, só o que mudou é redesenhado. Acontecia também restaurando pela barra
  (`ShowWindow`), mais raro. Contorno: no `Resized` de volta de minimizada, o app muda
  `repintar`, que suja um retângulo de fundo do tamanho da janela. Medido (release, `PrintWindow`,
  15 restaurações: 1 ou 2 pedidos com 0/300/2000 ms e pela barra): vazia em 12/15 sem o contorno, 0/15 com ele.

### Iniciar com o sistema (Fase 4, bloco 4, 2026-10-22)
- Configurações, cartão "Iniciar com o sistema" (chave) + "Só na bandeja" (chave, desabilitada com a
  1ª desligada). Fora do Windows o cartão some (`iniciar-disponivel`; macOS na Fase 5).
- Módulo `partida` (crate `app`): valor `mic-limpo` em `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
  = `"<exe atual>"` + ` --bandeja` se marcado; desligar apaga o valor. Sem administrador, sem crate
  novo (feature `Win32_System_Registry` do `windows`).
- **O registro é a fonte da verdade:** a tela lê dele ao abrir a janela (e depois de cada mudança).
  Desativado no Gerenciador de Tarefas (`Explorer\StartupApproved\Run`, 1º byte ímpar) conta como
  desligado; religar pelo app apaga essa marca. O `config.ini` guarda só a preferência
  `so_bandeja` (padrão `sim`), para desligar e religar sem perdê-la.
- `mic-limpo --bandeja`: só o ícone, sem janela. A cópia da partida é a 1ª; o atalho depois só traz
  a janela (instância única).
- Limitação: o caminho do exe é o de quem mexeu na chave por último (ex.: o de debug); a chave
  aparece ligada mesmo se o registro apontar para outro exe. Mexer na chave regrava com o atual.
- Conferido (2026-10-22, debug, cliques simulados + `reg query`): ligar grava com `--bandeja`;
  desmarcar "Só na bandeja" tira o argumento; desligar apaga; desativação simulada do Gerenciador
  (`03…`) → chave desligada ao reabrir, e religar limpa a marca; `so_bandeja` gravado no
  `config.ini`. Release, linha registrada (`"…\release\mic-limpo.exe" --bandeja`): só o ícone,
  nenhuma janela; abrir o exe de novo trouxe a janela no mesmo processo. +2 testes (24 passam).
  **Pendente (Alex):** ligar no app de release e reiniciar o PC de verdade.

### Visual vidro, fundo sólido (2026-10-23)
- `telas.slint` reescrito no layout do `b-vidro.html` (tabela acima). Tudo ligado ao motor como
  antes; conferido no app de release com cliques simulados: botão redondo (status "Sem filtro",
  ladrilhos apagados), − / + (config gravada), ladrilho Leve, pílula Baixa latência, lua (tema escuro,
  barra de título acompanhou), lista do microfone abre, Configurações (iniciar com o sistema liga e
  desliga no registro), Teste (gravar, ouvir, parar), Sobre, voltar; flyout pela bandeja (Forte
  troca e a janela acompanha, "Abrir ›").
- Onda: `Dados.onda` (40 `Barra { mic, limpo }`, a mais nova no fim). A cada tick do medidor (30 Hz)
  o app empurra o **pico do tick** (sem a queda do medidor) com `empurrar_onda` (`crates/ui/src/lib.rs`),
  que muda as linhas no lugar; só com a janela visível; recomeça vazia ao abrir a janela.
- Ladrilhos ficam sem clique com o cancelamento desligado (como o segmento aprovado em 2026-10-21;
  no mockup eles respondiam).

Medições (release, 12 threads, janelas de 20 s; % de 1 núcleo, entre parênteses do PC):

| Caso | RAM (working set) | Privada | CPU |
|---|---|---|---|
| App, Leve, mic em silêncio, Início (onda) | 25,0 MB | 6,2 MB | 2,7–3,1 % (0,22–0,25 %) |
| App, idem, Configurações (sem onda; mesmo timer) | 25,3 MB | 6,2 MB | 0,9–1,4 % (0,07–0,12 %) |
| Prévia antiga, Início com medidor a 30 Hz | 20,9 MB | 4,0 MB | 2,1–2,3 % (0,18–0,19 %) |
| Prévia nova, Início com onda a 30 Hz (11 janelas) | 20,4 MB | 4,3 MB | 2,9–4,2 %, ~3,7 % (0,24–0,35 %) |
| Prévias paradas (antiga / nova) | 20,1 / 20,4 MB | 4,0 / 4,4 MB | 0,00 % |

- **Hipótese fechada:** a onda custa ~+1,5 ponto de 1 núcleo (~0,13 % do PC) a mais que o medidor,
  sem tendência de subir em 3 min; RAM igual (+0,3 MB privada). Na janela do app antigo eram
  24,8 / 5,4 MB (bloco 3). Uma janela de 20 s da prévia nova deu 31,4 MB de working set e não se
  repetiu (privada igual). Se pesar: onda a 15 Hz ou menos barras (não medido).

### Desfoque: só no Mac (decidido 2026-10-23)
- Alex: no Windows o vidro não precisa de desfoque; fica o fundo sólido. Desfoque só no Mac (Fase 5).
- Achados do teste no Windows (código desfeito, não commitado; nada medido de CPU/RAM):
  - Não precisa de crate: o winit 0.30 do Slint tem `with_transparent(true)` +
    `with_system_backdrop(MainWindow/TransientWindow)` (= `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)`),
    aplicáveis no gancho `atributos`. O `renderer-software` do Slint 1.18 escreve alfa pré-multiplicado no
    softbuffer e limpa a área suja com o fundo da janela, então um `Window.background` transparente funciona.
  - Visto no app de release: Mica na janela (atributo 38 = 2), claro e escuro legíveis com o véu
    `#faf7f466` / `#28201c80`; flyout (Acrylic) não chegou a ser visto.
- `window-vibrancy` 0.8.1 (MIT/Apache, sem rede; Mac: `objc2*`): candidato para a Fase 5
  (`NSVisualEffectView`); avaliar lá, com CPU/RAM medidos.

### Pendências de UI
- O timer do medidor ainda acorda (sem redesenhar) com a janela minimizada.
- Pendências visuais: desfoque só no Mac (Fase 5). A barra de
  título acompanhou o tema forçado pela lua em 2026-10-23 (antes anotada como não seguindo; não
  revisto pelo Configurações). A lista aberta dos dispositivos é a do Fluent (cinza, marca na cor
  do Windows), fora do estilo vidro. Ícone próprio da janela e da bandeja: `crates/ui/ui/icone.svg` (VoiceRune, 2026-10-30: microfone
  rúnico em hexágono laranja). Derivados em `assets/icone/` (`voicerune.ico` 16–256, `voicerune.icns`
  16–1024, PNG 256/1024), gerados pelo `resvg` (CLI) + Pillow fora do projeto; o `.ico` ainda não está
  embutido no `.exe` (precisaria de recurso Win32 no build). Em 16–24 px o desenho embola: se
  incomodar na bandeja, fazer uma versão simplificada só para tamanhos pequenos.
- Armadilha resolvida: **nunca** pôr `Palette.color-scheme = ColorScheme.unknown`. Os widgets do
  Fluent e as nossas cores passam a divergir (texto branco em fundo branco: caixas de seleção
  "somem"). "Do sistema" volta para o esquema que o sistema tinha antes do 1º ajuste.
