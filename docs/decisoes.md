# Log de decisões

| Data | Decisão | Motivo |
|---|---|---|
| 2026-10-20 | Reescrever do zero em Rust, sem RE do binário do Voicemeeter | EULA; comportamento e manual bastam |
| 2026-10-20 | REA não será usado por padrão | Pouco ganho; Ghidra no Windows é experimental |
| 2026-10-20 | Requisitos: simples, leve, bonito, Win + Mac, privado | Uso o dia todo (jogos/trabalho); Voicemeeter é poluído |
| 2026-10-20 | IA só na CPU; candidatos RNNoise e DeepFilterNet | GPU livre para jogos; GTX 1650 sem NVIDIA Broadcast |
| 2026-10-20 | UI em Slint | Leve, moderno, multiplataforma |
| 2026-10-20 | Protótipo usa VB-Cable (Win) / BlackHole (Mac); dispositivo próprio só na fase 6 | Driver próprio é a parte mais difícil |
| 2026-10-20 | Licença MIT | Open source e permissiva (pode mudar antes do 1º release) |
| 2026-10-20 | Projeto em Documentos (OneDrive), `target/` fora do OneDrive | Regra: coisas pesadas fora do OneDrive |
| 2026-10-20 | Nome provisório `mic-limpo` | Nome definitivo depois |
| 2026-10-20 | Protótipo da fase 1 usa RNNoise (`nnnoiseless`, sem features padrão); DeepFilterNet só na fase 2 | Rust puro, sem modelo externo; menor custo para validar o pipeline |
| 2026-10-20 | IA roda na thread de captura; entrada e saída ligadas por `rtrb` | Uma cópia a menos; RNNoise cabe folgado no callback (0,04 ms de 10 ms) |
| 2026-10-20 | Deriva de relógio tratada provisoriamente com descarte/silêncio | Simples para o protótipo; reamostragem adaptativa fica para depois |
| 2026-10-20 | CLI do protótipo sem `clap` (parse manual) | Leveza; poucas opções |
| 2026-10-20 | Folga 10 ms / teto 20 ms continuam como padrão | Valores menores ganham ≤ 8 ms e causam underruns/estalos |
| 2026-10-20 | MMCSS (`realtime`) fica como feature opcional, desligada | Sem ganho medido com o sistema ocioso; reavaliar com jogo aberto |
| 2026-10-20 | Meta de < 20 ms descartada como inalcançável; nova meta pendente (Alex decide) | RNNoise sozinho impõe ~20 ms e o VB-Cable ~55 ms; medido ~135 ms ponta a ponta |
| 2026-10-20 | Método de comparação de latência: correlação cruzada mic direto × saída de cada caminho, gravados juntos (`--comparar`) | Não precisa injetar sinal no mic; mede app e Voicemeeter com o mesmo som; validado contra os cliques |
| 2026-10-20 | Arquitetura: `core` (IA + motor + métricas), `audio` (backends), `app` (usuário), `ferramentas` (`mic-limpo-dev`) | Motor independente de backend (WASAPI/macOS/UI reaproveitam), testável sem dispositivo; ferramentas fora do app final. Substitui o `platform/` planejado |
| 2026-10-20 | Backend WASAPI de baixa latência (IAudioClient3 + MMCSS) no crate `audio`, opcional (`--baixa-latencia`) | Medido: −30 a −35 ms, +0,2 ponto de CPU, 0 underruns; padrão continua cpal até testar com jogo |
| 2026-10-20 | DeepFilterNet3 via libDF 0.5.6 (git, commit `978576a`) atrás da feature `dfn` do `core` (ligada por padrão no app e nas ferramentas) | Não está no crates.io; feature mantém o build só-RNNoise com 1,6 MB (com DFN: ~26 MB); medido 8 % de um núcleo, 20 MB de RAM, nada de rede |
| 2026-10-20 | DFN roda numa thread própria (`mic-limpo-ia`) ligada por `rtrb` + `unpark`; RNNoise segue na thread de áudio | DFN aloca por quadro e não é `Send`; contrato `processar`/`preencher` intacto |
| 2026-10-20 | Limiares do DFN = os do binário oficial (−15/35/35 dB), não os do `default_with_ch` | Com os do plugin LADSPA o DFN ficou pior que o RNNoise (SI-SDR 4,7 × 16,4 dB); com os oficiais, 16,6 dB e voz limpa 32,8 dB |
| 2026-10-20 | Integridade do modelo DFN pelo commit fixado no `Cargo.lock`, sem copiar o `.tar.gz` para o repo | Evita 8 MB no git/OneDrive; o hash do commit cobre o conteúdo |
