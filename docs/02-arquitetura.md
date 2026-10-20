# Arquitetura

## Estrutura (decidida em 2026-10-20)

```
crates/
  core/         → IA (denoise), motor (quadros, ring buffer, deriva) e métricas. Rust puro:
                  sem SO, sem dispositivo, sem rede. Testável sem microfone.
  audio/        → backends de áudio atrás de callbacks simples. Hoje: cpal (WASAPI compartilhado
                  no Windows, CoreAudio no macOS). Único crate do produto que fala com API de áudio.
  app/          → `mic-limpo`, o programa do usuário: escolhe backend e liga ao motor.
                  Hoje CLI; interface (Slint) e bandeja na fase 4.
  ferramentas/  → `mic-limpo-dev`, só para desenvolvimento: bench, latencia, comparar, sondar.
                  Não vai para o usuário final.
  ui/           → (futuro, fase 4) Slint, mesmo código, estilo por SO
```
Dependências: `app` → `core` + `audio`; `ferramentas` → `core` + `audio` (+ `windows` para a
sonda). `core` e `audio` não dependem um do outro: o app converte `audio::Pacote` em
`motor::Tempos`.

## Contrato do motor
- `motor::criar(Config)` devolve `(Arc<Controle>, Entrada, Saida)`.
- O backend chama, na thread de áudio, `Entrada::processar(dados, canais, tempos)` e
  `Saida::preencher(dados, canais, tempos)`. As duas não alocam nem travam.
- `Controle` (atômico) liga/desliga a IA e expõe as `Metricas` para quem quiser mostrar.
- Ferramentas que precisam mexer no sinal (ex.: injetar cliques) fazem isso **antes** de chamar
  o motor, sem tocar nele.

## Como estender sem complicar
- **Backend novo** (WASAPI baixa latência, CoreAudio direto): um módulo em `audio/` que entrega
  `Pacote`s. Motor e app não mudam, fora a escolha do backend.
- **Modelo novo** (DeepFilterNet): em `core/`, atrás da mesma interface do `Denoiser`.
- **Ferramenta nova de medição**: em `ferramentas/`, nunca no `app`.

## Princípios (decididos)
- **Thread de áudio em tempo real:** sem alocação, sem locks, sem I/O. Comunicação com o resto
  por ring buffer lock-free (`rtrb`) e parâmetros atômicos.
- Prioridade: MMCSS "Pro Audio" no Windows; thread time-constraint / audio workgroup no macOS.
- Sem polling nem timers em segundo plano: se nada mudou, nada acorda.
- `unsafe` só em `audio/` (FFI) e na sonda de `ferramentas/`.
- Versões das dependências centralizadas em `[workspace.dependencies]`.

## Pendências conhecidas
- `audio` imprime erros de stream com `eprintln!`; trocar por canal de erro quando houver UI.
- Plugin HAL (macOS) e driver (Windows) da fase 6 ficarão em crates próprios.

## Hipótese (decidir na fase 4)
- **Processo de áudio separado da UI:** a janela fecha de verdade e o processamento continua
  leve. IPC local (named pipe com ACL do usuário no Windows; socket Unix 0600 no macOS).
