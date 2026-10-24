//! Pure conversion; no IPC, clock reads, logging or printer access.
use crate::model::{PrintState, PrintStatus, PrinterKind};
use serde::Serialize;

#[derive(Clone, PartialEq, Eq, Serialize, Debug)]
pub struct Activity {
    pub details: String,
    pub state: String,
    pub assets: Assets,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<Timestamps>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Debug)]
pub struct Assets {
    pub large_image: String,
    pub large_text: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Debug)]
pub struct Timestamps {
    pub end: u64,
}

#[derive(Clone, Debug)]
pub struct PresenceOptions {
    pub show_file_name: bool,
    /// `en` (including regional variants); other values fall back to Turkish.
    pub lang: String,
}

impl Default for PresenceOptions {
    fn default() -> Self {
        Self {
            show_file_name: true,
            lang: "tr".into(),
        }
    }
}

pub fn activity_for(
    status: &PrintStatus,
    opts: &PresenceOptions,
    now_unix: u64,
) -> Option<Activity> {
    let english = opts
        .lang
        .split(['-', '_'])
        .next()
        .is_some_and(|s| s.eq_ignore_ascii_case("en"));
    let (fallback, printer, label) = match (english, status.state) {
        (false, PrintState::Preparing) => ("3D baskı alıyorum", "3D yazıcı", "Hazırlanıyor"),
        (false, PrintState::Printing) => ("3D baskı alıyorum", "3D yazıcı", "Basılıyor"),
        (false, PrintState::Paused) => ("3D baskı alıyorum", "3D yazıcı", "Duraklatıldı"),
        (true, PrintState::Preparing) => ("3D printing", "3D printer", "Preparing"),
        (true, PrintState::Printing) => ("3D printing", "3D printer", "Printing"),
        (true, PrintState::Paused) => ("3D printing", "3D printer", "Paused"),
        _ => return None,
    };
    let details = if opts.show_file_name {
        status
            .file_name
            .as_deref()
            .map(clean_file_name)
            .filter(|s| s.chars().count() >= 2)
    } else {
        None
    }
    .unwrap_or_else(|| fallback.into());
    let model: String = status
        .printer_model
        .as_deref()
        .map(str::trim)
        .filter(|s| s.chars().count() >= 2)
        .unwrap_or(printer)
        .chars()
        .take(80)
        .collect();
    let mut state = format!("{model} · {label}");
    if let Some(percent) = status.progress.filter(|percent| *percent <= 100) {
        // Turkish writes the percent sign first.
        if english {
            state.push_str(&format!(" · {percent}%"));
        } else {
            state.push_str(&format!(" · %{percent}"));
        }
    }
    // Use the observation time so repeated conversion of the same telemetry does
    // not move the deadline forward or defeat duplicate suppression.
    let timestamps = if status.state == PrintState::Printing {
        status
            .remaining_secs
            .filter(|secs| *secs > 0)
            .and_then(|secs| {
                let observed = if status.observed_at_ms == 0 {
                    now_unix
                } else {
                    status.observed_at_ms / 1000
                };
                let end = observed.saturating_add(u64::from(secs));
                (end > now_unix).then_some(Timestamps { end })
            })
    } else {
        None
    };
    Some(Activity {
        details,
        state,
        timestamps,
        assets: Assets {
            large_image: asset_key(status.kind).into(),
            large_text: model,
        },
    })
}

fn clean_file_name(name: &str) -> String {
    let base = name.trim().rsplit(['/', '\\']).next().unwrap_or("").trim();
    let lower = base.to_ascii_lowercase();
    let suffix_len = [".gcode.3mf", ".gcode", ".3mf"]
        .iter()
        .find(|suffix| lower.ends_with(**suffix))
        .map_or(0, |suffix| suffix.len());
    base[..base.len() - suffix_len]
        .trim()
        .chars()
        .take(100)
        .collect()
}

#[allow(unreachable_patterns)] // Reserve a generic asset for future printer kinds.
fn asset_key(kind: PrinterKind) -> &'static str {
    match kind {
        PrinterKind::Bambu => "bambu",
        PrinterKind::Moonraker => "klipper",
        PrinterKind::OctoPrint => "octoprint",
        PrinterKind::PrusaLink => "prusa",
        _ => "printer",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn status() -> PrintStatus {
        PrintStatus {
            kind: PrinterKind::Bambu,
            state: PrintState::Printing,
            progress: Some(42),
            remaining_secs: Some(120),
            file_name: Some("fake-test.gcode".into()),
            printer_model: Some("Test Printer".into()),
            layer: None,
            total_layers: None,
            nozzle_temp: None,
            bed_temp: None,
            observed_at_ms: 1_000_000,
        }
    }
    fn activity(status: &PrintStatus) -> Activity {
        activity_for(status, &PresenceOptions::default(), 1000).unwrap()
    }
    #[test]
    fn inactive_states_clear() {
        for state in [PrintState::Idle, PrintState::Finished, PrintState::Failed] {
            assert!(activity_for(
                &PrintStatus { state, ..status() },
                &PresenceOptions::default(),
                1000
            )
            .is_none());
        }
    }
    #[test]
    fn preparing_has_label_without_timestamp() {
        let a = activity(&PrintStatus {
            state: PrintState::Preparing,
            ..status()
        });
        assert_eq!(a.state, "Test Printer · Hazırlanıyor · %42");
        assert_eq!(a.timestamps, None);
    }
    #[test]
    fn printing_uses_timestamp_instead_of_time_text() {
        let a = activity(&status());
        assert_eq!(a.state, "Test Printer · Basılıyor · %42");
        assert_eq!(a.timestamps, Some(Timestamps { end: 1120 }));
    }
    #[test]
    fn paused_has_no_timestamp() {
        let a = activity(&PrintStatus {
            state: PrintState::Paused,
            ..status()
        });
        assert!(a.state.contains("Duraklatıldı"));
        assert_eq!(a.timestamps, None);
        assert!(serde_json::to_value(a).unwrap().get("timestamps").is_none());
    }
    #[test]
    fn windows_and_unix_paths_and_extensions_are_removed() {
        for name in [
            r"C:\fake\example.gcode.3mf",
            "/fake/example.3mf",
            "example.GCODE",
            " example.gCoDe.3Mf ",
        ] {
            assert_eq!(
                activity(&PrintStatus {
                    file_name: Some(name.into()),
                    ..status()
                })
                .details,
                "example"
            );
        }
    }
    #[test]
    fn unrelated_extensions_are_preserved() {
        assert_eq!(clean_file_name("example.stl"), "example.stl");
    }
    #[test]
    fn missing_short_and_empty_names_use_default() {
        for name in [
            None,
            Some(""),
            Some("a.gcode"),
            Some("/fake/"),
            Some(".3mf"),
        ] {
            assert_eq!(
                activity(&PrintStatus {
                    file_name: name.map(str::to_owned),
                    ..status()
                })
                .details,
                "3D baskı alıyorum"
            );
        }
    }
    #[test]
    fn unicode_names_are_truncated_by_character() {
        let a = activity(&PrintStatus {
            file_name: Some(format!("{}.gcode", "ş🖨".repeat(60))),
            ..status()
        });
        assert_eq!(a.details.chars().count(), 100);
        assert!(!a.details.ends_with(".gcode"));
    }
    #[test]
    fn hidden_name_is_localized() {
        for (lang, expected) in [("tr", "3D baskı alıyorum"), ("en-US", "3D printing")] {
            let a = activity_for(
                &status(),
                &PresenceOptions {
                    show_file_name: false,
                    lang: lang.into(),
                },
                1000,
            )
            .unwrap();
            assert_eq!(a.details, expected);
        }
    }
    #[test]
    fn english_labels_and_default_model() {
        for (state, label) in [
            (PrintState::Preparing, "Preparing"),
            (PrintState::Printing, "Printing"),
            (PrintState::Paused, "Paused"),
        ] {
            let a = activity_for(
                &PrintStatus {
                    state,
                    printer_model: None,
                    ..status()
                },
                &PresenceOptions {
                    lang: "en".into(),
                    ..PresenceOptions::default()
                },
                1000,
            )
            .unwrap();
            assert_eq!(a.state, format!("3D printer · {label} · 42%"));
        }
    }
    #[test]
    fn each_brand_uses_its_asset() {
        for (kind, key) in [
            (PrinterKind::Bambu, "bambu"),
            (PrinterKind::Moonraker, "klipper"),
            (PrinterKind::OctoPrint, "octoprint"),
            (PrinterKind::PrusaLink, "prusa"),
        ] {
            let a = activity(&PrintStatus { kind, ..status() });
            assert_eq!(a.assets.large_image, key);
            assert_eq!(a.assets.large_text, "Test Printer");
        }
    }
    #[test]
    fn missing_and_invalid_progress_is_omitted() {
        for progress in [None, Some(101), Some(255)] {
            assert_eq!(
                activity(&PrintStatus {
                    progress,
                    ..status()
                })
                .state,
                "Test Printer · Basılıyor"
            );
        }
    }
    #[test]
    fn timestamp_is_stable_as_observation_ages() {
        assert_eq!(
            activity_for(&status(), &PresenceOptions::default(), 1010),
            activity_for(&status(), &PresenceOptions::default(), 1020)
        );
    }
    #[test]
    fn absent_zero_or_expired_remaining_has_no_countdown() {
        for remaining_secs in [None, Some(0)] {
            assert_eq!(
                activity(&PrintStatus {
                    remaining_secs,
                    ..status()
                })
                .timestamps,
                None
            );
        }
        assert_eq!(
            activity_for(&status(), &PresenceOptions::default(), 1120)
                .unwrap()
                .timestamps,
            None
        );
    }
    #[test]
    fn unknown_observation_uses_supplied_clock() {
        assert_eq!(
            activity(&PrintStatus {
                observed_at_ms: 0,
                ..status()
            })
            .timestamps,
            Some(Timestamps { end: 1120 })
        );
    }
    #[test]
    fn state_transition_clears_then_resumes() {
        let opts = PresenceOptions::default();
        for state in [
            PrintState::Preparing,
            PrintState::Printing,
            PrintState::Paused,
            PrintState::Finished,
            PrintState::Printing,
        ] {
            assert_eq!(
                activity_for(&PrintStatus { state, ..status() }, &opts, 1000).is_some(),
                state.is_active()
            );
        }
    }
}
