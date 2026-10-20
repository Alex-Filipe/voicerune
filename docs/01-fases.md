# Fases

**Status atual:** Fase 0 em andamento.
**Próximo passo:** instalar a toolchain Rust no Windows (ver `08-ambiente.md`) e compilar o workspace vazio.

| Fase | Entrega | Critério de pronto | Status |
|---|---|---|---|
| 0. Fundação | Estrutura, docs, git, workspace Rust vazio | `cargo build` passa no Windows | em andamento |
| 1. Protótipo do núcleo | Mic → IA → VB-Cable (Windows) | Funciona no Discord; CPU/RAM/latência medidas | — |
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
