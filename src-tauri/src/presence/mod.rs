//! Local Discord IPC and pure printer-status presentation.
pub mod activity;
pub mod client;

pub use activity::{activity_for, Activity, PresenceOptions};
pub use client::{PresenceClient, PresenceState};

pub const DEFAULT_APP_ID: Option<&str> = option_env!("PRINT_PRESENCE_DISCORD_APP_ID");

/// An explicitly supplied, invalid override disables presence rather than silently
/// selecting a different application.
pub fn app_id(r#override: Option<&str>) -> Option<String> {
    r#override
        .or(DEFAULT_APP_ID)
        .filter(|id| valid_app_id(id))
        .map(str::to_owned)
}

fn valid_app_id(id: &str) -> bool {
    (17..=22).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_id_override_and_validation() {
        assert_eq!(
            app_id(Some("123456789012345678")),
            Some("123456789012345678".into())
        );
        for id in [
            "",
            "1234567890123456",
            "12345678901234567890123",
            "1234567890123456a",
            " 123456789012345678",
            "１２３４５６７８９",
        ] {
            assert_eq!(app_id(Some(id)), None);
        }
        assert!(valid_app_id("12345678901234567"));
        assert!(valid_app_id("1234567890123456789012"));
        assert_eq!(
            app_id(None),
            DEFAULT_APP_ID
                .filter(|id| valid_app_id(id))
                .map(str::to_owned)
        );
    }
}
