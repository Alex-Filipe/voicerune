# Segurança, privacidade e licenças

## Privacidade (decidido)
- Nenhum acesso à rede no app; nenhuma telemetria. Verificar com `cargo deny` e revisão de dependências.
- Modelos de IA embutidos no binário e verificados por hash.
- Config local em TOML.
- IPC local restrito ao usuário (ver `02-arquitetura.md`).

## Licenças
- Projeto: **MIT**.
- **BlackHole é GPL-3:** pode ser usado como referência ou instalado à parte, mas **não embutir**
  no app (forçaria GPL). O plugin próprio de macOS (fase 6) deve ser escrito do zero.
- `audio_thread_priority` (puxado pela feature opcional `realtime` do app, para MMCSS) é
  **MPL-2.0**: copyleft só por arquivo, compatível com distribuir o app como MIT sem alterar os
  arquivos dele. Hoje a feature está desligada por padrão. O modo baixa latência faz MMCSS
  direto pelo crate `windows` (MIT/Apache), sem essa dependência.
- **Slint** (UI): GPL-3 **ou** licença royalty-free 2.0 (escolhida). A royalty-free permite app
  desktop fechado ou aberto sob qualquer licença, desde que mostre o selo do Slint (`AboutSlint`,
  na página Sobre). Revisar o texto da licença antes do 1º release.
- Voicemeeter: proprietário. Sem engenharia reversa do binário (EULA); só comportamento,
  manual e API pública.

## Assinatura / distribuição (futuro)
- Windows: assinatura de código; driver de kernel exige certificado EV + atestação Microsoft.
- macOS: Apple Developer (US$ 99/ano) para assinar e notarizar.
