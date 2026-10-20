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

## Rodar o protótipo (fase 1)
- `cargo build --release`; binário em `C:\dev\cargo-target\mic-limpo\release\mic-limpo.exe`.
- `mic-limpo --listar` · `mic-limpo --bench` · `mic-limpo --entrada Microphone` (Enter liga/desliga a IA, `q` encerra).
- **Atenção:** a entrada padrão do Windows hoje é `Voicemeeter Out B1`, então passar `--entrada Microphone`.
- No Discord: microfone = `CABLE Output (VB-Audio Virtual Cable)`.
- Saída redirecionada no PowerShell mostra acentos quebrados (UTF-8 lido como ANSI); só visual.
