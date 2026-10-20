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
_Nenhuma ainda. Os custos acima são expectativas, não medidas._
