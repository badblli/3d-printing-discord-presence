//! Pure conversion; no IPC, clock reads, logging or printer access.
use super::{images, template};
use crate::{
    config::model::Button,
    model::{PrintState, PrintStatus},
};
use serde::Serialize;

#[derive(Clone, PartialEq, Eq, Serialize, Debug)]
pub struct Activity {
    pub details: String,
    pub state: String,
    pub assets: Assets,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<Button>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<Timestamps>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Debug)]
pub struct Assets {
    pub large_image: String,
    pub large_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_text: Option<String>,
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
    pub details_template: Option<String>,
    pub state_template: Option<String>,
    pub show_status_badge: bool,
    pub buttons: Vec<Button>,
}

impl Default for PresenceOptions {
    fn default() -> Self {
        Self {
            show_file_name: true,
            lang: "tr".into(),
            details_template: None,
            state_template: None,
            show_status_badge: true,
            buttons: Vec::new(),
        }
    }
}

pub fn activity_for(
    status: &PrintStatus,
    opts: &PresenceOptions,
    now_unix: u64,
) -> Option<Activity> {
    if !status.state.is_active() {
        return None;
    }
    let english = template::is_english(&opts.lang);
    let fallback = template::fallback(english);
    let model = template::model_name(status, english);
    let label = template::status_label(status.state, english);
    let details = template::render(
        opts.details_template
            .as_deref()
            .unwrap_or(template::DEFAULT_DETAILS),
        status,
        opts,
        fallback,
    );
    let state = template::render(
        opts.state_template.as_deref().unwrap_or(if english {
            template::DEFAULT_STATE_EN
        } else {
            template::DEFAULT_STATE_TR
        }),
        status,
        opts,
        fallback,
    );
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
        buttons: opts.buttons.clone(),
        timestamps,
        assets: Assets {
            large_image: images::large_image(status.kind, &model),
            large_text: model,
            small_image: opts
                .show_status_badge
                .then(|| images::small_image(status.state))
                .flatten(),
            small_text: opts.show_status_badge.then(|| label.to_owned()),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PrinterKind;

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
                    ..PresenceOptions::default()
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
    fn each_brand_uses_model_photo_or_app_logo() {
        for kind in [
            PrinterKind::Bambu,
            PrinterKind::Moonraker,
            PrinterKind::OctoPrint,
            PrinterKind::PrusaLink,
        ] {
            let a = activity(&PrintStatus { kind, ..status() });
            assert_eq!(a.assets.large_image, images::APP_LOGO_URL);
            assert_eq!(a.assets.large_text, "Test Printer");
        }
        let a = activity(&PrintStatus {
            printer_model: Some("Bambu Lab A1".into()),
            ..status()
        });
        assert!(a.assets.large_image.ends_with("/printer_preview_N2S.png"));
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

    #[test]
    fn badge_and_buttons_are_optional_in_ipc_payload() {
        let mut opts = PresenceOptions {
            show_status_badge: false,
            ..PresenceOptions::default()
        };
        let without = serde_json::to_value(activity_for(&status(), &opts, 1000).unwrap()).unwrap();
        assert!(without["assets"].get("small_image").is_none());
        assert!(without["assets"].get("small_text").is_none());
        assert!(without.get("buttons").is_none());
        opts.show_status_badge = true;
        opts.buttons = vec![Button {
            label: "Fake model".into(),
            url: "https://example.com/model".into(),
        }];
        let with = serde_json::to_value(activity_for(&status(), &opts, 1000).unwrap()).unwrap();
        assert_eq!(with["assets"]["small_text"], "Basılıyor");
        assert!(with["assets"]["small_image"]
            .as_str()
            .unwrap()
            .ends_with("/status-printing.png"));
        assert_eq!(with["buttons"][0]["label"], "Fake model");
    }
}
