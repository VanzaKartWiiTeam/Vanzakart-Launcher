//! La testa: dal `CharInfo` alle maglie da disegnare.
//!
//! È il lavoro che FFL fa in `FFLiCharModel`: sceglie le forme di viso,
//! capelli, naso, barba e occhiali, le aggancia ai punti del viso, e compone
//! due texture — quella del viso (trucco, rughe) e la **maschera** con occhi,
//! sopracciglia, bocca, baffi e neo — disegnandole a loro volta con il
//! rasterizzatore.
//!
//! Il risultato non dipende dall'inquadratura, quindi si mette in cache: ruotare
//! il Mii o cambiarne la dimensione non rifà questo lavoro.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::archive::{shape, texture, ResourceArchive};
use crate::charinfo::CharInfo;
use crate::error::{RenderError, RenderResult};
use crate::math::{round_even, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use crate::raster::{self, Blend, Cull, Frame, Mesh, Modulate, Target};
use crate::shape::{decode_shape, decode_texture, Shape, Texture, DEFAULT_PARAMETER};
use crate::tables::{self, material_for, modulate};

/// Le parti dell'archivio, decodificate una volta sola.
#[derive(Debug)]
pub struct Parts {
    archive: ResourceArchive,
    shapes: Mutex<HashMap<(usize, usize), Result<Arc<Shape>, String>>>,
    textures: Mutex<HashMap<(usize, usize), Option<Arc<Texture>>>>,
}

impl Parts {
    pub fn new(archive: ResourceArchive) -> Self {
        Self {
            archive,
            shapes: Mutex::default(),
            textures: Mutex::default(),
        }
    }

    pub fn archive(&self) -> &ResourceArchive {
        &self.archive
    }

    /// Una forma, oppure il motivo per cui non c'è.
    fn shape(&self, part: usize, index: i32) -> Result<Arc<Shape>, String> {
        let Ok(index) = usize::try_from(index) else {
            return Err(format!("negative shape index {index}"));
        };

        if let Some(cached) = self.shapes.lock().unwrap().get(&(part, index)) {
            return cached.clone();
        }

        let decoded = self
            .archive
            .shape_part(part, index)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                if bytes.is_empty() {
                    return Err(format!("shape part {part}:{index} is empty"));
                }
                decode_shape(
                    &bytes,
                    part,
                    self.archive.big_endian(),
                    self.archive.half_float_layout,
                )
                .map(Arc::new)
                .map_err(|error| error.to_string())
            });

        self.shapes
            .lock()
            .unwrap()
            .insert((part, index), decoded.clone());
        decoded
    }

    /// Una texture. `None` quando la parte è vuota: quel tratto non si disegna.
    fn texture(&self, part: usize, index: i32) -> RenderResult<Option<Arc<Texture>>> {
        let Ok(index) = usize::try_from(index) else {
            return Ok(None);
        };

        if let Some(cached) = self.textures.lock().unwrap().get(&(part, index)) {
            return Ok(cached.clone());
        }

        let bytes = self.archive.texture_part(part, index)?;
        let decoded = decode_texture(&bytes, self.archive.big_endian())?.map(Arc::new);

        self.textures
            .lock()
            .unwrap()
            .insert((part, index), decoded.clone());
        Ok(decoded)
    }
}

/// La testa pronta da disegnare.
#[derive(Debug)]
pub struct Head {
    pub meshes: Vec<Mesh>,
}

/// Costruisce la testa di un Mii.
///
/// `resolution` è il lato della maschera: 256 per le immagini piccole, 512
/// per le altre.
pub fn build(
    parts: &Parts,
    info: &CharInfo,
    expression: usize,
    resolution: usize,
) -> RenderResult<Head> {
    let is_afl = parts.archive().is_afl;
    let mut meshes = Vec::with_capacity(12);

    let faceline = parts
        .shape(shape::FACELINE, info.face_type)
        .map_err(RenderError::InvalidResource)?;
    let anchors = faceline.faceline.unwrap_or_default();

    let faceline_texture = faceline_texture(parts, info, resolution)?;
    let mask_texture = mask_texture(parts, info, expression, resolution)?;

    let skin = tables::faceline_color(info.faceline_color);
    let faceline_modulate = match &faceline_texture {
        Some(texture) => Modulate::new(1, None, None, None, Some(texture.clone())),
        None => Modulate::color(0, skin),
    };
    meshes.push(shape_mesh(
        &faceline,
        ShapePlacement::default(),
        Cull::Back,
        faceline_modulate,
        modulate::FACELINE,
    )?);

    let hair_color = tables::hair_color(info.hair_color);
    let hair_flip = info.hair_dir > 0;
    let hair_cull = if hair_flip { Cull::Front } else { Cull::Back };
    let hair_placement = ShapePlacement {
        translate: Some(anchors.hair),
        flip_x: hair_flip,
        ..ShapePlacement::default()
    };

    if let Ok(hair) = parts.shape(shape::HAIR, info.hair_type) {
        meshes.push(shape_mesh(
            &hair,
            hair_placement,
            hair_cull,
            Modulate::color(0, hair_color),
            modulate::HAIR,
        )?);
    }

    if let Ok(forehead) = parts.shape(shape::FOREHEAD, info.hair_type) {
        meshes.push(shape_mesh(
            &forehead,
            hair_placement,
            hair_cull,
            Modulate::color(0, skin),
            modulate::FOREHEAD,
        )?);
    }

    if let Some(cap) = parts.texture(texture::CAP, info.hair_type)? {
        if let Ok(hat) = parts.shape(shape::HAT, info.hair_type) {
            meshes.push(shape_mesh(
                &hat,
                hair_placement,
                hair_cull,
                Modulate::new(
                    5,
                    Some(tables::favorite_color(info.favorite_color)),
                    None,
                    None,
                    Some(cap),
                ),
                modulate::CAP,
            )?);
        }
    }

    if (0..4).contains(&info.beard_type) {
        if let Ok(beard) = parts.shape(shape::BEARD, info.beard_type) {
            meshes.push(shape_mesh(
                &beard,
                ShapePlacement {
                    translate: Some(anchors.beard),
                    ..ShapePlacement::default()
                },
                Cull::Back,
                Modulate::color(0, hair_color),
                modulate::BEARD,
            )?);
        }
    }

    // Le espressioni con la bocca aperta di profilo non hanno naso né maschera.
    let skip_nose_and_mask = matches!(expression, 49..=52 | 61 | 62);
    if !skip_nose_and_mask {
        let nose_scale = info.nose_scale as f32 * 0.175 + 0.4;
        let nose_placement = ShapePlacement {
            scale_x: nose_scale,
            scale_y: nose_scale,
            translate: Some(vec3(
                anchors.nose.x,
                anchors.nose.y + (info.nose_position_y - 8) as f32 * -1.5,
                anchors.nose.z,
            )),
            flip_x: false,
        };

        if let Ok(nose) = parts.shape(shape::NOSE, info.nose_type) {
            meshes.push(shape_mesh(
                &nose,
                nose_placement,
                Cull::Back,
                Modulate::color(0, skin),
                modulate::NOSE,
            )?);
        }

        if let Some(line) = parts.texture(texture::NOSE_LINE, info.nose_type)? {
            if let Ok(nose_line) = parts.shape(shape::NOSE_LINE, info.nose_type) {
                meshes.push(shape_mesh(
                    &nose_line,
                    nose_placement,
                    Cull::Back,
                    Modulate::new(3, Some(vec4(0.0, 0.0, 0.0, 1.0)), None, None, Some(line)),
                    modulate::NOSE_LINE,
                )?);
            }
        }

        if let Some(mask) = mask_texture {
            if let Ok(mask_shape) = parts.shape(shape::MASK, info.face_type) {
                let cull = if is_afl { Cull::None } else { Cull::Back };
                meshes.push(shape_mesh(
                    &mask_shape,
                    ShapePlacement::default(),
                    cull,
                    Modulate::new(1, None, None, None, Some(mask)),
                    modulate::MASK,
                )?);
            }
        }
    }

    if info.glass_type > 0 {
        if let Some(glass_texture) = parts.texture(texture::GLASS, info.glass_type)? {
            // La AFL scala gli occhiali con il fattore del naso: è un suo
            // difetto, e va riprodotto.
            let factor = if is_afl { 0.175 } else { 0.15 };
            let scale = info.glass_scale as f32 * factor + 0.4;
            let placement = ShapePlacement {
                scale_x: scale,
                scale_y: scale,
                translate: Some(vec3(
                    anchors.nose.x,
                    anchors.nose.y + (info.glass_position_y - 11) as f32 * -1.5 + 5.0,
                    anchors.nose.z + 2.0,
                )),
                flip_x: false,
            };

            if let Ok(glass) = parts.shape(shape::GLASS, 0) {
                meshes.push(shape_mesh(
                    &glass,
                    placement,
                    Cull::None,
                    Modulate::new(
                        4,
                        Some(tables::glass_color(info.glass_color)),
                        None,
                        None,
                        Some(glass_texture),
                    ),
                    modulate::GLASS,
                )?);
            }
        }
    }

    Ok(Head { meshes })
}

#[derive(Debug, Clone, Copy)]
struct ShapePlacement {
    scale_x: f32,
    scale_y: f32,
    translate: Option<Vec3>,
    flip_x: bool,
}

impl Default for ShapePlacement {
    fn default() -> Self {
        Self {
            scale_x: 1.0,
            scale_y: 1.0,
            translate: None,
            flip_x: false,
        }
    }
}

/// Una forma collocata, con attributi quantizzati come nei buffer di FFL.
///
/// Normali, tangenti e parametri passano per gli stessi formati compatti del
/// buffer di FFL (10 bit, 8 bit con segno, 8 bit): senza, le luci
/// differirebbero di qualche livello di grigio.
fn shape_mesh(
    shape: &Shape,
    placement: ShapePlacement,
    cull: Cull,
    modulate: Modulate,
    modulate_type: usize,
) -> RenderResult<Mesh> {
    if shape.positions.is_empty() || shape.indices.len() < 3 {
        return Err(RenderError::InvalidResource(
            "shape has no drawable geometry".into(),
        ));
    }

    let ShapePlacement {
        scale_x,
        scale_y,
        translate,
        flip_x,
    } = placement;
    let scale_z = (scale_x + scale_y) * 0.5;
    let t = translate.unwrap_or(Vec3::ZERO);
    let count = shape.positions.len();

    let mut positions = Vec::with_capacity(count);
    let mut normals = Vec::with_capacity(count);
    let mut tangents = Vec::with_capacity(count);
    let mut texcoords = Vec::with_capacity(count);
    let mut parameters = Vec::with_capacity(count);

    for index in 0..count {
        let mut p = shape.positions[index];
        if flip_x {
            p.x = -p.x;
        }
        positions.push(vec3(
            p.x * scale_x + t.x,
            p.y * scale_y + t.y,
            p.z * scale_z + t.z,
        ));

        let mut n = shape.normals.get(index).copied().unwrap_or(Vec3::UNIT_Z);
        let mut tangent = shape.tangents.get(index).copied().unwrap_or(Vec3::ZERO);
        if flip_x {
            n.x = -n.x;
            tangent.x = -tangent.x;
        }
        normals.push(quantize_normal(n));
        tangents.push(quantize_tangent(tangent));
        texcoords.push(shape.texcoords.get(index).copied().unwrap_or_default());
        parameters.push(quantize_parameter(
            shape
                .parameters
                .get(index)
                .copied()
                .unwrap_or(DEFAULT_PARAMETER),
        ));
    }

    Ok(Mesh {
        positions,
        texcoords,
        normals,
        tangents,
        parameters,
        indices: shape.indices.clone(),
        cull,
        modulate,
        material: material_for(modulate_type),
        has_tangent: true,
        constant_color: None,
    })
}

fn quantize_normal(normal: Vec3) -> Vec3 {
    fn pack(value: f32) -> i32 {
        (round_even(value.clamp(-1.0, 1.0) * 511.0) as i32).clamp(-512, 511) & 0x3FF
    }
    crate::shape::decode_int2101010(
        pack(normal.x) | (pack(normal.y) << 10) | (pack(normal.z) << 20),
    )
}

fn quantize_tangent(tangent: Vec3) -> Vec3 {
    fn pack(value: f32) -> f32 {
        let packed = (round_even(value.clamp(-1.0, 1.0) * 127.0) as i32).clamp(-127, 127);
        packed as f32 / 127.0
    }
    vec3(pack(tangent.x), pack(tangent.y), pack(tangent.z)).normalize_or(Vec3::ZERO)
}

fn quantize_parameter(parameter: Vec4) -> Vec4 {
    fn pack(value: f32) -> f32 {
        f32::from(crate::math::to_byte01(value)) / 255.0
    }
    vec4(
        pack(parameter.x),
        pack(parameter.y),
        pack(parameter.z),
        pack(parameter.w),
    )
}

// ---------------------------------------------------------------------------
// Texture composte
// ---------------------------------------------------------------------------

/// Un quadrilatero piatto, senza luci, per comporre le texture.
fn overlay_quad(
    positions: [Vec3; 4],
    texcoords: [Vec2; 4],
    indices: [u32; 6],
    modulate: Modulate,
) -> Mesh {
    let shape = Shape {
        positions: positions.to_vec(),
        texcoords: texcoords.to_vec(),
        normals: vec![Vec3::UNIT_Z; 4],
        tangents: vec![Vec3::ZERO; 4],
        parameters: vec![DEFAULT_PARAMETER; 4],
        indices: indices.to_vec(),
        faceline: None,
    };
    shape_mesh(
        &shape,
        ShapePlacement::default(),
        Cull::None,
        modulate,
        modulate::FACELINE,
    )
    .expect("a quad always has geometry")
}

fn full_screen(modulate: Modulate) -> Mesh {
    overlay_quad(
        [
            vec3(-1.0, 1.0, 0.0),
            vec3(1.0, 1.0, 0.0),
            vec3(-1.0, -1.0, 0.0),
            vec3(1.0, -1.0, 0.0),
        ],
        [
            vec2(0.0, 0.0),
            vec2(1.0, 0.0),
            vec2(0.0, 1.0),
            vec2(1.0, 1.0),
        ],
        [0, 1, 2, 2, 1, 3],
        modulate,
    )
}

/// Disegna i quadrilateri in una texture nuova.
fn compose(
    meshes: &[Mesh],
    width: usize,
    height: usize,
    clear: Vec4,
    blend: Blend,
) -> Arc<Texture> {
    let clear = [
        crate::math::to_byte01(clear.x),
        crate::math::to_byte01(clear.y),
        crate::math::to_byte01(clear.z),
        crate::math::to_byte01(clear.w),
    ];
    let mut target = Target::new(width, height, clear, false);
    let frame = Frame {
        x: 0.0,
        width: width as f32,
        height: height as f32,
    };

    for mesh in meshes {
        raster::draw(
            &mut target,
            mesh,
            &Mat4::IDENTITY,
            &Mat4::IDENTITY,
            &Mat4::IDENTITY,
            frame,
            false,
            blend,
        );
    }

    Arc::new(Texture::rgba(width, height, target.pixels))
}

/// La texture del viso: trucco e rughe sopra il colore della pelle.
fn faceline_texture(
    parts: &Parts,
    info: &CharInfo,
    resolution: usize,
) -> RenderResult<Option<Arc<Texture>>> {
    if info.face_line == 0 && info.face_makeup == 0 && info.beard_type < 4 {
        return Ok(None);
    }

    let mut overlays = Vec::with_capacity(3);

    if info.face_makeup > 0 {
        if let Some(makeup) = parts.texture(texture::FACE_MAKEUP, info.face_makeup)? {
            overlays.push(full_screen(Modulate::new(
                1,
                None,
                None,
                None,
                Some(makeup),
            )));
        }
    }

    if info.face_line > 0 {
        if let Some(line) = parts.texture(texture::FACE_LINE, info.face_line)? {
            overlays.push(full_screen(Modulate::new(
                3,
                Some(vec4(0.0, 0.0, 0.0, 1.0)),
                None,
                None,
                Some(line),
            )));
        }
    }

    if info.beard_type >= 4 {
        if let Some(beard) = parts.texture(texture::BEARD, info.beard_type - 3)? {
            overlays.push(full_screen(Modulate::new(
                3,
                Some(tables::hair_color(info.beard_color)),
                None,
                None,
                Some(beard),
            )));
        }
    }

    if overlays.is_empty() {
        return Ok(None);
    }

    Ok(Some(compose(
        &overlays,
        (resolution / 2).max(1),
        resolution.max(1),
        tables::faceline_color(info.faceline_color),
        Blend::Faceline,
    )))
}

#[derive(Debug, Clone, Copy)]
enum Origin {
    Center,
    Left,
    Right,
}

/// Un elemento della maschera, nello spazio 64x64 di FFL.
#[derive(Debug, Clone, Copy)]
struct MaskPart {
    position: Vec2,
    scale: Vec2,
    rotation: f32,
    origin: Origin,
}

fn mask_quad(part: MaskPart, modulate: Modulate) -> Mesh {
    let offset_x = match part.origin {
        Origin::Center => -0.5,
        Origin::Left => -1.0,
        Origin::Right => 0.0,
    };
    let (u01, u23) = match part.origin {
        Origin::Right => (0.0, 1.0),
        _ => (1.0, 0.0),
    };

    const BASE_X: [f32; 4] = [1.0, 1.0, 0.0, 0.0];
    const BASE_Y: [f32; 4] = [-0.5, 0.5, 0.5, -0.5];
    const UV_Y: [f32; 4] = [0.0, 1.0, 1.0, 0.0];
    const TEX_SCALE_X: f32 = 0.889_614_64;
    const TEX_SCALE_Y: f32 = 0.927_667_5;

    let radians = part.rotation * (std::f32::consts::PI / 180.0);
    let (sin, cos) = radians.sin_cos();

    let mut positions = [Vec3::ZERO; 4];
    let mut texcoords = [Vec2::default(); 4];
    for i in 0..4 {
        let lx = BASE_X[i] + offset_x;
        let ly = BASE_Y[i];
        let xr = lx * part.scale.x * cos - ly * part.scale.y * sin;
        let yr = lx * part.scale.x * sin + ly * part.scale.y * cos;
        let xw = TEX_SCALE_X * xr + part.position.x;
        let yw = TEX_SCALE_Y * yr + part.position.y;

        positions[i] = vec3(xw * (2.0 / 64.0) - 1.0, 1.0 - yw * (2.0 / 64.0), 0.0);
        texcoords[i] = vec2(if i < 2 { u01 } else { u23 }, UV_Y[i]);
    }

    overlay_quad(positions, texcoords, [2, 1, 3, 1, 3, 0], modulate)
}

struct MaskLayout {
    eye_right: MaskPart,
    eye_left: MaskPart,
    eyebrow_right: MaskPart,
    eyebrow_left: MaskPart,
    mouth: MaskPart,
    mustache_right: MaskPart,
    mustache_left: MaskPart,
    mole: MaskPart,
}

/// Dove stanno occhi, sopracciglia, bocca, baffi e neo nella maschera.
fn mask_layout(info: &CharInfo) -> MaskLayout {
    const ADD_X: f32 = 3.532_331_2;
    const ADD_Y: f32 = 4.629_278;
    const SPACING: f32 = 0.889_614_64;
    const MUL_X: f32 = 1.779_229_3;
    const MUL_Y: f32 = 1.076_094_3;

    let add_y_eye = ADD_Y + 13.822_246;
    let add_y_eyebrow = ADD_Y + 11.920_528;
    let add_y_mouth = ADD_Y + 24.629_572;
    let add_y_mustache = ADD_Y + 27.134_275;
    let add_x_mole = ADD_X + 14.233_834;
    let add_y_mole = ADD_Y + 11.178_394 + 2.0 * MUL_Y;

    let f = |value: i32| value as f32;

    let eye_spacing = f(info.eye_spacing_x) * SPACING;
    let eye_scale = 0.4 * f(info.eye_scale) + 1.0;
    let eye_scale_y = 0.12 * f(info.eye_scale_y) + 0.64;
    let eye_size = vec2(5.343_75 * eye_scale, 4.5 * eye_scale * eye_scale_y);
    let eye_y = f(info.eye_position_y) * MUL_Y + add_y_eye;
    let eye_rotate =
        ((info.eye_rotate + tables::eye_rotate_offset(info.eye_type)) % 32) as f32 * (360.0 / 32.0);

    let brow_spacing = f(info.eyebrow_spacing_x) * SPACING;
    let brow_scale = 0.4 * f(info.eyebrow_scale) + 1.0;
    let brow_scale_y = 0.12 * f(info.eyebrow_scale_y) + 0.64;
    let brow_size = vec2(5.0625 * brow_scale, 4.5 * brow_scale * brow_scale_y);
    let brow_y = f(info.eyebrow_position_y) * MUL_Y + add_y_eyebrow;
    let brow_rotate = ((info.eyebrow_rotate + tables::eyebrow_rotate_offset(info.eyebrow_type))
        % 32) as f32
        * (360.0 / 32.0);

    let mouth_scale = 0.4 * f(info.mouth_scale) + 1.0;
    let mouth_scale_y = 0.12 * f(info.mouth_scale_y) + 0.64;
    let mouth_size = vec2(6.1875 * mouth_scale, 4.5 * mouth_scale * mouth_scale_y);
    let mouth_y = f(info.mouth_position_y) * MUL_Y + add_y_mouth;

    let mustache_scale = 0.4 * f(info.mustache_scale) + 1.0;
    let mustache_size = vec2(4.5 * mustache_scale, 9.0 * mustache_scale);
    let mustache_y = f(info.mustache_position_y) * MUL_Y + add_y_mustache;

    let mole_scale = 0.4 * f(info.mole_scale) + 1.0;
    let mole_x = f(info.mole_position_x) * MUL_X + add_x_mole;
    let mole_y = f(info.mole_position_y) * MUL_Y + add_y_mole;

    let part = |position: Vec2, scale: Vec2, rotation: f32, origin: Origin| MaskPart {
        position,
        scale,
        rotation,
        origin,
    };

    MaskLayout {
        eye_right: part(
            vec2(32.0 - eye_spacing, eye_y),
            eye_size,
            eye_rotate,
            Origin::Left,
        ),
        eye_left: part(
            vec2(eye_spacing + 32.0, eye_y),
            eye_size,
            360.0 - eye_rotate,
            Origin::Right,
        ),
        eyebrow_right: part(
            vec2(32.0 - brow_spacing, brow_y),
            brow_size,
            brow_rotate,
            Origin::Left,
        ),
        eyebrow_left: part(
            vec2(brow_spacing + 32.0, brow_y),
            brow_size,
            360.0 - brow_rotate,
            Origin::Right,
        ),
        mouth: part(vec2(32.0, mouth_y), mouth_size, 0.0, Origin::Center),
        mustache_right: part(vec2(32.0, mustache_y), mustache_size, 0.0, Origin::Left),
        mustache_left: part(vec2(32.0, mustache_y), mustache_size, 0.0, Origin::Right),
        mole: part(
            vec2(mole_x, mole_y),
            vec2(mole_scale, mole_scale),
            0.0,
            Origin::Center,
        ),
    }
}

fn eye_index(info: &CharInfo, kind: i32) -> i32 {
    match kind {
        1 => 60,
        3 => 61,
        4 => 26,
        5 => 47,
        _ => info.eye_type,
    }
}

fn mouth_index(info: &CharInfo, kind: i32) -> i32 {
    match kind {
        1 => 10,
        2 => 12,
        3 => 36,
        5 => 19,
        _ => info.mouth_type,
    }
}

/// La maschera: occhi, sopracciglia, bocca, baffi e neo in una texture sola.
fn mask_texture(
    parts: &Parts,
    info: &CharInfo,
    expression: usize,
    resolution: usize,
) -> RenderResult<Option<Arc<Texture>>> {
    let element = tables::EXPRESSIONS[expression.min(tables::EXPRESSIONS.len() - 1)];
    let eye_right_index = eye_index(info, element.eye_right);
    let eye_left_index = eye_index(info, element.eye_left);
    let mouth = mouth_index(info, element.mouth);
    let eyebrow = if element.eyebrow == 0 {
        info.eyebrow_type
    } else {
        element.eyebrow
    };

    let layout = mask_layout(info);
    let mut overlays = Vec::with_capacity(8);

    if info.mustache_type != 0 {
        if let Some(mustache) = parts.texture(texture::MUSTACHE, info.mustache_type)? {
            let modulate = Modulate::new(
                3,
                Some(tables::hair_color(info.beard_color)),
                None,
                None,
                Some(mustache),
            );
            overlays.push(mask_quad(layout.mustache_right, modulate.clone()));
            overlays.push(mask_quad(layout.mustache_left, modulate));
        }
    }

    if let Some(mouth_texture) = parts.texture(texture::MOUTH, mouth)? {
        let modulate = if mouth > 36 {
            Modulate::new(1, None, None, None, Some(mouth_texture))
        } else {
            Modulate::new(
                2,
                Some(tables::mouth_color_r(info.mouth_color)),
                Some(tables::mouth_color_g(info.mouth_color)),
                Some(Vec4::ONE),
                Some(mouth_texture),
            )
        };
        overlays.push(mask_quad(layout.mouth, modulate));
    }

    // Il sopracciglio 23 è "nessun sopracciglio".
    if eyebrow != 23 {
        if let Some(brow) = parts.texture(texture::EYEBROW, eyebrow)? {
            let modulate = Modulate::new(
                3,
                Some(tables::hair_color(info.eyebrow_color)),
                None,
                None,
                Some(brow),
            );
            overlays.push(mask_quad(layout.eyebrow_right, modulate.clone()));
            overlays.push(mask_quad(layout.eyebrow_left, modulate));
        }
    }

    let eye_right = parts.texture(texture::EYE, eye_right_index)?;
    let eye_left = parts.texture(texture::EYE, eye_left_index)?;
    let eye_modulate = |index: i32, eye: Arc<Texture>| {
        if tables::eye_texture_is_direct(index) {
            Modulate::new(1, None, None, None, Some(eye))
        } else {
            Modulate::new(
                2,
                Some(vec4(0.0, 1.0, 1.0, 1.0)),
                Some(Vec4::ONE),
                Some(tables::eye_color_b(info.eye_color)),
                Some(eye),
            )
        }
    };
    if let Some(eye) = eye_right {
        overlays.push(mask_quad(
            layout.eye_right,
            eye_modulate(eye_right_index, eye),
        ));
    }
    if let Some(eye) = eye_left {
        overlays.push(mask_quad(
            layout.eye_left,
            eye_modulate(eye_left_index, eye),
        ));
    }

    if info.mole_type != 0 {
        if let Some(mole) = parts.texture(texture::MOLE, info.mole_type)? {
            overlays.push(mask_quad(
                layout.mole,
                Modulate::new(3, Some(tables::MOLE_COLOR), None, None, Some(mole)),
            ));
        }
    }

    if overlays.is_empty() {
        return Ok(None);
    }

    let size = resolution.max(1);
    Ok(Some(compose(
        &overlays,
        size,
        size,
        Vec4::ZERO,
        Blend::Mask,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantized_normals_stay_unit_length() {
        let normal = quantize_normal(vec3(0.3, -0.5, 0.81).normalize());
        assert!((normal.length_squared() - 1.0).abs() < 1e-5);
        assert_eq!(quantize_normal(Vec3::ZERO), Vec3::UNIT_Z);
    }

    #[test]
    fn quantized_parameters_land_on_byte_steps() {
        let parameter = quantize_parameter(vec4(0.5, 1.2, -1.0, 0.25));
        assert_eq!(parameter.x, 128.0 / 255.0);
        assert_eq!(parameter.y, 1.0);
        assert_eq!(parameter.z, 0.0);
        assert_eq!(parameter.w, 64.0 / 255.0);
    }

    #[test]
    fn the_left_eye_mirrors_the_right_one() {
        let info = CharInfo {
            eye_spacing_x: 4,
            eye_position_y: 12,
            eye_scale: 4,
            eye_scale_y: 3,
            eye_rotate: 4,
            eye_type: 2,
            ..CharInfo::default()
        };
        let layout = mask_layout(&info);

        let right = 32.0 - layout.eye_right.position.x;
        let left = layout.eye_left.position.x - 32.0;
        assert!((right - left).abs() < 1e-4, "{right} vs {left}");
        assert_eq!(layout.eye_right.position.y, layout.eye_left.position.y);
        assert_eq!(layout.eye_left.rotation, 360.0 - layout.eye_right.rotation);
    }
}
