# d1-05-runtime — Çalışma zamanı ve Tauri komutları

- Summary: Ayarları, yazıcı adaptörlerini ve Discord modülünü tepside çalışan tek bir servis olarak birbirine bağlar; arayüz için komutları ve olayları tanımlar.
- Engine: codex
- Model: deep
- Phase: D1
- Branch: `codex/d1-05-runtime` (base: `origin/v2`)
- Depends on: d1-02-presence, d1-03-bambu, d1-04-config (birleştirilmiş olmalı)
- Role: implementer

## Goal

Uygulama açılınca kayıtlı yazıcılara bağlanır, aktif baskıyı Discord'a yansıtır, pencere kapalıyken de tepside çalışmayı sürdürür;
arayüz durumu canlı olarak alır ve ayarları değiştirebilir.

## Scope

- In: `src-tauri/src/lib.rs`, `src-tauri/src/runtime/**`, `src-tauri/src/commands.rs`, `src-tauri/capabilities/**`,
  `src/lib/api.ts` (yalnızca tipli API sözleşmesi, arayüz bileşenleri değil).
- Out: `presence/`, `printers/`, `config/` içleri (yalnızca genel API'lerini kullan; hata bulursan raporla), Svelte bileşenleri (d1-06).

## Tasks

1. `runtime/`: `Supervisor` — yapılandırmadaki her etkin yazıcı için `PrinterSource` başlatır, ayar değişince yeniden başlatır,
   kapatmada `CancellationToken` ile temiz durdurur. Gösterilecek baskıyı seçer: `Printing` > `Preparing` > `Paused`; birden fazla
   basan yazıcıda bitişi en yakın olan. Seçilen durumu `presence::activity_for` → `PresenceClient::set`'e verir.
2. `commands.rs`: `get_config`, `save_config(config, secrets)`, `delete_printer(id)`, `get_snapshot` (yazıcı başına bağlantı + son durum,
   Discord durumu, seçilen etkinlik önizlemesi), `import_legacy` (Windows), `set_autostart(bool)`. Sırlar komutla gelir, yanıtlarda asla geri dönmez
   (yalnızca `has_secret: bool`).
3. Olaylar: `snapshot` olayı durum değiştikçe (en fazla saniyede bir) yayınlanır.
4. `src/lib/api.ts`: komut ve olay tipleri (Rust serde çıktısıyla birebir), `invoke`/`listen` sarmalayıcıları.
5. İlk açılışta yapılandırma yoksa ve Windows'ta eski ayarlar varsa `snapshot` içinde `legacy_available: true`.
6. İlk açılışta (yapılandırma dosyası yokken) dil, işletim sistemi diline göre seçilsin (`tr*` → Tr, diğer → En).
7. Discord Application ID: `presence::app_id(config.discord.app_id_override)`; `None` ise Discord devre dışı ve
   snapshot'ta `discord: "no_app_id"` durumu (arayüz kullanıcıya açıklar). Discord kapalıysa (`enabled=false`) `PresenceClient` oluşturulmaz.
8. Tepsi menüsü metinleri ayardaki dile göre (TR: Göster / Tamamen çık, EN: Show / Quit); dil değişince menü güncellenir.
9. Testler: seçim mantığı ve yeniden başlatma için sahte `PrinterSource` ile birim testleri.

## Acceptance criteria

- [ ] `cargo test --offline --manifest-path src-tauri/Cargo.toml` ve `cargo clippy ... --all-targets -- -D warnings` temiz.
- [ ] `npm run lint` temiz (`api.ts` tipleri).
- [ ] Hiçbir komut yanıtı veya olay erişim kodu/API anahtarı içermez (test).
- [ ] Lider `npm run tauri dev` ile uygulamanın açıldığını, tepsiye küçüldüğünü ve gerçek A1 Combo ile Discord kartının göründüğünü doğrular.

## Notes

- Mevcut modüller (v2'de): `presence::{activity_for, PresenceClient, PresenceState, app_id}`, `printers::bambu::BambuSource`,
  `config::{ConfigStore, KeyringStore, SecretStore, legacy::import_legacy}`. Bambu `model` alanı yapılandırmadan gelir.
- Moonraker/OctoPrint/PrusaLink kaynakları henüz yok (D2): bu türler yapılandırmada olabilir ama `Supervisor` onlar için
  yalnızca `Connection { Error(Protocol) }` değil, ayrı bir "desteklenmiyor" durumu raporlasın (snapshot'ta `unsupported: true`).

- Tauri izinlerini (`capabilities/`) yalnızca kullanılan komutlara aç.
