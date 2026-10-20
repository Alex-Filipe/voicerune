//! Sonda do WASAPI (só Windows): períodos e latências que cada dispositivo permite.
//! Serve para decidir se vale um backend próprio (IAudioClient3 ou modo exclusivo) no lugar do cpal.

use windows::core::{Interface, Result};
use windows::Win32::Devices::Properties::DEVPKEY_Device_FriendlyName;
use windows::Win32::Foundation::{PROPERTYKEY, S_OK};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;

/// Unidades de 100 ns → ms.
fn ms(hns: i64) -> f64 {
    hns as f64 / 10_000.0
}

fn quadros_ms(quadros: u32, taxa: u32) -> f64 {
    quadros as f64 * 1000.0 / taxa as f64
}

pub fn sondar(trechos: &[&str]) -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let col = en.EnumAudioEndpoints(eAll, DEVICE_STATE_ACTIVE)?;
        for i in 0..col.GetCount()? {
            let dev = col.Item(i)?;
            let nome = nome(&dev).unwrap_or_default();
            let minusculo = nome.to_lowercase();
            if !trechos.iter().any(|t| minusculo.contains(&t.to_lowercase())) {
                continue;
            }
            let saida = dev.cast::<IMMEndpoint>()?.GetDataFlow()? == eRender;
            println!("\n{nome} ({})", if saida { "saída" } else { "entrada" });
            if let Err(e) = sondar_dispositivo(&dev) {
                println!("  erro: {e}");
            }
        }
    }
    Ok(())
}

unsafe fn nome(dev: &IMMDevice) -> Result<String> {
    let props = dev.OpenPropertyStore(STGM_READ)?;
    let chave = &DEVPKEY_Device_FriendlyName as *const _ as *const PROPERTYKEY;
    let valor = props.GetValue(chave)?;
    let texto = PropVariantToStringAlloc(&valor)?;
    let s = texto.to_string().unwrap_or_default();
    CoTaskMemFree(Some(texto.0 as _));
    Ok(s)
}

unsafe fn sondar_dispositivo(dev: &IMMDevice) -> Result<()> {
    let cliente: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
    let fmt = cliente.GetMixFormat()?;
    let taxa = (*fmt).nSamplesPerSec;
    let (mut padrao, mut minimo) = (0i64, 0i64);
    cliente.GetDevicePeriod(Some(&mut padrao), Some(&mut minimo))?;
    println!(
        "  mixer: {taxa} Hz, {} canais, {} bits | período padrão {:.2} ms, mínimo {:.2} ms",
        { (*fmt).nChannels },
        { (*fmt).wBitsPerSample },
        ms(padrao),
        ms(minimo)
    );

    // Compartilhado com buffer 0 (o que o cpal faz com BufferSize::Default).
    let c: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
    c.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 0, 0, fmt, None)?;
    let buf = c.GetBufferSize()?;
    println!(
        "  compartilhado (como o cpal): buffer {buf} quadros = {:.2} ms | latência do stream {:.2} ms",
        quadros_ms(buf, taxa),
        ms(c.GetStreamLatency()?)
    );

    // IAudioClient3: período menor no modo compartilhado, se o driver permitir.
    match cliente.cast::<IAudioClient3>() {
        Ok(c3) => {
            let (mut p, mut passo, mut pmin, mut pmax) = (0u32, 0u32, 0u32, 0u32);
            c3.GetSharedModeEnginePeriod(fmt, &mut p, &mut passo, &mut pmin, &mut pmax)?;
            println!(
                "  IAudioClient3: período padrão {p}, mín {pmin} ({:.2} ms), máx {pmax}, passo {passo} quadros",
                quadros_ms(pmin, taxa)
            );
            if pmin < p {
                let c: IAudioClient3 = dev.Activate(CLSCTX_ALL, None)?;
                match c.InitializeSharedAudioStream(AUDCLNT_STREAMFLAGS_EVENTCALLBACK, pmin, fmt, None) {
                    Ok(()) => {
                        let buf = c.GetBufferSize()?;
                        println!(
                            "    com período {pmin}: buffer {buf} quadros = {:.2} ms | latência do stream {:.2} ms",
                            quadros_ms(buf, taxa),
                            ms(c.GetStreamLatency()?)
                        );
                    }
                    Err(e) => println!("    InitializeSharedAudioStream falhou: {e}"),
                }
            }
        }
        Err(_) => println!("  IAudioClient3: indisponível"),
    }

    // Exclusivo: testa o formato do mixer e PCM 16 bits estéreo 48 kHz.
    let pcm16 = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 2,
        nSamplesPerSec: 48_000,
        nAvgBytesPerSec: 48_000 * 4,
        nBlockAlign: 4,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    for (rotulo, f) in [("formato do mixer", fmt as *const WAVEFORMATEX), ("PCM 16 bits", &pcm16 as *const _)] {
        if cliente.IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, f, None) != S_OK {
            println!("  exclusivo, {rotulo}: não suportado");
            continue;
        }
        let c: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
        match c.Initialize(AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, minimo, minimo, f, None) {
            Ok(()) => {
                let buf = c.GetBufferSize()?;
                println!(
                    "  exclusivo, {rotulo}: buffer {buf} quadros = {:.2} ms | latência do stream {:.2} ms",
                    quadros_ms(buf, (*f).nSamplesPerSec),
                    ms(c.GetStreamLatency()?)
                );
            }
            Err(e) => println!("  exclusivo, {rotulo}: suportado, mas Initialize falhou: {e}"),
        }
    }
    CoTaskMemFree(Some(fmt as _));
    Ok(())
}
