# Fases

**Status atual:** Fase 1 em andamento — bloco 1 (pipeline funcionando) concluído.
**Próximo passo:** bloco 2 — reduzir e medir a latência (hoje ~76 ms estimados, meta < 20 ms):
buffer de saída menor (`BufferSize::Fixed` / período mínimo do WASAPI), folga menor, MMCSS
(feature `realtime` do cpal) e medição ponta a ponta por loopback (CABLE Output). Depois, bloco 3:
Alex testa no Discord (microfone = `CABLE Output`). Ver `03-audio.md`.

| Fase | Entrega | Critério de pronto | Status |
|---|---|---|---|
| 0. Fundação | Estrutura, docs, git, workspace Rust vazio | `cargo build` passa no Windows | concluída |
| 1. Protótipo do núcleo | Mic → IA → VB-Cable (Windows) | Funciona no Discord; CPU/RAM/latência medidas | em andamento |
| 2. Comparação de modelos | RNNoise vs DeepFilterNet vs Voicemeeter atual | Tabela custo × qualidade; define modos Leve/Forte | — |
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
