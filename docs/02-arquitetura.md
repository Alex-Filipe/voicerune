# Arquitetura

## Estrutura (decidida em 2026-10-20)

```
crates/
  core/         → IA (`modelo`: RNNoise e, com a feature `dfn`, DeepFilterNet3; `denoise`:
                  acumula quadros de 10 ms), motor (quadros, ring buffer, deriva) e métricas.
                  Rust puro: sem SO, sem dispositivo, sem rede. Testável sem microfone.
  audio/        → backends de áudio atrás de callbacks simples: cpal (padrão; WASAPI compartilhado
                  no Windows, CoreAudio no macOS) e `wasapi` (baixa latência, só Windows).
                  Único crate do produto que fala com API de áudio. `Pedido.backend` escolhe.
  app/          → `mic-limpo`, o programa do usuário. `controlador`: ajustes do usuário →
                  motor + fluxos (troca sem corte). Hoje CLI; interface (Slint) e bandeja na fase 4.
  ferramentas/  → `mic-limpo-dev`, só para desenvolvimento: bench, latencia, comparar, sondar.
                  Não vai para o usuário final.
  ui/           → `mic-limpo-ui`: telas em Slint (janela principal + flyout), compiladas no build;
                  dados pelo global `Dados`. Binário `mic-limpo-ui-previa` com dados estáticos.
                  A fase 4 liga ao motor.
```
Dependências: `app` → `core` + `audio`; `ferramentas` → `core` + `audio` (+ `windows` para a
sonda). `core` e `audio` não dependem um do outro: o app converte `audio::Pacote` em
`motor::Tempos`.

## Contrato do motor
- `motor::criar(Config)` devolve `(Arc<Controle>, Entrada, Saida)`.
- O backend chama, na thread de áudio, `Entrada::processar(dados, canais, tempos)` e
  `Saida::preencher(dados, canais, tempos)`. As duas não alocam nem travam.
- `Controle` (atômico) liga/desliga a IA, muda o ganho de saída (rampa de um pacote, satura em
  ±1), dá o pico da entrada para o medidor (`tomar_pico`, antes da IA), avisa quando a saída
  começou a tocar (`tocando`) e expõe as `Metricas` (`cpu_pct`, `latencia_ms`, `falhas`).
- `Config.modelo` (`rnnoise` | `dfn`) e `Config.limite_db` escolhem a IA. `criar` devolve
  `Result` (carregar o DFN leva ~0,2 s e pode falhar).
- Modelo que aloca ou não é `Send` (DFN: tract usa `Rc`) roda na thread própria
  `mic-limpo-ia`, criada por `criar`, que carrega o modelo nela: `processar` só empurra as
  amostras num `rtrb` e chama `unpark` (sem lock nem alocação); a thread processa e empurra
  direto no ring buffer da saída. RNNoise continua na thread de áudio. O contrato não muda.
- Ferramentas que precisam mexer no sinal (ex.: injetar cliques) fazem isso **antes** de chamar
  o motor, sem tocar nele.

## Como estender sem complicar
- **Backend novo** (WASAPI baixa latência, CoreAudio direto): um módulo em `audio/` que entrega
  `Pacote`s. Motor e app não mudam, fora a escolha do backend.
- **Modelo novo**: implementar `modelo::Modelo` (quadro de 480 → 480) em `core/`, um valor em
  `modelo::Tipo` (atraso, se precisa de thread própria) e, se pesado, atrás de uma feature.
- **Ferramenta nova de medição**: em `ferramentas/`, nunca no `app`.

## Princípios (decididos)
- **Thread de áudio em tempo real:** sem alocação, sem locks, sem I/O. Comunicação com o resto
  por ring buffer lock-free (`rtrb`) e parâmetros atômicos.
- Prioridade: MMCSS "Pro Audio" no Windows; thread time-constraint / audio workgroup no macOS.
- Sem polling nem timers em segundo plano: se nada mudou, nada acorda.
- `unsafe` só em `audio/` (FFI) e na sonda de `ferramentas/`.
- Versões das dependências centralizadas em `[workspace.dependencies]`.

## Pendências conhecidas
- (resolvido) erros de stream vão por canal (`Pedido.avisos`), não mais `eprintln!`.
- Plugin HAL (macOS) e driver (Windows) da fase 6 ficarão em crates próprios.

## Processo (decidido na fase 4)
- **Processo único** para áudio e UI. Fechar a janela destrói o componente Slint e fica só a
  bandeja; medir a RAM nesse estado. Processo separado (IPC local) só se a medição pedir.
