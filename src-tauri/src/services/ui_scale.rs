//! Scala dell'interfaccia secondo la dimensione della finestra (§D-108).
//!
//! La UI è disegnata per la finestra di default, 1320×840 punti. Massimizzata
//! su uno schermo grande — un 1440p, un 4K al 100% — resterebbe della stessa
//! dimensione in mezzo a tanto spazio vuoto. Lo zoom della webview la fa
//! crescere, ma meno della finestra: gli elementi diventano più grandi *e*
//! c'è più spazio per il layout, che è fluido.
//!
//! Si usa lo zoom della webview, non un fattore CSS: scala tutto insieme —
//! testo, bordi, tooltip, menu, canvas — e le coordinate del mouse restano
//! coerenti con quelle del layout, cosa che `zoom` in CSS non garantisce.

/// La finestra per cui la UI è disegnata, in punti logici.
const DESIGN_WIDTH: f64 = 1320.0;
const DESIGN_HEIGHT: f64 = 840.0;

/// Quanto della crescita della finestra passa agli elementi: il resto resta
/// al layout. Con 0.75 un 1440p massimizzato ingrandisce del 50% circa e
/// guadagna comunque un terzo di spazio in larghezza.
const GROWTH: f64 = 0.75;

/// Oltre questo il testo è già enorme anche su un 4K al 100%.
const MAX_ZOOM: f64 = 2.25;

/// Zoom della webview per una finestra di `width`×`height` punti logici.
///
/// Mai sotto 1: in una finestra più piccola di quella di default la UI ha già
/// il suo layout stretto, e rimpicciolire il testo lo renderebbe illeggibile.
/// Conta il lato più corto rispetto al disegno, così un monitor ultrawide non
/// gonfia gli elementi fino a farli uscire in altezza. Arrotondato al
/// centesimo: trascinando il bordo non si ricalcola il layout a ogni pixel.
pub fn zoom_for(width: f64, height: f64) -> f64 {
    if !(width.is_finite() && height.is_finite()) || width <= 0.0 || height <= 0.0 {
        return 1.0;
    }

    let ratio = (width / DESIGN_WIDTH).min(height / DESIGN_HEIGHT);
    let zoom = (1.0 + (ratio - 1.0) * GROWTH).clamp(1.0, MAX_ZOOM);
    (zoom * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_window_and_smaller_ones_are_not_zoomed() {
        assert_eq!(zoom_for(1320.0, 840.0), 1.0);
        // La minima consentita e un portatile 1366×768 massimizzato.
        assert_eq!(zoom_for(1100.0, 720.0), 1.0);
        assert_eq!(zoom_for(1366.0, 728.0), 1.0);
    }

    #[test]
    fn a_maximized_window_grows_with_the_screen() {
        // Area di lavoro, in punti logici, con la barra delle applicazioni.
        let full_hd = zoom_for(1920.0, 1032.0);
        let qhd = zoom_for(2560.0, 1392.0);
        let uhd_100 = zoom_for(3840.0, 2112.0);

        assert!((1.15..1.2).contains(&full_hd), "{full_hd}");
        assert!((1.45..1.55).contains(&qhd), "{qhd}");
        assert!((2.0..=MAX_ZOOM).contains(&uhd_100), "{uhd_100}");
        assert!(full_hd < qhd && qhd < uhd_100);

        // Un 4K al 150% ha gli stessi punti logici di un 1440p al 100%.
        assert_eq!(zoom_for(2560.0, 1392.0), qhd);
    }

    #[test]
    fn the_layout_keeps_more_room_than_the_design() {
        // Gli elementi crescono meno della finestra: in punti CSS la pagina
        // resta più larga e più alta di quella di default.
        for (width, height) in [(1920.0, 1032.0), (2560.0, 1392.0), (3840.0, 2112.0)] {
            let zoom = zoom_for(width, height);
            assert!(width / zoom > DESIGN_WIDTH, "{width}×{height}");
            assert!(height / zoom >= DESIGN_HEIGHT, "{width}×{height}");
        }
    }

    #[test]
    fn an_ultrawide_screen_is_limited_by_its_height() {
        assert_eq!(zoom_for(3440.0, 1392.0), zoom_for(2560.0, 1392.0));
    }

    #[test]
    fn nonsense_sizes_leave_the_zoom_alone() {
        assert_eq!(zoom_for(0.0, 840.0), 1.0);
        assert_eq!(zoom_for(f64::NAN, 840.0), 1.0);
        assert_eq!(zoom_for(1320.0, f64::INFINITY), 1.0);
        assert_eq!(zoom_for(1e9, 1e9), MAX_ZOOM);
    }
}
