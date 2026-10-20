use std::sync::OnceLock;

use fontdue::layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle};
use fontdue::{Font, FontSettings};

use crate::model::{TextConfig, TextLine};

pub const ATLAS_WIDTH: u32 = 2048;
pub const ATLAS_HEIGHT: u32 = 256;
pub const MAX_LINES: usize = 4;

// Tamanho da fonte dentro da textura. O controle de tamanho da UI escala a
// textura na tela; este valor só define a resolução do desenho.
const FONT_PX: f32 = 180.0;
const PADDING: f32 = 24.0;

const FONT_BYTES: &[u8] = include_bytes!("../assets/Inter.ttf");

fn font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        Font::from_bytes(FONT_BYTES, FontSettings::default()).expect("fonte Inter inválida")
    })
}

/// Rasteriza até 4 linhas de texto num atlas RGBA.
/// Cada linha ocupa uma camada de 2048x256. O RGB é branco e o alpha é a
/// cobertura da letra; a cor final é aplicada no shader.
pub fn rasterize(config: &TextConfig) -> Vec<u8> {
    let mut data = vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * MAX_LINES as u32 * 4) as usize];
    if !config.enabled {
        return data;
    }

    let font = font();
    for (layer, line) in config.lines.iter().take(MAX_LINES).enumerate() {
        if !line.is_active() {
            continue;
        }
        draw_line(&mut data, layer, line, font);
    }
    data
}

fn draw_line(data: &mut [u8], layer: usize, line: &TextLine, font: &Font) {
    // Se o texto for maior que a textura, reduz o px até caber.
    let mut px = FONT_PX;
    let mut glyphs = layout_line(font, line, px);
    for _ in 0..3 {
        let width = text_width(&glyphs);
        if width <= ATLAS_WIDTH as f32 - PADDING * 2.0 {
            break;
        }
        px *= (ATLAS_WIDTH as f32 - PADDING * 2.0) / width.max(1.0);
        glyphs = layout_line(font, line, px);
    }

    let min_x = glyphs
        .iter()
        .map(|glyph| glyph.x)
        .fold(f32::INFINITY, f32::min);
    let max_x = glyphs
        .iter()
        .map(|glyph| glyph.x + glyph.width as f32)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = glyphs
        .iter()
        .map(|glyph| glyph.y)
        .fold(f32::INFINITY, f32::min);
    let max_y = glyphs
        .iter()
        .map(|glyph| glyph.y + glyph.height as f32)
        .fold(f32::NEG_INFINITY, f32::max);

    let offset_x = (ATLAS_WIDTH as f32 - (max_x - min_x)) * 0.5 - min_x;
    let offset_y = (ATLAS_HEIGHT as f32 - (max_y - min_y)) * 0.5 - min_y;

    for glyph in glyphs {
        let (metrics, bitmap) = font.rasterize(glyph.parent, px);
        let gx = (glyph.x + offset_x).round() as i32;
        let gy = (glyph.y + offset_y).round() as i32;

        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let coverage = bitmap[row * metrics.width + col];
                if coverage == 0 {
                    continue;
                }
                let x = gx + col as i32;
                let y = gy + row as i32;
                if x < 0 || y < 0 || x >= ATLAS_WIDTH as i32 || y >= ATLAS_HEIGHT as i32 {
                    continue;
                }
                let index = ((layer as u32 * ATLAS_HEIGHT + y as u32) * ATLAS_WIDTH + x as u32) * 4;
                let index = index as usize;
                data[index] = 255;
                data[index + 1] = 255;
                data[index + 2] = 255;
                data[index + 3] = coverage;
            }
        }
    }
}

fn layout_line(font: &Font, line: &TextLine, px: f32) -> Vec<fontdue::layout::GlyphPosition> {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        x: 0.0,
        y: 0.0,
        max_width: None,
        max_height: None,
        ..Default::default()
    });
    layout.append(&[font], &TextStyle::new(&line.text, px, 0));
    layout.glyphs().clone()
}

fn text_width(glyphs: &[fontdue::layout::GlyphPosition]) -> f32 {
    let min_x = glyphs
        .iter()
        .map(|glyph| glyph.x)
        .fold(f32::INFINITY, f32::min);
    let max_x = glyphs
        .iter()
        .map(|glyph| glyph.x + glyph.width as f32)
        .fold(f32::NEG_INFINITY, f32::max);
    max_x - min_x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TextConfig;

    #[test]
    fn rasterizes_default_lines() {
        let config = TextConfig {
            enabled: true,
            ..TextConfig::default()
        };
        let data = rasterize(&config);
        assert_eq!(
            data.len(),
            (ATLAS_WIDTH * ATLAS_HEIGHT * MAX_LINES as u32 * 4) as usize
        );
        assert!(data.chunks_exact(4).any(|pixel| pixel[3] > 0));

        std::fs::create_dir_all("target").unwrap();
        let layer_len = (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize;
        crate::encode::write_png(
            std::path::Path::new("target/text-atlas.png"),
            ATLAS_WIDTH,
            ATLAS_HEIGHT,
            &data[..layer_len],
        )
        .expect("png atlas");
    }

    #[test]
    fn disabled_text_is_transparent() {
        let data = rasterize(&TextConfig::default());
        assert!(data.iter().all(|byte| *byte == 0));
    }
}
