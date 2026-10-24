# d1-07-card — Özelleştirilebilir Discord kartı

- Summary: Discord kartında yazıcının fotoğrafı, durum rozeti, katman/sıcaklık bilgisi, kullanıcının diline göre metin şablonları ve bağlantı düğmeleri.
- Engine: codex
- Model: deep
- Phase: D1
- Branch: `codex/d1-07-card` (base: `origin/v2`)
- Depends on: d1-05-runtime
- Role: implementer

## Goal

Kart "3D yazıcı" ve uygulama ikonu yerine gerçek yazıcı modelini ve fotoğrafını gösterir; kullanıcı kartın metnini
kendi diline göre şablonlarla ve düğmelerle özelleştirebilir. (Ayar ekranı ayrı bir pakette; bu paket motor + ayar modeli.)

## Scope

- In: `src-tauri/src/presence/**`, `src-tauri/src/config/model.rs` + `config/validate.rs` + `config/tests.rs` (yalnızca `DiscordConfig` genişletmesi),
  `src-tauri/src/runtime/**` (yalnızca yeni ayarları `PresenceOptions`'a geçirmek), yeni dosya `src-tauri/src/printers/bambu/models.rs`
  (+ `bambu/mod.rs`'de tek satır `pub mod models;` ve modelin `PrintStatus.printer_model`'e yazılması), `src/lib/api.ts` (yalnızca tip ekleme).
- Out: Svelte bileşenleri (Pinky d1-06 üzerinde çalışıyor; `src/lib/api.ts` dışında `src/**`'e dokunma), logo/görsel üretimi.

## Tasks

1. **Model tespiti** (`bambu/models.rs`): yapılandırmada model yoksa seri numarası önekinden Bambu modeli
   (`Bambu Lab A1`, `A1 mini`, `P1P`, `P1S`, `X1 Carbon`, `X1E`, `H2D`… bildiğin önekler; bilinmeyen → `Bambu Lab`).
   Her öneki birim testiyle sabitle; emin olmadığın önekleri raporda listele.
2. **Görseller** (`presence/images.rs`): `large_image` = model fotoğrafı için **https URL**'si (Discord RPC dış URL kabul eder).
   Bambu modelleri için BambuStudio deposundaki `resources/images/printer_preview_*.png` görselleri
   (`https://raw.githubusercontent.com/bambulab/BambuStudio/master/resources/images/…`; eski Python uygulaması A1 için `printer_preview_N2S.png` kullanıyordu).
   Bilinmeyen model/marka → uygulama logosu için sabit bir URL sabiti (`APP_LOGO_URL`, şimdilik repo `v2` dalındaki `assets/app-icon-3d-printing-v1.png` raw URL'si).
   `small_image` = durum rozeti URL'leri (`assets/discord/status-printing.png`, `-preparing.png`, `-paused.png`; tek bir `ASSET_BASE_URL` sabitinden;
   dosyalar sonra eklenecek), `small_text` = yerelleştirilmiş durum etiketi. `large_text` = model adı. URL'ler tek bir modülde.
3. **Şablonlar** (`presence/template.rs`): `details` ve `state` için yer tutucular: `{file}`, `{model}`, `{status}`, `{progress}`, `{layer}`,
   `{total_layers}`, `{nozzle}`, `{bed}`, `{remaining}`. Yüzde, sıcaklık (`215°C`) ve kalan süre (`1 sa 20 dk` / `1h 20m`) **dile göre** biçimlenir
   (TR: `%42`, EN: `42%`). Değeri olmayan yer tutucu içeren ` · ` ile ayrılmış parça sessizce düşer. Sonuç 2–128 karakter (Discord sınırı);
   bozuk şablon (kapanmamış `{`, bilinmeyen ad) doğrulamada reddedilir. Varsayılan şablonlar dile göre:
   TR details `{file}`, state `{model} · {status} · %{progress}`; EN state `{model} · {status} · {progress}%`.
   `show_file_name=false` iken `{file}` her zaman yerelleştirilmiş "3D baskı alıyorum" / "3D printing".
4. **Ayar modeli** (`DiscordConfig`, `serde(default)` ile geriye uyumlu; `CONFIG_VERSION` değişmez):
   `details_template: Option<String>`, `state_template: Option<String>` (None → dile göre varsayılan),
   `show_status_badge: bool` (varsayılan true), `buttons: Vec<Button { label, url }>` (en fazla 2, etiket 1–32 karakter, yalnızca `https://`).
   Doğrulama hata kodlarıyla. Mevcut `config.json` dosyaları sorunsuz yüklenmeli (test).
5. **Runtime**: yeni ayarlar ve `config.lang` `PresenceOptions`'a geçer; dil değişince kart yeni dilde güncellenir.
6. **Düğmeler**: IPC etkinliğine `buttons` eklenir (Discord'da kartın sahibi kendi düğmelerini göremez, başkaları görür; `api.ts` yorumunda belirt).
7. Testler: şablon motoru (tüm yer tutucular, iki dil, eksik değer, uzunluk sınırı, bozuk şablon), model önekleri, görsel seçimi,
   eski config dosyasının yüklenmesi, düğme doğrulaması.

## Acceptance criteria

- [ ] `cargo test --offline --manifest-path src-tauri/Cargo.toml` ve `cargo clippy --offline ... --all-targets -- -D warnings` temiz.
- [ ] Eski (bu alanlar olmadan kaydedilmiş) `config.json` yüklenir (test).
- [ ] Varsayılan şablonlarla TR ve EN çıktıları testte sabit.
- [ ] `src/lib/api.ts` yeni alanlarla güncel; `src/**` içinde başka dosya değişmemiş.
- [ ] Lider gerçek A1 Combo ile kartta model adı, yazıcı fotoğrafı ve şablonun göründüğünü doğrular.

## Notes

- Mevcut davranış: `presence/activity.rs` (`activity_for`, `PresenceOptions`). Bitiş zaman damgası (`timestamps.end`) korunur.
- Sandbox'ta ağ yok: URL'leri doğrulayamazsan raporda "doğrulanmadı" de; lider kontrol eder.
