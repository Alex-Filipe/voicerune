# Áudio

## Captura e saída
- Crate `cpal` (WASAPI no Windows, CoreAudio no macOS). Alternativa no Windows: crate `wasapi`.
- Processar só o necessário: mic mono, 48 kHz.

## Dispositivo virtual (saída "mic limpo" que outros apps enxergam)

| | Windows | macOS |
|---|---|---|
| Mecanismo | Driver de **kernel** | **AudioServerPlugIn** (modo usuário) |
| Dificuldade | Alta; assinatura exige certificado EV + atestação MS | Média |
| Protótipo usa | VB-Cable (grátis, instalado à parte) | BlackHole (GPL-3, instalado à parte) |
| Próprio | Fase 6 (por último) | Fase 6 (primeiro dos dois) |

## Latência
- Meta (hipótese): **< 20 ms** adicionais ponta a ponta. Medir na fase 1.

## Protótipo atual (fase 1)
- `cpal` 0.18 (WASAPI compartilhado), 48 kHz f32 nos dois lados; mic misturado para mono
  (média dos canais), saída mono copiada em todos os canais.
- Entrada → RNNoise na própria thread de captura → ring buffer `rtrb` (200 ms) → saída.
- Folga de 10 ms (`--folga-ms`). Deriva de relógio entre os dispositivos: **provisório** —
  descarta o excesso (acima de folga + 20 ms) ou silencia até reencher após underrun.
  Solução definitiva: reamostragem adaptativa (a decidir).
- Sem prioridade MMCSS ainda (feature `realtime` do cpal não ativada).

## Medições

**2026-10-20 — teste de 8 s, Microphone (HDA) → CABLE Input, IA ligada, folga 10 ms**

| Item | Valor |
|---|---|
| Underruns / descartadas / perdidas | 0 / 0 / 0 |
| Atraso driver entrada (callback − captura) | 5,1 ms |
| Acúmulo até completar quadro de 480 | 4,9 ms |
| RNNoise (algorítmico) | 10,0 ms |
| Fila do ring buffer | 14,5 ms |
| Atraso driver saída (reprodução prevista − callback) | 41,3 ms |
| **Soma (estimativa)** | **75,8 ms** — acima da meta de 20 ms |

A soma é **estimativa** a partir dos timestamps do cpal, não medição ponta a ponta. Não inclui o
trânsito interno do VB-Cable. Maior parcela: buffer de saída do WASAPI (`BufferSize::Default`).
