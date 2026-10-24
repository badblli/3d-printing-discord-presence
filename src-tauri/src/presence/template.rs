//! Pure, single-pass substitution. Telemetry values are never parsed as templates.
use super::PresenceOptions;
use crate::model::{PrintState, PrintStatus};

pub const DEFAULT_DETAILS: &str = "{file}";
pub const DEFAULT_STATE_TR: &str = "{model} · {status} · %{progress}";
pub const DEFAULT_STATE_EN: &str = "{model} · {status} · {progress}%";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTemplate;

enum Token<'a> {
    Literal(&'a str),
    Placeholder(&'a str),
}

fn parse(mut text: &str) -> Result<Vec<Token<'_>>, InvalidTemplate> {
    let mut tokens = Vec::new();
    while let Some(index) = text.find(['{', '}']) {
        tokens.push(Token::Literal(&text[..index]));
        text = &text[index..];
        if text.starts_with('}') {
            return Err(InvalidTemplate);
        }
        let end = text.find('}').ok_or(InvalidTemplate)?;
        let name = &text[1..end];
        if !matches!(
            name,
            "file"
                | "model"
                | "status"
                | "progress"
                | "layer"
                | "total_layers"
                | "nozzle"
                | "bed"
                | "remaining"
        ) {
            return Err(InvalidTemplate);
        }
        tokens.push(Token::Placeholder(name));
        text = &text[end + 1..];
    }
    tokens.push(Token::Literal(text));
    Ok(tokens)
}

pub fn validate(template: &str) -> Result<(), InvalidTemplate> {
    parse(template).map(|_| ())
}

pub fn is_english(lang: &str) -> bool {
    lang.split(['-', '_'])
        .next()
        .is_some_and(|s| s.eq_ignore_ascii_case("en"))
}

pub fn fallback(english: bool) -> &'static str {
    if english {
        "3D printing"
    } else {
        "3D baskı alıyorum"
    }
}

pub fn status_label(state: PrintState, english: bool) -> &'static str {
    match (state, english) {
        (PrintState::Preparing, false) => "Hazırlanıyor",
        (PrintState::Printing, false) => "Basılıyor",
        (PrintState::Paused, false) => "Duraklatıldı",
        (PrintState::Preparing, true) => "Preparing",
        (PrintState::Printing, true) => "Printing",
        (PrintState::Paused, true) => "Paused",
        _ => "",
    }
}

pub fn model_name(status: &PrintStatus, english: bool) -> String {
    status
        .printer_model
        .as_deref()
        .map(str::trim)
        .filter(|s| s.chars().count() >= 2)
        .unwrap_or(if english { "3D printer" } else { "3D yazıcı" })
        .chars()
        .take(80)
        .collect()
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

fn temperature(value: Option<f32>) -> Option<String> {
    value.filter(|n| n.is_finite()).map(|n| format!("{n:.0}°C"))
}

fn remaining(secs: u32, english: bool) -> String {
    let minutes = secs.div_ceil(60);
    let hours = minutes / 60;
    let minutes = minutes % 60;
    match (english, hours) {
        (true, 0) => format!("{minutes}m"),
        (false, 0) => format!("{minutes} dk"),
        (true, _) => format!("{hours}h {minutes}m"),
        (false, _) => format!("{hours} sa {minutes} dk"),
    }
}

fn value(name: &str, status: &PrintStatus, opts: &PresenceOptions) -> Option<String> {
    let english = is_english(&opts.lang);
    match name {
        "file" => Some(if opts.show_file_name {
            status
                .file_name
                .as_deref()
                .map(clean_file_name)
                .filter(|s| s.chars().count() >= 2)
                .unwrap_or_else(|| fallback(english).into())
        } else {
            fallback(english).into()
        }),
        "model" => Some(model_name(status, english)),
        "status" => Some(status_label(status.state, english).into()),
        "progress" => status.progress.filter(|p| *p <= 100).map(|p| {
            if english {
                format!("{p}%")
            } else {
                format!("%{p}")
            }
        }),
        "layer" => status.layer.map(|n| n.to_string()),
        "total_layers" => status.total_layers.map(|n| n.to_string()),
        "nozzle" => temperature(status.nozzle_temp),
        "bed" => temperature(status.bed_temp),
        "remaining" => status.remaining_secs.map(|secs| remaining(secs, english)),
        _ => None,
    }
}

/// A missing value drops its entire ` · ` segment. Empty/one-character results
/// use a localized fallback; long results are truncated at a Unicode boundary.
/// Legacy percent wrappers are accepted, with exactly one localized percent sign.
pub fn render(
    template: &str,
    status: &PrintStatus,
    opts: &PresenceOptions,
    default: &str,
) -> String {
    if validate(template).is_err() {
        return bounded(default, default);
    }
    let normalized = template
        .replace("%{progress}", "{progress}")
        .replace("{progress}%", "{progress}");
    let segments: Vec<String> = normalized
        .split(" · ")
        .filter_map(|segment| {
            let mut output = String::new();
            for token in parse(segment).ok()? {
                match token {
                    Token::Literal(text) => output.push_str(text),
                    Token::Placeholder(name) => output.push_str(&value(name, status, opts)?),
                }
            }
            let output = output.trim();
            (!output.is_empty()).then(|| output.to_owned())
        })
        .collect();
    bounded(&segments.join(" · "), default)
}

fn bounded(text: &str, default: &str) -> String {
    let text = text.trim();
    let text = if text.chars().count() >= 2 {
        text
    } else {
        default
    };
    text.chars().take(128).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PrinterKind;

    fn status() -> PrintStatus {
        PrintStatus {
            kind: PrinterKind::Bambu,
            state: PrintState::Printing,
            progress: Some(42),
            remaining_secs: Some(4800),
            file_name: Some("/fake/example.gcode.3mf".into()),
            printer_model: Some("Bambu Lab A1".into()),
            layer: Some(12),
            total_layers: Some(200),
            nozzle_temp: Some(215.0),
            bed_temp: Some(60.0),
            observed_at_ms: 1_000_000,
        }
    }

    fn render_in(text: &str, lang: &str, status: &PrintStatus) -> String {
        render(
            text,
            status,
            &PresenceOptions {
                lang: lang.into(),
                ..Default::default()
            },
            fallback(is_english(lang)),
        )
    }

    #[test]
    fn defaults_are_fixed_for_both_languages() {
        assert_eq!(render_in(DEFAULT_DETAILS, "tr", &status()), "example");
        assert_eq!(render_in(DEFAULT_DETAILS, "en", &status()), "example");
        assert_eq!(
            render_in(DEFAULT_STATE_TR, "tr", &status()),
            "Bambu Lab A1 · Basılıyor · %42"
        );
        assert_eq!(
            render_in(DEFAULT_STATE_EN, "en", &status()),
            "Bambu Lab A1 · Printing · 42%"
        );
    }

    #[test]
    fn every_placeholder_and_percent_wrapper_is_localized() {
        let text = "{file} · {model} · {status} · {progress} · {layer}/{total_layers} · {nozzle}/{bed} · {remaining}";
        assert_eq!(
            render_in(text, "tr", &status()),
            "example · Bambu Lab A1 · Basılıyor · %42 · 12/200 · 215°C/60°C · 1 sa 20 dk"
        );
        assert_eq!(
            render_in(text, "en-US", &status()),
            "example · Bambu Lab A1 · Printing · 42% · 12/200 · 215°C/60°C · 1h 20m"
        );
        for text in ["{progress}", "%{progress}", "{progress}%"] {
            assert_eq!(render_in(text, "tr", &status()), "%42");
            assert_eq!(render_in(text, "en", &status()), "42%");
        }
    }

    #[test]
    fn missing_values_drop_whole_segments() {
        let status = PrintStatus {
            layer: None,
            progress: None,
            remaining_secs: None,
            nozzle_temp: None,
            bed_temp: None,
            total_layers: None,
            ..status()
        };
        assert_eq!(render_in("{model} · Layer {layer}/{total_layers} · {progress}% · {nozzle} · {bed} · {remaining} · OK", "en", &status), "Bambu Lab A1 · OK");
        assert_eq!(render_in("{layer}", "en", &status), "3D printing");
        for temp in [f32::NAN, f32::INFINITY] {
            assert_eq!(
                render_in(
                    "{nozzle} · OK",
                    "en",
                    &PrintStatus {
                        nozzle_temp: Some(temp),
                        ..status.clone()
                    }
                ),
                "OK"
            );
        }
    }

    #[test]
    fn output_limits_count_unicode_characters_and_use_fallback() {
        for text in ["", " ", "a", "🖨", "{layer}"] {
            assert_eq!(
                render_in(
                    text,
                    "tr",
                    &PrintStatus {
                        layer: None,
                        ..status()
                    }
                ),
                "3D baskı alıyorum"
            );
        }
        assert_eq!(render_in("OK", "en", &status()), "OK");
        for len in [127, 128, 129, 200] {
            assert_eq!(
                render_in(&"ş🖨".repeat(len), "en", &status())
                    .chars()
                    .count(),
                128
            );
        }
        assert_eq!(
            render_in(&"ş".repeat(127), "en", &status()).chars().count(),
            127
        );
    }

    #[test]
    fn malformed_templates_are_rejected_without_echoing_input() {
        for text in [
            "{",
            "{file",
            "{unknown}",
            "{{file}}",
            "{file}}",
            "}",
            "{}",
            "{file{model}",
        ] {
            assert_eq!(validate(text), Err(InvalidTemplate));
            assert_eq!(render_in(text, "en", &status()), "3D printing");
        }
    }

    #[test]
    fn file_values_are_not_reparsed_and_privacy_applies_to_custom_templates() {
        let status = PrintStatus {
            file_name: Some("{nozzle}.gcode".into()),
            ..status()
        };
        assert_eq!(render_in("{file}", "en", &status), "{nozzle}");
        for (lang, expected) in [("tr", "3D baskı alıyorum"), ("en", "3D printing")] {
            let opts = PresenceOptions {
                show_file_name: false,
                lang: lang.into(),
                ..Default::default()
            };
            assert_eq!(
                render("{file} · {file}", &status, &opts, expected),
                format!("{expected} · {expected}")
            );
        }
    }

    #[test]
    fn short_durations_zero_progress_and_zero_temperatures_are_present() {
        let status = PrintStatus {
            remaining_secs: Some(0),
            progress: Some(0),
            nozzle_temp: Some(0.0),
            ..status()
        };
        assert_eq!(
            render_in("{progress} · {nozzle} · {remaining}", "en", &status),
            "0% · 0°C · 0m"
        );
        assert_eq!(render_in("{remaining}", "tr", &status), "0 dk");
        assert_eq!(
            render_in(
                "{remaining}",
                "en",
                &PrintStatus {
                    remaining_secs: Some(120),
                    ..status
                }
            ),
            "2m"
        );
    }

    #[test]
    fn unrelated_file_extensions_are_preserved() {
        assert_eq!(clean_file_name("example.stl"), "example.stl");
    }
}
