//! Tabelle di colori, materiali e costanti del renderer.
//!
//! Sono quelle di FFL: cambiarle a occhio cambia la faccia di ogni Mii.

use crate::charinfo::COMMON_COLOR;
use crate::math::{vec3, vec4, Vec3, Vec4};

/// Tipo di "modulazione": dice al renderer quale parte sta disegnando, e quindi
/// quale materiale usare.
pub mod modulate {
    pub const FACELINE: usize = 0;
    pub const BEARD: usize = 1;
    pub const NOSE: usize = 2;
    pub const FOREHEAD: usize = 3;
    pub const HAIR: usize = 4;
    pub const CAP: usize = 5;
    pub const MASK: usize = 6;
    pub const NOSE_LINE: usize = 7;
    pub const GLASS: usize = 8;
    pub const BODY: usize = 9;
    pub const PANTS: usize = 10;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Specular {
    Blinn,
    Anisotropic,
}

#[derive(Debug, Clone, Copy)]
pub struct Material {
    pub ambient: Vec3,
    pub diffuse: Vec3,
    pub specular: Vec3,
    pub specular_power: f32,
    pub specular_mode: Specular,
    pub rim: Vec3,
}

const fn material(
    ambient: f32,
    ambient_rest: f32,
    diffuse: f32,
    specular: f32,
    power: f32,
    mode: Specular,
    rim: f32,
) -> Material {
    Material {
        ambient: vec3(ambient, ambient_rest, ambient_rest),
        diffuse: vec3(diffuse, diffuse, diffuse),
        specular: vec3(specular, specular, specular),
        specular_power: power,
        specular_mode: mode,
        rim: vec3(rim, rim, rim),
    }
}

/// Un materiale per tipo di modulazione.
const MATERIALS: [Material; 11] = [
    material(0.85, 0.75, 0.75, 0.30, 1.2, Specular::Blinn, 0.3),
    material(1.0, 1.0, 0.7, 0.0, 40.0, Specular::Blinn, 0.3),
    material(0.90, 0.85, 0.75, 0.22, 1.5, Specular::Blinn, 0.3),
    material(0.85, 0.75, 0.75, 0.30, 1.2, Specular::Blinn, 0.3),
    material(1.00, 1.00, 0.70, 0.10, 10.0, Specular::Anisotropic, 0.3),
    material(0.75, 0.75, 0.72, 0.30, 1.5, Specular::Blinn, 0.3),
    material(1.0, 1.0, 0.7, 0.0, 40.0, Specular::Anisotropic, 0.3),
    material(1.0, 1.0, 0.7, 0.0, 40.0, Specular::Anisotropic, 0.3),
    material(1.0, 1.0, 0.7, 0.0, 40.0, Specular::Anisotropic, 0.3),
    material(
        0.95622,
        0.95622,
        0.496_733,
        0.2409,
        3.0,
        Specular::Blinn,
        0.4,
    ),
    material(
        0.95622,
        0.95622,
        1.084_967,
        0.2409,
        3.0,
        Specular::Blinn,
        0.4,
    ),
];

pub fn material_for(modulate_type: usize) -> Material {
    MATERIALS
        .get(modulate_type)
        .copied()
        .unwrap_or(MATERIALS[modulate::FACELINE])
}

/// Luci della scena.
pub const LIGHT_AMBIENT: Vec3 = vec3(0.73, 0.73, 0.73);
pub const LIGHT_DIFFUSE: Vec3 = vec3(0.60, 0.60, 0.60);
pub const LIGHT_SPECULAR: Vec3 = vec3(0.70, 0.70, 0.70);
/// Già normalizzata: `(-0.4531539381, 0.4226179123, 0.7848858833)` ha modulo 1.
pub const LIGHT_DIRECTION: Vec3 = vec3(-0.453_153_94, 0.422_617_9, 0.784_885_9);

/// Il profilo di luce, uguale per tutti i Mii.
#[derive(Debug, Clone, Copy)]
pub struct Lighting {
    pub ambient_scale: f32,
    pub directional_influence: f32,
    pub diffuse_scale: f32,
    pub diffuse_floor: f32,
    pub specular_scale: f32,
    pub rim_scale: f32,
    pub rim_power: f32,
}

pub const LIGHTING: Lighting = Lighting {
    ambient_scale: 0.71,
    directional_influence: 0.32,
    diffuse_scale: 1.32,
    diffuse_floor: 0.23,
    specular_scale: 0.78,
    rim_scale: 0.88,
    rim_power: 2.56,
};

// Sono colori, non costanti matematiche: 0.318 somiglia a 1/π per caso.
#[allow(clippy::approx_constant)]
const FACELINE_COLORS: [Vec4; 6] = [
    vec4(1.000, 0.827, 0.678, 1.0),
    vec4(1.000, 0.714, 0.420, 1.0),
    vec4(0.870, 0.475, 0.259, 1.0),
    vec4(1.000, 0.667, 0.549, 1.0),
    vec4(0.678, 0.318, 0.161, 1.0),
    vec4(0.388, 0.173, 0.094, 1.0),
];

pub fn faceline_color(index: i32) -> Vec4 {
    FACELINE_COLORS[index.clamp(0, FACELINE_COLORS.len() as i32 - 1) as usize]
}

/// La tavolozza comune del formato Mii Studio, in sRGB.
const COMMON_COLORS: [Vec4; 24] = [
    vec4(0.176_470_6, 0.156_862_8, 0.156_862_8, 1.0),
    vec4(0.250_980_4, 0.125_490_2, 0.062_745_1, 1.0),
    vec4(0.360_784_4, 0.094_117_7, 0.039_215_7, 1.0),
    vec4(0.486_274_6, 0.227_451, 0.078_431_4, 1.0),
    vec4(0.470_588_3, 0.470_588_3, 0.501_960_8, 1.0),
    vec4(0.305_882_4, 0.243_137_3, 0.062_745_1, 1.0),
    vec4(0.533_333_4, 0.345_098_1, 0.094_117_7, 1.0),
    vec4(0.815_686_3, 0.627_451, 0.290_196_1, 1.0),
    vec4(0.0, 0.0, 0.0, 1.0),
    vec4(0.423_529_5, 0.439_215_7, 0.439_215_7, 1.0),
    vec4(0.4, 0.235_294_2, 0.172_549_1, 1.0),
    vec4(0.376_470_6, 0.368_627_5, 0.188_235_3, 1.0),
    vec4(0.274_509_9, 0.329_411_8, 0.658_823_6, 1.0),
    vec4(0.219_607_9, 0.439_215_7, 0.345_098_1, 1.0),
    vec4(0.376_470_6, 0.219_607_9, 0.062_745_1, 1.0),
    vec4(0.658_823_6, 0.062_745_1, 0.031_372_6, 1.0),
    vec4(0.125_490_2, 0.188_235_3, 0.407_843_2, 1.0),
    vec4(0.658_823_6, 0.376_470_6, 0.0, 1.0),
    vec4(0.470_588_3, 0.439_215_7, 0.407_843_2, 1.0),
    vec4(0.847_058_9, 0.321_568_7, 0.031_372_6, 1.0),
    vec4(0.941_176_5, 0.047_058_9, 0.031_372_6, 1.0),
    vec4(0.960_784_4, 0.282_353, 0.282_353, 1.0),
    vec4(0.941_176_5, 0.603_921_6, 0.454_902, 1.0),
    vec4(0.549_019_7, 0.313_725_5, 0.250_980_4, 1.0),
];

/// Il labbro superiore ha una sua tavolozza, più scura.
const UPPER_LIP_COMMON_COLORS: [Vec4; 24] = [
    vec4(0.090_196_1, 0.078_431_4, 0.078_431_4, 1.0),
    vec4(0.125_490_2, 0.062_745_1, 0.031_372_6, 1.0),
    vec4(0.180_392_2, 0.047_058_9, 0.019_607_9, 1.0),
    vec4(0.290_196_1, 0.137_255, 0.047_058_9, 1.0),
    vec4(0.329_411_8, 0.329_411_8, 0.352_941_2, 1.0),
    vec4(0.152_941_2, 0.121_568_7, 0.031_372_6, 1.0),
    vec4(0.321_568_7, 0.207_843_2, 0.054_902, 1.0),
    vec4(0.694_117_7, 0.501_960_8, 0.156_862_8, 1.0),
    vec4(0.0, 0.0, 0.0, 1.0),
    vec4(0.298_039_3, 0.305_882_4, 0.305_882_4, 1.0),
    vec4(0.2, 0.117_647_1, 0.086_274_6, 1.0),
    vec4(0.227_451, 0.219_607_9, 0.113_725_5, 1.0),
    vec4(0.164_705_9, 0.196_078_5, 0.396_078_5, 1.0),
    vec4(0.152_941_2, 0.305_882_4, 0.243_137_3, 1.0),
    vec4(0.188_235_3, 0.109_804, 0.031_372_6, 1.0),
    vec4(0.396_078_5, 0.039_215_7, 0.019_607_9, 1.0),
    vec4(0.062_745_1, 0.094_117_7, 0.203_921_6, 1.0),
    vec4(0.462_745_1, 0.262_745_1, 0.0, 1.0),
    vec4(0.329_411_8, 0.305_882_4, 0.286_274_6, 1.0),
    vec4(0.509_804, 0.188_235_3, 0.094_117_7, 1.0),
    vec4(0.470_588_3, 0.047_058_9, 0.047_058_9, 1.0),
    vec4(0.533_333_4, 0.125_490_2, 0.156_862_8, 1.0),
    vec4(0.862_745_1, 0.470_588_3, 0.313_725_5, 1.0),
    vec4(0.274_509_9, 0.117_647_1, 0.039_215_7, 1.0),
];

fn common(palette: &[Vec4; 24], encoded: i32) -> Option<Vec4> {
    if encoded & COMMON_COLOR == 0 {
        return None;
    }
    let index = (encoded & 0xFF) as usize;
    Some(palette.get(index).copied().unwrap_or(palette[0]))
}

fn resolve(encoded: i32, palette: &[Vec4]) -> Vec4 {
    let index = if encoded & COMMON_COLOR != 0 {
        encoded & 0xFF
    } else {
        encoded
    };
    palette[index.clamp(0, palette.len() as i32 - 1) as usize]
}

pub fn hair_color(encoded: i32) -> Vec4 {
    const COLORS: [Vec4; 8] = [
        vec4(0.118, 0.102, 0.094, 1.0),
        vec4(0.251, 0.125, 0.063, 1.0),
        vec4(0.361, 0.094, 0.039, 1.0),
        vec4(0.486, 0.227, 0.078, 1.0),
        vec4(0.471, 0.471, 0.502, 1.0),
        vec4(0.306, 0.243, 0.063, 1.0),
        vec4(0.533, 0.345, 0.094, 1.0),
        vec4(0.816, 0.627, 0.290, 1.0),
    ];
    common(&COMMON_COLORS, encoded).unwrap_or_else(|| resolve(encoded, &COLORS))
}

pub fn glass_color(encoded: i32) -> Vec4 {
    const COLORS: [Vec4; 6] = [
        vec4(0.094, 0.094, 0.094, 1.0),
        vec4(0.376, 0.219, 0.062, 1.0),
        vec4(0.658, 0.062, 0.031, 1.0),
        vec4(0.125, 0.188, 0.407, 1.0),
        vec4(0.658, 0.376, 0.000, 1.0),
        vec4(0.470, 0.439, 0.407, 1.0),
    ];
    common(&COMMON_COLORS, encoded).unwrap_or_else(|| resolve(encoded, &COLORS))
}

pub fn eye_color_b(encoded: i32) -> Vec4 {
    const COLORS: [Vec4; 6] = [
        vec4(0.000, 0.000, 0.000, 1.0),
        vec4(0.424, 0.439, 0.439, 1.0),
        vec4(0.400, 0.235, 0.173, 1.0),
        vec4(0.376, 0.369, 0.188, 1.0),
        vec4(0.275, 0.329, 0.659, 1.0),
        vec4(0.220, 0.439, 0.345, 1.0),
    ];
    common(&COMMON_COLORS, encoded).unwrap_or_else(|| resolve(encoded, &COLORS))
}

pub fn mouth_color_r(encoded: i32) -> Vec4 {
    const COLORS: [Vec4; 5] = [
        vec4(0.847, 0.322, 0.031, 1.0),
        vec4(0.941, 0.047, 0.031, 1.0),
        vec4(0.961, 0.282, 0.282, 1.0),
        vec4(0.941, 0.604, 0.455, 1.0),
        vec4(0.549, 0.314, 0.251, 1.0),
    ];
    common(&COMMON_COLORS, encoded).unwrap_or_else(|| resolve(encoded, &COLORS))
}

pub fn mouth_color_g(encoded: i32) -> Vec4 {
    const COLORS: [Vec4; 5] = [
        vec4(0.510, 0.188, 0.094, 1.0),
        vec4(0.471, 0.047, 0.047, 1.0),
        vec4(0.533, 0.125, 0.157, 1.0),
        vec4(0.863, 0.471, 0.314, 1.0),
        vec4(0.275, 0.118, 0.039, 1.0),
    ];
    common(&UPPER_LIP_COMMON_COLORS, encoded).unwrap_or_else(|| resolve(encoded, &COLORS))
}

pub fn favorite_color(index: i32) -> Vec4 {
    const COLORS: [Vec4; 12] = [
        vec4(0.824, 0.118, 0.078, 1.0),
        vec4(1.000, 0.431, 0.098, 1.0),
        vec4(1.000, 0.847, 0.125, 1.0),
        vec4(0.471, 0.824, 0.125, 1.0),
        vec4(0.000, 0.471, 0.188, 1.0),
        vec4(0.039, 0.282, 0.706, 1.0),
        vec4(0.235, 0.667, 0.871, 1.0),
        vec4(0.961, 0.353, 0.490, 1.0),
        vec4(0.451, 0.157, 0.678, 1.0),
        vec4(0.282, 0.220, 0.094, 1.0),
        vec4(0.878, 0.878, 0.878, 1.0),
        vec4(0.094, 0.094, 0.078, 1.0),
    ];
    COLORS[index.clamp(0, COLORS.len() as i32 - 1) as usize]
}

/// Colore dei pantaloni, uguale per tutti.
pub const PANTS_COLOR: Vec4 = vec4(0.250_980_4, 0.274_509_9, 0.305_882_4, 1.0);

/// Colore del neo.
pub const MOLE_COLOR: Vec4 = vec4(0.071, 0.059, 0.059, 1.0);

/// Quale occhio, bocca e sopracciglio usa ogni espressione.
#[derive(Debug, Clone, Copy)]
pub struct ExpressionElements {
    pub eye_right: i32,
    pub eye_left: i32,
    pub mouth: i32,
    pub eyebrow: i32,
}

const fn element(eye_right: i32, eye_left: i32, mouth: i32, eyebrow: i32) -> ExpressionElements {
    ExpressionElements {
        eye_right,
        eye_left,
        mouth,
        eyebrow,
    }
}

pub const EXPRESSIONS: [ExpressionElements; 19] = [
    element(0, 0, 0, 0),
    element(1, 1, 0, 0),
    element(0, 0, 1, 0),
    element(2, 2, 2, 0),
    element(3, 3, 0, 0),
    element(4, 4, 0, 0),
    element(0, 0, 3, 0),
    element(1, 1, 3, 0),
    element(0, 0, 3, 0),
    element(2, 2, 3, 0),
    element(3, 3, 3, 0),
    element(4, 4, 3, 0),
    element(5, 0, 0, 0),
    element(0, 5, 0, 0),
    element(5, 0, 3, 0),
    element(0, 5, 3, 0),
    element(5, 0, 5, 0),
    element(0, 5, 5, 0),
    element(5, 5, 2, 0),
];

/// Rotazione di base di ogni occhio, in trentaduesimi di giro.
const EYE_ROTATE: [u8; 79] = [
    3, 4, 4, 4, 3, 4, 4, 4, 3, 4, 4, 4, 4, 3, 3, 4, 4, 4, 3, 3, 4, 3, 4, 3, 3, 4, 3, 4, 4, 3, 4, 4,
    4, 3, 3, 3, 4, 4, 3, 3, 3, 4, 4, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 3, 4, 4, 3, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
];

/// Rotazione di base di ogni sopracciglio.
const EYEBROW_ROTATE: [u8; 28] = [
    6, 6, 5, 7, 6, 7, 6, 7, 4, 7, 6, 8, 5, 5, 6, 6, 7, 7, 6, 6, 5, 6, 7, 5, 6, 6, 6, 6,
];

pub fn eye_rotate_offset(eye_type: i32) -> i32 {
    32 - i32::from(EYE_ROTATE[eye_type.clamp(0, EYE_ROTATE.len() as i32 - 1) as usize])
}

pub fn eyebrow_rotate_offset(eyebrow_type: i32) -> i32 {
    32 - i32::from(EYEBROW_ROTATE[eyebrow_type.clamp(0, EYEBROW_ROTATE.len() as i32 - 1) as usize])
}

/// Occhi disegnati così come sono, senza ricolorare l'iride.
pub fn eye_texture_is_direct(index: i32) -> bool {
    matches!(index, 60 | 62 | 65 | 69..=75 | 78 | 79)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_light_direction_is_a_unit_vector() {
        assert!((LIGHT_DIRECTION.length_squared() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn common_colours_win_over_the_part_palette() {
        assert_eq!(hair_color(COMMON_COLOR | 8), vec4(0.0, 0.0, 0.0, 1.0));
        assert_eq!(hair_color(1), vec4(0.251, 0.125, 0.063, 1.0));
        // Un indice fuori dalla tavolozza comune ricade sul primo colore.
        assert_eq!(hair_color(COMMON_COLOR | 99), COMMON_COLORS[0]);
    }

    #[test]
    fn unknown_materials_fall_back_to_the_faceline() {
        assert_eq!(material_for(999).specular_power, 1.2);
        assert_eq!(
            material_for(modulate::HAIR).specular_mode,
            Specular::Anisotropic
        );
    }
}
