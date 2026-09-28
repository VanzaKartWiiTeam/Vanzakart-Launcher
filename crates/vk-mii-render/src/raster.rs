//! Il rasterizzatore software.
//!
//! Triangoli con interpolazione corretta in prospettiva, test di profondità,
//! le luci di FFL (Blinn per la pelle, anisotropica per i capelli, più il
//! bordo luminoso) e tre modi di fusione: quello normale per la scena e i due
//! con cui FFL compone le texture del viso e della maschera.
//!
//! Formule, ordine delle operazioni e arrotondamento dei byte sono fissati:
//! cambiarne uno cambia i pixel di ogni Mii.

use std::sync::Arc;

use crate::math::{to_byte01, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use crate::shape::Texture;
use crate::tables::{
    Material, Specular, LIGHTING, LIGHT_AMBIENT, LIGHT_DIFFUSE, LIGHT_DIRECTION, LIGHT_SPECULAR,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cull {
    None,
    Back,
    Front,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    /// Composizione normale della scena.
    Over,
    /// Come FFL compone la texture del viso sopra il colore della pelle.
    Faceline,
    /// Come FFL compone occhi, bocca e sopracciglia nella maschera.
    Mask,
}

/// Come si colora una parte: modo di FFL, tre colori e una texture.
#[derive(Debug, Clone)]
pub struct Modulate {
    pub mode: u8,
    pub color_r: Vec4,
    pub color_g: Vec4,
    pub color_b: Vec4,
    pub texture: Option<Arc<Texture>>,
}

impl Modulate {
    /// I colori assenti valgono bianco, come un puntatore nullo in FFL.
    pub fn new(
        mode: u8,
        color_r: Option<Vec4>,
        color_g: Option<Vec4>,
        color_b: Option<Vec4>,
        texture: Option<Arc<Texture>>,
    ) -> Self {
        let color = |value: Option<Vec4>| value.map_or(Vec4::ONE, Vec4::clamp01);
        Self {
            mode,
            color_r: color(color_r),
            color_g: color(color_g),
            color_b: color(color_b),
            texture,
        }
    }

    pub fn color(mode: u8, color: Vec4) -> Self {
        Self::new(mode, Some(color), None, None, None)
    }
}

/// Una maglia pronta da disegnare, in coordinate del modello.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    pub texcoords: Vec<Vec2>,
    pub normals: Vec<Vec3>,
    pub tangents: Vec<Vec3>,
    pub parameters: Vec<Vec4>,
    pub indices: Vec<u32>,
    pub cull: Cull,
    pub modulate: Modulate,
    pub material: Material,
    pub has_tangent: bool,
    /// Tinta unita al posto della modulazione: il corpo.
    pub constant_color: Option<Vec4>,
}

/// Il bersaglio del disegno: pixel RGBA non premoltiplicati e, se serve, la
/// profondità.
#[derive(Debug)]
pub struct Target {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    pub depth: Option<Vec<f32>>,
}

impl Target {
    pub fn new(width: usize, height: usize, clear: [u8; 4], with_depth: bool) -> Self {
        let mut pixels = vec![0u8; width * height * 4];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&clear);
        }

        Self {
            width,
            height,
            pixels,
            depth: with_depth.then(|| vec![1.0; width * height]),
        }
    }
}

/// Dove cade la cornice nel bersaglio.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub x: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy)]
struct RasterVertex {
    screen: Vec2,
    depth: f32,
    inv_w: f32,
    texcoord: Vec2,
    view_position: Vec3,
    normal: Vec3,
    tangent: Vec3,
    parameter: Vec4,
}

/// Disegna una maglia con le matrici date.
pub fn draw(
    target: &mut Target,
    mesh: &Mesh,
    model: &Mat4,
    view: &Mat4,
    projection: &Mat4,
    frame: Frame,
    lit: bool,
    blend: Blend,
) {
    if mesh.positions.is_empty() || mesh.indices.len() < 3 {
        return;
    }

    let model_view = *model * *view;
    let mut vertices = Vec::with_capacity(mesh.positions.len());

    for (index, position) in mesh.positions.iter().enumerate() {
        let world = model.transform_point(*position);
        let view_position = view.transform_point(world);
        let clip =
            projection.transform_vec4(vec4(view_position.x, view_position.y, view_position.z, 1.0));
        if clip.w.abs() <= 1e-6 {
            return;
        }

        let inv_w = 1.0 / clip.w;
        let ndc = vec3(clip.x * inv_w, clip.y * inv_w, clip.z * inv_w);

        vertices.push(RasterVertex {
            screen: Vec2 {
                x: frame.x + (ndc.x * 0.5 + 0.5) * frame.width,
                y: (1.0 - (ndc.y * 0.5 + 0.5)) * frame.height,
            },
            depth: ndc.z * 0.5 + 0.5,
            inv_w,
            texcoord: mesh.texcoords.get(index).copied().unwrap_or_default(),
            view_position,
            normal: model_view
                .transform_normal(mesh.normals.get(index).copied().unwrap_or(Vec3::UNIT_Z)),
            tangent: model_view
                .transform_normal(mesh.tangents.get(index).copied().unwrap_or(Vec3::ZERO)),
            parameter: mesh
                .parameters
                .get(index)
                .copied()
                .unwrap_or(crate::shape::DEFAULT_PARAMETER),
        });
    }

    let shader = Shader {
        mesh,
        lit,
        light: LIGHT_DIRECTION.normalize(),
    };

    for triangle in mesh.indices.chunks_exact(3) {
        let (Some(a), Some(b), Some(c)) = (
            vertices.get(triangle[0] as usize),
            vertices.get(triangle[1] as usize),
            vertices.get(triangle[2] as usize),
        ) else {
            continue;
        };
        rasterize_triangle(target, &shader, blend, a, b, c);
    }
}

fn cross2d(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn rasterize_triangle(
    target: &mut Target,
    shader: &Shader<'_>,
    blend: Blend,
    a: &RasterVertex,
    b: &RasterVertex,
    c: &RasterVertex,
) {
    let area = cross2d(a.screen, b.screen, c.screen);
    if area.abs() < 1e-6 {
        return;
    }
    match shader.mesh.cull {
        Cull::Back if area >= 0.0 => return,
        Cull::Front if area <= 0.0 => return,
        _ => {}
    }

    let (ax, ay) = (a.screen.x, a.screen.y);
    let (bx, by) = (b.screen.x, b.screen.y);
    let (cx, cy) = (c.screen.x, c.screen.y);

    let max_x = target.width as i32 - 1;
    let max_y = target.height as i32 - 1;
    let min_px = (ax.min(bx.min(cx)).floor() as i32).clamp(0, max_x);
    let max_px = (ax.max(bx.max(cx)).ceil() as i32).clamp(0, max_x);
    let min_py = (ay.min(by.min(cy)).floor() as i32).clamp(0, max_y);
    let max_py = (ay.max(by.max(cy)).ceil() as i32).clamp(0, max_y);

    let inv_area = 1.0 / area;
    let sample_x = min_px as f32 + 0.5;
    let sample_y = min_py as f32 + 0.5;

    let (e0x, e0y, e0c) = (by - cy, cx - bx, bx * cy - by * cx);
    let (e1x, e1y, e1c) = (cy - ay, ax - cx, cx * ay - cy * ax);
    let (e2x, e2y, e2c) = (ay - by, bx - ax, ax * by - ay * bx);

    let mut e0_row = e0x * sample_x + e0y * sample_y + e0c;
    let mut e1_row = e1x * sample_x + e1y * sample_y + e1c;
    let mut e2_row = e2x * sample_x + e2y * sample_y + e2c;

    for y in min_py..=max_py {
        let (mut e0, mut e1, mut e2) = (e0_row, e1_row, e2_row);
        let row = y as usize * target.width;

        for x in min_px..=max_px {
            let pixel = row + x as usize;
            let negative = e0 < 0.0 || e1 < 0.0 || e2 < 0.0;
            let positive = e0 > 0.0 || e1 > 0.0 || e2 > 0.0;

            if !(negative && positive) {
                shade_pixel(
                    target,
                    shader,
                    blend,
                    pixel,
                    [a, b, c],
                    [e0 * inv_area, e1 * inv_area, e2 * inv_area],
                );
            }

            e0 += e0x;
            e1 += e1x;
            e2 += e2x;
        }

        e0_row += e0y;
        e1_row += e1y;
        e2_row += e2y;
    }
}

fn shade_pixel(
    target: &mut Target,
    shader: &Shader<'_>,
    blend: Blend,
    pixel: usize,
    [a, b, c]: [&RasterVertex; 3],
    [w0, w1, w2]: [f32; 3],
) {
    let denominator = w0 * a.inv_w + w1 * b.inv_w + w2 * c.inv_w;
    if denominator.abs() < 1e-8 {
        return;
    }

    let depth =
        (w0 * a.depth * a.inv_w + w1 * b.depth * b.inv_w + w2 * c.depth * c.inv_w) / denominator;
    if !(0.0..=1.0).contains(&depth) {
        return;
    }
    if let Some(buffer) = &target.depth {
        if depth > buffer[pixel] {
            return;
        }
    }

    macro_rules! interpolate {
        ($field:ident) => {
            (a.$field * w0 * a.inv_w + b.$field * w1 * b.inv_w + c.$field * w2 * c.inv_w)
                / denominator
        };
    }

    let color = shader.evaluate(
        interpolate!(texcoord),
        interpolate!(view_position),
        interpolate!(normal),
        interpolate!(tangent),
        interpolate!(parameter),
    );
    if color.w <= 0.0 {
        return;
    }

    blend_pixel(&mut target.pixels[pixel * 4..pixel * 4 + 4], color, blend);
    if let Some(buffer) = &mut target.depth {
        buffer[pixel] = depth;
    }
}

struct Shader<'a> {
    mesh: &'a Mesh,
    lit: bool,
    light: Vec3,
}

impl Shader<'_> {
    fn evaluate(
        &self,
        uv: Vec2,
        view_position: Vec3,
        normal: Vec3,
        tangent: Vec3,
        parameter: Vec4,
    ) -> Vec4 {
        let mesh = self.mesh;
        let base = match mesh.constant_color {
            Some(color) => color,
            None => {
                let modulate = &mesh.modulate;
                let texel = modulate
                    .texture
                    .as_ref()
                    .map_or(Vec4::ONE, |texture| texture.sample(uv));
                let (r, g, b) = (modulate.color_r, modulate.color_g, modulate.color_b);

                let color = match modulate.mode {
                    0 => vec4(r.x, r.y, r.z, 1.0),
                    1 => texel,
                    2 => vec4(
                        texel.x * r.x + texel.y * g.x + texel.z * b.x,
                        texel.x * r.y + texel.y * g.y + texel.z * b.y,
                        texel.x * r.z + texel.y * g.z + texel.z * b.z,
                        texel.w,
                    ),
                    3 => vec4(r.x, r.y, r.z, texel.x),
                    4 => vec4(texel.y * r.x, texel.y * r.y, texel.y * r.z, texel.x),
                    5 => vec4(texel.x * r.x, texel.x * r.y, texel.x * r.z, 1.0),
                    _ => Vec4::ONE,
                };

                if modulate.mode != 0 && color.w <= 0.0 {
                    return Vec4::ZERO;
                }
                color
            }
        };

        if !self.lit {
            return base.clamp01();
        }

        let material = &mesh.material;
        let n = normal.normalize_or(Vec3::UNIT_Z);
        let eye = (-view_position).normalize_or(Vec3::UNIT_Z);
        let light = self.light;

        let ambient = LIGHT_AMBIENT.hadamard(material.ambient) * LIGHTING.ambient_scale;
        let directional = light.dot(n).max(LIGHTING.diffuse_floor);
        let diffuse_factor = 1.0 + (directional - 1.0) * LIGHTING.directional_influence;
        let diffuse =
            LIGHT_DIFFUSE.hadamard(material.diffuse) * (diffuse_factor * LIGHTING.diffuse_scale);

        let power = material.specular_power;
        let blinn = (-light).reflect(n).dot(eye).max(0.0).powf(power);
        let mode = if mesh.has_tangent {
            material.specular_mode
        } else {
            Specular::Blinn
        };

        let (strength, reflection) = match mode {
            Specular::Blinn => (1.0, blinn),
            Specular::Anisotropic => {
                let t = tangent.normalize_or(vec3(1.0, 0.0, 0.0));
                let dot_lt = light.dot(t);
                let dot_vt = eye.dot(t);
                let dot_ln = (1.0 - dot_lt * dot_lt).max(0.0).sqrt();
                let dot_vr = dot_ln * (1.0 - dot_vt * dot_vt).max(0.0).sqrt() - dot_lt * dot_vt;
                let anisotropic = dot_vr.max(0.0).powf(power);
                (
                    parameter.y,
                    anisotropic + (blinn - anisotropic) * parameter.x,
                )
            }
        };

        let specular = LIGHT_SPECULAR.hadamard(material.specular)
            * reflection
            * strength
            * LIGHTING.specular_scale;
        let rim_factor = (parameter.w * (1.0 - n.z.abs()))
            .max(0.0)
            .powf(LIGHTING.rim_power);
        let rim = material.rim * (rim_factor * LIGHTING.rim_scale);

        let lit = (ambient + diffuse).hadamard(base.xyz()) + specular + rim;
        vec4(lit.x, lit.y, lit.z, base.w).clamp01()
    }
}

fn blend_pixel(target: &mut [u8], source: Vec4, blend: Blend) {
    let source_alpha = source.w.clamp(0.0, 1.0);
    if source_alpha <= 0.0 {
        return;
    }

    if blend == Blend::Over && source_alpha >= 0.999 {
        target[0] = to_byte01(source.x);
        target[1] = to_byte01(source.y);
        target[2] = to_byte01(source.z);
        target[3] = 255;
        return;
    }

    let channel = |value: u8| f32::from(value) / 255.0;
    let destination = [channel(target[0]), channel(target[1]), channel(target[2])];
    let destination_alpha = channel(target[3]);
    let source_rgb = [source.x, source.y, source.z];

    let (rgb, alpha): ([f32; 3], f32) = match blend {
        Blend::Faceline => (
            std::array::from_fn(|i| {
                source_rgb[i] * source_alpha + destination[i] * (1.0 - source_alpha)
            }),
            source_alpha + destination_alpha,
        ),
        Blend::Mask => (
            std::array::from_fn(|i| {
                source_rgb[i] * (1.0 - destination_alpha) + destination[i] * destination_alpha
            }),
            source_alpha * source_alpha + destination_alpha * destination_alpha,
        ),
        Blend::Over => {
            let alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
            if alpha <= 0.0 {
                return;
            }
            (
                std::array::from_fn(|i| {
                    (source_rgb[i] * source_alpha
                        + destination[i] * destination_alpha * (1.0 - source_alpha))
                        / alpha
                }),
                alpha,
            )
        }
    };

    target[0] = to_byte01(rgb[0]);
    target[1] = to_byte01(rgb[1]);
    target[2] = to_byte01(rgb[2]);
    target[3] = to_byte01(alpha);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::{material_for, modulate};

    fn quad(color: Vec4, cull: Cull) -> Mesh {
        Mesh {
            positions: vec![
                vec3(-1.0, 1.0, 0.0),
                vec3(1.0, 1.0, 0.0),
                vec3(-1.0, -1.0, 0.0),
                vec3(1.0, -1.0, 0.0),
            ],
            texcoords: vec![Vec2::default(); 4],
            normals: vec![Vec3::UNIT_Z; 4],
            tangents: vec![Vec3::ZERO; 4],
            parameters: vec![crate::shape::DEFAULT_PARAMETER; 4],
            indices: vec![0, 1, 2, 2, 1, 3],
            cull,
            modulate: Modulate::color(0, color),
            material: material_for(modulate::FACELINE),
            has_tangent: false,
            constant_color: None,
        }
    }

    fn draw_flat(target: &mut Target, mesh: &Mesh) {
        let frame = Frame {
            x: 0.0,
            width: target.width as f32,
            height: target.height as f32,
        };
        draw(
            target,
            mesh,
            &Mat4::IDENTITY,
            &Mat4::IDENTITY,
            &Mat4::IDENTITY,
            frame,
            false,
            Blend::Over,
        );
    }

    #[test]
    fn a_full_screen_quad_covers_every_pixel() {
        let mut target = Target::new(8, 8, [0, 0, 0, 0], true);
        draw_flat(&mut target, &quad(vec4(1.0, 0.0, 0.0, 1.0), Cull::None));

        assert!(target
            .pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255]));
    }

    #[test]
    fn back_faces_are_culled() {
        // Sullo schermo, con l'asse Y verso il basso, il quadrato ha area
        // positiva: per la convenzione di FFL è una faccia posteriore.
        let mut target = Target::new(4, 4, [0, 0, 0, 0], false);
        draw_flat(&mut target, &quad(vec4(1.0, 1.0, 1.0, 1.0), Cull::Back));
        assert!(target.pixels.iter().all(|byte| *byte == 0));

        draw_flat(&mut target, &quad(vec4(1.0, 1.0, 1.0, 1.0), Cull::Front));
        assert!(target.pixels.iter().all(|byte| *byte == 255));
    }

    #[test]
    fn a_farther_surface_does_not_overwrite_a_nearer_one() {
        let mut target = Target::new(4, 4, [0, 0, 0, 0], true);
        let mut near = quad(vec4(0.0, 1.0, 0.0, 1.0), Cull::None);
        near.positions.iter_mut().for_each(|p| p.z = -0.5);
        draw_flat(&mut target, &near);

        let far = quad(vec4(1.0, 0.0, 0.0, 1.0), Cull::None);
        draw_flat(&mut target, &far);

        assert_eq!(&target.pixels[0..4], &[0, 255, 0, 255]);
    }

    #[test]
    fn translucent_colours_blend_over_the_background() {
        let mut target = [0u8, 0, 255, 255];
        blend_pixel(&mut target, vec4(1.0, 0.0, 0.0, 0.5), Blend::Over);
        assert_eq!(target, [128, 0, 128, 255]);

        // Su un fondo trasparente il colore resta pieno.
        let mut target = [255u8, 255, 255, 0];
        blend_pixel(&mut target, vec4(1.0, 0.0, 0.0, 0.5), Blend::Over);
        assert_eq!(target, [255, 0, 0, 128]);
    }
}
