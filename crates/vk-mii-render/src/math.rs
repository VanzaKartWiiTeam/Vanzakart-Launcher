//! Vettori e matrici con la semantica di `System.Numerics`.
//!
//! Le matrici sono **per righe** e i vettori si moltiplicano a sinistra
//! (`v * M`), quindi `A * B` significa "prima A, poi B". Tenere questa
//! convenzione, invece di tradurre ogni formula in quella a colonne, rende
//! il risultato riproducibile pixel per pixel.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

pub const fn vec3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3 { x, y, z }
}

pub const fn vec4(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
    Vec4 { x, y, z, w }
}

impl Vec3 {
    pub const ZERO: Self = vec3(0.0, 0.0, 0.0);
    pub const UNIT_Z: Self = vec3(0.0, 0.0, 1.0);

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Self) -> Self {
        vec3(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    pub fn normalize(self) -> Self {
        self / self.length_squared().sqrt()
    }

    /// Normalizzato, oppure `fallback` se il vettore è (quasi) nullo.
    pub fn normalize_or(self, fallback: Self) -> Self {
        if self.length_squared() < 1e-8 {
            fallback
        } else {
            self.normalize()
        }
    }

    /// Prodotto componente per componente.
    pub fn hadamard(self, other: Self) -> Self {
        vec3(self.x * other.x, self.y * other.y, self.z * other.z)
    }

    /// `Vector3.Reflect`: `v - 2 * dot(v, n) * n`.
    pub fn reflect(self, normal: Self) -> Self {
        self - normal * (2.0 * self.dot(normal))
    }
}

impl Vec4 {
    pub const ONE: Self = vec4(1.0, 1.0, 1.0, 1.0);
    pub const ZERO: Self = vec4(0.0, 0.0, 0.0, 0.0);

    pub fn lerp(self, other: Self, amount: f32) -> Self {
        self * (1.0 - amount) + other * amount
    }

    pub fn xyz(self) -> Vec3 {
        vec3(self.x, self.y, self.z)
    }

    /// Ogni canale riportato in `[0, 1]`.
    pub fn clamp01(self) -> Self {
        vec4(
            clamp01(self.x),
            clamp01(self.y),
            clamp01(self.z),
            clamp01(self.w),
        )
    }
}

macro_rules! vector_ops {
    ($type:ident { $($field:ident),+ }) => {
        impl Add for $type {
            type Output = Self;
            fn add(self, rhs: Self) -> Self {
                Self { $($field: self.$field + rhs.$field),+ }
            }
        }

        impl AddAssign for $type {
            fn add_assign(&mut self, rhs: Self) {
                $(self.$field += rhs.$field;)+
            }
        }

        impl Sub for $type {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self {
                Self { $($field: self.$field - rhs.$field),+ }
            }
        }

        impl Mul<f32> for $type {
            type Output = Self;
            fn mul(self, rhs: f32) -> Self {
                Self { $($field: self.$field * rhs),+ }
            }
        }

        impl Div<f32> for $type {
            type Output = Self;
            fn div(self, rhs: f32) -> Self {
                Self { $($field: self.$field / rhs),+ }
            }
        }

        impl Neg for $type {
            type Output = Self;
            fn neg(self) -> Self {
                Self { $($field: -self.$field),+ }
            }
        }
    };
}

vector_ops!(Vec2 { x, y });
vector_ops!(Vec3 { x, y, z });
vector_ops!(Vec4 { x, y, z, w });

pub fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// `MathF.Round`: arrotonda al pari, come fa .NET per impostazione predefinita.
pub fn round_even(value: f32) -> f32 {
    value.round_ties_even()
}

/// Byte da un canale in `[0, 1]`, con l'arrotondamento di .NET.
pub fn to_byte01(value: f32) -> u8 {
    (round_even(clamp01(value) * 255.0) as i32).clamp(0, 255) as u8
}

/// Matrice 4x4 per righe: `m[r][c]` è `M(r+1)(c+1)` di `System.Numerics`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    pub m: [[f32; 4]; 4],
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    pub fn translation(t: Vec3) -> Self {
        let mut out = Self::IDENTITY;
        out.m[3][0] = t.x;
        out.m[3][1] = t.y;
        out.m[3][2] = t.z;
        out
    }

    pub fn scale(s: Vec3) -> Self {
        let mut out = Self::IDENTITY;
        out.m[0][0] = s.x;
        out.m[1][1] = s.y;
        out.m[2][2] = s.z;
        out
    }

    pub fn rotation_x(radians: f32) -> Self {
        let (s, c) = radians.sin_cos();
        let mut out = Self::IDENTITY;
        out.m[1][1] = c;
        out.m[1][2] = s;
        out.m[2][1] = -s;
        out.m[2][2] = c;
        out
    }

    pub fn rotation_y(radians: f32) -> Self {
        let (s, c) = radians.sin_cos();
        let mut out = Self::IDENTITY;
        out.m[0][0] = c;
        out.m[0][2] = -s;
        out.m[2][0] = s;
        out.m[2][2] = c;
        out
    }

    pub fn rotation_z(radians: f32) -> Self {
        let (s, c) = radians.sin_cos();
        let mut out = Self::IDENTITY;
        out.m[0][0] = c;
        out.m[0][1] = s;
        out.m[1][0] = -s;
        out.m[1][1] = c;
        out
    }

    /// `Matrix4x4.CreateLookAt`, destrorsa.
    pub fn look_at(position: Vec3, target: Vec3, up: Vec3) -> Self {
        let z = (position - target).normalize();
        let x = up.cross(z).normalize();
        let y = z.cross(x);

        Self {
            m: [
                [x.x, y.x, z.x, 0.0],
                [x.y, y.y, z.y, 0.0],
                [x.z, y.z, z.z, 0.0],
                [-x.dot(position), -y.dot(position), -z.dot(position), 1.0],
            ],
        }
    }

    /// `Matrix4x4.CreatePerspectiveFieldOfView`, destrorsa, profondità `[0, 1]`.
    pub fn perspective_fov(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        let y_scale = 1.0 / (fov_y * 0.5).tan();
        let x_scale = y_scale / aspect;
        let range = far / (near - far);

        Self {
            m: [
                [x_scale, 0.0, 0.0, 0.0],
                [0.0, y_scale, 0.0, 0.0],
                [0.0, 0.0, range, -1.0],
                [0.0, 0.0, near * range, 0.0],
            ],
        }
    }

    /// `Vector3.Transform`: punto con `w = 1`, senza divisione prospettica.
    pub fn transform_point(&self, v: Vec3) -> Vec3 {
        let m = &self.m;
        vec3(
            v.x * m[0][0] + v.y * m[1][0] + v.z * m[2][0] + m[3][0],
            v.x * m[0][1] + v.y * m[1][1] + v.z * m[2][1] + m[3][1],
            v.x * m[0][2] + v.y * m[1][2] + v.z * m[2][2] + m[3][2],
        )
    }

    /// `Vector4.Transform`.
    pub fn transform_vec4(&self, v: Vec4) -> Vec4 {
        let m = &self.m;
        vec4(
            v.x * m[0][0] + v.y * m[1][0] + v.z * m[2][0] + v.w * m[3][0],
            v.x * m[0][1] + v.y * m[1][1] + v.z * m[2][1] + v.w * m[3][1],
            v.x * m[0][2] + v.y * m[1][2] + v.z * m[2][2] + v.w * m[3][2],
            v.x * m[0][3] + v.y * m[1][3] + v.z * m[2][3] + v.w * m[3][3],
        )
    }

    /// `Vector3.TransformNormal`: ignora la traslazione.
    pub fn transform_normal(&self, v: Vec3) -> Vec3 {
        let m = &self.m;
        vec3(
            v.x * m[0][0] + v.y * m[1][0] + v.z * m[2][0],
            v.x * m[0][1] + v.y * m[1][1] + v.z * m[2][1],
            v.x * m[0][2] + v.y * m[1][2] + v.z * m[2][2],
        )
    }
}

impl Mul for Mat4 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        let mut out = [[0.0f32; 4]; 4];
        for (row, out_row) in out.iter_mut().enumerate() {
            for (column, cell) in out_row.iter_mut().enumerate() {
                *cell = (0..4)
                    .map(|k| self.m[row][k] * rhs.m[k][column])
                    .sum::<f32>();
            }
        }
        Self { m: out }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length_squared() < 1e-8
    }

    #[test]
    fn a_rotation_then_a_translation_is_applied_in_reading_order() {
        // Convenzione per righe: `R * T` ruota prima e trasla poi.
        let rotate = Mat4::rotation_y(std::f32::consts::FRAC_PI_2);
        let translate = Mat4::translation(vec3(10.0, 0.0, 0.0));

        let moved = (rotate * translate).transform_point(vec3(1.0, 0.0, 0.0));
        assert!(close(moved, vec3(10.0, 0.0, -1.0)), "{moved:?}");
    }

    #[test]
    fn look_at_puts_the_target_straight_ahead() {
        let view = Mat4::look_at(vec3(0.0, 0.0, 10.0), Vec3::ZERO, vec3(0.0, 1.0, 0.0));
        let target = view.transform_point(Vec3::ZERO);
        assert!(close(target, vec3(0.0, 0.0, -10.0)), "{target:?}");
    }

    #[test]
    fn the_projection_maps_near_and_far_to_zero_and_one() {
        let projection = Mat4::perspective_fov(0.5, 1.0, 10.0, 1200.0);
        let near = projection.transform_vec4(vec4(0.0, 0.0, -10.0, 1.0));
        let far = projection.transform_vec4(vec4(0.0, 0.0, -1200.0, 1.0));

        assert!((near.z / near.w).abs() < 1e-5);
        assert!((far.z / far.w - 1.0).abs() < 1e-5);
    }

    #[test]
    fn bytes_round_half_to_even_like_dotnet() {
        assert_eq!(round_even(0.5), 0.0);
        assert_eq!(round_even(1.5), 2.0);
        assert_eq!(round_even(2.5), 2.0);
        assert_eq!(to_byte01(1.0), 255);
        assert_eq!(to_byte01(-3.0), 0);
    }
}
