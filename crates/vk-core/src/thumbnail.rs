//! Miniature delle immagini che il launcher mostra piccole.
//!
//! Le immagini dei rank del sito sono PNG da 914×1080 e fino a 450 KB, con
//! più di metà della superficie trasparente attorno al disegno. Mostrate a
//! 24 px accanto a un nome, così come sono diventano un puntino in mezzo al
//! vuoto, e viaggiano verso la UI come data URI da centinaia di KB ciascuna.
//!
//! [`badge_thumbnail`] le ritaglia sul disegno e le riduce al lato che serve:
//! qualche KB, e il simbolo occupa tutto lo spazio che ha (§D-094).

use std::io::Cursor;

use crate::error::{CoreError, CoreResult};

/// Lato massimo accettato in ingresso: oltre, il file non è un'icona e
/// decodificarlo vorrebbe dire allocare centinaia di MB.
const MAX_INPUT_SIDE: u32 = 4096;

/// Sotto questa opacità un pixel conta come sfondo: tiene fuori il rumore
/// dell'esportazione e dentro gli aloni del disegno.
const ALPHA_THRESHOLD: u8 = 8;

/// Ritaglia `png` sul contenuto non trasparente e lo riduce perché il lato
/// più lungo non superi `max_side`. Restituisce un PNG RGBA.
///
/// Un'immagine già più piccola non viene ingrandita: si ritaglia soltanto.
pub fn badge_thumbnail(png: &[u8], max_side: u32) -> CoreResult<Vec<u8>> {
    let image = decode_rgba(png)?;
    let image = trim_transparent(image);

    let longest = image.width.max(image.height).max(1);
    let scale = (f64::from(max_side.max(1)) / f64::from(longest)).min(1.0);
    let width = ((f64::from(image.width) * scale).round() as u32).max(1);
    let height = ((f64::from(image.height) * scale).round() as u32).max(1);

    let image = if (width, height) == (image.width, image.height) {
        image
    } else {
        downscale(&image, width, height)
    };
    encode_rgba(&image)
}

/// Un'immagine RGBA a 8 bit, non premoltiplicata.
#[derive(Debug, Clone)]
struct Rgba {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn invalid(reason: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidArchive(format!("invalid image: {reason}"))
}

fn decode_rgba(bytes: &[u8]) -> CoreResult<Rgba> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(invalid)?;

    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    if width == 0 || height == 0 || width > MAX_INPUT_SIDE || height > MAX_INPUT_SIDE {
        return Err(invalid(format!("{width}×{height} is not an icon")));
    }

    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buffer).map_err(invalid)?;
    let data = &buffer[..frame.buffer_size()];
    let count = (width * height) as usize;

    let pixels = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .flat_map(|px| [px[0], px[1], px[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .flat_map(|px| [px[0], px[0], px[0], px[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        png::ColorType::Indexed => return Err(invalid("palette not expanded")),
    };
    if pixels.len() != count * 4 {
        return Err(invalid("unexpected pixel layout"));
    }

    Ok(Rgba {
        width,
        height,
        pixels,
    })
}

/// Il rettangolo più piccolo che contiene tutti i pixel visibili. Un'immagine
/// del tutto trasparente resta com'è.
fn trim_transparent(image: Rgba) -> Rgba {
    let (width, height) = (image.width as usize, image.height as usize);
    let mut bounds: Option<(usize, usize, usize, usize)> = None;

    for y in 0..height {
        for x in 0..width {
            if image.pixels[(y * width + x) * 4 + 3] > ALPHA_THRESHOLD {
                bounds = Some(match bounds {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                });
            }
        }
    }

    let Some((x0, y0, x1, y1)) = bounds else {
        return image;
    };
    let (trimmed_width, trimmed_height) = (x1 - x0 + 1, y1 - y0 + 1);
    if (trimmed_width, trimmed_height) == (width, height) {
        return image;
    }

    let mut pixels = Vec::with_capacity(trimmed_width * trimmed_height * 4);
    for y in y0..=y1 {
        let start = (y * width + x0) * 4;
        pixels.extend_from_slice(&image.pixels[start..start + trimmed_width * 4]);
    }
    Rgba {
        width: trimmed_width as u32,
        height: trimmed_height as u32,
        pixels,
    }
}

/// Riduzione a media d'area, in alfa premoltiplicato: senza, i bordi
/// semitrasparenti prenderebbero il colore dei pixel invisibili (nero) e
/// il simbolo avrebbe un contorno scuro.
fn downscale(image: &Rgba, width: u32, height: u32) -> Rgba {
    let (src_width, src_height) = (image.width as usize, image.height as usize);
    let (width, height) = (width as usize, height as usize);
    let mut pixels = vec![0u8; width * height * 4];

    for dy in 0..height {
        let y0 = dy * src_height / height;
        let y1 = ((dy + 1) * src_height / height).max(y0 + 1);
        for dx in 0..width {
            let x0 = dx * src_width / width;
            let x1 = ((dx + 1) * src_width / width).max(x0 + 1);

            let mut sum = [0u64; 4];
            for y in y0..y1 {
                for x in x0..x1 {
                    let px = &image.pixels[(y * src_width + x) * 4..][..4];
                    let alpha = u64::from(px[3]);
                    sum[0] += u64::from(px[0]) * alpha;
                    sum[1] += u64::from(px[1]) * alpha;
                    sum[2] += u64::from(px[2]) * alpha;
                    sum[3] += alpha;
                }
            }

            let samples = ((y1 - y0) * (x1 - x0)) as u64;
            let out = &mut pixels[(dy * width + dx) * 4..][..4];
            for channel in 0..3 {
                if let Some(value) = (sum[channel] + sum[3] / 2).checked_div(sum[3]) {
                    out[channel] = value.min(255) as u8;
                }
            }
            out[3] = ((sum[3] + samples / 2) / samples).min(255) as u8;
        }
    }

    Rgba {
        width: width as u32,
        height: height as u32,
        pixels,
    }
}

fn encode_rgba(image: &Rgba) -> CoreResult<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(invalid)?;
        writer.write_image_data(&image.pixels).map_err(invalid)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un rettangolo opaco di `inner` pixel in mezzo a un bordo trasparente.
    fn framed(width: u32, height: u32, inner: (u32, u32, u32, u32)) -> Vec<u8> {
        let (x0, y0, x1, y1) = inner;
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let inside = x >= x0 && x < x1 && y >= y0 && y < y1;
                pixels.extend_from_slice(if inside {
                    &[0, 200, 255, 255]
                } else {
                    &[0, 0, 0, 0]
                });
            }
        }
        encode_rgba(&Rgba {
            width,
            height,
            pixels,
        })
        .expect("png")
    }

    #[test]
    fn the_transparent_frame_is_cut_away_and_the_symbol_fills_the_icon() {
        // 200×120 con un disegno 60×40 al centro: come i rank del sito.
        let source = framed(200, 120, (70, 40, 130, 80));
        let thumb = decode_rgba(&badge_thumbnail(&source, 30).expect("miniatura")).expect("png");

        assert_eq!((thumb.width, thumb.height), (30, 20), "3:2 come il disegno");
        assert!(
            thumb.pixels.chunks_exact(4).all(|px| px[3] == 255),
            "niente bordo trasparente rimasto"
        );
    }

    #[test]
    fn a_small_image_is_not_enlarged() {
        let source = framed(20, 20, (0, 0, 20, 20));
        let thumb = decode_rgba(&badge_thumbnail(&source, 96).expect("miniatura")).expect("png");
        assert_eq!((thumb.width, thumb.height), (20, 20));
    }

    /// I bordi semitrasparenti tengono il colore del disegno, non il nero
    /// dei pixel invisibili accanto.
    #[test]
    fn edges_keep_the_colour_of_the_symbol() {
        let source = framed(4, 2, (0, 0, 2, 2)); // metà opaca, metà trasparente
        let image = decode_rgba(&source).expect("png");
        let half = downscale(&image, 1, 1);
        assert_eq!(&half.pixels[..3], &[0, 200, 255]);
        assert_eq!(half.pixels[3], 128);
    }

    #[test]
    fn what_is_not_a_png_is_refused() {
        assert!(badge_thumbnail(b"<html>404</html>", 64).is_err());
        assert!(badge_thumbnail(&[], 64).is_err());
    }
}
