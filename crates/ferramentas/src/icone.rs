//! `assets/icone/voicerune.ico` a partir dos SVGs: 16–48 px da versão pequena (`crates/ui/ui/icone.svg`,
//! sem fundo, a mesma da janela e da bandeja) e 64–256 px da grande (`assets/icone/voicerune.svg`, com
//! fundo). É o `.ico` que o `build.rs` do app embute no exe. Os PNG e o `.icns` ainda saem fora do projeto.

use std::fs;
use std::path::{Path, PathBuf};

use resvg::{tiny_skia, usvg};

use crate::Res;

const PEQUENOS: [u32; 6] = [16, 20, 24, 32, 40, 48];
const GRANDES: [u32; 3] = [64, 128, 256];

fn abrir(caminho: &Path) -> Res<usvg::Tree> {
    let dados = fs::read(caminho).map_err(|e| format!("{}: {e}", caminho.display()))?;
    Ok(usvg::Tree::from_data(&dados, &usvg::Options::default())?)
}

/// PNG quadrado de `lado` px.
fn desenhar(svg: &usvg::Tree, lado: u32) -> Res<Vec<u8>> {
    let mut p = tiny_skia::Pixmap::new(lado, lado).ok_or("tamanho inválido")?;
    let escala = lado as f32 / svg.size().width();
    resvg::render(svg, tiny_skia::Transform::from_scale(escala, escala), &mut p.as_mut());
    Ok(p.encode_png()?)
}

/// `.ico` com as imagens em PNG (aceito pelo Windows desde o Vista, em qualquer tamanho).
fn ico(imagens: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut b = vec![0, 0, 1, 0];
    b.extend((imagens.len() as u16).to_le_bytes());
    let mut pos = 6 + 16 * imagens.len();
    for (lado, png) in imagens {
        // 0 = 256 px.
        let l = if *lado >= 256 { 0 } else { *lado as u8 };
        b.extend([l, l, 0, 0]);
        b.extend(1u16.to_le_bytes());
        b.extend(32u16.to_le_bytes());
        b.extend((png.len() as u32).to_le_bytes());
        b.extend((pos as u32).to_le_bytes());
        pos += png.len();
    }
    for (_, png) in imagens {
        b.extend(png);
    }
    b
}

pub fn gerar() -> Res<()> {
    let raiz: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let pequeno = abrir(&raiz.join("crates/ui/ui/icone.svg"))?;
    let grande = abrir(&raiz.join("assets/icone/voicerune.svg"))?;
    let mut imagens = Vec::new();
    for lado in PEQUENOS {
        imagens.push((lado, desenhar(&pequeno, lado)?));
    }
    for lado in GRANDES {
        imagens.push((lado, desenhar(&grande, lado)?));
    }
    let saida = raiz.join("assets/icone/voicerune.ico");
    let dados = ico(&imagens);
    fs::write(&saida, &dados)?;
    println!("{} ({} imagens, {} bytes)", saida.display(), imagens.len(), dados.len());
    Ok(())
}
