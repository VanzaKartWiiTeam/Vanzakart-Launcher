//! Il corpo del Mii.
//!
//! FFL disegna solo la testa: il corpo viene da due modelli statici (maschio e
//! femmina) nel formato `riomodel`, incorporati nel crate. La testa si
//! appoggia sul collo, e altezza e corporatura del Mii scalano il busto.

use crate::error::{RenderError, RenderResult};
use crate::math::{vec2, vec3, Mat4, Vec2, Vec3};

const MALE: &[u8] = include_bytes!("../resources/mii_static_body_3ds_male_LE.rmdl");
const FEMALE: &[u8] = include_bytes!("../resources/mii_static_body_3ds_female_LE.rmdl");

/// Da `body_models.csv`, riga del corpo "3ds": scala del modello e altezza a
/// cui si aggancia la testa.
const MODEL_SCALE: f32 = 7.0;
const HEAD_Y: f32 = 10.7766;

/// Una maglia del corpo, con la sua trasformazione già applicata.
#[derive(Debug, Clone)]
pub struct BodyMesh {
    pub positions: Vec<Vec3>,
    pub texcoords: Vec<Vec2>,
    pub normals: Vec<Vec3>,
    pub indices: Vec<u32>,
    /// Le maglie dispari sono i pantaloni.
    pub pants: bool,
}

#[derive(Debug, Clone)]
pub struct BodyModels {
    pub male: Vec<BodyMesh>,
    pub female: Vec<BodyMesh>,
}

/// Dove sta la testa e quanto è grande il busto, per un Mii.
#[derive(Debug, Clone, Copy)]
pub struct BodyPose {
    pub scale: Vec3,
    pub head_translation: Vec3,
}

impl BodyModels {
    pub fn load() -> RenderResult<Self> {
        Ok(Self {
            male: parse_rio_model(MALE)?,
            female: parse_rio_model(FEMALE)?,
        })
    }

    pub fn for_gender(&self, gender: i32) -> &[BodyMesh] {
        if gender.rem_euclid(2) == 1 {
            &self.female
        } else {
            &self.male
        }
    }
}

/// Scala del busto e posizione della testa per corporatura e altezza.
pub fn pose(build: i32, height: i32) -> BodyPose {
    let build = build.clamp(0, 127) as f32;
    let height = height.clamp(0, 127) as f32;

    let x = (build * (height * 0.003_671_875 + 0.4)) / 128.0 + height * 0.001_796_875 + 0.4;
    let y = height * 0.006_015_625 + 0.5;
    let scale = vec3(x, y, x);

    BodyPose {
        scale,
        head_translation: vec3(0.0, HEAD_Y, 0.0).hadamard(scale) * MODEL_SCALE,
    }
}

fn parse_rio_model(bytes: &[u8]) -> RenderResult<Vec<BodyMesh>> {
    let fail = |reason: &str| RenderError::InvalidResource(format!("body model: {reason}"));

    if bytes.len() < 0x20 || &bytes[0..8] != b"riomodel" {
        return Err(fail("not a riomodel file"));
    }

    let declared = u32_at(bytes, 0x0C) as usize;
    if declared > 0 && declared != bytes.len() {
        return Err(fail("size mismatch"));
    }

    let mesh_list = 0x10 + i32_at(bytes, 0x10) as isize;
    let mesh_count = u32_at(bytes, 0x14) as usize;
    if mesh_count == 0 || mesh_count > 1024 || mesh_list < 0 {
        return Err(fail("invalid mesh table"));
    }

    const MESH_SIZE: usize = 0x38;
    const VERTEX_SIZE: usize = 0x20;
    let mut meshes = Vec::with_capacity(mesh_count);

    for mesh_index in 0..mesh_count {
        let mesh = mesh_list as usize + mesh_index * MESH_SIZE;
        if mesh + MESH_SIZE > bytes.len() {
            return Err(fail("mesh header out of bounds"));
        }

        let vertex_count = u32_at(bytes, mesh + 0x04) as usize;
        let index_count = u32_at(bytes, mesh + 0x0C) as usize;
        let vector = |offset: usize| {
            vec3(
                f32_at(bytes, mesh + offset),
                f32_at(bytes, mesh + offset + 4),
                f32_at(bytes, mesh + offset + 8),
            )
        };
        let (scale, rotate, translate) = (vector(0x10), vector(0x1C), vector(0x28));

        if vertex_count == 0 || index_count < 3 {
            continue;
        }
        if vertex_count > 200_000 || index_count > 2_000_000 {
            return Err(fail("mesh too large"));
        }

        let vertices = (mesh as isize + i32_at(bytes, mesh) as isize) as usize;
        let indices = (mesh as isize + 0x08 + i32_at(bytes, mesh + 0x08) as isize) as usize;
        if vertices + vertex_count * VERTEX_SIZE > bytes.len()
            || indices + index_count * 4 > bytes.len()
        {
            return Err(fail("mesh data out of bounds"));
        }

        // Scala, rotazione e traslazione della maglia si applicano una volta
        // qui, invece che a ogni fotogramma: il risultato è lo stesso.
        let srt = Mat4::scale(scale)
            * (Mat4::rotation_x(rotate.x)
                * Mat4::rotation_y(rotate.y)
                * Mat4::rotation_z(rotate.z))
            * Mat4::translation(translate);

        let mut positions = Vec::with_capacity(vertex_count);
        let mut texcoords = Vec::with_capacity(vertex_count);
        let mut normals = Vec::with_capacity(vertex_count);
        for vertex in 0..vertex_count {
            let at = vertices + vertex * VERTEX_SIZE;
            let float = |offset: usize| f32_at(bytes, at + offset);
            positions.push(srt.transform_point(vec3(float(0x00), float(0x04), float(0x08))));
            texcoords.push(vec2(float(0x0C), float(0x10)));
            normals.push(srt.transform_normal(vec3(float(0x14), float(0x18), float(0x1C))));
        }

        let indices = (0..index_count)
            .map(|index| {
                let value = u32_at(bytes, indices + index * 4);
                if (value as usize) < vertex_count {
                    Ok(value)
                } else {
                    Err(fail("index outside the vertex buffer"))
                }
            })
            .collect::<RenderResult<Vec<u32>>>()?;

        meshes.push(BodyMesh {
            positions,
            texcoords,
            normals,
            indices,
            pants: mesh_index % 2 == 1,
        });
    }

    if meshes.is_empty() {
        return Err(fail("no meshes"));
    }
    Ok(meshes)
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn i32_at(bytes: &[u8], offset: usize) -> i32 {
    u32_at(bytes, offset) as i32
}

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_bits(u32_at(bytes, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_embedded_bodies_load() {
        let models = BodyModels::load().unwrap();
        assert!(!models.male.is_empty());
        assert!(!models.female.is_empty());
        assert!(models.male.iter().any(|mesh| mesh.pants));
    }

    #[test]
    fn taller_miis_carry_their_head_higher() {
        let short = pose(64, 0);
        let tall = pose(64, 127);
        assert!(tall.head_translation.y > short.head_translation.y);
        assert!(tall.scale.y > short.scale.y);
    }

    #[test]
    fn a_broken_model_is_refused() {
        assert!(parse_rio_model(b"not a model at all, clearly").is_err());
    }
}
