# Fases

**Status atual:** Fase 4 em andamento (branch `claude/fase4-app`), blocos 1 a 3 de 5 concluídos;
bloco 4 começado (última configuração salva). Página **Teste** (gravar e ouvir A/B) feita.
- Modos: **Leve** = RNNoise (padrão) · **Forte** = DeepFilterNet3. `mic-limpo` abre a janela
  Slint ligada ao motor e põe um ícone na bandeja (clique = flyout; menu Abrir/Sair); fechar a
  janela deixa o app só na bandeja. A última configuração volta ao abrir
  (`%APPDATA%\mic-limpo\config.ini`). Sem janela nem bandeja: `mic-limpo-dev rodar`.
- Latência com a IA ligada: Leve ~98 ms / Forte ~115 ms com `--baixa-latencia` (padrão ~130 /
  ~149 ms); Voicemeeter atual ~128 ms. Nova meta pendente (Alex decide).

**Plano da Fase 4** (decidido: processo único; trocar Leve ↔ Forte recarrega o modelo):

| Bloco | O quê | Status |
|---|---|---|
| 1. Motor controlável | ganho e medidor no `core`; `Controlador` no app troca modo/dispositivo/backend sem reiniciar; avisos por canal | concluído |
| 2. UI ligada | `mic-limpo` abre a janela Slint com dados reais; `aplicar` fora da thread da janela; medidor só com a janela visível; reagir a aviso fatal | concluído |
| 3. Bandeja + flyout | `SystemTrayIcon` do Slint 1.18 (`Shell_NotifyIconW`, sem crate extra); fechar destrói o componente; flyout junto da bandeja | concluído |
| 4. Config + presets | arquivo em `%APPDATA%\mic-limpo`, presets, iniciar com o sistema | em andamento: config salva feita |
| 5. Uso diário | Alex usa alguns dias | — |

**Próximo passo:** Alex quer melhorias antes de seguir. A primeira (página Teste) está feita; no
teste ele achou o **Forte ruim (às vezes corta)** e cogita trocar o Forte por um modo
**Personalizado**. Causa achada e **corrigida** (dither no invólucro do DFN; ver `04-ia-ruido.md`):
quadros zerados 127 → 2, bordas de palavra cortadas 20 % → 4 %. Próximo: Alex reouve o Forte na
página Teste (exe de release já recompilado); se ainda picotar, medir underruns ao vivo
(`mic-limpo-dev rodar --modelo dfn`); depois decidir se o Personalizado ainda faz falta. Depois: resto do bloco 4 — presets, iniciar com o sistema (com "iniciar só na
bandeja") e instância única (o atalho abre um 2º app hoje). Antes: Alex clica no ícone de verdade, fala e
confere medidor, flyout, trocas e status. Pendências que não bloqueiam: CPU com medidor animado e
Forte com voz (Discord fechado); reabrir após aviso fatal (desplugar um dispositivo); listas de
dispositivos não atualizam ao plugar; teste com jogo aberto; underrun na partida do DFN; gate do
mic antes do app; corte audível da troca não medido ponta a ponta (hipótese ~0); timer do
medidor acorda com a janela minimizada.

| Fase | Entrega | Critério de pronto | Status |
|---|---|---|---|
| 0. Fundação | Estrutura, docs, git, workspace Rust vazio | `cargo build` passa no Windows | concluída |
| 1. Protótipo do núcleo | Mic → IA → VB-Cable (Windows) | Funciona no Discord; CPU/RAM/latência medidas | concluída |
| 2. Comparação de modelos | RNNoise vs DeepFilterNet vs Voicemeeter atual | Tabela custo × qualidade; define modos Leve/Forte | concluída: Leve = RNNoise, Forte = DFN3 |
| 3. Mockup da UI | Tela principal + flyout da bandeja | Visual aprovado pelo Alex | concluída |
| 4. App mínimo usável | UI Slint + bandeja + presets + config | Uso diário no Windows | em andamento (bloco 3/5) |
| 5. macOS | Mesmo app no M4 com BlackHole | Funciona nos dois SOs | — |
| 6. Dispositivo virtual próprio | Plugin HAL (Mac), depois driver Windows | Sem depender de VB-Cable/BlackHole | — |
| 7. Extras | Roteamento avançado, atalhos, auto-start, instalador | Conforme necessidade | — |

## Histórico
- 2026-10-20 — Fase 0 iniciada: pasta, docs, git e esqueleto do workspace criados.
  Rust ainda não instalado no Windows → build pendente.
- 2026-10-20 — Rust 1.99.1 (stable-msvc, via rustup) e VS Build Tools 2022 17.14 (workload C++)
  instalados via winget. `cargo build` passa → Fase 0 concluída.
- 2026-10-20 — Fase 1, bloco 1: protótipo CLI `mic-limpo` (cpal 0.18 → RNNoise/`nnnoiseless` →
  `rtrb` → CABLE Input) com `--listar`, `--bench`, `--sem-ia`, `--folga-ms`, `--segundos`.
  Teste de 8 s sem underruns; RNNoise ~0,4 % de um núcleo; RAM privada 2,1 MB; latência estimada
  75,8 ms (maior parte no buffer de saída do WASAPI). Medições em `03-audio.md` e `04-ia-ruido.md`.
- 2026-10-20 — Fase 1, bloco 2: `--medir-latencia` (cliques → CABLE Output), `--sondar`
  (WASAPI direto), `--buffer`, `--teto-ms` e feature `realtime`. Medido ~125 ms (IA off) /
  ~135 ms (RNNoise); VB-Cable ~55 ms; `cpal` não reduz o período abaixo de 10 ms. Sonda mostra
  IAudioClient3 com período ~3 ms no mic e no CABLE Input. Folga/teto menores e MMCSS sem ganho real.
- 2026-10-20 — Latência do Voicemeeter atual medida com `--comparar` (correlação, mesmo som):
  Voicemeeter ~128 ms × app com RNNoise ~130 ms. Latência do app equivale à de hoje.
- 2026-10-20 — Fase 1, bloco 3: teste no Discord pelo Alex (entrada = CABLE Output, Voicemeeter
  aberto em paralelo): teclado e TikTok no celular não audíveis com a IA. **Fase 1 concluída.**
- 2026-10-20 — Refatoração da arquitetura: motor e métricas foram para `core` (com 3 testes
  novos), backend `cpal` para o crate `audio`, ferramentas para `mic-limpo-dev`. App de 8 s igual
  a antes (0 underruns, DSP 0,4 %, atrasos iguais); bench igual. Ver `02-arquitetura.md`.
- 2026-10-20 — Validação da refatoração: `latencia` 124,1 / 116,7 ms, 0 underruns (igual a antes).
- 2026-10-20 — Backend WASAPI de baixa latência (`crates/audio/src/wasapi.rs`, `--baixa-latencia`):
  medido 83,9 / 88,1 ms (padrão 118,6 ms), 0 underruns, CPU 1,37 % × 1,17 %, RAM igual.
- 2026-10-20 — Fase 2, bloco 1 (branch `claude/fase2-modelos-3e5d58`): sonda isolada do DeepFilterNet3
  (libDF 0.5.6 + tract 0.19, git): compila no Rust 1.99; ~6,2 % de um núcleo (RNNoise 0,4 %),
  atraso algorítmico 30 ms (RNNoise 10 ms), RAM privada 20,5 MB, binário 25 MB (21,9 com LTO),
  nada de rede. Aloca por quadro → precisa de thread própria. Tabela em `04-ia-ruido.md`.
  **Próximo passo:** Alex decide se aceita o peso; se sim, integrar no `core` (trait de modelo,
  DFN em thread própria), `--modelo`, bench e `processar` para escuta A/B.
- 2026-10-20 — Fase 2, bloco 2: DFN3 integrado no `core` (feature `dfn`, thread própria
  `mic-limpo-ia`), `--modelo rnnoise|dfn` e `--limite-db N` no app e no `latencia`; `bench` compara
  os modelos; novos `mic-limpo-dev gravar` e `processar` (WAV por modelo + SI-SDR). 10 testes ok.
  Armadilha: limiares do `default_with_ch` (plugin LADSPA) deixavam o DFN pior que o RNNoise;
  com os do binário oficial o DFN ganha ~6 dB de SI-SDR num ruído e empata no outro, a 8 % de um
  núcleo. Modos propostos: Leve = RNNoise, Forte = DFN3 (tabelas em `04-ia-ruido.md`).
  **Próximo passo:** com o Discord fechado, teste em tempo real (`mic-limpo --modelo dfn`) e
  `mic-limpo-dev latencia --modelo dfn` (padrão e `--baixa-latencia`); Alex grava a própria voz
  com ruído (`gravar`) e escuta os WAVs do `processar` para decidir modos e limite de redução.
- 2026-10-20 — Fase 2, bloco 3 (Discord fechado): latência ponta a ponta com a IA ligada —
  Leve ~130 ms / Forte ~149 ms no padrão; ~98 / ~115 ms com `--baixa-latencia` (Voicemeeter
  ~128). Thread própria do DFN sem custo de latência mensurável. Tempo real com DFN: 0 perdas,
  1 underrun só na partida em ~metade das vezes (some com folga 15 ms). Resumo do app agora mostra
  o modelo. **Próximo passo:** Alex grava a voz com ruído (`mic-limpo-dev gravar voz.wav`), escuta
  os WAVs do `processar` e decide modos e limite; depois decidir se o Forte exige `--baixa-latencia`.
- 2026-10-20 — Fase 2, bloco 4: escuta A/B do Alex (voz + teclado + TikTok, 4 variantes):
  nenhuma diferença percebida; teclado audível em todas. Descoberto gate no mic antes do app
  (17 % de quadros com zeros digitais). **Próximo passo:** Alex decide os modos (ver
  `04-ia-ruido.md`) e, se quiser, refaz a gravação com as melhorias do mic desligadas.
- 2026-10-20 — **Fase 2 concluída.** Alex decidiu: Leve = RNNoise (padrão) e Forte = DFN3 no app
  (`--modelo dfn`, recomendado com `--baixa-latencia`); limite de redução desligado por padrão.
  Pendências que não bloqueiam: teste difícil do Forte com o mic sem gate (desligar melhorias do
  Windows); underrun único na partida com DFN. **Próximo passo:** mesclar a branch da fase 2 e
  seguir na fase 3/4 (UI com seletor Leve/Forte).
- 2026-10-20 — Fase 3: 3 direções em HTML; Alex escolheu a A (Fluent compacto) com barra lateral
  de ícones. Feita em Slint (`crates/ui`, prévia `mic-limpo-ui-previa`, dados estáticos):
  janela parada 20 MB (4 MB privada), 0 % CPU; com medidor a 30 Hz, 1,6 % de 1 núcleo; laço de
  eventos rodando em 30–106 ms; sem dependências de rede. Capturas em `docs/mockups/capturas/`.
  Falta: aprovação das capturas pelo Alex.
- 2026-10-20 — Fase 3: corrigidas caixas de seleção invisíveis com o tema do sistema (ver
  `05-interface.md`); ícone próprio da janela. **Visual aprovado pelo Alex → Fase 3 concluída.**
  Fase 4 parte de `crates/ui` (telas + global `Dados`) e das notas "Para a Fase 4" em `05-interface.md`.
- 2026-10-20 — Branches das fases 2 (`claude/fase2-modelos-3e5d58`) e 3
  (`claude/fase3-mockup-ui-32f66c`) mescladas na `main`.
- 2026-10-20 — Fase 4 iniciada (branch `claude/fase4-app`). Alex decidiu: processo único e
  recarregar o modelo na troca. Bloco 1: `core` ganhou ganho de saída (rampa, satura), pico da
  entrada e `tocando` (+2 testes; todos passam); `audio` manda avisos por canal e ignora o Xrun
  espúrio do 1º pacote; `app` ganhou o `Controlador` (troca sem corte) e comandos na CLI
  (l, f, b, g N, p). Medido: Leve→Forte ~0,26 s de bloqueio (DFN 0,17–0,19 s), antigo tocando
  até o novo chegar; 1 de 5 trocas teve 2 underruns (partida do DFN). Ver `03-audio.md`.
- 2026-10-20 — Fase 4, bloco 2: `mic-limpo` abre a janela Slint ligada ao `Controlador` (thread
  de controle + `invoke_from_event_loop`); CLI virou `mic-limpo-dev rodar`/`listar` (escolha do
  Alex); busca de dispositivo prefere o nome exato (+1 teste). Testado com cliques simulados:
  troca de mic, Leve/Forte, baixa latência (atraso do app ~93 → ~46 ms), troca que falha volta ao
  anterior. Medido: 24 MB / 5,4 MB privada no Leve, 48 / 18 MB no Forte, ~0,5–1 % de um núcleo
  com a entrada em silêncio; janela em ~0,2 s; fecha em ~30 ms. Ver `05-interface.md`.
- 2026-10-20 — Fase 4, bloco 3: ícone na bandeja com o `SystemTrayIcon` do Slint 1.18 (features
  `system-tray` + `unstable-winit-030`; `cargo tree` sem crate novo no Windows, `ksni` só no
  Linux). Clique abre o flyout junto da barra (cursor + área útil, cantos do DWM, fora da barra de
  tarefas), que fecha ao perder o foco; menu Abrir/Sair. Janela e flyout criados e destruídos sob
  demanda, com o estado no Rust; o timer do medidor só existe com um deles. Medido (release, Leve):
  só bandeja 24,0 MB / 4,5 MB privada e 0,31–0,47 % de um núcleo, com a thread da UI em 0 ciclos
  (janela: 24,8 / 5,4 MB, 40 Mciclos em 10 s); reabrir em 37–75 ms (debug). A medir: +3,4 MB de
  working set e +0,3–0,9 ponto de CPU depois de 10 abre/fecha. Ver `05-interface.md`.
- 2026-10-20 — Fase 4, bloco 3 (fim): 30 abre/fecha da janela sem vazamento — +3,5 MB de working
  set e ~+1 MB privada uma vez só (estabiliza), nenhuma janela oculta sobra, threads e handles
  estáveis; CPU por thread igual antes e depois (só os fluxos do cpal trabalham). Ver `05-interface.md`.
- 2026-10-20 — Atalho `mic-limpo` na área de trabalho apontando para o exe de release (fechar o
  app antes de compilar; ver `08-ambiente.md`).
- 2026-10-20 — Fase 4, bloco 4 (parte 1, pedido do Alex): a última configuração volta ao abrir.
  `%APPDATA%\mic-limpo\config.ini` em texto `chave = valor` (módulo `salvo`, sem dependência nova),
  gravado 0,5 s após cada mudança, ao fechar a janela e no Sair; dispositivo ausente volta ao
  padrão; janela fora da tela é ignorada. Testado de ponta a ponta em debug; +2 testes (14 passam).
- 2026-10-21 — Página **Teste** (pedido do Alex, opção "gravar e ouvir"): grava 5 s de original e
  limpo lado a lado (cópia no motor, ring buffer à parte) e toca na saída escolhida, alternando
  Original ↔ Limpo na mesma posição. Testado com cliques simulados e pelo Alex ("ficou bom");
  +5 testes (21 passam). Medido: +3,7 MB privada depois de gravar (realocação corrigida, não
  medido de novo); +1 thread após tocar (não verificado se acumula). Alex: Forte "às vezes
  corta"; cogita trocar por Personalizado. Ver `05-interface.md`.
- 2026-10-21 — Corte do Forte diagnosticado com a gravação do Alex: a libDF zera quadros abaixo de
  −70 dBFS (27 % do mic dele), comendo ~30 ms do fim das palavras; o limite de redução não age
  ali. Correção proposta, não feita. Ver `04-ia-ruido.md`.
- 2026-10-21 — Corte do Forte corrigido: dither só nos quadros abaixo de −70 dBFS antes da libDF.
  Com a gravação do Alex: quadros zerados 127 → 2; bordas de palavra > 10 dB 20 % → 4 %; resíduo
  no silêncio ≤ −85 dBFS; DFN ~0,59 → ~0,74 ms por quadro. +1 teste (17 no core).
- 2026-10-21 — Mic do Alex ajustado no `mmsys.cpl` (aperfeiçoamentos desligados, +20 dB, 48 kHz):
  voz −42 → −30 dBFS, gate sumiu (zeros 11 % → 0 %), sem saturação. Ver `08-ambiente.md`.

