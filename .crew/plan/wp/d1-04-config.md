# d1-04-config — Ayarlar + işletim sistemi anahtarlığı

- Summary: Yazıcı ve Discord ayarlarını diske, şifreleri ve erişim kodlarını işletim sisteminin anahtarlığına güvenle kaydeden modül; eski Windows sürümünün ayarlarını içe aktarır.
- Engine: codex
- Model: standard
- Phase: D1
- Branch: `codex/d1-04-config` (base: `main`)
- Depends on: d1-01-skeleton (lead)
- Role: implementer

## Goal

Tek bir `AppConfig` modeli; gizli değerler JSON'a hiç yazılmaz; eski Python uygulamasından geçiş tek tıkla olur.

## Scope

- In: `src-tauri/src/config/**`.
- Out: `model.rs`, Tauri komutları ve `lib.rs` (d1-05), arayüz.

## Tasks

1. `config/model.rs`: `AppConfig { version: u32, printers: Vec<PrinterConfig>, discord: DiscordConfig, autostart: bool, lang: Lang }`;
   `PrinterConfig { id: Uuid, name: String, enabled: bool, connection: PrinterConnection }`;
   `PrinterConnection::{ Bambu { host, serial, model: Option<String> }, Moonraker { url }, OctoPrint { url }, PrusaLink { url } }`
   (serde `tag = "kind"`); `DiscordConfig { enabled, show_file_name, app_id_override: Option<String> }`. Varsayılanlar mantıklı olsun.
2. `config/validate.rs`: tür bazında doğrulama, hata kodları enum olarak (metin arayüzde). Bambu: özel IPv4 (Python `validate()` kuralları),
   seri 8–32 alfanümerik, erişim kodu 8 alfanümerik. URL türleri: `http(s)://` + yerel ağ adresi veya `.local` ad; kimlik bilgisi URL içinde olamaz.
3. `config/secrets.rs`: `trait SecretStore { get, set, delete }` + `KeyringStore` (`keyring` crate, servis adı `3d-printing-presence`,
   hesap `printer:<uuid>`) + testler için `MemoryStore`. Yazıcı silinince sırrı da silinir.
4. `config/store.rs`: `ConfigStore::new(dir)`; `load()` / `save()` — atomik yazma (geçici dosya + rename), bozuk dosyada
   yedeğe (`config.json.bak`) kaçırıp varsayılanla devam ve bunu bildiren bir bayrak. Dizin parametre olarak gelir (Tauri yol çözümü d1-05'te).
5. `config/legacy.rs` (`#[cfg(windows)]`): `%LOCALAPPDATA%/BambuDiscord/settings.json` varsa DPAPI ile (`windows` crate,
   `CryptUnprotectData`) erişim kodunu çöz ve tek Bambu yazıcılı `AppConfig` + sır üret. Eski dosyaya dokunma. Diğer platformlarda `None`.
6. Testler (geçici dizinler, `MemoryStore`): gidiş-dönüş, JSON'da gizli değer olmaması, bozuk dosya kurtarma, doğrulama kuralları,
   eski biçim ayrıştırma (DPAPI kısmı hariç saf ayrıştırma testi).

## Acceptance criteria

- [ ] `cargo test --offline --manifest-path src-tauri/Cargo.toml config` geçer; en az 12 test.
- [ ] `cargo clippy --offline --manifest-path src-tauri/Cargo.toml -- -D warnings` temiz.
- [ ] Kaydedilen JSON'da erişim kodu/API anahtarı bulunmadığını doğrulayan test var.
- [ ] Gerçek kişisel veri yok; testlerde sahte değerler.

## Notes

- Kaynak: `settings.py`, `core.py → validate()`.
