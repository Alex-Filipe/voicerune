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

## Medições
_Nenhuma ainda._
