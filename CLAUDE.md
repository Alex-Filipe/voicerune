# mic-limpo (nome provisório)

App open source em Rust para **cancelamento de ruído do microfone com IA**, inspirado no
Voicemeeter Banana, mas **simples, leve, bonito e multiplataforma (Windows + macOS)**.

## Regras do projeto

- Ler **só o documento do tema** necessário (índice abaixo), não todos.
- Ao terminar algo que muda o estado do projeto, atualizar o doc do tema + `docs/01-fases.md`
  e registrar decisões em `docs/decisoes.md` (data, decisão, motivo).
- Separar sempre **decidido** de **hipótese a medir**. Nada é "testado" sem medição registrada.
- Leveza é requisito: medir CPU/RAM/latência antes de aceitar uma dependência pesada.
- Privacidade: **zero rede e zero telemetria** no app. Nenhuma dependência que faça rede.
- `target/` fica **fora do OneDrive** (`C:\dev\cargo-target\mic-limpo`, ver `.cargo/config.toml`).
- Commits pequenos e frequentes; mensagens em português.
- Sem engenharia reversa do binário do Voicemeeter (EULA): estudar só comportamento,
  manual e API pública (clean-room).

## Fluxo de sessões (economia de tokens)

- **Sessões curtas, uma tarefa por sessão.** Ao concluir a tarefa, sugerir abrir sessão nova.
- **Início de sessão:** ler este arquivo + `docs/01-fases.md` (status e próximo passo); o resto só sob demanda.
- **Fim de sessão:** atualizar `docs/01-fases.md` (status, histórico, **próximo passo** claro)
  e commitar — a próxima sessão retoma só por ele, sem depender do histórico da conversa.
- Esforço de raciocínio conforme a tarefa; **nunca usar o nível "max"**.

## Índice

| Doc | Conteúdo |
|---|---|
| [docs/00-visao.md](docs/00-visao.md) | Objetivo, requisitos, fora do escopo |
| [docs/01-fases.md](docs/01-fases.md) | Fases, status atual, próximo passo |
| [docs/02-arquitetura.md](docs/02-arquitetura.md) | Crates, threads, IPC, estrutura de pastas |
| [docs/03-audio.md](docs/03-audio.md) | Captura/saída, dispositivos virtuais, latência |
| [docs/04-ia-ruido.md](docs/04-ia-ruido.md) | Modelos de cancelamento (RNNoise, DeepFilterNet) |
| [docs/05-interface.md](docs/05-interface.md) | Framework de UI, princípios de UX, telas |
| [docs/06-seguranca.md](docs/06-seguranca.md) | Privacidade, licenças, assinatura |
| [docs/07-referencias.md](docs/07-referencias.md) | Voicemeeter, REA, links |
| [docs/08-ambiente.md](docs/08-ambiente.md) | Máquinas, toolchain, como compilar |
| [docs/decisoes.md](docs/decisoes.md) | Log de decisões |
