# Arquitetura

## Estrutura de pastas (planejada)

```
crates/
  core/      → DSP, IA, roteamento, config — 100% portável, sem I/O de SO
  app/       → binário; liga core + plataforma + UI
  platform/  → (futuro) windows/ (MMCSS, driver), macos/ (thread RT, plugin HAL, permissão mic)
  ui/        → (futuro) Slint, mesmo código, estilo por SO
```
Hoje existem só `core` e `app` (vazios).

## Princípios (decididos)
- **Thread de áudio em tempo real:** sem alocação, sem locks, sem I/O. Comunicação com o resto
  por ring buffer lock-free (`rtrb`) e parâmetros atômicos.
- Prioridade: MMCSS "Pro Audio" no Windows; thread time-constraint / audio workgroup no macOS.
- Sem polling nem timers em segundo plano: se nada mudou, nada acorda.
- `unsafe` só na camada de FFI/plataforma.

## Hipótese (decidir na fase 4)
- **Processo de áudio separado da UI:** a janela fecha de verdade e o processamento continua
  leve. IPC local (named pipe com ACL do usuário no Windows; socket Unix 0600 no macOS).
