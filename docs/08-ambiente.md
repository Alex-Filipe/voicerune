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

## Repositório
- GitHub (público): https://github.com/Alex-Filipe/voicerune — remoto `origin`, branch `main`.

## Build
- `target/` fora do OneDrive: `C:\dev\cargo-target\mic-limpo` (configurado em `.cargo/config.toml`; ficou com o nome antigo para não recompilar as dependências).
- `cargo build` na raiz.

## macOS — status em 2026-11-07
- MacBook M4, macOS 26.6.2, `aarch64-apple-darwin`.
- Xcode Command Line Tools ✅ (Apple clang 21) · Homebrew ✅ · `gh` logado ✅.
- Rust 1.99.1 / cargo 1.99.1 ✅ — via instalador oficial do rustup (`sh.rustup.rs`), toolchain `stable`.
- Repositório em `~/dev/voicerune` (fora do iCloud).
- **`target-dir`:** o `.cargo/config.toml` fixa `C:/dev/cargo-target/mic-limpo`; no Mac isso criaria uma pasta
  `C:` dentro do repositório. Usar sempre `CARGO_TARGET_DIR` (tem prioridade sobre o arquivo):
  `export CARGO_TARGET_DIR="$HOME/dev/cargo-target/voicerune"` (pôr no `~/.zshrc` para não esquecer).
- `cargo build --workspace` passa sem mudar código (~63 s no 1º build em debug; 1 aviso: `MARGEM_FLYOUT`
  sem uso fora do Windows). `cargo test --workspace`: 25 passam, 1 ignorado (DFN em tempo real, só em `--release`).
- Só no Windows (no Mac compila a versão vazia ou devolve erro claro): backend de baixa latência (WASAPI) e
  `sondar`, iniciar com o sistema (registro), instância única, posição do flyout, "CPU do app" (mostra "—").
- **Config não é salva no Mac:** `salvo.rs` usa `%APPDATA%`, que não existe lá. A fazer na Fase 5
  (`~/Library/Application Support/VoiceRune`).
- App ainda **não aberto** no Mac (janela, bandeja, permissão de microfone): nada testado em tempo real.

## Rodar
- `cargo build --release`; binários em `C:\dev\cargo-target\mic-limpo\release\`:
  `voicerune.exe` (app) e `voicerune-dev.exe` (ferramentas; até 2026-10-30 `mic-limpo*.exe`, que ficaram velhos na pasta). `cargo test --workspace` roda os testes; o teste de tempo real do DFN só roda em
  `cargo test --workspace --release` (em debug o modelo é lento demais e fica ignorado).
- Atalho na área de trabalho (`VoiceRune.lnk`; era `mic-limpo.lnk`, criado em 2026-10-20, renomeado e
  reapontado em 2026-10-30) aponta direto para o `releaseoicerune.exe`: sempre a versão compilada mais nova. Ícone do atalho explícito (`voicerune.exe,0`): com `,0` o cache do Windows ficou no ícone genérico. **Fechar o app (Sair na bandeja)
  antes de `cargo build --release`**, senão o build falha (exe em uso). Ícone embutido desde 2026-10-30.
- Configuração do app: `%APPDATA%\VoiceRune\config.ini` (texto; apagar volta aos padrões; sem ele o app lê o antigo `%APPDATA%\mic-limpo\config.ini`).
- App: `voicerune` abre a janela (em release, sem console). Sem janela, pelas ferramentas:
  `voicerune-dev listar` · `voicerune-dev rodar --entrada Microphone` (Enter liga/desliga a IA;
  `l`/`f` Leve/Forte, `b` baixa latência, `g N` ganho, `p` pausa, `q` encerra).
  Modo baixa latência: acrescentar `--baixa-latencia` (Windows).
- Ferramentas (`voicerune-dev --ajuda`):
  - `bench` — custo do RNNoise, offline.
  - `latencia` — cliques → CABLE Output; **escreve no CABLE Input: conferir que o Discord não
    está usando o CABLE Output numa call**.
  - `comparar --alvo "Voicemeeter Out B1,CABLE Output"` — só lê, mas precisa de voz.
  - `sondar` — capacidades do WASAPI.
  - `icone` — regera `assets/icone/voicerune.ico` dos SVGs (embutido no exe pelo `build.rs` do app).
- MMCSS: `cargo build --release --features realtime`.
- **Atenção:** a entrada padrão do Windows hoje é `Voicemeeter Out B1`, então passar `--entrada Microphone`.
- No Discord: microfone = `CABLE Output (VB-Audio Virtual Cable)`.
- Saída redirecionada no PowerShell mostra acentos quebrados (UTF-8 lido como ANSI); só visual.

## Mic do Alex no Windows (ajustado em 2026-10-21)
`mmsys.cpl` → Gravação → Microphone (High Definition Audio Device) (a tela nova de Configurações
fica bloqueada sem ativação do Windows): Níveis 100 + aumento +20 dB; **aperfeiçoamentos de áudio
desligados** (eram o gate); formato 48000 Hz (estava 44100). Medido com `gravar` (10 s de fala):

| | Voz (p90) | Pico | Quadros zero (gate) | Quadros < −70 dBFS | Piso (p10) |
|---|---|---|---|---|---|
| antes | −42,0 dBFS | −24,9 | 11 % | 27 % | zero digital |
| depois | −29,6 dBFS | −14,4 | 0 % | 0 % | −57,1 dBFS |

Sem saturação. O piso subiu com o ganho (esperado; é o ruído que a IA tira).

