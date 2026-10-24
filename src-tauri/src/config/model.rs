use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CONFIG_VERSION: u32 = 1;

/// Secrets deliberately have no representation in this serializable model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub version: u32,
    pub printers: Vec<PrinterConfig>,
    pub discord: DiscordConfig,
    pub autostart: bool,
    pub lang: Lang,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            printers: Vec::new(),
            discord: DiscordConfig::default(),
            autostart: false,
            lang: Lang::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    Tr,
    #[default]
    En,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrinterConfig {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub connection: PrinterConnection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrinterConnection {
    Bambu {
        host: String,
        serial: String,
        model: Option<String>,
    },
    Moonraker {
        url: String,
    },
    OctoPrint {
        url: String,
    },
    PrusaLink {
        url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscordConfig {
    pub enabled: bool,
    pub show_file_name: bool,
    pub app_id_override: Option<String>,
    #[serde(default)]
    pub details_template: Option<String>,
    #[serde(default)]
    pub state_template: Option<String>,
    #[serde(default = "default_status_badge")]
    pub show_status_badge: bool,
    #[serde(default)]
    pub buttons: Vec<Button>,
}

fn default_status_badge() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Button {
    pub label: String,
    pub url: String,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_file_name: true,
            app_id_override: None,
            details_template: None,
            state_template: None,
            show_status_badge: true,
            buttons: Vec::new(),
        }
    }
}
