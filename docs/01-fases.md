# Fases

**Status atual:** Fase 1 concluída — funciona no Discord; CPU/RAM/latência medidas.
Latência: app com RNNoise **~130 ms** ≈ Voicemeeter atual **~128 ms**. Meta antiga de < 20 ms é
inalcançável; **nova meta pendente (Alex decide; sugestão: não piorar em relação ao Voicemeeter)**.
**Próximo passo:** iniciar a Fase 2 (RNNoise × DeepFilterNet: custo, qualidade e latência,
incluindo teste de escuta gravado) e/ou a Fase 3 (mockup da UI) — podem andar em paralelo.
Modo `--baixa-latencia` pronto (WASAPI + IAudioClient3): ~85 ms em vez de ~119 ms, +0,2 ponto
de CPU. **Falta:** Alex testar com jogo aberto (estalos? CPU?) para decidir se vira o padrão.

| Fase | Entrega | Critério de pronto | Status |
|---|---|---|---|
| 0. Fundação | Estrutura, docs, git, workspace Rust vazio | `cargo build` passa no Windows | concluída |
| 1. Protótipo do núcleo | Mic → IA → VB-Cable (Windows) | Funciona no Discord; CPU/RAM/latência medidas | concluída |
| 2. Comparação de modelos | RNNoise vs DeepFilterNet vs Voicemeeter atual | Tabela custo × qualidade; define modos Leve/Forte | em andamento (bloco 1: custo do DFN3 medido) |
| 3. Mockup da UI | Tela principal + flyout da bandeja | Visual aprovado pelo Alex | — |
| 4. App mínimo usável | UI Slint + bandeja + presets + config | Uso diário no Windows | — |
| 5. macOS | Mesmo app no M4 com BlackHole | Funciona nos dois SOs | — |
| 6. Dispositivo virtual próprio | Plugin HAL (Mac), depois driver Windows | Sem depender de VB-Cable/BlackHole | — |
| 7. Extras | Roteamento avançado, atalhos, auto-start, instalador | Conforme necessidade | — |

Fases 2 e 3 podem andar em paralelo.

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
