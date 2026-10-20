# Ambiente

## Máquinas

| Máquina | Uso | Observações |
|---|---|---|
| Windows 11, GTX 1650 4 GB | Desenvolvimento principal; teste de pior caso (jogos) | IA só na CPU |
| MacBook M4 (Apple Silicon) | Build/teste macOS | Alvo `aarch64-apple-darwin` |

## Toolchain Windows — status em 2026-10-20
- git 2.41 ✅
- Rust 1.99.1 / cargo 1.99.1 ✅ — via `winget install Rustlang.Rustup`, toolchain `stable-x86_64-pc-windows-msvc`.
- VS Build Tools 2022 17.14 ✅ — via winget, workload `Microsoft.VisualStudio.Workload.VCTools` (linker MSVC).
- `cargo build` do workspace passa ✅

## Build
- `target/` fora do OneDrive: `C:\dev\cargo-target\mic-limpo` (configurado em `.cargo/config.toml`).
- `cargo build` na raiz.

## macOS
- Compilar lá (Claude Code no Mac) ou via CI no GitHub Actions (quando o repo existir).

## Rodar
- `cargo build --release`; binários em `C:\dev\cargo-target\mic-limpo\release\`:
  `mic-limpo.exe` (app) e `mic-limpo-dev.exe` (ferramentas). `cargo test --workspace` roda os testes; o teste de tempo real do DFN só roda em
  `cargo test --workspace --release` (em debug o modelo é lento demais e fica ignorado).
- App: `mic-limpo --listar` · `mic-limpo --entrada Microphone` (Enter liga/desliga a IA, `q` encerra).
  Modo baixa latência: acrescentar `--baixa-latencia` (Windows).
- Ferramentas (`mic-limpo-dev --ajuda`):
  - `bench` — custo do RNNoise, offline.
  - `latencia` — cliques → CABLE Output; **escreve no CABLE Input: conferir que o Discord não
    está usando o CABLE Output numa call**.
  - `comparar --alvo "Voicemeeter Out B1,CABLE Output"` — só lê, mas precisa de voz.
  - `sondar` — capacidades do WASAPI.
- MMCSS: `cargo build --release --features realtime`.
- **Atenção:** a entrada padrão do Windows hoje é `Voicemeeter Out B1`, então passar `--entrada Microphone`.
- No Discord: microfone = `CABLE Output (VB-Audio Virtual Cable)`.
- Saída redirecionada no PowerShell mostra acentos quebrados (UTF-8 lido como ANSI); só visual.
