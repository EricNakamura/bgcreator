use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use fontdue::layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle};
use fontdue::{Font, FontSettings};

use crate::model::{DEFAULT_FONT, TextConfig};

pub const ATLAS_WIDTH: u32 = 2048;
pub const ATLAS_HEIGHT: u32 = 256;

// Tamanho da fonte dentro da textura. O controle de tamanho da UI escala a
// textura na tela; este valor só define a resolução do desenho.
const FONT_PX: f32 = 180.0;

// Respiro nas laterais do tile. Ele vira o espaço entre uma repetição e a
// seguinte; é pequeno de propósito para a palavra seguinte ficar perto.
const PADDING: f32 = 24.0;

const FONT_BYTES: &[u8] = include_bytes!("../assets/Inter.ttf");

#[derive(Clone, Debug)]
pub struct FontOption {
    pub label: String,
    pub path: String,
}

fn fallback_font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        Font::from_bytes(FONT_BYTES, FontSettings::default()).expect("fonte Inter inválida")
    })
}

fn load_font(path: &str) -> Option<Font> {
    let bytes = std::fs::read(path).ok()?;
    Font::from_bytes(bytes, FontSettings::default()).ok()
}

pub fn font_label(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Fonte")
        .to_owned()
}

/// Lista a fonte embutida, a pasta `fonts/` e as fontes do sistema.
/// A varredura acontece só quando o seletor é aberto.
pub fn available_fonts() -> Vec<FontOption> {
    let mut fonts = vec![FontOption {
        label: "Inter".into(),
        path: DEFAULT_FONT.into(),
    }];
    let mut seen = HashSet::from([DEFAULT_FONT.to_owned()]);
    let mut dirs = vec![PathBuf::from("fonts")];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join("Library/Fonts"));
    }
    dirs.push(PathBuf::from("/Library/Fonts"));
    dirs.push(PathBuf::from("/System/Library/Fonts/Supplemental"));

    for dir in dirs {
        collect_fonts(&dir, &mut fonts, &mut seen);
    }
    fonts[1..].sort_by_key(|a| a.label.to_lowercase());
    fonts
}

fn collect_fonts(dir: &Path, fonts: &mut Vec<FontOption>, seen: &mut HashSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_font_file(path))
        .collect();
    paths.sort();

    for path in paths {
        let path = path.to_string_lossy().into_owned();
        if seen.insert(path.clone()) {
            fonts.push(FontOption {
                label: font_label(&path),
                path,
            });
        }
    }
}

fn is_font_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("ttf" | "otf")
    )
}

/// Textura rasterizada e a largura útil do tile, em pixels do atlas.
/// O shader repete só essa faixa; o resto da textura fica transparente.
pub struct TextAtlas {
    pub data: Vec<u8>,
    pub tile_px: u32,
}

/// Rasteriza uma frase num atlas RGBA de 2048x256.
/// O RGB é branco e o alpha é a cobertura da letra; a cor final vem do shader.
pub fn rasterize(config: &TextConfig) -> TextAtlas {
    let mut data = vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize];
    if !config.enabled || !config.is_active() {
        return TextAtlas { data, tile_px: 1 };
    }

    let loaded = load_font(&config.font);
    let font = loaded.as_ref().unwrap_or_else(|| fallback_font());
    let tile_px = draw_text(&mut data, config, font);
    TextAtlas { data, tile_px }
}

fn draw_text(data: &mut [u8], config: &TextConfig, font: &Font) -> u32 {
    // Se a frase for maior que a textura, reduz o px até caber.
    let max_width = ATLAS_WIDTH as f32 - PADDING * 2.0;
    let max_height = ATLAS_HEIGHT as f32 - PADDING * 2.0;
    let mut px = FONT_PX;
    let mut glyphs = layout_line(font, &config.text, px);
    for _ in 0..4 {
        let Some(bounds) = bounds(&glyphs) else {
            return 1;
        };
        if bounds.width <= max_width && bounds.height <= max_height {
            break;
        }
        let fit = (max_width / bounds.width.max(1.0)).min(max_height / bounds.height.max(1.0));
        if !fit.is_finite() || fit >= 1.0 {
            break;
        }
        px = (px * fit).max(1.0);
        glyphs = layout_line(font, &config.text, px);
    }

    let Some(bounds) = bounds(&glyphs) else {
        return 1;
    };

    // O tile é a largura real do texto + respiro dos dois lados. É isso que
    // faz "Jesus Jesus" ficar próximo, independentemente do tamanho da palavra.
    let tile_px = ((bounds.width + PADDING * 2.0).ceil() as u32).clamp(1, ATLAS_WIDTH);
    let offset_x = (tile_px as f32 - bounds.width) * 0.5 - bounds.min_x;
    let offset_y = (ATLAS_HEIGHT as f32 - bounds.height) * 0.5 - bounds.min_y;

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
                if x < 0 || y < 0 || x >= tile_px as i32 || y >= ATLAS_HEIGHT as i32 {
                    continue;
                }
                let index = ((y as u32 * ATLAS_WIDTH + x as u32) * 4) as usize;
                data[index] = 255;
                data[index + 1] = 255;
                data[index + 2] = 255;
                data[index + 3] = coverage;
            }
        }
    }

    tile_px
}

fn layout_line(font: &Font, text: &str, px: f32) -> Vec<fontdue::layout::GlyphPosition> {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        x: 0.0,
        y: 0.0,
        max_width: None,
        max_height: None,
        ..Default::default()
    });
    layout.append(&[font], &TextStyle::new(text, px, 0));
    layout.glyphs().clone()
}

struct Bounds {
    min_x: f32,
    min_y: f32,
    width: f32,
    height: f32,
}

fn bounds(glyphs: &[fontdue::layout::GlyphPosition]) -> Option<Bounds> {
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
    if !min_x.is_finite() || !max_x.is_finite() || !min_y.is_finite() || !max_y.is_finite() {
        return None;
    }
    Some(Bounds {
        min_x,
        min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atlas(text: &str) -> TextAtlas {
        rasterize(&TextConfig {
            enabled: true,
            text: text.into(),
            ..TextConfig::default()
        })
    }

    #[test]
    fn rasterizes_text_and_tight_tile() {
        let atlas = atlas("Jesus");
        assert_eq!(atlas.data.len(), (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize);
        assert!(atlas.tile_px > 0 && atlas.tile_px < ATLAS_WIDTH);
        assert!(atlas.data.chunks_exact(4).any(|pixel| pixel[3] > 0));

        // Fora da faixa do tile não pode sobrar pixel: o shader só amostra
        // [0, tile_px], e o resto precisa ser transparente.
        for row in 0..ATLAS_HEIGHT {
            for x in atlas.tile_px..ATLAS_WIDTH {
                let index = ((row * ATLAS_WIDTH + x) * 4) as usize;
                assert_eq!(atlas.data[index + 3], 0);
            }
        }

        std::fs::create_dir_all("target").unwrap();
        crate::encode::write_png(
            std::path::Path::new("target/text-atlas.png"),
            ATLAS_WIDTH,
            ATLAS_HEIGHT,
            &atlas.data,
        )
        .expect("png atlas");
    }

    #[test]
    fn longer_text_has_wider_tile() {
        let short = atlas("Jesus").tile_px;
        let long = atlas("Jesus Cristo").tile_px;
        assert!(long > short, "tile curto {short}, tile longo {long}");
    }

    #[test]
    fn missing_font_falls_back_to_inter() {
        let atlas = rasterize(&TextConfig {
            enabled: true,
            font: "fonts/nao-existe.ttf".into(),
            ..TextConfig::default()
        });
        assert!(atlas.tile_px > 1);
        assert!(atlas.data.chunks_exact(4).any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn disabled_text_is_transparent() {
        let atlas = rasterize(&TextConfig::default());
        assert!(atlas.data.iter().all(|byte| *byte == 0));
        assert_eq!(atlas.tile_px, 1);
    }
}
