//! All public Discord image URLs live here. Referenced assets are served by
//! GitHub; the client itself does not fetch images.
use crate::model::{PrintState, PrinterKind};

pub const ASSET_BASE_URL: &str =
    "https://raw.githubusercontent.com/badblli/3d-printing-discord-presence/main/assets";
pub const APP_LOGO_URL: &str =
    "https://raw.githubusercontent.com/badblli/3d-printing-discord-presence/main/assets/brand/discord-app-icon.png";
const BAMBU_BASE_URL: &str =
    "https://raw.githubusercontent.com/bambulab/BambuStudio/master/resources/images";

pub fn large_image(kind: PrinterKind, model: &str) -> String {
    if kind != PrinterKind::Bambu {
        return APP_LOGO_URL.into();
    }
    let model = model.to_ascii_lowercase();
    let name = model
        .trim()
        .strip_prefix("bambu lab ")
        .unwrap_or(model.trim());
    let file = match name {
        "a1" | "a1 combo" => "printer_preview_N2S.png",
        "a1 mini" | "a1 mini combo" => "printer_preview_N1.png",
        "p1p" => "printer_preview_C11.png",
        "p1s" | "p1s combo" => "printer_preview_C12.png",
        "x1 carbon" | "x1c" | "x1 carbon combo" => "printer_preview_BL-P001.png",
        "x1" => "printer_preview_BL-P002.png",
        "x1e" => "printer_preview_C13.png",
        "h2d" | "h2d combo" => "printer_preview_O1D.png",
        "h2s" => "printer_preview_O1S.png",
        "p2s" => "printer_preview_N7.png",
        _ => return APP_LOGO_URL.into(),
    };
    // The filename is chosen from a closed catalog, never from user input.
    format!("{BAMBU_BASE_URL}/{file}")
}

pub fn small_image(state: PrintState) -> Option<String> {
    let file = match state {
        PrintState::Printing => "status-printing.png",
        PrintState::Preparing => "status-preparing.png",
        PrintState::Paused => "status-paused.png",
        _ => return None,
    };
    Some(format!("{ASSET_BASE_URL}/discord/{file}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_bambu_models_select_printer_photos() {
        for (model, file) in [
            ("Bambu Lab A1", "N2S"),
            ("A1 Combo", "N2S"),
            ("Bambu Lab A1 mini", "N1"),
            ("P1P", "C11"),
            ("P1S", "C12"),
            ("X1 Carbon", "BL-P001"),
            ("X1", "BL-P002"),
            ("X1E", "C13"),
            ("H2D", "O1D"),
            ("H2S", "O1S"),
            ("P2S", "N7"),
        ] {
            assert_eq!(
                large_image(PrinterKind::Bambu, model),
                format!("{BAMBU_BASE_URL}/printer_preview_{file}.png")
            );
        }
    }

    #[test]
    fn unknown_model_and_other_brands_use_app_logo() {
        assert_eq!(large_image(PrinterKind::Bambu, "Bambu Lab"), APP_LOGO_URL);
        for kind in [
            PrinterKind::Moonraker,
            PrinterKind::OctoPrint,
            PrinterKind::PrusaLink,
        ] {
            assert_eq!(large_image(kind, "Bambu Lab A1"), APP_LOGO_URL);
        }
        assert!(APP_LOGO_URL.starts_with("https://"));
    }

    #[test]
    fn badges_use_shared_asset_base() {
        for (state, file) in [
            (PrintState::Printing, "printing"),
            (PrintState::Preparing, "preparing"),
            (PrintState::Paused, "paused"),
        ] {
            assert_eq!(
                small_image(state),
                Some(format!("{ASSET_BASE_URL}/discord/status-{file}.png"))
            );
        }
        assert_eq!(small_image(PrintState::Idle), None);
    }
}
