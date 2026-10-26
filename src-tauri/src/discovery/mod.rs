//! Explicit, local-only discovery. Credentials never enter discovery responses.
pub mod ssdp;
pub mod studio;

#[derive(serde::Serialize)]
pub struct Discovered {
    pub host: String,
    pub serial: String,
    pub name: String,
    pub model: String,
    pub studio_code: bool,
}

pub fn discover(seconds: Option<u8>) -> Vec<Discovered> {
    let mut printers = ssdp::discover(seconds.unwrap_or(6).clamp(1, 15));
    let root = dirs::config_dir();
    for printer in &mut printers {
        printer.studio_code = root
            .as_deref()
            .and_then(|root| studio::access_code(root, &printer.serial))
            .is_some();
    }
    printers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_ipc_contains_availability_only() {
        let response = serde_json::to_value(Discovered {
            host: "192.168.1.50".into(),
            serial: "00M00A000000000".into(),
            name: "Fake printer".into(),
            model: "Bambu Lab X1 Carbon".into(),
            studio_code: true,
        })
        .unwrap();
        assert_eq!(response.as_object().unwrap().len(), 5);
        assert_eq!(response["studio_code"], true);
        assert!(response.get("secret").is_none());
        assert!(response.get("access_code").is_none());
    }
}
