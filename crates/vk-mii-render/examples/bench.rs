//! Tempi di un render, separati per fase.
//!
//! ```text
//! cargo run -p vk-mii-render --release --example bench -- FFLResHigh.dat
//! ```

use std::time::Instant;

use vk_mii_render::{MiiRenderer, RenderRequest, View};
use vk_save::mii::{self, MiiEditorState};

fn main() {
    let path = std::env::args().nth(1).expect("percorso di FFLResHigh.dat");
    let renderer = MiiRenderer::new(std::fs::read(path).unwrap()).unwrap();

    for (size, view) in [(512, View::Face), (256, View::Face), (512, View::AllBody)] {
        let mut fresh = 0.0;
        let mut cached = 0.0;
        let mut png = 0.0;
        let mut base64 = 0.0;
        let rounds = 12;

        for round in 0..rounds {
            let state = MiiEditorState {
                hair_type: (round * 5) as u8,
                eye_type: round as u8,
                ..MiiEditorState::default()
            };
            let studio = mii::studio_data(&mii::write_editor_state(&state));
            let request = RenderRequest {
                size,
                view,
                ..RenderRequest::default()
            };

            let start = Instant::now();
            renderer.render_hex(&studio, &request).unwrap();
            fresh += start.elapsed().as_secs_f64();

            let start = Instant::now();
            let image = renderer.render_hex(&studio, &request).unwrap();
            cached += start.elapsed().as_secs_f64();

            let start = Instant::now();
            let bytes = image.to_png().unwrap();
            png += start.elapsed().as_secs_f64();

            let start = Instant::now();
            let _ = mii::base64_encode(&bytes);
            base64 += start.elapsed().as_secs_f64();
        }

        let ms = |total: f64| total * 1000.0 / rounds as f64;
        println!(
            "{view:?} {size}: testa nuova {:.1} ms, testa in cache {:.1} ms, png {:.1} ms, base64 {:.1} ms",
            ms(fresh),
            ms(cached),
            ms(png),
            ms(base64)
        );
    }
}
