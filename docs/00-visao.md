# Visão

## Objetivo
Substituir o uso do Voicemeeter Banana (hoje usado só para cancelamento de ruído) por um app
próprio, open source, escrito do zero em Rust, com cancelamento de ruído por IA.

## Requisitos (decididos)

| Requisito | Significado prático |
|---|---|
| **Simples** | Tela principal: mic → cancelamento (Desligado/Leve/Forte) → saída. Avançado escondido. Sem jargão. |
| **Leve** | Roda o tempo todo (jogos, trabalho). Processo de áudio pequeno; janela fecha de verdade; IA só na CPU. |
| **Bonito e moderno** | Visual Windows 11 / macOS, tema claro/escuro do sistema, animações sutis. |
| **Multiplataforma** | Windows 11 e macOS (Apple Silicon M4). |
| **Privado e seguro** | Sem rede, sem telemetria, config local. |
| **Open source** | Licença MIT. |

## Fora do escopo (por enquanto)
- Mixer completo estilo Voicemeeter (vários buses, ASIO etc.) — talvez na fase 7.
- Suporte a Mac Intel.
