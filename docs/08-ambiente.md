# Ambiente

## Máquinas

| Máquina | Uso | Observações |
|---|---|---|
| Windows 11, GTX 1650 4 GB | Desenvolvimento principal; teste de pior caso (jogos) | IA só na CPU |
| MacBook M4 (Apple Silicon) | Build/teste macOS | Alvo `aarch64-apple-darwin` |

## Toolchain Windows — status em 2026-10-20
- git 2.41 ✅
- **Rust: não instalado** ❌ → instalar `rustup` (https://rustup.rs)
- **MSVC Build Tools: não encontrados** ❌ → o Rust no Windows precisa do linker MSVC
  (Visual Studio Build Tools 2022, workload "Desenvolvimento para desktop com C++").

## Build
- `target/` fora do OneDrive: `C:\dev\cargo-target\mic-limpo` (configurado em `.cargo/config.toml`).
- `cargo build` na raiz.

## macOS
- Compilar lá (Claude Code no Mac) ou via CI no GitHub Actions (quando o repo existir).
