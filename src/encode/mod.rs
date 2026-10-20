use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use anyhow::Context;

pub mod mp4;

pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("criando {}", parent.display()))?;
    }
    let file = File::create(path).with_context(|| format!("criando {}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().context("escrevendo cabeçalho PNG")?;
    writer
        .write_image_data(rgba)
        .context("escrevendo pixels PNG")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_roundtrip() {
        let dir = std::env::temp_dir().join("bgcreator-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.png");
        let pixels: [u8; 16] = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        write_png(&path, 2, 2, &pixels).unwrap();

        let decoder = png::Decoder::new(std::io::BufReader::new(File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buffer).unwrap();
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!(&buffer[..info.buffer_size()], &pixels);
    }
}
