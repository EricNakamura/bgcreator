use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

pub const EXPORT_WIDTH: u32 = 1920;
pub const EXPORT_HEIGHT: u32 = 1200;
pub const FPS: u32 = 30;
pub const LOOP_SECONDS: f32 = 12.0;
pub const DEFAULT_PRESET_PATH: &str = "presets/shine.toml";
pub const DEFAULT_FONT: &str = "assets/Inter.ttf";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneId {
    Shine,
    Gradient,
}

impl SceneId {
    pub const ALL: [Self; 2] = [Self::Shine, Self::Gradient];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Shine => "Shine",
            Self::Gradient => "Gradient",
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Shine => "shine",
            Self::Gradient => "gradient",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientStyle {
    #[default]
    Horizon,
    Aurora,
    Mesh,
    Waves,
    Haze,
    Prism,
}

impl GradientStyle {
    pub const ALL: [Self; 6] = [
        Self::Horizon,
        Self::Aurora,
        Self::Mesh,
        Self::Waves,
        Self::Haze,
        Self::Prism,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Horizon => "Horizonte",
            Self::Aurora => "Aurora",
            Self::Mesh => "Mesh",
            Self::Waves => "Ondas",
            Self::Haze => "Névoa",
            Self::Prism => "Prisma",
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Horizon => "horizon",
            Self::Aurora => "aurora",
            Self::Mesh => "mesh",
            Self::Waves => "waves",
            Self::Haze => "haze",
            Self::Prism => "prism",
        }
    }

    pub const fn index(self) -> u32 {
        match self {
            Self::Horizon => 0,
            Self::Aurora => 1,
            Self::Mesh => 2,
            Self::Waves => 3,
            Self::Haze => 4,
            Self::Prism => 5,
        }
    }

    pub fn default_palette(self) -> Palette {
        match self {
            Self::Horizon => Palette::dawn(),
            Self::Waves => Palette::sorbet(),
            Self::Aurora => Palette::mint(),
            Self::Mesh => Palette::sorbet(),
            Self::Haze => Palette::violet(),
            Self::Prism => Palette::ice(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub name: String,
    pub colors: [String; 4],
}

impl Palette {
    fn from_hex(name: &str, colors: [&str; 4]) -> Self {
        Self {
            name: name.into(),
            colors: colors.map(|color| color.to_owned()),
        }
    }

    pub fn gold() -> Self {
        Self::from_hex("gold", ["#050403", "#2a1608", "#c96a1a", "#fff0b8"])
    }

    pub fn sand() -> Self {
        Self::from_hex("sand", ["#0b0805", "#3a2a1a", "#c99a4a", "#fff1d0"])
    }

    pub fn ember() -> Self {
        Self::from_hex("ember", ["#0a0504", "#33100a", "#d94a1e", "#ffe3b0"])
    }

    pub fn peach() -> Self {
        Self::from_hex("peach", ["#0b0508", "#3a1a2a", "#f08a7a", "#fff0dc"])
    }

    pub fn crimson() -> Self {
        Self::from_hex("crimson", ["#0a0305", "#3a0b17", "#c42a4a", "#ffd9df"])
    }

    pub fn wine() -> Self {
        Self::from_hex("wine", ["#080306", "#2e0a22", "#8e2a6a", "#ffd9ee"])
    }

    pub fn sorbet() -> Self {
        Self::from_hex("sorbet", ["#08101f", "#5a1e5a", "#ff7a59", "#ffe8a3"])
    }

    pub fn dawn() -> Self {
        Self::from_hex("dawn", ["#070b1a", "#3a2a5c", "#e47b5a", "#ffe6b0"])
    }

    pub fn ice() -> Self {
        Self::from_hex("ice", ["#02050a", "#071a2e", "#2f8fd6", "#e6f7ff"])
    }

    pub fn sky() -> Self {
        Self::from_hex("sky", ["#03070d", "#1a2f45", "#6fb7e8", "#f2fbff"])
    }

    pub fn ocean() -> Self {
        Self::from_hex("ocean", ["#02060b", "#062a3a", "#1a9cc4", "#d7fbff"])
    }

    pub fn steel() -> Self {
        Self::from_hex("steel", ["#050608", "#1e2630", "#7c93a8", "#f2f7fa"])
    }

    pub fn midnight() -> Self {
        Self::from_hex("midnight", ["#01030a", "#0b1638", "#3b5bdb", "#dce8ff"])
    }

    pub fn violet() -> Self {
        Self::from_hex("violet", ["#05020a", "#1a0b33", "#7b3fd4", "#ffe4f7"])
    }

    pub fn lavender() -> Self {
        Self::from_hex("lavender", ["#08050f", "#2b1d45", "#9a7bd6", "#f2e6ff"])
    }

    pub fn mint() -> Self {
        Self::from_hex("mint", ["#020f12", "#0b3b3a", "#28c2a0", "#e6fff7"])
    }

    pub fn emerald() -> Self {
        Self::from_hex("emerald", ["#020806", "#0b2e22", "#1fbf7a", "#dcfff0"])
    }

    pub fn forest() -> Self {
        Self::from_hex("forest", ["#040704", "#1c2a12", "#7aa33a", "#f2ffd9"])
    }

    pub fn all() -> [Self; 18] {
        [
            Self::gold(),
            Self::sand(),
            Self::ember(),
            Self::peach(),
            Self::crimson(),
            Self::wine(),
            Self::sorbet(),
            Self::dawn(),
            Self::ice(),
            Self::sky(),
            Self::ocean(),
            Self::steel(),
            Self::midnight(),
            Self::violet(),
            Self::lavender(),
            Self::mint(),
            Self::emerald(),
            Self::forest(),
        ]
    }

    pub fn rgba(&self) -> [[f32; 4]; 4] {
        let mut out = [[0.0, 0.0, 0.0, 1.0]; 4];
        for (slot, hex) in out.iter_mut().zip(&self.colors) {
            let [r, g, b] = hex_to_rgb(hex).unwrap_or([0.0, 0.0, 0.0]);
            *slot = [r, g, b, 1.0];
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorGrade {
    pub exposure: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub temperature: f32,
    pub tint: f32,
    pub gamma: f32,
    pub sharpness: f32,
}

impl Default for ColorGrade {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            saturation: 1.0,
            temperature: 0.0,
            tint: 0.0,
            gamma: 1.0,
            sharpness: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarpMap {
    #[default]
    Fbm,
    Turbulence,
    Voronoi,
    Waves,
    Swirl,
    Ripple,
}

impl WarpMap {
    pub const ALL: [Self; 6] = [
        Self::Fbm,
        Self::Turbulence,
        Self::Voronoi,
        Self::Waves,
        Self::Swirl,
        Self::Ripple,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Fbm => "FBM",
            Self::Turbulence => "Turbulência",
            Self::Voronoi => "Voronoi",
            Self::Waves => "Ondas",
            Self::Swirl => "Swirl",
            Self::Ripple => "Ripple",
        }
    }

    pub const fn index(self) -> u32 {
        match self {
            Self::Fbm => 0,
            Self::Turbulence => 1,
            Self::Voronoi => 2,
            Self::Waves => 3,
            Self::Swirl => 4,
            Self::Ripple => 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WarpConfig {
    pub enabled: bool,
    pub map: WarpMap,
    pub strength: f32,
    pub scale: f32,
    pub speed: f32,
    pub seed: f32,
}

impl Default for WarpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            map: WarpMap::Fbm,
            strength: 0.02,
            scale: 1.0,
            speed: 0.3,
            seed: 3.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BarConfig {
    pub enabled: bool,
    pub style: GradientStyle,
    pub palette: Palette,
    pub phase: f32,
    pub mirrored: bool,
}

impl BarConfig {
    pub fn defaults() -> [Self; 4] {
        [
            Self {
                enabled: true,
                style: GradientStyle::Aurora,
                palette: Palette::mint(),
                phase: 0.0,
                mirrored: false,
            },
            Self {
                enabled: true,
                style: GradientStyle::Mesh,
                palette: Palette::sorbet(),
                phase: 0.25,
                mirrored: true,
            },
            Self {
                enabled: true,
                style: GradientStyle::Waves,
                palette: Palette::dawn(),
                phase: 0.5,
                mirrored: false,
            },
            Self {
                enabled: true,
                style: GradientStyle::Haze,
                palette: Palette::violet(),
                phase: 0.75,
                mirrored: true,
            },
        ]
    }
}

fn default_bar_gap() -> f32 {
    0.04
}

fn default_bar_softness() -> f32 {
    0.02
}

fn default_bars() -> [BarConfig; 4] {
    BarConfig::defaults()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextDirection {
    #[default]
    Left,
    Right,
}

impl TextDirection {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Left => "Esquerda",
            Self::Right => "Direita",
        }
    }

    pub const fn sign(self) -> f32 {
        // O offset é aplicado na coordenada da textura: somar desloca o
        // padrão para a esquerda na tela. Por isso "direita" usa -1.
        match self {
            Self::Left => 1.0,
            Self::Right => -1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextLayout {
    #[default]
    Single,
    Fill,
}

impl TextLayout {
    pub const ALL: [Self; 2] = [Self::Single, Self::Fill];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Single => "Linha única",
            Self::Fill => "Preencher tela",
        }
    }

    pub const fn fill(self) -> bool {
        matches!(self, Self::Fill)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextMotion {
    #[default]
    Same,
    Alternate,
}

impl TextMotion {
    pub const ALL: [Self; 2] = [Self::Same, Self::Alternate];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Same => "Mesmo lado",
            Self::Alternate => "Alternado",
        }
    }

    pub const fn index(self) -> u32 {
        match self {
            Self::Same => 0,
            Self::Alternate => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextMask {
    #[default]
    Off,
    Positive,
    Negative,
}

impl TextMask {
    pub const ALL: [Self; 3] = [Self::Off, Self::Positive, Self::Negative];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Nenhuma",
            Self::Positive => "Positiva",
            Self::Negative => "Negativa",
        }
    }

    pub const fn index(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Positive => 1,
            Self::Negative => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextConfig {
    pub enabled: bool,
    pub text: String,
    pub font: String,
    pub layout: TextLayout,
    pub mask: TextMask,
    pub motion: TextMotion,
    pub direction: TextDirection,
    pub speed: f32,
    pub offset: f32,
    pub row_spacing: f32,
    pub size: f32,
    pub color: String,
    pub opacity: f32,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            text: "Jesus".into(),
            font: DEFAULT_FONT.into(),
            layout: TextLayout::Single,
            mask: TextMask::Off,
            motion: TextMotion::Same,
            direction: TextDirection::Right,
            speed: 1.0,
            offset: 0.0,
            row_spacing: 1.35,
            size: 120.0,
            color: "#ffffff".into(),
            opacity: 0.9,
        }
    }
}

impl TextConfig {
    pub fn is_active(&self) -> bool {
        !self.text.trim().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub scene: SceneId,
    #[serde(default)]
    pub gradient_style: GradientStyle,
    #[serde(default)]
    pub grade: ColorGrade,
    #[serde(default)]
    pub bars_enabled: bool,
    #[serde(default = "default_bar_gap")]
    pub bar_gap: f32,
    #[serde(default = "default_bar_softness")]
    pub bar_softness: f32,
    #[serde(default = "default_bars")]
    pub bars: [BarConfig; 4],
    #[serde(default)]
    pub warp: WarpConfig,
    #[serde(default)]
    pub text: TextConfig,
    pub seed: f32,
    pub palette: Palette,
    pub speed: f32,
    pub scale: f32,
    pub intensity: f32,
    pub vignette: f32,
    pub center_dim: f32,
}

impl Preset {
    pub fn shine() -> Self {
        Self {
            name: "Shine dourado".into(),
            scene: SceneId::Shine,
            gradient_style: GradientStyle::Horizon,
            grade: ColorGrade::default(),
            bars_enabled: false,
            bar_gap: default_bar_gap(),
            bar_softness: default_bar_softness(),
            bars: default_bars(),
            warp: WarpConfig::default(),
            text: TextConfig::default(),
            seed: 7.0,
            palette: Palette::gold(),
            speed: 0.35,
            scale: 1.0,
            intensity: 1.0,
            vignette: 0.55,
            center_dim: 0.55,
        }
    }

    pub fn gradient(style: GradientStyle) -> Self {
        let (center_dim, vignette) = match style {
            GradientStyle::Horizon => (0.6, 0.4),
            GradientStyle::Aurora => (0.45, 0.35),
            GradientStyle::Mesh => (0.5, 0.35),
            GradientStyle::Waves => (0.5, 0.4),
            GradientStyle::Haze => (0.55, 0.35),
            GradientStyle::Prism => (0.5, 0.4),
        };
        Self {
            name: format!("Gradient {}", style.label()),
            scene: SceneId::Gradient,
            gradient_style: style,
            grade: ColorGrade::default(),
            bars_enabled: false,
            bar_gap: default_bar_gap(),
            bar_softness: default_bar_softness(),
            bars: default_bars(),
            warp: WarpConfig::default(),
            text: TextConfig::default(),
            seed: 11.0,
            palette: style.default_palette(),
            speed: 0.3,
            scale: 1.0,
            intensity: 1.0,
            vignette,
            center_dim,
        }
    }

    pub fn for_scene(scene: SceneId) -> Self {
        match scene {
            SceneId::Shine => Self::shine(),
            SceneId::Gradient => Self::gradient(GradientStyle::Horizon),
        }
    }

    pub fn file_stem(&self) -> String {
        match self.scene {
            SceneId::Shine => format!("shine-seed{:.0}", self.seed),
            SceneId::Gradient => {
                format!(
                    "gradient-{}-seed{:.0}",
                    self.gradient_style.slug(),
                    self.seed
                )
            }
        }
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("lendo preset {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("interpretando preset {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("criando {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("serializando preset")?;
        std::fs::write(path, text).with_context(|| format!("gravando preset {}", path.display()))
    }
}

impl Default for Preset {
    fn default() -> Self {
        Self::shine()
    }
}

pub fn hex_to_rgb(hex: &str) -> Option<[f32; 3]> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| {
        u8::from_str_radix(&hex[i..i + 2], 16)
            .ok()
            .map(|v| v as f32 / 255.0)
    };
    Some([channel(0)?, channel(2)?, channel(4)?])
}

pub fn loop_frames() -> u32 {
    (LOOP_SECONDS * FPS as f32).round() as u32
}

pub fn frame_time(frame: u32, total_frames: u32) -> f32 {
    if total_frames == 0 {
        return 0.0;
    }
    frame as f32 / total_frames as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(hex_to_rgb("#ff8000"), Some([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(hex_to_rgb("00ff00"), Some([0.0, 1.0, 0.0]));
        assert_eq!(hex_to_rgb("#fff"), None);
        assert_eq!(hex_to_rgb("#gg0000"), None);
    }

    #[test]
    fn all_palettes_are_valid() {
        let palettes = Palette::all();
        assert_eq!(palettes.len(), 18);
        let mut names = std::collections::HashSet::new();
        for palette in &palettes {
            assert!(
                names.insert(palette.name.clone()),
                "nome duplicado: {}",
                palette.name
            );
            for color in &palette.colors {
                assert!(
                    hex_to_rgb(color).is_some(),
                    "cor inválida em {}: {color}",
                    palette.name
                );
            }
        }
    }

    #[test]
    fn preset_roundtrip() {
        let preset = Preset::shine();
        let text = toml::to_string_pretty(&preset).unwrap();
        let parsed: Preset = toml::from_str(&text).unwrap();
        assert_eq!(preset, parsed);
    }

    #[test]
    fn gradient_roundtrip_and_backcompat() {
        for style in GradientStyle::ALL {
            let preset = Preset::gradient(style);
            let text = toml::to_string_pretty(&preset).unwrap();
            let parsed: Preset = toml::from_str(&text).unwrap();
            assert_eq!(preset, parsed);
            assert_eq!(parsed.gradient_style.index(), style.index());
        }

        let old = r##"
name = "Shine antigo"
scene = "shine"
seed = 7.0
speed = 0.35
scale = 1.0
intensity = 1.0
vignette = 0.55
center_dim = 0.55

[palette]
name = "gold"
colors = ["#050403", "#2a1608", "#c96a1a", "#fff0b8"]
"##;
        let parsed: Preset = toml::from_str(old).unwrap();
        assert_eq!(parsed.gradient_style, GradientStyle::Horizon);
        assert_eq!(parsed.grade, ColorGrade::default());
        assert!(!parsed.bars_enabled);
        assert_eq!(parsed.warp, WarpConfig::default());
        assert_eq!(parsed.text, TextConfig::default());
    }

    #[test]
    fn text_and_warp_roundtrip() {
        let mut preset = Preset::gradient(GradientStyle::Aurora);
        preset.bars_enabled = true;
        preset.text.enabled = true;
        preset.text.mask = TextMask::Negative;
        preset.warp.enabled = true;
        preset.warp.map = WarpMap::Swirl;
        let text = toml::to_string_pretty(&preset).unwrap();
        let parsed: Preset = toml::from_str(&text).unwrap();
        assert_eq!(parsed.text.mask, TextMask::Negative);
        assert_eq!(preset, parsed);
    }

    #[test]
    fn shipped_gradient_preset_parses() {
        let text = include_str!("../presets/gradient.toml");
        let preset: Preset = toml::from_str(text).unwrap();
        assert_eq!(preset.scene, SceneId::Gradient);
        assert_eq!(preset.gradient_style, GradientStyle::Horizon);
        assert_eq!(preset.file_stem(), "gradient-horizon-seed11");
    }

    #[test]
    fn loop_is_exact() {
        assert_eq!(loop_frames(), 360);
        assert_eq!(frame_time(0, 360), 0.0);
        assert_eq!(frame_time(180, 360), 0.5);
        assert!((frame_time(359, 360) - 359.0 / 360.0).abs() < 1e-6);
        assert_eq!(frame_time(0, 0), 0.0);
    }
}
