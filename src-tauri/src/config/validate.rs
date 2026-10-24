use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};

use serde::Serialize;
use tauri::Url;

use super::model::{AppConfig, PrinterConnection, CONFIG_VERSION};
use crate::presence::template;

/// Codes only: neither errors nor their Debug output contain user input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ValidationError {
    #[error("unsupported_config_version")]
    UnsupportedVersion,
    #[error("duplicate_printer_id")]
    DuplicatePrinterId,
    #[error("invalid_host")]
    InvalidHost,
    #[error("invalid_serial")]
    InvalidSerial,
    #[error("invalid_access_code")]
    InvalidAccessCode,
    #[error("invalid_url")]
    InvalidUrl,
    #[error("url_credentials_forbidden")]
    UrlCredentialsForbidden,
    #[error("url_query_or_fragment_forbidden")]
    UrlQueryOrFragmentForbidden,
    #[error("non_local_address")]
    NonLocalAddress,
    #[error("invalid_app_id")]
    InvalidAppId,
    #[error("invalid_details_template")]
    InvalidDetailsTemplate,
    #[error("invalid_state_template")]
    InvalidStateTemplate,
    #[error("too_many_buttons")]
    TooManyButtons,
    #[error("invalid_button_label")]
    InvalidButtonLabel,
    #[error("invalid_button_url")]
    InvalidButtonUrl,
}

fn local_ipv4(ip: Ipv4Addr) -> bool {
    // LAN ranges plus link-local, matching the legacy validator's intended LAN policy.
    ip.is_private() || ip.is_link_local()
}

pub fn validate_url(value: &str) -> Result<(), ValidationError> {
    // URL parsers normalize whitespace and backslashes.
    // Reject ambiguous syntax before parsing rather than silently persisting it.
    if value
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return Err(ValidationError::InvalidUrl);
    }
    let (scheme, rest) = value.split_once("://").ok_or(ValidationError::InvalidUrl)?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return Err(ValidationError::InvalidUrl);
    }
    if rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .contains('@')
    {
        return Err(ValidationError::UrlCredentialsForbidden);
    }
    let url = Url::parse(value).map_err(|_| ValidationError::InvalidUrl)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ValidationError::UrlCredentialsForbidden);
    }
    // API keys must go through SecretStore, never query strings or fragments.
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ValidationError::UrlQueryOrFragmentForbidden);
    }
    let host = url.host_str().ok_or(ValidationError::InvalidUrl)?;
    let address = host.trim_start_matches('[').trim_end_matches(']');
    let local = match address.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => local_ipv4(ip),
        Ok(IpAddr::V6(ip)) => {
            let segments = ip.segments();
            segments[0] & 0xfe00 == 0xfc00 || segments[0] & 0xffc0 == 0xfe80
        }
        Err(_) => {
            let host = host.trim_end_matches('.');
            host.ends_with(".local")
                && host.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                })
        }
    };
    if !local {
        return Err(ValidationError::NonLocalAddress);
    }
    Ok(())
}

pub fn validate_connection(connection: &PrinterConnection) -> Result<(), ValidationError> {
    match connection {
        PrinterConnection::Bambu { host, serial, .. } => {
            let ip = host
                .parse::<Ipv4Addr>()
                .map_err(|_| ValidationError::InvalidHost)?;
            if !local_ipv4(ip) {
                return Err(ValidationError::NonLocalAddress);
            }
            if !(8..=32).contains(&serial.len())
                || !serial.bytes().all(|c| c.is_ascii_alphanumeric())
            {
                return Err(ValidationError::InvalidSerial);
            }
            Ok(())
        }
        PrinterConnection::Moonraker { url }
        | PrinterConnection::OctoPrint { url }
        | PrinterConnection::PrusaLink { url } => validate_url(url),
    }
}

pub fn validate_access_code(value: &str) -> Result<(), ValidationError> {
    if value.len() != 8 || !value.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ValidationError::InvalidAccessCode);
    }
    Ok(())
}

pub fn validate_config(config: &AppConfig) -> Result<(), ValidationError> {
    if config.version != CONFIG_VERSION {
        return Err(ValidationError::UnsupportedVersion);
    }
    let mut ids = HashSet::new();
    for printer in &config.printers {
        if !ids.insert(printer.id) {
            return Err(ValidationError::DuplicatePrinterId);
        }
        validate_connection(&printer.connection)?;
    }
    if let Some(id) = &config.discord.app_id_override {
        if !(17..=22).contains(&id.len()) || !id.bytes().all(|c| c.is_ascii_digit()) {
            return Err(ValidationError::InvalidAppId);
        }
    }
    if config
        .discord
        .details_template
        .as_deref()
        .is_some_and(|s| template::validate(s).is_err())
    {
        return Err(ValidationError::InvalidDetailsTemplate);
    }
    if config
        .discord
        .state_template
        .as_deref()
        .is_some_and(|s| template::validate(s).is_err())
    {
        return Err(ValidationError::InvalidStateTemplate);
    }
    if config.discord.buttons.len() > 2 {
        return Err(ValidationError::TooManyButtons);
    }
    for button in &config.discord.buttons {
        if button.label.trim().is_empty()
            || !(1..=32).contains(&button.label.chars().count())
            || button.label.chars().any(char::is_control)
        {
            return Err(ValidationError::InvalidButtonLabel);
        }
        if !button.url.starts_with("https://")
            || button
                .url
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
        {
            return Err(ValidationError::InvalidButtonUrl);
        }
        let url = Url::parse(&button.url).map_err(|_| ValidationError::InvalidButtonUrl)?;
        if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
            return Err(ValidationError::InvalidButtonUrl);
        }
    }
    Ok(())
}
