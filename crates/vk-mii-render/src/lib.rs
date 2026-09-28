//! # vk-mii-render
//!
//! Render **nativo** dei Mii: niente servizio di Mii Studio, niente rete.
//! Legge le forme e le texture di ogni tratto da `FFLResHigh.dat` — la stessa
//! risorsa che il launcher installa per Dolphin (§D-011) — e disegna il Mii
//! con un rasterizzatore software.
//!
//! La pipeline è quella di FFL: sceglie le forme, compone la texture del viso
//! e la maschera con occhi, bocca e sopracciglia, poi rasterizza con le sue
//! luci. Un render da 512 px richiede pochi millisecondi, ed è ciò che rende
//! l'editor dei Mii istantaneo invece che legato alla rete.
//!
//! Il crate è puro: non legge l'orologio, non tocca il disco. Riceve i byte
//! della risorsa e la studio data, restituisce pixel o un PNG.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod archive;
mod body;
pub mod charinfo;
pub mod error;
mod head;
pub mod math;
mod raster;
pub mod shape;
mod tables;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use body::BodyModels;
use charinfo::{CharInfo, STUDIO_SIZE};
pub use error::{RenderError, RenderResult};
use head::{Head, Parts};
use math::{vec3, Mat4, Vec3};
use raster::{Blend, Cull, Frame, Mesh, Modulate, Target};
use tables::modulate;

/// Teste già costruite tenute in memoria. Ognuna porta con sé due texture
/// (fino a 1,5 MB a 512 px): trenta bastano per una pagina di avatar e per
/// l'andirivieni dell'editor.
const HEAD_CACHE_ENTRIES: usize = 32;

/// Inquadrature disponibili.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// Ritratto, con l'attacco delle spalle.
    #[default]
    Face,
    /// Solo la testa.
    FaceOnly,
    /// Figura intera.
    AllBody,
}

/// Espressioni del viso, con gli identificativi di FFL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Expression {
    #[default]
    Normal,
    Smile,
    Anger,
    Sorrow,
    Surprise,
    Blink,
    OpenMouth,
}

impl Expression {
    fn ffl_id(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Smile => 1,
            Self::Anger => 2,
            Self::Sorrow => 3,
            Self::Surprise => 4,
            Self::Blink => 5,
            Self::OpenMouth => 6,
        }
    }
}

/// Cosa disegnare e come inquadrarlo.
#[derive(Debug, Clone, Copy)]
pub struct RenderRequest {
    /// Lato dell'immagine in pixel, fra 16 e 4096.
    pub size: u32,
    pub view: View,
    pub expression: Expression,
    /// Rotazione del personaggio in gradi, attorno a X, Y e Z.
    pub character_rotation: [f32; 3],
    /// Rotazione della telecamera in gradi, attorno al soggetto.
    pub camera_rotation: [f32; 3],
    pub camera_vertical_offset: f32,
    /// 1 è l'inquadratura normale; il valore è limitato a `[0.35, 3]`.
    pub camera_zoom: f32,
    /// Colore di fondo RGBA.
    pub background: [u8; 4],
}

impl Default for RenderRequest {
    fn default() -> Self {
        Self {
            size: 512,
            view: View::Face,
            expression: Expression::Normal,
            character_rotation: [0.0; 3],
            camera_rotation: [0.0; 3],
            camera_vertical_offset: 0.0,
            camera_zoom: 1.0,
            background: [255, 255, 255, 0],
        }
    }
}

/// Un'immagine RGBA non premoltiplicata.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// Codifica in PNG, con la compressione veloce: l'immagine viaggia in
    /// memoria verso la UI, non su disco.
    pub fn to_png(&self) -> RenderResult<Vec<u8>> {
        let mut out = Vec::with_capacity(self.rgba.len() / 4);
        {
            let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Fast);
            let mut writer = encoder
                .write_header()
                .map_err(|error| RenderError::Encoding(error.to_string()))?;
            writer
                .write_image_data(&self.rgba)
                .map_err(|error| RenderError::Encoding(error.to_string()))?;
        }
        Ok(out)
    }
}

type HeadKey = ([u8; STUDIO_SIZE], usize, usize);

/// Il renderer, con la risorsa caricata e le sue cache.
///
/// È `Sync`: più render possono girare in parallelo su thread diversi.
#[derive(Debug)]
pub struct MiiRenderer {
    parts: Parts,
    bodies: BodyModels,
    heads: Mutex<VecDeque<(HeadKey, Arc<Head>)>>,
}

impl MiiRenderer {
    /// Carica la risorsa dai suoi byte.
    pub fn new(resource: Vec<u8>) -> RenderResult<Self> {
        Ok(Self {
            parts: Parts::new(archive::ResourceArchive::parse(resource)?),
            bodies: BodyModels::load()?,
            heads: Mutex::default(),
        })
    }

    /// Render a partire dalla studio data esadecimale.
    pub fn render_hex(&self, studio_hex: &str, request: &RenderRequest) -> RenderResult<Image> {
        self.render(&charinfo::decode_studio_hex(studio_hex)?, request)
    }

    /// Render a partire dai 46 byte in chiaro della studio data.
    pub fn render(
        &self,
        studio: &[u8; STUDIO_SIZE],
        request: &RenderRequest,
    ) -> RenderResult<Image> {
        let size = request.size.clamp(16, 4096) as usize;
        let info = CharInfo::from_studio(studio);
        let expression = request.expression.ffl_id();
        let resolution = if size <= 384 { 256 } else { 512 };

        let head = self.head(studio, &info, expression, resolution)?;
        if head.meshes.is_empty() {
            return Err(RenderError::NothingToDraw("no head meshes".into()));
        }

        let camera = Camera::for_view(request.view);
        let width = size;
        let height = round_up_even((width as f32 * camera.aspect_height).ceil() as usize).max(1);

        let mut target = Target::new(width, height, request.background, true);
        let frame = Frame {
            x: 0.0,
            width: width as f32,
            height: height as f32,
        };

        let camera_rotation = radians(request.camera_rotation);
        let model_rotation = radians(request.character_rotation);
        let zoom = request.camera_zoom.clamp(0.35, 3.0);

        let mut position = orbit(camera.orbit_radius * zoom, camera_rotation);
        position.y += camera.base_y + request.camera_vertical_offset;
        let up = vec3(camera_rotation.z.sin(), camera_rotation.z.cos(), 0.0);
        let base_rotation = Mat4::rotation_x(model_rotation.x)
            * Mat4::rotation_y(model_rotation.y)
            * Mat4::rotation_z(model_rotation.z);
        let mut look_target = camera.target + vec3(0.0, request.camera_vertical_offset, 0.0);

        let body = (request.view != View::FaceOnly).then(|| body::pose(info.build, info.height));
        let mut head_model = base_rotation;
        if let Some(pose) = body {
            head_model = base_rotation * Mat4::translation(pose.head_translation);
            if !camera.absolute {
                position += pose.head_translation;
                look_target += pose.head_translation;
            }
        }

        let view = Mat4::look_at(position, look_target, up);

        if let Some(pose) = body {
            let model = base_rotation * Mat4::scale(pose.scale);
            let body_color = tables::favorite_color(info.favorite_color);

            for mesh in self.bodies.for_gender(info.gender) {
                let (color, kind) = if mesh.pants {
                    (tables::PANTS_COLOR, modulate::PANTS)
                } else {
                    (body_color, modulate::BODY)
                };
                let count = mesh.positions.len();
                let body_mesh = Mesh {
                    positions: mesh.positions.clone(),
                    texcoords: mesh.texcoords.clone(),
                    normals: mesh.normals.clone(),
                    tangents: vec![Vec3::ZERO; count],
                    parameters: vec![shape::DEFAULT_PARAMETER; count],
                    indices: mesh.indices.clone(),
                    cull: Cull::Back,
                    modulate: Modulate::color(0, color),
                    material: tables::material_for(kind),
                    has_tangent: false,
                    constant_color: Some(color),
                };
                raster::draw(
                    &mut target,
                    &body_mesh,
                    &model,
                    &view,
                    &camera.projection,
                    frame,
                    true,
                    Blend::Over,
                );
            }
        }

        for mesh in &head.meshes {
            raster::draw(
                &mut target,
                mesh,
                &head_model,
                &view,
                &camera.projection,
                frame,
                true,
                Blend::Over,
            );
        }

        Ok(Image {
            width: width as u32,
            height: height as u32,
            rgba: target.pixels,
        })
    }

    /// La testa del Mii, dalla cache o costruita adesso.
    fn head(
        &self,
        studio: &[u8; STUDIO_SIZE],
        info: &CharInfo,
        expression: usize,
        resolution: usize,
    ) -> RenderResult<Arc<Head>> {
        let key = (*studio, expression, resolution);

        {
            let mut heads = self.heads.lock().unwrap();
            if let Some(position) = heads.iter().position(|(cached, _)| *cached == key) {
                let entry = heads.remove(position).expect("posizione appena trovata");
                let head = entry.1.clone();
                heads.push_back(entry);
                return Ok(head);
            }
        }

        // La costruzione avviene fuori dal lucchetto: due thread che chiedono
        // la stessa testa fanno lo stesso lavoro, ma nessuno aspetta l'altro.
        let head = Arc::new(head::build(&self.parts, info, expression, resolution)?);

        let mut heads = self.heads.lock().unwrap();
        if !heads.iter().any(|(cached, _)| *cached == key) {
            heads.push_back((key, head.clone()));
            while heads.len() > HEAD_CACHE_ENTRIES {
                heads.pop_front();
            }
        }
        Ok(head)
    }
}

/// Inquadratura di un tipo di immagine.
struct Camera {
    base_y: f32,
    orbit_radius: f32,
    target: Vec3,
    projection: Mat4,
    aspect_height: f32,
    /// La telecamera non segue la testa: la figura intera sta ferma.
    absolute: bool,
}

impl Camera {
    fn for_view(view: View) -> Self {
        let projection =
            Mat4::perspective_fov(15.0 * (std::f32::consts::PI / 180.0), 1.0, 10.0, 1200.0);

        match view {
            View::Face | View::FaceOnly => {
                const SCALE: f32 = 0.14;
                let y = 4.805 / SCALE;
                let z = 57.553 / SCALE;
                Self {
                    base_y: y,
                    orbit_radius: z,
                    target: vec3(0.0, y, 0.0),
                    projection,
                    aspect_height: 1.0,
                    absolute: false,
                }
            }
            View::AllBody => Self {
                base_y: 90.0,
                orbit_radius: 760.0,
                target: vec3(0.0, 95.0, 0.0),
                projection,
                aspect_height: 1.0,
                absolute: true,
            },
        }
    }
}

fn orbit(radius: f32, rotation: Vec3) -> Vec3 {
    vec3(
        radius * -rotation.y.sin() * rotation.x.cos(),
        radius * rotation.x.sin(),
        radius * rotation.y.cos() * rotation.x.cos(),
    )
}

fn radians(degrees: [f32; 3]) -> Vec3 {
    let convert = |value: f32| ieee_remainder(value, 360.0) * (std::f32::consts::PI / 180.0);
    vec3(
        convert(degrees[0]),
        convert(degrees[1]),
        convert(degrees[2]),
    )
}

/// `MathF.IEEERemainder`: il resto rispetto al multiplo più vicino.
fn ieee_remainder(x: f32, y: f32) -> f32 {
    let regular = x % y;
    if regular.is_nan() {
        return f32::NAN;
    }
    if regular == 0.0 && x.is_sign_negative() {
        return -0.0;
    }

    let alternative = regular - y.abs() * x.signum();
    if alternative.abs() == regular.abs() {
        let division = x / y;
        let rounded = math::round_even(division);
        if rounded.abs() > division.abs() {
            alternative
        } else {
            regular
        }
    } else if alternative.abs() < regular.abs() {
        alternative
    } else {
        regular
    }
}

fn round_up_even(value: usize) -> usize {
    if value % 2 == 0 {
        value
    } else {
        value + 1
    }
}

/// Colore di fondo da `RRGGBBAA`, come il parametro `bgColor` di Mii Studio.
pub fn parse_background(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.trim();
    if hex.len() != 8 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(value.to_be_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_remainder_matches_dotnet() {
        assert_eq!(ieee_remainder(370.0, 360.0), 10.0);
        assert_eq!(ieee_remainder(190.0, 360.0), -170.0);
        assert_eq!(ieee_remainder(-190.0, 360.0), 170.0);
        assert_eq!(ieee_remainder(180.0, 360.0), 180.0);
        assert_eq!(ieee_remainder(0.0, 360.0), 0.0);
    }

    #[test]
    fn backgrounds_parse_like_mii_studio() {
        assert_eq!(parse_background("FFFFFF00"), Some([255, 255, 255, 0]));
        assert_eq!(parse_background("12345678"), Some([0x12, 0x34, 0x56, 0x78]));
        assert_eq!(parse_background("fff"), None);
        assert_eq!(parse_background("GGGGGGGG"), None);
    }

    #[test]
    fn a_bad_resource_is_refused_before_rendering() {
        assert!(MiiRenderer::new(vec![0; 64]).is_err());
    }
}
