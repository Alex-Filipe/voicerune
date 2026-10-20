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
- Meta original (hipótese): **< 20 ms** adicionais. **Inalcançável com RNNoise + VB-Cable**
  (ver medições abaixo): só o RNNoise impõe ~20 ms e o VB-Cable ~55 ms. Nova meta: **a decidir
  pelo Alex**. Referência medida: o Voicemeeter atual adiciona **~128 ms** e o app com RNNoise
  **~130 ms** (ver comparação abaixo). Sugestão: meta = não piorar em relação ao Voicemeeter.

## Protótipo atual (fase 1)
- `cpal` 0.18 (WASAPI compartilhado), 48 kHz f32 nos dois lados; mic misturado para mono
  (média dos canais), saída mono copiada em todos os canais.
- Entrada → RNNoise na própria thread de captura → ring buffer `rtrb` (200 ms) → saída.
- Folga de 10 ms (`--folga-ms`) e teto de 20 ms (`--teto-ms`). Deriva de relógio entre os
  dispositivos: **provisório** — descarta o excesso acima de folga + teto ou silencia até reencher
  após underrun. Solução definitiva: reamostragem adaptativa (a decidir).
- MMCSS: feature opcional `realtime` (`cargo build --release --features realtime`), desligada
  por padrão (sem ganho medido com o sistema ocioso; testar com jogo aberto).
- Limites do `cpal` 0.18 no WASAPI: só modo compartilhado; `BufferSize::Fixed` abaixo de 10 ms é
  ignorado (o período continua 480 quadros); não usa IAudioClient3 nem modo exclusivo.
- Ferramentas: `--medir-latencia` (troca o mic por cliques a cada 500 ms, IA desligada, e detecta
  no `CABLE Output` comparando timestamps de captura no mesmo relógio QPC) e `--sondar`
  (consulta o WASAPI direto: períodos, buffers, IAudioClient3, exclusivo).
- `--comparar --alvo "A,B"`: grava o mic direto e os alvos ao mesmo tempo (só leitura) e acha o
  atraso de cada alvo por correlação cruzada do mesmo som. Tempo ancorado por reta ajustada aos
  timestamps de todos os pacotes (os do Voicemeeter oscilam ~0,3 ms por pacote).
- Armadilha: com o ambiente quieto o driver do mic às vezes entrega **silêncio digital**
  (−150 dBFS, e o B1 do Voicemeeter fica em zero junto). Medir **falando**.

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

**2026-10-20 — medição ponta a ponta por cliques (`--medir-latencia`, 8 s, 16 cliques por execução)**

"Latência adicional" = captura do clique no lugar do mic → captura no `CABLE Output` (o que o
Discord leria). IA desligada (o clique não passa pelo RNNoise); com IA, somar 10 ms algorítmicos.

| Configuração | Underruns | Latência medida (IA off) | Obs. |
|---|---|---|---|
| Padrão (folga 10, teto 20) | 0 | 122,8–129,2 ms em 6 execuções (~125 ms) | `--buffer` 480/240/128 não muda nada |
| folga 2, teto 10 | 2 | 117,2 ms | descartes = estalos |
| folga 0, teto 10 | 10 | 120,2 ms | |
| folga 0, teto 5 | 28 | 121,7 ms | |
| MMCSS, folga 10, teto 20 | 1 | 122,8 ms | sem ganho com o sistema ocioso |
| MMCSS, folga 0/2, teto 10 | 6 / 18 | 131,4 / 122,3 ms | |

- **Com RNNoise ligado: ~135 ms** adicionais na configuração padrão.
- Divisão aproximada (usa a previsão de reprodução do cpal, que parece inflada): app até a saída
  ~70 ms · dentro do VB-Cable ~55 ms (53–63 ms).
- Variação entre execuções ~±3 ms; diferenças menores que isso não são conclusivas.
- Apertar folga/teto ganha no máximo ~5–8 ms e gera underruns (descompasso de períodos: mic 488 ×
  saída 480 quadros). **Decidido:** manter folga 10 / teto 20.

**Piso do RNNoise (cálculo, não medição):** quadro de 480 precisa estar completo (10 ms) + atraso
algorítmico de 480 amostras (10 ms, medido no `--bench`) = **~20 ms**, mesmo com driver ideal.

**2026-10-20 — sonda do WASAPI (`--sondar`)**

| Dispositivo | Compartilhado (cpal) | IAudioClient3 (compartilhado baixa latência) | Exclusivo |
|---|---|---|---|
| CABLE Input (saída) | período 10 ms, buffer 22,0 ms | período mín. 128 (2,67 ms), buffer 5,83 ms | não suportado |
| Microphone (HDA) | período 10,16 ms, buffer 22,3 ms | período mín. 139 (2,90 ms), buffer 6,33 ms | PCM 16 suportado, Initialize falha (0x88890019) |
| CABLE Output (entrada) | período 10 ms, buffer 22,0 ms | só 480 (10 ms) | não suportado |

Latência de stream reportada: 0 ms em todos. **Hipótese a medir:** um backend próprio com
IAudioClient3 no mic e no CABLE Input reduziria ~25–35 ms (período 10 → ~3 ms e buffer de saída
22 → ~6 ms). O lado do `CABLE Output` é aberto pelo Discord e não depende do app.

**2026-10-20 — app × Voicemeeter atual (`--comparar`, mesmo som, gravados ao mesmo tempo)**

Atraso até o que o Discord recebe, em relação ao mic direto. Voicemeeter Banana aberto com a
configuração atual do Alex (mic → B1). App: configuração padrão, rodando em paralelo.

| Caminho | Com voz, IA ligada (2 × 20 s) | Só ruído ambiente (rodadas com pico claro) |
|---|---|---|
| Voicemeeter (mic → Out B1) | 127,4 / 128,5 ms (corr. 0,87 / 0,90) | 128,2–138,0 ms (4 rodadas) |
| App + RNNoise (mic → CABLE Output) | **129,8 ms** (corr. 0,95, pico 3,8×) · 118,2 ms (pico ambíguo 1,5×, descartado) | — |
| App sem IA (mic → CABLE Output) | — | 124,2 ms (corr. 1,00), bate com os ~125 ms dos cliques |

- **Conclusão:** latência do app ≈ a do Voicemeeter atual (~128–130 ms). Sem 0 underruns em 48 s.
- A fila do ring buffer varia de uma sessão para outra (7,6 a 14,5 ms) conforme a fase em que os
  streams começam: a latência total pode variar ~±5 ms entre sessões.
