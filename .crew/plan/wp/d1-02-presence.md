# d1-02-presence — Discord IPC + presence motoru

- Summary: Baskı durumunu Discord profil kartına çeviren ve Discord masaüstü uygulamasına yerel IPC ile gönderen Rust modülü.
- Engine: codex
- Model: deep
- Phase: D1
- Branch: `codex/d1-02-presence` (base: `main`)
- Depends on: d1-01-skeleton (lead)
- Role: implementer

## Goal

`PrintStatus` → Discord etkinliği dönüşümü saf ve test edilebilir olsun; bağlantı, yeniden bağlanma ve
hız sınırı kendi içinde kapalı bir `PresenceClient` ile yönetilsin. Windows, macOS ve Linux'ta çalışsın.

## Scope

- In: `src-tauri/src/presence/**` (yalnızca bu dizin). Gerekirse `src-tauri/Cargo.toml`'a bağımlılık
  ekleme önerisi (raporda; ağ yok, önceden indirilmiş crate'leri kullan: `cargo build --offline`).
- Out: `model.rs` (değişiklik gerekiyorsa raporda iste), yazıcı adaptörleri, ayarlar, arayüz, `lib.rs` bağlantısı (d1-05).

## Tasks

1. `presence/activity.rs`: `fn activity_for(status: &PrintStatus, opts: &PresenceOptions, now_unix: u64) -> Option<Activity>`.
   Mevcut Python `core.py → Telemetry.activity` davranışını taşı:
   - Yalnızca `Preparing | Printing | Paused` için etkinlik; diğer durumlarda `None` (kart temizlenir).
   - `details`: dosya adı (yol ve uzantı `.gcode`/`.3mf`/`.gcode.3mf` atılır, en çok 100 karakter, 2 karakterden kısa ise varsayılan metin) veya `show_file_name=false` ise "3D baskı alıyorum".
   - `state`: model · durum etiketi · `%NN`.
   - Kalan süre için metin yerine **`timestamps.end`** kullan (Discord geri sayımı kendi gösterir); yalnızca `Printing` iken. `Paused` iken zaman damgası yok.
   - `large_image`: `PrinterKind`'a göre asset anahtarı (`bambu`, `klipper`, `octoprint`, `prusa`, bilinmeyen → `printer`); `large_text`: yazıcı modeli.
   - Metinler TR ve EN (`PresenceOptions.lang`); basit bir eşleme tablosu yeterli.
2. `presence/client.rs`: `PresenceClient` — Discord IPC (Windows: `\\.\pipe\discord-ipc-0..9`; Unix: `$XDG_RUNTIME_DIR`, `$TMPDIR`, `/tmp` ve Flatpak/Snap alt yolları `app/com.discordapp.Discord/`, `snap.discord/` altında `discord-ipc-0..9`).
   `discord-rich-presence` crate'i bu yolları ve `timestamps`'i karşılıyorsa onu sar; karşılamıyorsa
   küçük bir kendi istemcini yaz (çerçeve: opcode u32 LE + uzunluk u32 LE + JSON; HANDSHAKE v=1, SET_ACTIVITY, nonce).
   API: `set(Option<Activity>)` — aynı etkinliği tekrar göndermez, en fazla 15 sn'de bir günceller,
   `None` gelince bir kez temizler; bağlantı koparsa 15 sn geri çekilme ile yeniden bağlanır ve son etkinliği tekrar gönderir.
   Durum değişikliklerini `PresenceState { Disconnected, Connected, Error(&'static str) }` olarak bildirir.
3. `presence/mod.rs`: `pub const DEFAULT_APP_ID: Option<&str> = option_env!("PRINT_PRESENCE_DISCORD_APP_ID");`
   ve `fn app_id(override: Option<&str>) -> Option<String>` (override önce; yalnızca 17–22 haneli rakam kabul).
4. Birim testleri: `activity_for` için en az 10 durum (durum geçişleri, dosya adı temizleme, gizli dosya adı,
   bitiş zaman damgası, Paused, dil). İstemci için çerçeve kodlama/çözme testi ve "aynı etkinliği tekrar gönderme / 15 sn sınırı" mantığı sahte bir taşıyıcı (trait) ile.

## Acceptance criteria

- [ ] `cargo test --offline --manifest-path src-tauri/Cargo.toml presence` geçer; en az 12 test.
- [ ] `cargo clippy --offline --manifest-path src-tauri/Cargo.toml -- -D warnings` temiz.
- [ ] Hiçbir log veya hata mesajı Application ID dışındaki bir kimliği ya da IPC yanıt gövdesini yazdırmaz.
- [ ] Gerçek Discord olmadan testler geçer (IPC testleri sahte taşıyıcı ile).
- [ ] Diff yalnızca `src-tauri/src/presence/**` (ve raporda önerilen bağımlılıklar).

## Notes

- Kaynak davranış: `core.py` (`Telemetry.activity`, `Bridge.run` içindeki Discord döngüsü) ve `test_bridge.py`.
- Ortak tipler `src-tauri/src/model.rs`'de; değiştirmeden kullan.
