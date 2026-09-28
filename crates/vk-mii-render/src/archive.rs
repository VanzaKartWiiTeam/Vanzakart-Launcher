//! L'archivio di risorse FFL (`FFLResHigh.dat`).
//!
//! È il formato in cui le librerie Mii di Nintendo tengono forme e texture di
//! ogni tratto: un'intestazione `FFRA` con due tabelle di parti — texture e
//! forme — e per ogni parte posizione, dimensione e compressione. Il file che
//! il launcher installa è `AFLResHigh_2_3.dat` dell'archivio Miitomo (§D-011),
//! la variante "AFL 2.3" con più occhi, bocche e occhiali della FFL originale.
//!
//! L'archivio resta in memoria intero (pochi megabyte) e ogni parte si
//! decomprime solo quando serve.

use std::io::Read;

use crate::error::{RenderError, RenderResult};

const MAGIC: u32 = 0x4646_5241; // "FFRA"
const VERSION: u32 = 0x0007_0000;

const HEADER_SIZE: usize = 0x4A00;
const TEXTURE_HEADER_OFFSET: usize = 0x14;

/// Marcatore dei file con vertici a mezza precisione.
const HALF_FLOAT_LAYOUT: u32 = 0x841F_10A7;

const EXPANDED_SIZE_AFL: u32 = 0x0239_D5E0;
const EXPANDED_SIZE_AFL23: u32 = 0x0250_2DE0;

/// Numero di texture per tipo, nell'ordine dell'archivio: barba, cappello,
/// occhio, sopracciglio, linee del viso, trucco, occhiali, neo, bocca, baffi,
/// linea del naso.
const TEXTURE_COUNTS_FFL: [usize; 11] = [3, 132, 62, 24, 12, 12, 9, 2, 37, 6, 18];
const TEXTURE_COUNTS_AFL: [usize; 11] = [3, 132, 80, 28, 12, 12, 9, 2, 52, 6, 18];
const TEXTURE_COUNTS_AFL23: [usize; 11] = [3, 132, 80, 28, 12, 12, 20, 2, 52, 6, 18];

/// Numero di forme per tipo: barba, cappello, cappello con visiera, viso,
/// occhiali, maschera, linea del naso, naso, capelli, capelli con cappello,
/// fronte, fronte con cappello.
const SHAPE_COUNTS: [usize; 12] = [4, 132, 132, 12, 1, 12, 18, 18, 132, 132, 132, 132];

/// Tipi di texture, come indici della tabella.
pub mod texture {
    pub const BEARD: usize = 0;
    pub const CAP: usize = 1;
    pub const EYE: usize = 2;
    pub const EYEBROW: usize = 3;
    pub const FACE_LINE: usize = 4;
    pub const FACE_MAKEUP: usize = 5;
    pub const GLASS: usize = 6;
    pub const MOLE: usize = 7;
    pub const MOUTH: usize = 8;
    pub const MUSTACHE: usize = 9;
    pub const NOSE_LINE: usize = 10;
}

/// Tipi di forma, come indici della tabella.
pub mod shape {
    pub const BEARD: usize = 0;
    pub const HAT: usize = 1;
    pub const FACELINE: usize = 3;
    pub const GLASS: usize = 4;
    pub const MASK: usize = 5;
    pub const NOSE_LINE: usize = 6;
    pub const NOSE: usize = 7;
    pub const HAIR: usize = 8;
    pub const FOREHEAD: usize = 10;
}

#[derive(Debug, Clone, Copy)]
struct PartInfo {
    data_pos: usize,
    data_size: usize,
    compressed_size: usize,
    window_bits: u8,
    strategy: u8,
}

/// L'archivio caricato.
#[derive(Debug)]
pub struct ResourceArchive {
    bytes: Vec<u8>,
    big_endian: bool,
    texture_parts: Vec<Vec<PartInfo>>,
    shape_parts: Vec<Vec<PartInfo>>,
    /// Variante AFL: texture lineari e alcune costanti diverse dalla FFL.
    pub is_afl: bool,
    /// Vertici in mezza precisione invece che in `f32`.
    pub half_float_layout: bool,
}

impl ResourceArchive {
    /// Interpreta i byte di `FFLResHigh.dat`.
    pub fn parse(bytes: Vec<u8>) -> RenderResult<Self> {
        if bytes.len() < HEADER_SIZE {
            return Err(RenderError::InvalidResource(format!(
                "the resource is too small ({} bytes)",
                bytes.len()
            )));
        }

        let big_endian = if u32::from_be_bytes(bytes[0..4].try_into().unwrap()) == MAGIC {
            true
        } else if u32::from_le_bytes(bytes[0..4].try_into().unwrap()) == MAGIC {
            false
        } else {
            return Err(RenderError::InvalidResource(
                "the resource does not start with FFRA".into(),
            ));
        };

        let version = read_u32(&bytes, 4, big_endian);
        if version != VERSION {
            return Err(RenderError::InvalidResource(format!(
                "unsupported resource version {version:#010x}"
            )));
        }

        let expanded = read_u32(&bytes, 12, big_endian);
        let half_float_layout = read_u32(&bytes, 16, big_endian) == HALF_FLOAT_LAYOUT;

        let hint = expanded >> 29;
        let expanded = expanded & 0x1FFF_FFFF;
        let is_afl23 = hint == 3 || expanded == EXPANDED_SIZE_AFL23;
        let is_afl = is_afl23 || hint == 2 || expanded == EXPANDED_SIZE_AFL;

        let texture_counts: &[usize] = if is_afl23 {
            &TEXTURE_COUNTS_AFL23
        } else if is_afl {
            &TEXTURE_COUNTS_AFL
        } else {
            &TEXTURE_COUNTS_FFL
        };

        let texture_table = TEXTURE_HEADER_OFFSET + texture_counts.len() * 4;
        let texture_parts = parse_parts(&bytes, big_endian, texture_table, texture_counts)?;

        let texture_header = texture_counts.len() * 4 + texture_counts.iter().sum::<usize>() * 16;
        let shape_table = TEXTURE_HEADER_OFFSET + texture_header + SHAPE_COUNTS.len() * 4;
        let shape_parts = parse_parts(&bytes, big_endian, shape_table, &SHAPE_COUNTS)?;

        Ok(Self {
            bytes,
            big_endian,
            texture_parts,
            shape_parts,
            is_afl,
            half_float_layout,
        })
    }

    pub fn big_endian(&self) -> bool {
        self.big_endian
    }

    /// Byte decompressi di una forma. Vuoto quando la parte non esiste.
    pub fn shape_part(&self, part: usize, index: usize) -> RenderResult<Vec<u8>> {
        self.load(&self.shape_parts, part, index, "shape")
    }

    /// Byte decompressi di una texture. Vuoto quando la parte non esiste.
    pub fn texture_part(&self, part: usize, index: usize) -> RenderResult<Vec<u8>> {
        self.load(&self.texture_parts, part, index, "texture")
    }

    fn load(
        &self,
        table: &[Vec<PartInfo>],
        part: usize,
        index: usize,
        kind: &str,
    ) -> RenderResult<Vec<u8>> {
        let info = table
            .get(part)
            .and_then(|entries| entries.get(index))
            .ok_or_else(|| {
                RenderError::InvalidResource(format!("{kind} part {part}:{index} is out of range"))
            })?;

        if info.data_size == 0 {
            return Ok(Vec::new());
        }

        self.decode(info).map_err(|reason| {
            RenderError::InvalidResource(format!("{kind} part {part}:{index}: {reason}"))
        })
    }

    fn decode(&self, info: &PartInfo) -> Result<Vec<u8>, String> {
        if info.data_pos >= self.bytes.len() {
            return Err("data position out of range".into());
        }

        // Strategia 5: la parte è salvata senza compressione.
        if info.strategy == 5 {
            return self
                .bytes
                .get(info.data_pos..info.data_pos + info.data_size)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| "uncompressed part out of bounds".into());
        }

        if info.compressed_size == 0 {
            return Err("compressed part without a size".into());
        }
        let compressed = self
            .bytes
            .get(info.data_pos..info.data_pos + info.compressed_size)
            .ok_or("compressed part out of bounds")?;

        let mut decoded = match info.window_bits {
            0..=7 => inflate_zlib(compressed),
            8..=15 => inflate_gzip(compressed),
            16 => inflate_zlib(compressed).or_else(|_| inflate_gzip(compressed)),
            _ => inflate_zlib(compressed),
        }
        .map_err(|error| format!("decompression failed: {error}"))?;

        decoded.resize(info.data_size, 0);
        Ok(decoded)
    }
}

fn inflate_zlib(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(bytes).read_to_end(&mut out)?;
    Ok(out)
}

fn inflate_gzip(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes).read_to_end(&mut out)?;
    Ok(out)
}

fn parse_parts(
    bytes: &[u8],
    big_endian: bool,
    base: usize,
    counts: &[usize],
) -> RenderResult<Vec<Vec<PartInfo>>> {
    let mut offset = base;
    let mut table = Vec::with_capacity(counts.len());

    for count in counts {
        let mut entries = Vec::with_capacity(*count);
        for _ in 0..*count {
            if offset + 16 > bytes.len() {
                return Err(RenderError::InvalidResource(
                    "the part table is truncated".into(),
                ));
            }

            entries.push(PartInfo {
                data_pos: read_u32(bytes, offset, big_endian) as usize,
                data_size: read_u32(bytes, offset + 4, big_endian) as usize,
                compressed_size: read_u32(bytes, offset + 8, big_endian) as usize,
                window_bits: bytes[offset + 13],
                strategy: bytes[offset + 15],
            });
            offset += 16;
        }
        table.push(entries);
    }

    Ok(table)
}

pub(crate) fn read_u16(bytes: &[u8], offset: usize, big_endian: bool) -> u16 {
    let pair = [bytes[offset], bytes[offset + 1]];
    if big_endian {
        u16::from_be_bytes(pair)
    } else {
        u16::from_le_bytes(pair)
    }
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize, big_endian: bool) -> u32 {
    let quad = [
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ];
    if big_endian {
        u32::from_be_bytes(quad)
    } else {
        u32::from_le_bytes(quad)
    }
}

pub(crate) fn read_f32(bytes: &[u8], offset: usize, big_endian: bool) -> f32 {
    f32::from_bits(read_u32(bytes, offset, big_endian))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_file_is_refused() {
        assert!(ResourceArchive::parse(vec![0; 16]).is_err());
    }

    #[test]
    fn a_file_without_the_magic_is_refused() {
        let error = ResourceArchive::parse(vec![0; HEADER_SIZE]).unwrap_err();
        assert!(error.to_string().contains("FFRA"));
    }

    #[test]
    fn an_unknown_version_is_refused() {
        let mut bytes = vec![0; HEADER_SIZE];
        bytes[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        bytes[4..8].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        assert!(ResourceArchive::parse(bytes)
            .unwrap_err()
            .to_string()
            .contains("version"));
    }
}
