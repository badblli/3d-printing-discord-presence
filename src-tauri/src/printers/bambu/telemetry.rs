//! Delta reports contain observations, never a complete replacement snapshot.
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::model::{PrintState, PrintStatus, PrinterKind};

pub const MAX_PAYLOAD: usize = 1024 * 1024;
const FIELDS: &[&str] = &[
    "subtask_id",
    "task_id",
    "subtask_name",
    "gcode_file",
    "gcode_state",
    "mc_percent",
    "mc_remaining_time",
    "layer_num",
    "total_layer_num",
    "nozzle_temper",
    "bed_temper",
];

#[derive(Default)]
pub struct Telemetry {
    data: Map<String, Value>,
    last_seen: Option<Instant>,
    remaining: Option<u32>,
    remaining_at: Option<Instant>,
}

fn numeric(value: &Value, maximum: f64) -> Option<f64> {
    let number = match value {
        Value::Number(value) => value.as_f64()?,
        Value::String(value) => value.trim().parse::<f64>().ok()?,
        Value::Bool(value) => u8::from(*value) as f64,
        _ => return None,
    };
    (number.is_finite() && (0.0..=maximum).contains(&number)).then_some(number)
}

fn number(value: &Value, maximum: u32) -> Option<u32> {
    numeric(value, maximum as f64).map(|value| value as u32)
}

impl Telemetry {
    /// Returns false for malformed, unrelated or oversized reports, without refreshing freshness.
    pub fn ingest(&mut self, payload: &[u8], now: Instant) -> bool {
        if payload.len() > MAX_PAYLOAD {
            return false;
        }
        let Ok(packet) = serde_json::from_slice::<Value>(payload) else {
            return false;
        };
        let Some(data) = packet.get("print").and_then(Value::as_object) else {
            return false;
        };
        if !FIELDS.iter().any(|field| data.contains_key(*field)) {
            return false;
        }

        let changed_job = ["subtask_id", "task_id", "subtask_name"].iter().any(|key| {
            match (self.data.get(*key), data.get(*key)) {
                (Some(old), Some(new)) => meaningful(old) && meaningful(new) && old != new,
                _ => false,
            }
        });
        let old_state = self
            .data
            .get("gcode_state")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let new_state = data.get("gcode_state").and_then(Value::as_str);
        if changed_job
            || (matches!(old_state.as_str(), "FINISH" | "FAILED" | "IDLE")
                && matches!(new_state, Some("RUNNING" | "PREPARE")))
        {
            *self = Self::default();
        } else if new_state == Some("PAUSE") && old_state == "RUNNING" {
            self.remaining = self.countdown(now);
            self.remaining_at = Some(now);
        } else if new_state == Some("RUNNING") && old_state == "PAUSE" {
            self.remaining_at = Some(now);
        }
        if let Some(value) = data.get("mc_remaining_time") {
            self.remaining = number(value, 100_000).map(|minutes| minutes * 60);
            self.remaining_at = Some(now);
        }
        for field in FIELDS {
            if let Some(value) = data.get(*field) {
                self.data.insert((*field).into(), value.clone());
            }
        }
        self.last_seen = Some(now);
        true
    }

    fn countdown(&self, now: Instant) -> Option<u32> {
        let remaining = self.remaining?;
        let elapsed = now
            .saturating_duration_since(self.remaining_at?)
            .as_secs()
            .min(u32::MAX as u64) as u32;
        Some(remaining.saturating_sub(elapsed))
    }

    /// `observed_at_ms` timestamps this normalized observation, including its adjusted countdown.
    pub fn status(
        &self,
        now: Instant,
        observed_at_ms: u64,
        model: Option<&str>,
    ) -> Option<PrintStatus> {
        if now.saturating_duration_since(self.last_seen?) > Duration::from_secs(90) {
            return None;
        }
        let state = match self.data.get("gcode_state")?.as_str()? {
            "RUNNING" => PrintState::Printing,
            "PREPARE" | "SLICING" => PrintState::Preparing,
            "PAUSE" => PrintState::Paused,
            "FINISH" => PrintState::Finished,
            "FAILED" => PrintState::Failed,
            "IDLE" => PrintState::Idle,
            _ => return None,
        };
        let field = |key: &str, max| self.data.get(key).and_then(|value| number(value, max));
        let temp = |key: &str, max| {
            self.data
                .get(key)
                .and_then(|value| numeric(value, max))
                .map(|v| v as f32)
        };
        let file_name = ["subtask_name", "gcode_file"]
            .iter()
            .filter_map(|key| self.data.get(*key).and_then(Value::as_str))
            .map(str::trim)
            .find(|name| !name.is_empty())
            .map(|name| {
                name.rsplit(['/', '\\'])
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(100)
                    .collect::<String>()
            })
            .filter(|name| !name.is_empty());
        Some(PrintStatus {
            kind: PrinterKind::Bambu,
            state,
            progress: field("mc_percent", 100).map(|v| v as u8),
            remaining_secs: (state == PrintState::Printing)
                .then(|| self.countdown(now))
                .flatten(),
            file_name,
            printer_model: model.map(str::to_owned),
            // Generous finite bounds also prevent integer overflow on corrupt packets.
            layer: field("layer_num", 1_000_000),
            total_layers: field("total_layer_num", 1_000_000),
            nozzle_temp: temp("nozzle_temper", 500.0),
            bed_temp: temp("bed_temper", 200.0),
            observed_at_ms,
        })
    }
}

fn meaningful(value: &Value) -> bool {
    !value.is_null() && value != "" && value != 0 && value != false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ingest(t: &mut Telemetry, value: Value, now: Instant) -> bool {
        t.ingest(&serde_json::to_vec(&value).unwrap(), now)
    }

    fn running() -> (Telemetry, Instant) {
        let now = Instant::now();
        let mut t = Telemetry::default();
        assert!(t.ingest(include_bytes!("fixtures/a1_snapshot.json"), now));
        (t, now)
    }

    fn status(t: &Telemetry, now: Instant) -> PrintStatus {
        t.status(now, 1_000, Some("A1 Combo")).unwrap()
    }

    #[test]
    fn bambu_delta_preserves_state_name_and_time() {
        let (mut t, now) = running();
        assert!(t.ingest(
            include_bytes!("fixtures/a1_delta.json"),
            now + Duration::from_secs(10)
        ));
        let s = status(&t, now + Duration::from_secs(10));
        assert_eq!(s.state, PrintState::Printing);
        assert_eq!(s.progress, Some(65));
        assert_eq!(s.file_name.as_deref(), Some("Fake_Benchy.3mf"));
        assert_eq!(s.remaining_secs, Some(42 * 60 - 10));
    }

    #[test]
    fn bambu_pause_has_no_countdown_and_resume_excludes_pause() {
        let (mut t, now) = running();
        ingest(
            &mut t,
            json!({"print":{"gcode_state":"PAUSE"}}),
            now + Duration::from_secs(10),
        );
        assert_eq!(
            status(&t, now + Duration::from_secs(20)).remaining_secs,
            None
        );
        ingest(
            &mut t,
            json!({"print":{"gcode_state":"RUNNING"}}),
            now + Duration::from_secs(70),
        );
        assert_eq!(
            status(&t, now + Duration::from_secs(80)).remaining_secs,
            Some(42 * 60 - 20)
        );
    }

    #[test]
    fn bambu_all_states_map_and_unknown_is_none() {
        for (raw, expected) in [
            ("RUNNING", PrintState::Printing),
            ("PREPARE", PrintState::Preparing),
            ("SLICING", PrintState::Preparing),
            ("PAUSE", PrintState::Paused),
            ("FINISH", PrintState::Finished),
            ("FAILED", PrintState::Failed),
            ("IDLE", PrintState::Idle),
        ] {
            let (mut t, now) = running();
            ingest(&mut t, json!({"print":{"gcode_state":raw}}), now);
            assert_eq!(status(&t, now).state, expected);
        }
        let (mut t, now) = running();
        ingest(&mut t, json!({"print":{"gcode_state":"UNKNOWN"}}), now);
        assert!(t.status(now, 0, None).is_none());
    }

    #[test]
    fn bambu_stale_boundary_is_ninety_seconds() {
        let (t, now) = running();
        assert!(t.status(now + Duration::from_secs(90), 0, None).is_some());
        assert!(t.status(now + Duration::from_secs(91), 0, None).is_none());
        assert!(Telemetry::default().status(now, 0, None).is_none());
    }

    #[test]
    fn bambu_each_job_identity_clears_previous_fields() {
        for (key, value) in [
            ("subtask_name", json!("Another fake print")),
            ("subtask_id", json!("2")),
            ("task_id", json!("2")),
        ] {
            let (mut t, now) = running();
            ingest(
                &mut t,
                json!({"print":{key:value,"gcode_state":"PREPARE"}}),
                now,
            );
            let s = status(&t, now);
            assert_eq!(s.progress, None);
            assert_eq!(s.layer, None);
            ingest(&mut t, json!({"print":{"gcode_state":"RUNNING"}}), now);
            assert_eq!(status(&t, now).remaining_secs, None);
        }
    }

    #[test]
    fn bambu_terminal_to_active_resets_even_same_job() {
        for terminal in ["FINISH", "FAILED", "IDLE"] {
            for active in ["RUNNING", "PREPARE"] {
                let (mut t, now) = running();
                ingest(&mut t, json!({"print":{"gcode_state":terminal}}), now);
                ingest(&mut t, json!({"print":{"gcode_state":active}}), now);
                let s = status(&t, now);
                assert_eq!(s.progress, None);
                assert_eq!(s.remaining_secs, None);
                assert_eq!(s.file_name, None);
            }
        }
    }

    #[test]
    fn bambu_invalid_numbers_replace_old_values() {
        for value in [
            Value::Null,
            json!("NaN"),
            json!(-2),
            json!("nonsense"),
            json!("inf"),
            json!(100_001),
        ] {
            let (mut t, now) = running();
            ingest(
                &mut t,
                json!({"print":{"mc_percent":value,"mc_remaining_time":value}}),
                now,
            );
            let s = status(&t, now);
            assert_eq!(s.progress, None);
            assert_eq!(s.remaining_secs, None);
        }
    }

    #[test]
    fn bambu_unrelated_packets_do_not_refresh_dead_data() {
        let (mut t, now) = running();
        for value in [
            Value::Null,
            json!([]),
            json!({"info":{}}),
            json!({"print":[]}),
            json!({"print":{}}),
            json!({"print":{"wifi_signal":"-40dBm"}}),
        ] {
            assert!(!ingest(&mut t, value, now + Duration::from_secs(90)));
        }
        assert!(t.status(now + Duration::from_secs(91), 0, None).is_none());
    }

    #[test]
    fn bambu_delta_does_not_restart_countdown() {
        let (mut t, now) = running();
        ingest(
            &mut t,
            json!({"print":{"mc_percent":70}}),
            now + Duration::from_secs(70),
        );
        assert_eq!(
            status(&t, now + Duration::from_secs(70)).remaining_secs,
            Some(42 * 60 - 70)
        );
    }

    #[test]
    fn bambu_payload_limit_and_invalid_json_do_not_refresh() {
        let (mut t, now) = running();
        for payload in [vec![b' '; MAX_PAYLOAD + 1], b"not json".to_vec(), vec![255]] {
            assert!(!t.ingest(&payload, now + Duration::from_secs(90)));
        }
        let mut at_limit = br#"{"print":{"mc_percent":1}}"#.to_vec();
        at_limit.resize(MAX_PAYLOAD, b' ');
        assert!(t.ingest(&at_limit, now));
        assert!(t.status(now + Duration::from_secs(91), 0, None).is_none());
    }

    #[test]
    fn bambu_numeric_strings_fractions_and_bounds() {
        let (mut t, now) = running();
        ingest(
            &mut t,
            json!({"print":{"mc_percent":"99.9","mc_remaining_time":"1.9","layer_num":"12.9","total_layer_num":1000000,"nozzle_temper":"220.5","bed_temper":60}}),
            now,
        );
        let s = status(&t, now);
        assert_eq!(
            (s.progress, s.remaining_secs, s.layer, s.total_layers),
            (Some(99), Some(60), Some(12), Some(1_000_000))
        );
        assert_eq!((s.nozzle_temp, s.bed_temp), (Some(220.5), Some(60.0)));
        ingest(
            &mut t,
            json!({"print":{"layer_num":-1,"total_layer_num":1000001,"nozzle_temper":501,"bed_temper":201}}),
            now,
        );
        let s = status(&t, now);
        assert_eq!(
            (s.layer, s.total_layers, s.nozzle_temp, s.bed_temp),
            (None, None, None, None)
        );
    }

    #[test]
    fn bambu_file_fallback_basename_and_model() {
        let now = Instant::now();
        let mut t = Telemetry::default();
        ingest(
            &mut t,
            json!({"print":{"gcode_state":"RUNNING","subtask_name":" ","gcode_file":"C:\\fake\\example.gcode"}}),
            now,
        );
        let s = status(&t, now);
        assert_eq!(s.file_name.as_deref(), Some("example.gcode"));
        assert_eq!(s.printer_model.as_deref(), Some("A1 Combo"));
        assert_eq!(s.observed_at_ms, 1_000);
        assert_eq!(t.status(now, 0, None).unwrap().printer_model, None);
    }

    #[test]
    fn bambu_countdown_stops_at_zero() {
        let (mut t, now) = running();
        ingest(&mut t, json!({"print":{"mc_remaining_time":1}}), now);
        assert_eq!(
            status(&t, now + Duration::from_secs(90)).remaining_secs,
            Some(0)
        );
    }
}
