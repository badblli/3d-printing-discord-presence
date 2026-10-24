//! Best-effort local identification; never expose the serial to presence or logs.
//! Prefix reference: synman/bambu-printer-manager, src/bpm/bambutools.py,
//! getPrinterModelBySerial. Configured models always take precedence.
pub fn resolve<'a>(configured: Option<&'a str>, serial: &str) -> &'a str {
    if let Some(model) = configured.map(str::trim).filter(|model| !model.is_empty()) {
        return model;
    }
    match serial.get(..3) {
        Some("039") => "Bambu Lab A1",
        Some("030") => "Bambu Lab A1 mini",
        Some("01S") => "Bambu Lab P1P",
        Some("01P") => "Bambu Lab P1S",
        Some("00M") => "Bambu Lab X1 Carbon",
        Some("00W") => "Bambu Lab X1",
        Some("03W") => "Bambu Lab X1E",
        Some("094") => "Bambu Lab H2D",
        Some("093") => "Bambu Lab H2S",
        Some("22E") => "Bambu Lab P2S",
        _ => "Bambu Lab",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_prefixes_use_only_fake_serials() {
        for (prefix, model) in [
            ("039", "A1"),
            ("030", "A1 mini"),
            ("01S", "P1P"),
            ("01P", "P1S"),
            ("00M", "X1 Carbon"),
            ("00W", "X1"),
            ("03W", "X1E"),
            ("094", "H2D"),
            ("093", "H2S"),
            ("22E", "P2S"),
        ] {
            assert_eq!(
                resolve(None, &format!("{prefix}00A000000000")),
                format!("Bambu Lab {model}")
            );
        }
    }

    #[test]
    fn explicit_model_wins_and_blank_model_is_detected() {
        assert_eq!(resolve(Some(" A1 Combo "), "00M00A000000000"), "A1 Combo");
        assert_eq!(resolve(Some("  "), "03900A000000000"), "Bambu Lab A1");
        for serial in ["", "03", "🖨", "XYZ00A000000000"] {
            assert_eq!(resolve(None, serial), "Bambu Lab");
        }
    }
}
