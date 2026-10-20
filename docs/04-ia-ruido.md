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

Qualidade do RNNoise ainda **não avaliada** (falta teste de escuta / Discord).
DeepFilterNet: ainda não medido (fase 2).
