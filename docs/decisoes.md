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
| 2026-10-20 | Visual: direção A (cartões estilo Configurações do Windows 11) com barra lateral de ícones (Início, Efeitos apagado, Configurações, Sobre) | Escolha do Alex entre 3 direções; a lateral deixa crescer sem poluir a tela inicial |
| 2026-10-20 | Slint sem features padrão: `backend-winit` + `renderer-software` (sem OpenGL), estilo Fluent, `.slint` compilado no build | Leveza medida: 20 MB working set, 0 % CPU parado, laço rodando em ~0,1 s |
| 2026-10-20 | Cor de destaque segue a do sistema | Comportamento nativo do Windows 11 |
| 2026-10-20 | Slint sob a licença royalty-free (selo `AboutSlint` na página Sobre) | Permite manter o app MIT sem GPL |
