# Cancelamento de ruído com IA

## Candidatos

| Modelo | Implementação | Qualidade esperada | Custo esperado |
|---|---|---|---|
| RNNoise | `nnnoiseless` (Rust puro) | Boa para ruído constante | Muito baixo |
| DeepFilterNet3 | libDF (Rust) + `tract` | Muito boa, inclui teclado e ruído variável | Baixo, maior que RNNoise |

Descartados: NVIDIA Broadcast (só RTX; a máquina tem GTX 1650) e GPU em geral (fica livre para jogos).

## Modos (decidido pelo Alex em 2026-10-20, fase 2)
- **Leve** = RNNoise (padrão) · **Forte** = DeepFilterNet3 (`--modelo dfn`), recomendado com
  `--baixa-latencia` (~115 ms ponta a ponta; no backend padrão ~149 ms). Limite de redução
  desligado por padrão (`--limite-db N` disponível; decidir na UI, fase 4). Medições abaixo.
- Pular a rede neural no silêncio (VAD) para economizar CPU.

## Metas (hipótese)
- RAM do processo de áudio < ~30 MB com a janela fechada.
- CPU baixa o bastante para não interferir em jogos no Windows (pior caso).

## Medições

**2026-10-20 — RNNoise (`nnnoiseless` 0.5.2, sem features padrão), Windows, build release**

| Medida | Valor | Como |
|---|---|---|
| Tempo por quadro de 10 ms | média 0,044 ms · p99 0,103 ms · máx 1,66 ms | `--bench`, 20 s de voz sintética + ruído |
| Custo médio | ~0,4 % de um núcleo | média ÷ 10 ms |
| Atraso algorítmico | 479 amostras ≈ 10 ms | pico da correlação entrada × saída |
| Em tempo real (8 s, mic real) | média 0,043 ms/quadro, pior callback 0,24 ms | resumo do protótipo |
| Processo inteiro (8 s) | CPU ~0,8 % de um núcleo · RAM privada 2,1 MB · working set 10,9 MB | `Get-Process` |

**2026-10-20 — teste de escuta no Discord (Alex, "Vamos checar", entrada = CABLE Output,
supressão do Discord desligada):** ruído de teclado e áudio de TikTok no celular **não foram
ouvidos** com a IA ligada; "acho que ficou bom". Avaliação informal (uma pessoa, sem gravação):
não mede artefatos na voz nem compara com outros modelos — isso fica para a fase 2.

**2026-10-20 — volume "às vezes baixo" (Alex, A/B com Enter no "Vamos checar"):** igual com a IA
ligada e desligada → **não é o RNNoise**. Coerente com a medição: mic −37,7 dBFS = saída com IA
−37,7 dBFS. Causa provável: nível do mic já baixo na origem (voz costuma ficar em −20 a −25 dBFS).

**Latência própria do RNNoise (cálculo):** ~20 ms em fluxo contínuo = 10 ms para completar o
quadro + 10 ms de atraso algorítmico. Qualquer modelo com quadros de 10 ms e janela de 20 ms tem
piso parecido; DeepFilterNet3 deve ter piso maior (lookahead) — medir na fase 2.

**2026-10-20 — DeepFilterNet3, sonda isolada (fase 2, bloco 1)** — `deep_filter` (libDF) v0.5.6 via
git (rev `978576a`; no crates.io só existe a 0.2.5 antiga), features `tract` + `default-model`,
sem padrão. Crate descartável fora do repo, mesma voz sintética + ruído do `bench`, 20 s, 1 thread.

| Medida | DeepFilterNet3 | RNNoise (ref.) |
|---|---|---|
| Tempo por quadro de 10 ms | média 0,62 ms · p99 1,3 ms · máx 1,8 ms | 0,044 · 0,10 · 1,7 ms |
| Custo médio | **~6,2 % de um núcleo** (~15×) | ~0,4 % |
| Atraso algorítmico | **1440 amostras = 30,0 ms** (pico da correlação) | 10 ms |
| Carga do modelo | ~185 ms (uma vez) | ~0 |
| RAM do processo (sonda) | privada 20,5 MB · working set (pico) 48,3 MB | 2,1 · 10,9 MB (app inteiro) |
| Binário | 25,0 MB release · 21,9 MB com LTO + strip | app hoje 1,6 MB |
| Compilação (do zero) | 1 min 57 s release · 5 min 37 s com LTO | — |
| Dependências | 96 crates (tract 0.19.16, ndarray, prost, flate2, tar…); `cargo tree`: nenhuma de rede | — |

Fatos do código (libDF `tract.rs`): FFT 960 / hop 480 (quadro de 10 ms, igual ao RNNoise);
lookahead 2 quadros → 480 + 960 = 1440 amostras, confere com a medição. Piso de latência própria
~40 ms (10 ms para completar o quadro + 30 ms), ou seja **~+20 ms ponta a ponta** sobre o RNNoise.
**Aloca a cada quadro** (clones de tensor, `tvec!`) → não pode rodar na thread de áudio; precisa de
thread própria ligada por ring buffer. Tem limite de redução pronto (`atten_lim_db`: mistura de
volta parte do sinal original). Modelo de 8 MB (`.tar.gz`) embutido por `include_bytes!`; o commit
fixado no `Cargo.lock` garante o conteúdo. Licença MIT/Apache-2.0 (código e modelos).

**2026-10-20 — RNNoise × DeepFilterNet3 integrados (fase 2, bloco 2)** — `mic-limpo-dev bench`
(mesma voz sintética + ruído, 20 s, 1 thread, build release), DFN com os limiares do binário
oficial (ver abaixo):

| Medida | RNNoise | DeepFilterNet3 |
|---|---|---|
| Tempo por quadro de 10 ms | média 0,041 · p99 0,071 · máx 0,23 ms | média 0,81 · p99 1,21 · máx 1,78 ms |
| Custo médio | **0,4 %** de um núcleo | **8,1 %** de um núcleo (~20×) |
| Atraso algorítmico | 479 amostras = 10,0 ms | 1440 amostras = 30,0 ms |
| Onde roda | thread de áudio | thread própria `mic-limpo-ia` (aloca e não é `Send`) |

**Armadilha resolvida: limiares do DFN.** O `RuntimeParams::default_with_ch` da libDF (padrão
do plugin LADSPA) usa limiares de SNR local −10 / 30 / 20 dB: pula o 2º estágio (deep filtering)
acima de 20 dB e zera abaixo de −10 dB. Com eles o DFN ficou **pior** que o RNNoise (SI-SDR 4,7 ×
16,4 dB a 5 dB de SNR; voz limpa só 13 dB). Com os do binário oficial `deep-filter` (−15 / 35 /
35 dB, a configuração avaliada no artigo): voz limpa 32,8 dB e resultados abaixo. Escolhidos por
serem o padrão publicado, não ajustados nesta amostra. Custo sobe de ~6 % para ~8 %.

**Qualidade objetiva (amostra pública, não é teste final)** — `mic-limpo-dev processar voz.wav
--ruido R.wav --snr N`: voz limpa (freesound 33711, 21 s) misturada a dois ruídos (freesound
2530 e 573577) em SNR conhecido; SI-SDR em dB contra a voz limpa (maior = melhor), saídas
alinhadas (atraso descontado). Arquivos só locais (33711 é CC BY-NC), fora do repo.

| Ruído · SNR | Entrada | RNNoise | RNNoise lim. 20 dB | DFN3 | DFN3 lim. 20 dB |
|---|---|---|---|---|---|
| 2530 · 0 dB | 0,1 | 3,8 | 3,6 | **10,4** | 10,1 |
| 2530 · 5 dB | 5,0 | 8,3 | 8,2 | **14,6** | 14,4 |
| 2530 · 10 dB | 10,0 | 12,0 | 12,2 | **18,4** | 18,3 |
| 573577 · 0 dB | 0,0 | **14,1** | 13,0 | 13,7 | 12,0 |
| 573577 · 5 dB | 5,0 | 16,4 | 16,1 | **16,6** | 15,2 |
| 573577 · 10 dB | 10,0 | 17,7 | 17,9 | **18,5** | 17,5 |

Leitura: no ruído 2530 o DFN ganha **~6 dB** em todos os SNRs; no 573577 empata (±1 dB). Nível
da voz preservado pelos dois (≤ 1,5 dB abaixo da entrada). Piso de ruído (p10 do RMS por quadro):
no 573577 o RNNoise silencia mais (−94 × −69 dBFS); o limite de 20 dB sobe o piso para ~−62 a
−75 dBFS e custa até 1,5 dB de SI-SDR (esperado: o limite devolve ruído de propósito; o ganho dele
é naturalidade, que só a escuta mede).

**Limite de redução** (`--limite-db N`, nos dois modelos): mistura de volta o sinal original
para atenuar no máximo N dB. No DFN é o `atten_lim_db` da libDF; no RNNoise, o mesmo cálculo no
tempo (quadro anterior, alinhado com o atraso de 10 ms). Teste: 0 dB devolve o original.

### Modos (decididos: Leve + Forte no app)
| Modo | Modelo | CPU | Latência própria | Quando |
|---|---|---|---|---|
| **Leve** | RNNoise | 0,4 % de um núcleo | ~20 ms (10 + 10) | padrão; ruído constante; jogo pesado |
| **Forte** | DeepFilterNet3 | ~8 % de um núcleo | ~40 ms (10 + 30) → ~+20 ms ponta a ponta | ruído variável (teclado, vozes, rua) |

Hipóteses a medir: estalos/CPU com jogo aberto; se o limite de 20 dB soa melhor (escuta).

**2026-10-20 — latência ponta a ponta e tempo real (fase 2, bloco 3; Discord fechado)** —
`mic-limpo-dev latencia --modelo M` (cliques → CABLE Input → CABLE Output, 8 s; IA em bypass,
atraso algorítmico somado). Com DFN o caminho passa pela thread própria.

| Caminho | Backend | Medido em bypass (rodadas) | + algoritmo | **Com a IA ligada** |
|---|---|---|---|---|
| Leve (RNNoise) | padrão | 130,8 · 117,1 · 120,3 → mediana 120,3 ms | +10 ms | **~130 ms** |
| Forte (DFN3) | padrão | 115,6 · 120,0 · 119,1 → mediana 119,1 ms | +30 ms | **~149 ms** |
| Leve (RNNoise) | `--baixa-latencia` | 87,5 ms | +10 ms | **~98 ms** |
| Forte (DFN3) | `--baixa-latencia` | 84,7 ms | +30 ms | **~115 ms** |
| Voicemeeter atual (ref., fase 1) | — | — | — | ~128 ms |

Leitura: a thread própria do DFN **não acrescenta latência mensurável** (diferença dentro da
variação entre rodadas, ±7 ms); o Forte custa os +20 ms do algoritmo. Forte no backend padrão
(~149 ms) fica acima do Voicemeeter; com `--baixa-latencia` (~115 ms), abaixo.

App em tempo real com DFN (mic real, 6–20 s, várias rodadas): DSP médio 2,5–9,5 % de um núcleo
(menor que no bench quando o mic está quieto: abaixo de −15 dB de SNR local o DFN pula os
decodificadores), pior quadro ~2 ms, 0 perdas. **Limitação conhecida:** com folga de 10 ms, o DFN
dá **1 underrun na partida** em cerca de metade das partidas (nunca depois; 20 s → ainda 1);
some com `--folga-ms 15`. São 10 ms de silêncio no início. Aquecer o modelo antes (0,5 s de voz
sintética) não mudou o quadro (3 de 5) e foi revertido. O `latencia` mostra o mesmo às vezes
também com RNNoise. O erro "buffer underrun or overrun" do cpal na entrada aparece nos dois
modelos (não é do DFN).

**2026-10-20 — escuta A/B do Alex (fase 2, bloco 4)** — `mic-limpo-dev gravar` (20 s, mic real,
voz + teclado + TikTok no celular) → `processar` → 4 WAVs alinhados (rnnoise, rnnoise-lim20,
dfn, dfn-lim20) em `C:\devase2-escuta\` (fora do repo). Resultado: **nenhuma diferença
percebida** entre as variantes; **teclado audível em todas**, TikTok não percebido; "parece bom".
Avaliação informal (uma pessoa, uma gravação, sem cegamento).

Ressalva medida: o sinal do mic **chega com gate antes do app** — 17 % dos quadros de 10 ms são
zeros digitais exatos (silêncios entre falas), o que indica supressão/gate no Windows ou no driver
(suspeitos: "Melhorias de áudio"/Voice Clarity, Realtek). Os modelos só têm ruído para tratar
durante a fala, e a métrica de piso fica inútil (−200 dB). Nível baixo: voz −41 dBFS, pico −22 dBFS.

Conclusão (dados até aqui): no uso real do Alex o Forte **não mostrou ganho audível**; o ganho
objetivo (~6 dB de SI-SDR) apareceu só num dos dois ruídos públicos. Pendente: caso difícil
(teclado forte, voz de fundo) com o mic sem gate.

### Corte no Forte (2026-10-21, gravação do Alex: `C:\devoz-teste.wav`, 20 s, mic direto)
Alex ouviu na página Teste: o Forte "às vezes corta" (voz some e às vezes picota).
- **Causa achada na libDF** (`tract.rs`, `process`): quadro com média quadrática < 1e-7
  (**−70 dBFS**) sai **todo zero na hora** e pula o processamento (buffers e STFT não andam).
  Como a saída normal tem 30 ms de atraso, esse zero cai em cima do **fim da palavra anterior**.
- Medido: 27 % dos quadros do mic do Alex ficam abaixo de −70 dBFS (voz baixa: p90 −42 dBFS;
  gate do mic: 11 % de zeros digitais). Na saída do DFN, 127 quadros (1,3 s em 20 s) com entrada
  audível saem todos zero; em 123 deles o quadro 30 ms depois está abaixo do limiar.
- Nas bordas das palavras, 20 % dos quadros perdem mais de 10 dB (RNNoise: 0 %). O **limite de
  redução não muda nada** nesses quadros (o atalho vem antes dele); fora deles o limite funciona.
- No miolo da voz o DFN não corta (mediana 0,3 dB; 2 % > 10 dB; nenhum buraco ≥ 30 ms).
- A medição do quanto o ruído baixa ficou inválida: o "ruído" do Alex está a −60…−80 dBFS, perto
  do piso do WAV de 16 bits.
- O picote ao vivo (underrun) **ainda não foi medido**. Hipótese: parte do "picota" é este mesmo
  atalho, que também deixa um degrau no STFT.

