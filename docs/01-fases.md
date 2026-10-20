# Fases

**Status atual:** Fases 0–3 concluídas (branches das fases 2 e 3 mescladas na `main`).
- Modos: **Leve** = RNNoise (padrão) · **Forte** = DeepFilterNet3 (`--modelo dfn`, recomendado com
  `--baixa-latencia`). UI em Slint aprovada (`crates/ui`, prévia `mic-limpo-ui-previa`, sem áudio).
- Latência com a IA ligada: Leve ~98 ms / Forte ~115 ms com `--baixa-latencia` (padrão ~130 /
  ~149 ms); Voicemeeter atual ~128 ms. Nova meta pendente (Alex decide; sugestão: não piorar em
  relação ao Voicemeeter).

**Próximo passo:** Fase 4 — app mínimo usável: ligar `crates/ui` ao motor (seletor Leve/Forte,
baixa latência, ganho de saída), bandeja, presets e config (ver "Para a Fase 4" em
`05-interface.md`). Pendências que não bloqueiam: teste com jogo aberto (baixa latência e Forte);
underrun único na partida com DFN; gate do mic antes do app (melhorias do Windows).

| Fase | Entrega | Critério de pronto | Status |
|---|---|---|---|
| 0. Fundação | Estrutura, docs, git, workspace Rust vazio | `cargo build` passa no Windows | concluída |
| 1. Protótipo do núcleo | Mic → IA → VB-Cable (Windows) | Funciona no Discord; CPU/RAM/latência medidas | concluída |
| 2. Comparação de modelos | RNNoise vs DeepFilterNet vs Voicemeeter atual | Tabela custo × qualidade; define modos Leve/Forte | concluída: Leve = RNNoise, Forte = DFN3 |
| 3. Mockup da UI | Tela principal + flyout da bandeja | Visual aprovado pelo Alex | concluída |
| 4. App mínimo usável | UI Slint + bandeja + presets + config | Uso diário no Windows | — |
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
