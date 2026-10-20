# Referências

## Voicemeeter Banana — o que estudar (sem RE do binário)

| Aspecto | Trazer? | Fonte |
|---|---|---|
| Roteamento strips → buses | Sim (simplificado) | Manual, uso |
| Sincronização de clock / resampling entre dispositivos | Sim — parte difícil | Observação, testes |
| Escolha de driver / buffer | Parcial (WASAPI/CoreAudio resolvem) | Manual |
| Gate, compressor, EQ | Sim (aba avançada) | Literatura de DSP |
| Remote API (`VoicemeeterRemote.h`) | Referência de API de controle | Header público |
| Denoiser atual | Não (trocado por IA) | — |
| Interface / skins | Não (é o que queremos evitar) | — |

## REA — https://github.com/morluto/rea
- Servidor MCP de engenharia reversa (Node 22+; binários nativos via Ghidra/IDA/Hopper).
- Suporte a Windows/Ghidra descrito como **experimental** no README.
- A página mostrava ~343 mil estrelas — **confirmar a autenticidade** do repo e do pacote
  `rea-agents` antes de instalar (o próprio README alerta para repos com nome parecido).
- Avaliação: pouco útil aqui; uso pontual só se surgir dúvida específica.
- **Status: não instalado.**

## Links úteis
- DeepFilterNet: https://github.com/Rikorose/DeepFilterNet
- nnnoiseless (RNNoise em Rust): https://github.com/jneem/nnnoiseless
- Slint: https://slint.dev
- cpal: https://github.com/RustAudio/cpal
- BlackHole: https://github.com/ExistentialAudio/BlackHole
- VB-Cable: https://vb-audio.com/Cable/
