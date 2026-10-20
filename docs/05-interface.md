# Interface

## Framework (decidido: Slint)

| Opção | Por que sim / por que não |
|---|---|
| **Slint** ✅ | Declarativo, leve, estilos Fluent (Win) e Cupertino (Mac), animações |
| Iced | Leve, mas exige mais trabalho para chegar no visual |
| egui | Cara de ferramenta de dev |
| Tauri | WebView2 pesado — conflita com leveza |
| WinUI 3 | Só Windows; bindings Rust trabalhosos |

Extras: `window-vibrancy` (Mica/Acrylic no Win, vibrancy no Mac), `tray-icon` (bandeja / barra de menu).

## Princípios de UX
- Tela principal mínima: mic → **Cancelamento: Desligado / Leve / Forte** → saída; medidor de nível; mute.
- **Presets** ("Reunião", "Stream", "Jogo") em vez de botões técnicos.
- Avançado (gate, EQ, roteamento) numa aba escondida.
- Configuração automática de dispositivo e buffer; nunca mostrar "WDM/KS/MME".
- **Flyout na bandeja** (estilo controle de volume do Windows) para o uso diário.
- Botão **"Ouvir antes/depois"**.
- Primeiro uso em 3 passos: escolher mic → testar voz → onde usar. No Mac, inclui a permissão de microfone.
- Animações curtas e sutis (não pesar).
