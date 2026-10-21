#![allow(clippy::excessive_precision)] // constantes OKLab copiadas com precisão de referência

use std::f32::consts::TAU;

use crate::model::hex_to_rgb;

/// Cor em OKLCH: luminosidade percebida, croma e ângulo de matiz.
/// É o espaço usado para gerar paletas porque mexer em L e C mantém a cor
/// previsível, diferente de HSL, que costuma estourar saturação.
#[derive(Clone, Copy, Debug)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

pub fn hex_to_oklch(hex: &str) -> Option<Oklch> {
    let rgb = hex_to_rgb(hex)?;
    let lab = linear_to_oklab(rgb.map(srgb_to_linear));
    let c = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    let h = if c < 1e-6 {
        0.0
    } else {
        lab[2].atan2(lab[1]).rem_euclid(TAU)
    };
    Some(Oklch { l: lab[0], c, h })
}

pub fn oklch_to_hex(color: Oklch) -> String {
    rgb_to_hex(oklch_to_rgb(color))
}

pub fn rgb_to_hex(rgb: [f32; 3]) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(rgb[0]),
        channel(rgb[1]),
        channel(rgb[2])
    )
}

/// Converte OKLCH para sRGB, reduzindo o croma até caber no gamut.
/// Sem isso, cores muito saturadas podem gerar canais negativos ou > 1.
pub fn oklch_to_rgb(color: Oklch) -> [f32; 3] {
    let l = color.l.clamp(0.0, 1.0);
    let mut c = color.c.max(0.0);
    for _ in 0..32 {
        let rgb = oklch_to_rgb_raw(l, c, color.h);
        if rgb
            .iter()
            .all(|value| *value >= -0.0005 && *value <= 1.0005)
        {
            return rgb.map(|value| value.clamp(0.0, 1.0));
        }
        c *= 0.9;
    }
    oklch_to_rgb_raw(l, c, color.h).map(|value| value.clamp(0.0, 1.0))
}

fn oklch_to_rgb_raw(l: f32, c: f32, h: f32) -> [f32; 3] {
    let a = c * h.cos();
    let b = c * h.sin();
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    let linear = [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ];
    linear.map(linear_to_srgb)
}

fn linear_to_oklab(rgb: [f32; 3]) -> [f32; 3] {
    let l = 0.4122214708 * rgb[0] + 0.5363325363 * rgb[1] + 0.0514459929 * rgb[2];
    let m = 0.2119034982 * rgb[0] + 0.6806995451 * rgb[1] + 0.1073969566 * rgb[2];
    let s = 0.0883024619 * rgb[0] + 0.2817188376 * rgb[1] + 0.6299787005 * rgb[2];
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();

    [
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    ]
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_colors_roundtrip() {
        for hex in ["#ff0000", "#00ff00", "#0000ff", "#ffffff", "#000000"] {
            let oklch = hex_to_oklch(hex).unwrap();
            let back = oklch_to_hex(oklch);
            let expected = hex_to_rgb(hex).unwrap();
            let actual = hex_to_rgb(&back).unwrap();
            for channel in 0..3 {
                assert!(
                    (expected[channel] - actual[channel]).abs() <= 2.0 / 255.0,
                    "{hex} virou {back}"
                );
            }
        }
    }

    #[test]
    fn gray_has_no_chroma() {
        let color = hex_to_oklch("#808080").unwrap();
        assert!(color.c < 0.001);
    }
}
