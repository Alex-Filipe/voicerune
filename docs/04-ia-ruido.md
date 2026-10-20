# Cancelamento de ruído com IA

## Candidatos

| Modelo | Implementação | Qualidade esperada | Custo esperado |
|---|---|---|---|
| RNNoise | `nnnoiseless` (Rust puro) | Boa para ruído constante | Muito baixo |
| DeepFilterNet3 | libDF (Rust) + `tract` | Muito boa, inclui teclado e ruído variável | Baixo, maior que RNNoise |

Descartados: NVIDIA Broadcast (só RTX; a máquina tem GTX 1650) e GPU em geral (fica livre para jogos).

## Proposta de modos (hipótese, confirmar na fase 2)
- **Leve** = RNNoise · **Forte** = DeepFilterNet.
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
