//! Última configuração do usuário: gravada em `%APPDATA%\VoiceRune\config.ini` (no macOS,
//! `~/Library/Application Support/VoiceRune/config.ini`) a cada mudança e lida ao abrir.
//! Sem ela, vale a de antes do nome VoiceRune (`%APPDATA%\mic-limpo\config.ini`),
//! que nunca é apagada: a próxima gravação já cria a nova. Texto simples `chave = valor` (sem
//! dependência nova): chave ausente ou valor inválido fica no padrão, chave desconhecida é ignorada.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::controlador::{Ajustes, Modo};

/// Faixa do controle de volume da janela (dB).
const GANHO: (f32, f32) = (-12.0, 24.0);
const MODOS: [(Modo, &str); 2] = [(Modo::Leve, "leve"), (Modo::Forte, "forte")];
const TEMAS: [&str; 3] = ["sistema", "claro", "escuro"];
/// Pastas em `%APPDATA%`: a atual e a de antes do nome VoiceRune (até 2026-10-30).
const PASTA: &str = "VoiceRune";
const PASTA_ANTIGA: &str = "mic-limpo";
const ARQUIVO: &str = "config.ini";

/// O que fica de uma execução para a outra.
#[derive(Clone, Debug, PartialEq)]
pub struct Salvo {
    /// Só os campos da janela (cancelamento, modo, ganho, dispositivos, baixa latência). O app
    /// sempre abre ligado: `ligado` de versões antigas é ignorado (a janela não tem mais pausa).
    pub ajustes: Ajustes,
    /// 0 = do sistema, 1 = claro, 2 = escuro.
    pub tema: i32,
    /// Posição e tamanho da janela em px físicos: x, y, largura, altura.
    pub janela: Option<(i32, i32, u32, u32)>,
    /// Preferência "iniciar com o sistema só na bandeja", guardada mesmo com a partida desligada.
    /// Se ligar ou não fica no registro do Windows (`partida`), não aqui.
    pub so_bandeja: bool,
}

impl Default for Salvo {
    fn default() -> Self {
        Self { ajustes: Ajustes::default(), tema: 0, janela: None, so_bandeja: true }
    }
}

impl Salvo {
    /// Pasta de dados de apps do usuário: `%APPDATA%` no Windows, `~/Library/Application Support` no macOS.
    fn appdata() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        return std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
        #[cfg(not(target_os = "macos"))]
        std::env::var_os("APPDATA").map(PathBuf::from)
    }

    pub fn caminho() -> Option<PathBuf> {
        Some(Self::appdata()?.join(PASTA).join(ARQUIVO))
    }

    /// Lê o arquivo (o novo ou, sem ele, o antigo); sem nenhum (1ª vez) ou ilegível, os padrões.
    pub fn carregar() -> Self {
        Self::appdata().and_then(|b| ler(&b)).map_or_else(Self::default, |t| Self::de_texto(&t))
    }

    /// Grava num temporário e troca: um arquivo pela metade nunca fica no lugar.
    pub fn gravar(&self) -> io::Result<()> {
        let c = Self::caminho().ok_or_else(|| io::Error::other("APPDATA não definido"))?;
        if let Some(pasta) = c.parent() {
            fs::create_dir_all(pasta)?;
        }
        let tmp = c.with_extension("ini.tmp");
        fs::write(&tmp, self.para_texto())?;
        fs::rename(&tmp, &c)
    }

    pub fn para_texto(&self) -> String {
        let a = &self.ajustes;
        let sim = |b: bool| if b { "sim" } else { "nao" };
        let nome = |n: &Option<String>| n.clone().unwrap_or_default();
        let modo = MODOS.iter().find(|(m, _)| *m == a.modo).map_or("leve", |(_, n)| *n);
        let mut t = String::from("# VoiceRune: última configuração (o app regrava a cada mudança)\n");
        t += "# entrada/saida vazias = o padrão do Windows\n";
        let _ = writeln!(t, "cancelamento = {}", sim(a.ia));
        let _ = writeln!(t, "modo = {modo}");
        let _ = writeln!(t, "ganho_db = {}", a.ganho_db);
        let _ = writeln!(t, "entrada = {}", nome(&a.entrada));
        let _ = writeln!(t, "saida = {}", nome(&a.saida));
        let _ = writeln!(t, "baixa_latencia = {}", sim(a.baixa_latencia));
        let _ = writeln!(t, "tema = {}", TEMAS.get(self.tema as usize).unwrap_or(&TEMAS[0]));
        if let Some((x, y, l, h)) = self.janela {
            let _ = writeln!(t, "janela = {x} {y} {l} {h}");
        }
        let _ = writeln!(t, "so_bandeja = {}", sim(self.so_bandeja));
        t
    }

    pub fn de_texto(texto: &str) -> Self {
        let mut s = Self::default();
        let sim = |v: &str| match v {
            "sim" => Some(true),
            "nao" | "não" => Some(false),
            _ => None,
        };
        let nome = |v: &str| (!v.is_empty()).then(|| v.to_string());
        // O Bloco de Notas pode gravar com BOM na frente.
        let texto = texto.trim_start_matches('\u{feff}');
        for linha in texto.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
            let Some((chave, valor)) = linha.split_once('=') else { continue };
            let (a, valor) = (&mut s.ajustes, valor.trim());
            match chave.trim() {
                "cancelamento" => a.ia = sim(valor).unwrap_or(a.ia),
                // Até 2026-10-21 "desligado" era um modo; vira a chave desligada.
                "modo" if valor == "desligado" => a.ia = false,
                "modo" => a.modo = MODOS.iter().find(|(_, n)| *n == valor).map_or(a.modo, |(m, _)| *m),
                "ganho_db" => {
                    if let Some(g) = valor.replace(',', ".").parse::<f32>().ok().filter(|g| g.is_finite()) {
                        a.ganho_db = g.clamp(GANHO.0, GANHO.1);
                    }
                }
                "entrada" => a.entrada = nome(valor),
                "saida" => a.saida = nome(valor),
                "baixa_latencia" => a.baixa_latencia = sim(valor).unwrap_or(a.baixa_latencia),
                "tema" => s.tema = TEMAS.iter().position(|n| *n == valor).map_or(s.tema, |i| i as i32),
                "janela" => s.janela = janela(valor),
                "so_bandeja" => s.so_bandeja = sim(valor).unwrap_or(s.so_bandeja),
                _ => {}
            }
        }
        s
    }
}

/// Texto da config em `base` (o `%APPDATA%`): a pasta nova primeiro, depois a antiga.
fn ler(base: &Path) -> Option<String> {
    [PASTA, PASTA_ANTIGA].iter().find_map(|p| fs::read_to_string(base.join(p).join(ARQUIVO)).ok())
}

fn janela(v: &str) -> Option<(i32, i32, u32, u32)> {
    let n: Vec<i64> = v.split_whitespace().map(str::parse).collect::<Result<_, _>>().ok()?;
    let [x, y, l, h] = n[..] else { return None };
    let tam = |v: i64| u32::try_from(v).ok().filter(|v| (1..=100_000).contains(v));
    Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?, tam(l)?, tam(h)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_e_volta() {
        let s = Salvo {
            ajustes: Ajustes {
                ia: false,
                modo: Modo::Forte,
                ganho_db: -3.5,
                entrada: Some("Microfone (Fifine K669 = USB)".into()),
                saida: None,
                baixa_latencia: true,
                ..Ajustes::default()
            },
            tema: 2,
            janela: Some((-1700, 40, 516, 579)),
            so_bandeja: false,
        };
        assert_eq!(Salvo::de_texto(&s.para_texto()), s);
    }

    #[test]
    fn arquivo_estranho_fica_no_padrao() {
        let s = Salvo::de_texto("lixo\nmodo = turbo\nganho_db = NaN\nligado = talvez\njanela = 1 2 0 4\nnova = 1\nso_bandeja = 2\n");
        assert_eq!(s, Salvo::default());
        let s = Salvo::de_texto("\u{feff}ganho_db = 99 \r\nsaida =\r\ntema = claro\r\n");
        assert_eq!(s.ajustes.ganho_db, GANHO.1);
        assert_eq!(s.ajustes.saida, None);
        assert_eq!(s.tema, 1);
    }

    #[test]
    fn config_antiga() {
        let s = Salvo::de_texto("ligado = nao
modo = desligado
");
        assert!(s.ajustes.ligado && !s.ajustes.ia);
        assert_eq!(s.ajustes.modo, Modo::Leve);
    }

    #[test]
    fn pasta_antiga_so_sem_a_nova() {
        let base = std::env::temp_dir().join(format!("voicerune-teste-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        assert_eq!(ler(&base), None);
        fs::create_dir_all(base.join(PASTA_ANTIGA)).unwrap();
        fs::write(base.join(PASTA_ANTIGA).join(ARQUIVO), "tema = claro
").unwrap();
        assert_eq!(ler(&base).as_deref(), Some("tema = claro
"));
        fs::create_dir_all(base.join(PASTA)).unwrap();
        fs::write(base.join(PASTA).join(ARQUIVO), "tema = escuro
").unwrap();
        assert_eq!(ler(&base).as_deref(), Some("tema = escuro
"));
        fs::remove_dir_all(&base).unwrap();
    }
}
