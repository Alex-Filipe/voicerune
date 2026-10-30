//! Embute o ícone (`assets/icone/voicerune.ico`, gerado por `voicerune-dev icone`) no exe do Windows:
//! é o que o Explorer, o atalho e a barra de tarefas mostram. Sem `rc.exe` nem crate novo: monta o
//! `.res` (RT_ICON + RT_GROUP_ICON) aqui e o passa ao linker do MSVC, que aceita `.res` direto.

use std::path::Path;
use std::{env, fs};

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;

fn main() {
    let ico = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icone/voicerune.ico");
    println!("cargo:rerun-if-changed={}", ico.display());
    let msvc = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !msvc {
        return;
    }
    let dados = fs::read(&ico).expect("assets/icone/voicerune.ico");
    let res = Path::new(&env::var("OUT_DIR").unwrap()).join("icone.res");
    fs::write(&res, montar(&dados)).unwrap();
    println!("cargo:rustc-link-arg-bins={}", res.display());
}

/// `.res` com cada imagem do `.ico` (ids 1..n) e o grupo 1, que vira o ícone do exe.
fn montar(ico: &[u8]) -> Vec<u8> {
    let u16_em = |i: usize| u16::from_le_bytes([ico[i], ico[i + 1]]);
    let u32_em = |i: usize| u32::from_le_bytes([ico[i], ico[i + 1], ico[i + 2], ico[i + 3]]) as usize;
    assert!(ico.len() >= 6 && u16_em(2) == 1, "não é um .ico");
    let n = u16_em(4) as usize;
    let mut res = Vec::new();
    // O 1º recurso de um .res é sempre o vazio.
    recurso(&mut res, 0, 0, 0, &[]);
    let mut grupo = ico[..6].to_vec();
    for i in 0..n {
        let e = 6 + 16 * i;
        let (tam, pos) = (u32_em(e + 8), u32_em(e + 12));
        recurso(&mut res, RT_ICON, i as u16 + 1, 0x1010, &ico[pos..pos + tam]);
        // A entrada do grupo é a do .ico com o id do recurso no lugar da posição.
        grupo.extend(&ico[e..e + 12]);
        grupo.extend((i as u16 + 1).to_le_bytes());
    }
    recurso(&mut res, RT_GROUP_ICON, 1, 0x1030, &grupo);
    res
}

/// Cabeçalho de 32 bytes (tipo e nome numéricos, idioma neutro) + dados, alinhado em 4.
fn recurso(res: &mut Vec<u8>, tipo: u16, id: u16, flags: u16, dados: &[u8]) {
    res.extend((dados.len() as u32).to_le_bytes());
    res.extend(32u32.to_le_bytes());
    for v in [0xFFFF, tipo, 0xFFFF, id] {
        res.extend(v.to_le_bytes());
    }
    res.extend(0u32.to_le_bytes());
    res.extend(flags.to_le_bytes());
    res.extend(0u16.to_le_bytes());
    res.extend([0; 8]);
    res.extend(dados);
    res.resize(res.len().next_multiple_of(4), 0);
}
