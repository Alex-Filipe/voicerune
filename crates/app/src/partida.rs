//! Iniciar com o sistema. No Windows é o valor `mic-limpo` em
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (sem administrador), com o caminho do exe
//! atual e `--bandeja` se for para abrir só o ícone. O registro é a fonte da verdade: a tela lê dele
//! ao abrir, e o Gerenciador de Tarefas pode desativar a entrada (`StartupApproved\Run`), o que conta
//! como desligado. O macOS fica para a Fase 5.

use std::io;
use std::path::Path;

/// Argumento da linha registrada: abrir sem a janela, só com o ícone na bandeja.
pub const ARG_BANDEJA: &str = "--bandeja";

/// Existe "iniciar com o sistema" nesta plataforma?
pub const DISPONIVEL: bool = cfg!(windows);

/// Linha de comando registrada: o exe entre aspas (o caminho pode ter espaços) e o argumento.
pub fn linha(exe: &Path, bandeja: bool) -> String {
    let arg = if bandeja { format!(" {ARG_BANDEJA}") } else { String::new() };
    format!("\"{}\"{arg}", exe.display())
}

/// A linha registrada abre só na bandeja? (Olha só os argumentos depois do exe.)
pub fn so_bandeja(linha: &str) -> bool {
    let args = match linha.trim_start().strip_prefix('"') {
        Some(resto) => resto.split_once('"').map_or("", |(_, a)| a),
        None => linha.split_once(' ').map_or("", |(_, a)| a),
    };
    args.split_whitespace().any(|a| a == ARG_BANDEJA)
}

/// O app abre com o sistema? `Some(só na bandeja)` se sim.
#[cfg(windows)]
pub fn ler() -> Option<bool> {
    let linha = registro::ler_texto(registro::RUN, registro::NOME)?;
    // Desativado pelo Gerenciador de Tarefas: 1º byte ímpar (02 = ativo, 03 = desativado).
    let aprovado = registro::ler_binario(registro::APROVADO, registro::NOME);
    if aprovado.is_some_and(|b| b.first().is_some_and(|b| b & 1 == 1)) {
        return None;
    }
    Some(so_bandeja(&linha))
}

/// Liga (com o exe atual) ou desliga. Ligar também tira a desativação do Gerenciador de Tarefas.
#[cfg(windows)]
pub fn gravar(ligado: bool, bandeja: bool) -> io::Result<()> {
    registro::apagar(registro::APROVADO, registro::NOME)?;
    if !ligado {
        return registro::apagar(registro::RUN, registro::NOME);
    }
    registro::gravar_texto(registro::RUN, registro::NOME, &linha(&std::env::current_exe()?, bandeja))
}

#[cfg(not(windows))]
pub fn ler() -> Option<bool> {
    None
}

#[cfg(not(windows))]
pub fn gravar(_ligado: bool, _bandeja: bool) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(windows)]
mod registro {
    use std::io;

    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR};
    use windows::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_ROUTINE_FLAGS, REG_SZ, RRF_RT_REG_BINARY,
        RRF_RT_REG_SZ,
    };

    pub const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    pub const APROVADO: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
    pub const NOME: PCWSTR = w!("mic-limpo");

    fn erro(e: WIN32_ERROR) -> io::Error {
        io::Error::from_raw_os_error(e.0 as i32)
    }

    /// Lê um valor em `HKCU\chave`; `None` se não existe ou não é do tipo pedido.
    fn ler(chave: PCWSTR, nome: PCWSTR, tipo: REG_ROUTINE_FLAGS) -> Option<Vec<u8>> {
        let mut tam = 0u32;
        // SAFETY: 1ª chamada só pergunta o tamanho; a 2ª escreve no máximo `tam` bytes no `buf`.
        unsafe {
            RegGetValueW(HKEY_CURRENT_USER, chave, nome, tipo, None, None, Some(&mut tam)).ok().ok()?;
            let mut buf = vec![0u8; tam as usize];
            RegGetValueW(HKEY_CURRENT_USER, chave, nome, tipo, None, Some(buf.as_mut_ptr().cast()), Some(&mut tam))
                .ok()
                .ok()?;
            buf.truncate(tam as usize);
            Some(buf)
        }
    }

    pub fn ler_texto(chave: PCWSTR, nome: PCWSTR) -> Option<String> {
        let b = ler(chave, nome, RRF_RT_REG_SZ)?;
        let u: Vec<u16> = b.as_chunks::<2>().0.iter().map(|&p| u16::from_le_bytes(p)).collect();
        Some(String::from_utf16_lossy(&u).trim_end_matches('\0').to_string())
    }

    pub fn ler_binario(chave: PCWSTR, nome: PCWSTR) -> Option<Vec<u8>> {
        ler(chave, nome, RRF_RT_REG_BINARY)
    }

    pub fn gravar_texto(chave: PCWSTR, nome: PCWSTR, texto: &str) -> io::Result<()> {
        let u: Vec<u16> = texto.encode_utf16().chain([0]).collect();
        // SAFETY: `u` vive até o fim da chamada e o tamanho em bytes inclui o 0 final.
        let r = unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, chave, nome, REG_SZ.0, Some(u.as_ptr().cast()), (u.len() * 2) as u32)
        };
        r.ok().map_err(|_| erro(r))
    }

    /// Apaga o valor; não existir já é o resultado pedido.
    pub fn apagar(chave: PCWSTR, nome: PCWSTR) -> io::Result<()> {
        // SAFETY: só strings constantes.
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, chave, nome) } {
            r if r.is_ok() || r == ERROR_FILE_NOT_FOUND => Ok(()),
            r => Err(erro(r)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linha_ida_e_volta() {
        let exe = Path::new(r"C:\Program Files\mic limpo\mic-limpo.exe");
        assert_eq!(linha(exe, true), r#""C:\Program Files\mic limpo\mic-limpo.exe" --bandeja"#);
        assert!(so_bandeja(&linha(exe, true)));
        assert!(!so_bandeja(&linha(exe, false)));
        // Um caminho com "--bandeja" no nome não conta; sem aspas também é lido.
        assert!(!so_bandeja(r#""C:\--bandeja\mic-limpo.exe""#));
        assert!(so_bandeja(r"C:\app\mic-limpo.exe --bandeja"));
        assert!(!so_bandeja(""));
    }
}
