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
| 2026-10-20 | Underrun único na partida com DFN (folga 10 ms) aceito como limitação conhecida, sem aumentar a folga padrão | 10 ms de silêncio só no início; folga 15 ms resolveria mas custa 5 ms sempre; aquecer o modelo não ajudou (medido) |
| 2026-10-20 | Modos: Leve = RNNoise (padrão), Forte = DeepFilterNet3 no app, recomendado com `--baixa-latencia`; limite de redução desligado por padrão | Escolha do Alex. Forte: +6 dB de SI-SDR num dos dois ruídos públicos, ~8 % de um núcleo, +20 ms, app ~26 MB; na escuta do Alex sem diferença percebida (mic com gate) |
| 2026-10-20 | Visual: direção A (cartões estilo Configurações do Windows 11) com barra lateral de ícones (Início, Efeitos apagado, Configurações, Sobre) | Escolha do Alex entre 3 direções; a lateral deixa crescer sem poluir a tela inicial |
| 2026-10-20 | Slint sem features padrão: `backend-winit` + `renderer-software` (sem OpenGL), estilo Fluent, `.slint` compilado no build | Leveza medida: 20 MB working set, 0 % CPU parado, laço rodando em ~0,1 s |
| 2026-10-20 | Cor de destaque segue a do sistema | Comportamento nativo do Windows 11 |
| 2026-10-20 | Slint sob a licença royalty-free (selo `AboutSlint` na página Sobre) | Permite manter o app MIT sem GPL |
| 2026-10-20 | Visual da Fase 3 aprovado (capturas em `docs/mockups/capturas/`) | Aprovado pelo Alex após a correção das caixas de seleção |
| 2026-10-20 | Áudio e UI no mesmo processo; fechar a janela destrói o componente e fica a bandeja | Escolha do Alex: mais simples; separar com IPC só se a RAM medida pedir |
| 2026-10-20 | Trocar Leve ↔ Forte recarrega o modelo (só o modelo em uso na memória) | Escolha do Alex: evita +20 MB do DFN sempre carregado |
| 2026-10-20 | Troca de motor sem corte: o novo é montado com o antigo tocando e o antigo sai quando o novo chegou ao dispositivo | Medido: espera de 18–47 ms além da carga; antigo continua se o novo falhar |
| 2026-10-20 | Ganho de saída depois da IA, em rampa de um pacote e saturado em ±1; medidor = pico da entrada antes da IA | Sem estalo ao mexer no controle; medidor mostra se o mic está captando |
| 2026-10-20 | Ignorar Xrun do cpal antes do 1º pacote da entrada | Flag indefinida no 1º pacote; aviso espúrio confirmado com duas instâncias |
| 2026-10-20 | CLI do app vira `mic-limpo-dev rodar`/`listar`; `mic-limpo` é só a janela (sem console em release) | Escolha do Alex: app final limpo; o `Controlador` virou lib do app para os dois usarem |
| 2026-10-20 | UI ↔ motor: thread de controle dona do `Controlador`, pedidos por canal (junta rajadas), estado volta por `invoke_from_event_loop` | `aplicar` bloqueia até ~0,26 s com o DFN; sem polling fora do medidor |
| 2026-10-20 | Cada controle muda só o seu campo do pedido | O mic segue "o padrão do sistema" até o usuário escolher um |
| 2026-10-20 | Busca de dispositivo prefere o nome exato | O nome vem da lista da UI; "contém" podia cair num parecido |
| 2026-10-20 | Medidor de −60 a 0 dBFS com queda suave; "CPU" mostra só a IA; atraso rotulado "do app" | Honestidade: o atraso não inclui o VB-Cable (~55–60 ms) |
| 2026-10-20 | Bandeja pelo `SystemTrayIcon` do Slint 1.18 (feature `system-tray`), não o crate `tray-icon` | No Windows é `Shell_NotifyIconW` com o `windows` 0.62 que já usamos: nenhum crate novo (`ksni` só no Linux) |
| 2026-10-20 | Feature `unstable-winit-030` do Slint para o flyout | Dá o evento de foco, o `skip_taskbar` e os cantos do DWM sem crate novo (o winit já vinha); API "unstable" fixada pelo `Cargo.lock` |
| 2026-10-20 | Flyout posicionado pelo cursor + área útil do monitor (Win32), não pelo retângulo do ícone | O `clicked` do Slint não traz a posição do ícone; o cursor está no ícone no clique |
| 2026-10-20 | Janela e flyout criados sob demanda; o estado mostrado mora no Rust e preenche cada tela nova | Fechar destrói o componente (~1 MB privada a menos); reabrir custa ~40–75 ms |
| 2026-10-20 | O app abre com a janela; a bandeja fica sempre | "Iniciar minimizado" entra com "Iniciar com o sistema" (bloco 4) |
| 2026-10-20 | Última configuração em `%APPDATA%\mic-limpo\config.ini`, texto `chave = valor` feito à mão | Sem dependência nova (serde/toml); legível e editável; chave inválida fica no padrão |
| 2026-10-20 | Gravar 0,5 s depois da última mudança (e ao fechar a janela e no Sair), num temporário trocado de lugar | Arrastar o volume = uma gravação; arquivo nunca fica pela metade |
| 2026-10-20 | Dispositivo salvo ausente na partida volta ao padrão; janela salva fora de qualquer monitor é ignorada | O áudio sempre abre; a janela nunca nasce fora da tela |
| 2026-10-21 | Teste de escuta = gravar 5 s e ouvir com A/B, numa página própria (não retorno ao vivo) | Escolha do Alex: ouvir-se ao vivo com ~100 ms de atraso atrapalha falar; o A/B na mesma posição compara o efeito da IA |
| 2026-10-21 | A cópia para o teste sai do `Denoiser` (original e limpo do mesmo quadro), só com a escuta ligada | Alinhados sem conta extra (só o atraso fixo do modelo); thread de áudio não aloca nem trava |
| 2026-10-21 | O volume de saída vale para Original e Limpo na reprodução | Comparação justa: o mais alto não parece o melhor |
| 2026-10-21 | Contornar o atalho de −70 dBFS da libDF com dither no nosso invólucro, sem fork da libDF | ~10 linhas, sem dependência nova; resíduo ≤ −85 dBFS; custa ~25 % a mais de CPU do DFN |
| 2026-10-21 | "Depois da IA" no mesmo medidor (camada por cima), na Início e não só num Personalizado | Serve aos três modos e mostra se o cancelamento funciona; sem ocupar mais espaço |
| 2026-10-21 | O medidor do limpo lê antes do volume de saída | Senão subir o volume pareceria a IA tirando menos |
| 2026-10-21 | Cancelamento = chave no título do cartão + Leve / Forte / Personalizado (desabilitado), com "?" que explica; sem pausa na janela | Escolha do Alex: desligar o filtro não deve calar o mic no Discord (a chave antiga do flyout pausava tudo); o modo fica lembrado |
| 2026-10-21 | Cartão "CPU do app" = processo inteiro (IA + áudio + janela) em % do PC, não a IA em % de um núcleo | Bate com o Gerenciador de Tarefas e não assusta (Forte "11,7 %" de um núcleo ≈ 1 % do PC) |
| 2026-10-21 | Instância única por evento nomeado do Windows (`Local\mic-limpo-abrir`); a 2ª cópia sinaliza e sai antes de abrir áudio e janela | Sem crate novo nem IPC; o mesmo objeto detecta e avisa; por sessão do Windows (cada usuário tem o seu) |
| 2026-10-21 | Contornar a janela vazia ao restaurar com um retângulo de fundo sujo por `repintar`, sem trocar de renderizador | O bug é do Slint 1.18 (`renderer-software` + softbuffer no Windows); femtovg/skia custariam GPU/RAM; o contorno só age ao voltar de minimizada |
| 2026-10-22 | Iniciar com o sistema pelo valor `mic-limpo` em `HKCU\...\CurrentVersion\Run` (exe atual + `--bandeja`), com o `windows` que já usamos | Aprovado pelo Alex: sem administrador, sem crate novo, aparece no Gerenciador de Tarefas como os outros apps |
| 2026-10-22 | O registro é a fonte da verdade do "iniciar com o sistema" (lido ao abrir a janela; desativado no Gerenciador = desligado); o `config.ini` só guarda a preferência `so_bandeja` | A tela nunca mente se o usuário mexer pelo Windows; desligar e religar não perde a escolha |
| 2026-10-23 | Novo visual = direção B "vidro" (`docs/mockups/b-vidro.html`): painel único sem barra lateral, botão redondo do cancelamento, onda ao vivo, volume grande, ladrilhos e pílulas; igual no Windows e no Mac | Escolha do Alex ("tudo que você criou eu gostei"); primeiro com fundo sólido, desfoque real (`window-vibrancy`) depois e só com CPU/RAM medidos |
| 2026-10-23 | Vidro com fundo sólido: acento azul fixo do mockup (#0a84ff claro / #4cc2ff escuro) no lugar da cor de destaque do Windows | Igual no Windows e no Mac, como o mockup aprovado; a lista aberta do ComboBox ainda usa a do sistema |
| 2026-10-23 | Faixa, chave, botões, ladrilhos e pílulas próprios; dispositivos desenhados à mão com um `ComboBox` de verdade invisível por cima | Os widgets Fluent seguem o acento do Windows e não têm o visual do mockup; o ComboBox mantém teclado e lista sem reescrevê-lo |
| 2026-10-23 | Onda = pico de cada tick do medidor (sem a queda), 40 barras a 30 Hz, só com a janela visível | As palavras não se borram; medido: ~+1,5 ponto de 1 núcleo (~0,13 % do PC) sobre o medidor, RAM igual |
| 2026-10-23 | Lua do rodapé alterna claro/escuro; "Do sistema" fica em Configurações | O mockup tem o ícone; um clique para o caso comum |
| 2026-10-23 | Capturas do vidro com nomes novos (`vidro-*.png`) | As antigas são resultado entregue e ficam |
| 2026-10-23 | Sem desfoque (Mica/Acrylic) no Windows: fica o vidro com fundo sólido; desfoque só no Mac (Fase 5) | Escolha do Alex depois de ver o Mica funcionando; menos código e nada a medir no Windows |
| 2026-10-30 | Nome do app: **VoiceRune** (substitui o provisório `mic-limpo`) | Escolha do Alex: gosta de RPG (Diablo, PoE); a renomeação no código fica para uma tarefa própria |
| 2026-10-30 | Ícone: microfone rúnico (traços retos) lilás `#c4b5ff` dentro de hexágono laranja `#ff8a3d`, fundo `#1b1530` | Escolha do Alex entre 12 esboços; um SVG só (`crates/ui/ui/icone.svg`) é a fonte, `.ico`/`.icns`/PNG saem dele |
| 2026-10-30 | Ícone trocado: runa Wunjo (ᚹ, som de "V") dourada `#f0c86a` soltando ondas de voz, em hexágono `#d4a64a`, fundo `#2a0d0d` | Escolha do Alex (esboço 4, centralizado), no lugar do microfone rúnico; lê melhor em 16–32 px |
| 2026-10-30 | Renome: crates e binários `voicerune*` (minúsculo, padrão do Cargo); nos textos, pasta da config, valor do `Run` e evento, "VoiceRune" | Escopo aprovado pelo Alex |
| 2026-10-30 | Config antiga (`%APPDATA%\mic-limpo`) é **lida** se a nova não existe, nunca movida nem apagada | Quem já usa não perde nada e a versão antiga continua funcionando; a próxima gravação cria a nova |
| 2026-10-30 | Valor `mic-limpo` do `Run` é **migrado** (regravado como `VoiceRune` com o exe atual, mesmo `--bandeja` e marca do Gerenciador) e apagado | Duas entradas abririam duas cópias; o antigo aponta para um exe que não é mais compilado |
| 2026-10-30 | `target-dir` (`C:\dev\cargo-target\mic-limpo`) e pasta do repositório mantêm o nome antigo | Mudar o target recompila todas as dependências; a pasta do repo mexe com worktrees e sessões |
| 2026-10-30 | Ícone em duas versões: sem fundo (runa + ondas, traços grossos) na janela e bandeja; com fundo e hexágono nos `.ico`/`.icns`/PNG | Escolha do Alex: em 16–32 px o com fundo virava borrão e quase nenhum ícone da bandeja usa fundo |
| 2026-10-30 | Ícone embutido no exe por `build.rs` próprio (monta o `.res` e passa ao linker do MSVC), `.ico` gerado por `voicerune-dev icone` com o `resvg` já presente | Escolha do Alex (embutir); zero dependência nova no app e nada baixado; `embed-resource`/`winresource` precisariam do `rc.exe` e de crate novo |
| 2026-11-01 | **Desfoque no Windows** (Mica na janela, Acrylic no flyout) pelo winit, sem crate novo; só no Windows 11 22H2+, antes disso fundo sólido. Substitui a decisão de 2026-10-23 | Pedido do Alex ("liquid glass" também no Windows); RAM/CPU do app iguais ao sólido |
| 2026-11-01 | Janela principal com **Acrylic** (não Mica) e véu mais fino | Com o Mica o Alex não viu diferença do fundo sólido; o Acrylic desfoca o que está atrás, como o "liquid glass" pedido |
