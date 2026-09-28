//! Forme e texture di una parte dell'archivio.

use crate::archive::{read_f32, read_u16, read_u32};
use crate::error::{RenderError, RenderResult};
use crate::math::{vec2, vec3, vec4, Vec2, Vec3, Vec4};

/// Parametri di vertice di chi non ne ha: niente anisotropia, niente bordo.
pub const DEFAULT_PARAMETER: Vec4 = vec4(1.0, 1.0, 0.0, 1.0);

/// Dove il viso aggancia capelli, naso e barba.
#[derive(Debug, Clone, Copy, Default)]
pub struct FacelineTransform {
    pub hair: Vec3,
    pub nose: Vec3,
    pub beard: Vec3,
}

/// Una forma decodificata: vertici, indici e, per il viso, i punti d'aggancio.
#[derive(Debug, Clone, Default)]
pub struct Shape {
    pub positions: Vec<Vec3>,
    pub texcoords: Vec<Vec2>,
    pub normals: Vec<Vec3>,
    pub tangents: Vec<Vec3>,
    pub parameters: Vec<Vec4>,
    pub indices: Vec<u32>,
    pub faceline: Option<FacelineTransform>,
}

const SHAPE_HEADER_SIZE: usize = 0x90;
/// Posizione, normale, coordinate texture, tangente, colore, indici.
const ELEMENTS: usize = 6;

/// Decodifica i byte di una forma.
pub fn decode_shape(
    bytes: &[u8],
    part: usize,
    big_endian: bool,
    half_float: bool,
) -> RenderResult<Shape> {
    let fail = |reason: &str| RenderError::InvalidResource(format!("shape part {part}: {reason}"));

    if bytes.len() < SHAPE_HEADER_SIZE {
        return Err(fail("truncated header"));
    }

    let mut position = [0usize; ELEMENTS];
    let mut size = [0usize; ELEMENTS];
    for element in 0..ELEMENTS {
        position[element] = read_u32(bytes, element * 4, big_endian) as usize;
        size[element] = read_u32(bytes, (ELEMENTS + element) * 4, big_endian) as usize;
        if size[element] == 0 {
            continue;
        }

        // Per gli indici la "dimensione" è un conteggio di `u16`.
        let byte_size = if element == 5 {
            size[element] * 2
        } else {
            size[element]
        };
        if position[element] >= bytes.len() || position[element] + byte_size > bytes.len() {
            return Err(fail("element exceeds the payload"));
        }
    }

    let position_bytes = size[0];
    if position_bytes == 0 {
        return Err(fail("empty position buffer"));
    }

    let mut stride = if half_float { 6 } else { 16 };
    if !half_float && position_bytes % stride != 0 && position_bytes % 12 == 0 {
        stride = 12;
    }
    let count = position_bytes / stride;
    if count == 0 {
        return Err(fail("no vertices"));
    }

    let half = |offset: usize| half_to_f32(read_u16(bytes, offset, big_endian));
    let float = |offset: usize| read_f32(bytes, offset, big_endian);

    let positions: Vec<Vec3> = (0..count)
        .map(|index| {
            let offset = position[0] + index * stride;
            if half_float {
                vec3(half(offset), half(offset + 2), half(offset + 4))
            } else {
                vec3(float(offset), float(offset + 4), float(offset + 8))
            }
        })
        .collect();

    let mut texcoords = vec![Vec2::default(); count];
    if size[2] > 0 {
        let stride = if half_float { 4 } else { 8 };
        for (index, texcoord) in texcoords.iter_mut().enumerate().take(size[2] / stride) {
            let offset = position[2] + index * stride;
            *texcoord = if half_float {
                vec2(half(offset), half(offset + 2))
            } else {
                vec2(float(offset), float(offset + 4))
            };
        }
    }

    let mut normals = vec![Vec3::UNIT_Z; count];
    if size[1] > 0 {
        for (index, normal) in normals.iter_mut().enumerate().take(size[1] / 4) {
            let offset = position[1] + index * 4;
            *normal = if half_float {
                snorm8(&bytes[offset..offset + 3]).normalize_or(Vec3::UNIT_Z)
            } else {
                decode_int2101010(read_u32(bytes, offset, big_endian) as i32)
            };
        }
    }

    let mut tangents = vec![Vec3::ZERO; count];
    if size[3] > 0 {
        for (index, tangent) in tangents.iter_mut().enumerate().take(size[3] / 4) {
            let offset = position[3] + index * 4;
            *tangent = snorm8(&bytes[offset..offset + 3]).normalize_or(Vec3::ZERO);
        }
    }

    let mut parameters = vec![DEFAULT_PARAMETER; count];
    if size[4] > 0 {
        for (index, parameter) in parameters.iter_mut().enumerate().take(size[4] / 4) {
            let offset = position[4] + index * 4;
            *parameter = unorm8x4(&bytes[offset..offset + 4]);
        }
    }

    if size[5] == 0 {
        return Err(fail("empty index buffer"));
    }
    let indices = (0..size[5])
        .map(|index| {
            let value = u32::from(read_u16(bytes, position[5] + index * 2, big_endian));
            if (value as usize) < count {
                Ok(value)
            } else {
                Err(fail("index outside the vertex buffer"))
            }
        })
        .collect::<RenderResult<Vec<u32>>>()?;

    // Solo il viso porta i punti d'aggancio, subito dopo il riquadro.
    let faceline =
        (part == crate::archive::shape::FACELINE && bytes.len() >= 0x48 + 0x24).then(|| {
            let at = |offset: usize| {
                vec3(
                    float(0x48 + offset),
                    float(0x48 + offset + 4),
                    float(0x48 + offset + 8),
                )
            };
            FacelineTransform {
                hair: at(0x00),
                nose: at(0x0C),
                beard: at(0x18),
            }
        });

    Ok(Shape {
        positions,
        texcoords,
        normals,
        tangents,
        parameters,
        indices,
        faceline,
    })
}

fn snorm8(bytes: &[u8]) -> Vec3 {
    vec3(
        f32::from(bytes[0] as i8) / 127.0,
        f32::from(bytes[1] as i8) / 127.0,
        f32::from(bytes[2] as i8) / 127.0,
    )
}

fn unorm8x4(bytes: &[u8]) -> Vec4 {
    vec4(
        f32::from(bytes[0]) / 255.0,
        f32::from(bytes[1]) / 255.0,
        f32::from(bytes[2]) / 255.0,
        f32::from(bytes[3]) / 255.0,
    )
}

/// Normale impacchettata in 10+10+10 bit con segno.
pub(crate) fn decode_int2101010(packed: i32) -> Vec3 {
    fn sign_extend(value: i32) -> i32 {
        let value = value & 0x3FF;
        if value & 0x200 != 0 {
            value - 0x400
        } else {
            value
        }
    }

    let normal = vec3(
        sign_extend(packed) as f32 / 511.0,
        sign_extend(packed >> 10) as f32 / 511.0,
        sign_extend(packed >> 20) as f32 / 511.0,
    );
    if normal.length_squared() < 1e-5 {
        Vec3::UNIT_Z
    } else {
        normal.normalize()
    }
}

/// `f16` → `f32`, con subnormali, infiniti e NaN.
pub(crate) fn half_to_f32(bits: u16) -> f32 {
    let negative = bits & 0x8000 != 0;
    let exponent = u32::from((bits >> 10) & 0x1F);
    let mantissa = u32::from(bits & 0x3FF);

    let magnitude = match (exponent, mantissa) {
        (0, 0) => 0.0,
        // Subnormale: `mantissa * 2^-24`, esatto in `f32`.
        (0, _) => mantissa as f32 * f32::powi(2.0, -24),
        (0x1F, 0) => f32::INFINITY,
        (0x1F, _) => f32::NAN,
        _ => f32::from_bits(((exponent + 112) << 23) | (mantissa << 13)),
    };

    if negative {
        -magnitude
    } else {
        magnitude
    }
}

/// Formati di texture dell'archivio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    R8,
    Rg8,
    Rgba8,
}

impl TextureFormat {
    fn stride(self) -> usize {
        match self {
            Self::R8 => 1,
            Self::Rg8 => 2,
            Self::Rgba8 => 4,
        }
    }
}

/// Una texture: il livello base, senza mipmap.
#[derive(Debug, Clone)]
pub struct Texture {
    pub width: usize,
    pub height: usize,
    pub format: TextureFormat,
    pub pixels: Vec<u8>,
}

impl Texture {
    /// Texture RGBA già pronta, per quelle che il renderer disegna da sé.
    pub fn rgba(width: usize, height: usize, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            format: TextureFormat::Rgba8,
            pixels,
        }
    }

    /// Un texel come colore in `[0, 1]`.
    pub fn texel(&self, x: usize, y: usize) -> Vec4 {
        let stride = self.format.stride();
        let index = (y * self.width + x) * stride;
        let Some(pixel) = self.pixels.get(index..index + stride) else {
            return Vec4::ONE;
        };

        let channel = |value: u8| f32::from(value) / 255.0;
        match self.format {
            TextureFormat::R8 => {
                let value = channel(pixel[0]);
                vec4(value, value, value, 1.0)
            }
            TextureFormat::Rg8 => vec4(channel(pixel[0]), channel(pixel[1]), 0.0, 1.0),
            TextureFormat::Rgba8 => vec4(
                channel(pixel[0]),
                channel(pixel[1]),
                channel(pixel[2]),
                channel(pixel[3]),
            ),
        }
    }

    /// Campionamento bilineare con ripetizione a specchio.
    pub fn sample(&self, uv: Vec2) -> Vec4 {
        if self.width <= 1 || self.height <= 1 {
            return self.texel(0, 0);
        }

        let fx = mirror_repeat(uv.x) * (self.width - 1) as f32;
        let fy = mirror_repeat(uv.y) * (self.height - 1) as f32;

        let x0 = (fx.floor() as isize).clamp(0, self.width as isize - 1) as usize;
        let y0 = (fy.floor() as isize).clamp(0, self.height as isize - 1) as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);

        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;

        let top = self.texel(x0, y0).lerp(self.texel(x1, y0), tx);
        let bottom = self.texel(x0, y1).lerp(self.texel(x1, y1), tx);
        top.lerp(bottom, ty)
    }
}

fn mirror_repeat(value: f32) -> f32 {
    let mut wrapped = value % 2.0;
    if wrapped < 0.0 {
        wrapped += 2.0;
    }
    if wrapped <= 1.0 {
        wrapped
    } else {
        2.0 - wrapped
    }
}

/// Decodifica una texture. `None` quando la parte è vuota.
///
/// Il piè di pagina da 12 byte dice larghezza, altezza, numero di mipmap e
/// formato; l'immagine base sta all'inizio.
pub fn decode_texture(bytes: &[u8], big_endian: bool) -> RenderResult<Option<Texture>> {
    if bytes.len() <= 12 {
        return Ok(None);
    }

    let footer = bytes.len() - 12;
    let width = usize::from(read_u16(bytes, footer + 4, big_endian));
    let height = usize::from(read_u16(bytes, footer + 6, big_endian));
    let format = match bytes[footer + 9] {
        0 => TextureFormat::R8,
        1 => TextureFormat::Rg8,
        2 => TextureFormat::Rgba8,
        other => {
            return Err(RenderError::InvalidResource(format!(
                "unsupported texture format {other}"
            )))
        }
    };

    if width == 0 || height == 0 {
        return Err(RenderError::InvalidResource(
            "texture with invalid dimensions".into(),
        ));
    }

    let size = width * height * format.stride();
    if footer < size {
        return Err(RenderError::InvalidResource(
            "texture payload is truncated".into(),
        ));
    }

    Ok(Some(Texture {
        width,
        height,
        format,
        pixels: bytes[..size].to_vec(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats_convert_exactly() {
        assert_eq!(half_to_f32(0x3C00), 1.0);
        assert_eq!(half_to_f32(0xC000), -2.0);
        assert_eq!(half_to_f32(0x3555), 0.333_251_95);
        assert_eq!(half_to_f32(0x0000), 0.0);
        assert_eq!(half_to_f32(0x0001), 5.960_464_5e-8);
        assert_eq!(half_to_f32(0x8001), -5.960_464_5e-8);
        assert!(half_to_f32(0x7C00).is_infinite());
        assert!(half_to_f32(0x7E00).is_nan());
    }

    #[test]
    fn packed_normals_decode_to_unit_vectors() {
        let up = decode_int2101010(511 << 10);
        assert!((up.y - 1.0).abs() < 1e-6);
        // Un vettore nullo ripiega sull'asse Z.
        assert_eq!(decode_int2101010(0), Vec3::UNIT_Z);
        // Il segno sta nel decimo bit.
        let down = decode_int2101010((0x3FF & -511) << 10);
        assert!((down.y + 1.0).abs() < 1e-6);
    }

    #[test]
    fn mirror_repeat_folds_back() {
        assert_eq!(mirror_repeat(0.25), 0.25);
        assert_eq!(mirror_repeat(1.25), 0.75);
        assert_eq!(mirror_repeat(-0.25), 0.25);
        assert_eq!(mirror_repeat(2.5), 0.5);
    }

    #[test]
    fn an_empty_texture_part_is_no_texture() {
        assert!(decode_texture(&[0; 12], false).unwrap().is_none());
    }

    #[test]
    fn a_texture_is_read_from_its_footer() {
        let mut bytes = vec![10, 20, 30, 40];
        bytes.extend_from_slice(&[0, 0, 0, 0, 2, 0, 2, 0, 1, 0, 0, 0]);
        let texture = decode_texture(&bytes, false).unwrap().unwrap();

        assert_eq!((texture.width, texture.height), (2, 2));
        assert_eq!(texture.format, TextureFormat::R8);
        assert_eq!(texture.texel(1, 1).x, 40.0 / 255.0);
    }
}
