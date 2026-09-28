//! Render con la risorsa vera.
//!
//! `FFLResHigh.dat` non sta nel repository (è un asset di Nintendo che il
//! launcher scarica su richiesta, §D-011), quindi questi test girano solo se
//! `VK_MII_RESOURCE` indica dove trovarlo:
//!
//! ```text
//! VK_MII_RESOURCE=%APPDATA%\VanzaKart\Launcher\mii-runtime\FFLResHigh.dat cargo test -p vk-mii-render
//! ```

use vk_mii_render::{MiiRenderer, RenderRequest, View};
use vk_save::mii::{self, MiiEditorState, LIMITS};

const DATABASE: &[u8] = include_bytes!("../../vk-save/fixtures/RFL_DB.dat");

fn renderer() -> Option<MiiRenderer> {
    let path = std::env::var_os("VK_MII_RESOURCE")?;
    let bytes = std::fs::read(path).expect("VK_MII_RESOURCE non leggibile");
    Some(MiiRenderer::new(bytes).expect("risorsa non valida"))
}

/// Pixel non trasparenti: un Mii vero ne ha tanti, un render vuoto nessuno.
fn opaque(image: &vk_mii_render::Image) -> usize {
    image
        .rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 0)
        .count()
}

#[test]
fn every_mii_of_the_fixture_renders() {
    let Some(renderer) = renderer() else {
        eprintln!("VK_MII_RESOURCE non impostata: test saltato");
        return;
    };

    for (id, parsed) in vk_save::miidb::read(DATABASE) {
        let studio = mii::studio_data(&parsed.raw);
        for view in [View::Face, View::AllBody] {
            let image = renderer
                .render_hex(
                    &studio,
                    &RenderRequest {
                        size: 128,
                        view,
                        ..RenderRequest::default()
                    },
                )
                .unwrap_or_else(|error| panic!("Mii {id:08x}: {error}"));
            assert!(opaque(&image) > 128 * 128 / 20, "Mii {id:08x} quasi vuoto");
        }
    }
}

/// Ogni valore che l'editor può proporre produce un'immagine.
///
/// È la garanzia che nessuna scelta nella griglia o sui cursori faccia
/// sparire l'anteprima: i limiti di `vk_save::mii::LIMITS` sono quelli che
/// il renderer sa disegnare.
#[test]
fn every_value_within_the_limits_renders() {
    let Some(renderer) = renderer() else {
        eprintln!("VK_MII_RESOURCE non impostata: test saltato");
        return;
    };

    // Neo e occhiali accesi, così i loro cursori cambiano davvero qualcosa.
    let base = MiiEditorState {
        mole_enabled: true,
        glasses_type: 1,
        mustache_type: 1,
        ..MiiEditorState::default()
    };

    for limit in LIMITS {
        for value in limit.min..=limit.max {
            let mut json = serde_json::to_value(&base).unwrap();
            json[limit.field] = serde_json::json!(value);
            let state: MiiEditorState = serde_json::from_value(json).unwrap();

            let studio = mii::studio_data(&mii::write_editor_state(&state));
            let image = renderer
                .render_hex(
                    &studio,
                    &RenderRequest {
                        size: 64,
                        ..RenderRequest::default()
                    },
                )
                .unwrap_or_else(|error| panic!("{} = {value}: {error}", limit.field));
            assert!(
                opaque(&image) > 0,
                "{} = {value}: immagine vuota",
                limit.field
            );
        }
    }
}

#[test]
fn the_same_mii_renders_the_same_pixels_twice() {
    let Some(renderer) = renderer() else {
        eprintln!("VK_MII_RESOURCE non impostata: test saltato");
        return;
    };

    let studio = mii::studio_data(&mii::write_editor_state(&MiiEditorState::default()));
    let request = RenderRequest {
        size: 256,
        character_rotation: [0.0, 30.0, 0.0],
        ..RenderRequest::default()
    };

    // La seconda volta la testa arriva dalla cache: il risultato non cambia.
    let first = renderer.render_hex(&studio, &request).unwrap();
    let second = renderer.render_hex(&studio, &request).unwrap();
    assert_eq!(first.rgba, second.rgba);

    let png = first.to_png().unwrap();
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
}
