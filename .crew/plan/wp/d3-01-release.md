# d3-01-release — Windows kurulum paketi, otomatik yayın ve uygulama içi güncelleme

- Summary: Etiket push edilince GitHub Actions Windows kurulum dosyasını derleyip imzalı güncellemeyle birlikte yayınlar; uygulama yeni sürümü kendisi fark edip kullanıcıya bildirir ve tek tıkla günceller.
- Engine: codex
- Model: deep
- Phase: D3
- Branch: `codex/d3-01-release` (base: `origin/v2`)
- Depends on: d1-09-native-ui
- Role: implementer

## Goal

Kullanıcı GitHub Releases'tan `3D Printing_x.y.z_x64-setup.exe` indirip yönetici izni istemeden kurar. Yeni bir sürüm
yayınlandığında çalışan uygulama bunu fark eder, kullanıcıya uygulama içinde (ve pencere kapalıysa Windows bildirimiyle)
haber verir; kullanıcı "Güncelle" deyince indirir, kurar ve yeniden başlatır.

## Scope

- In: `src-tauri/src/updates.rs` (yeni), `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/**`,
  `.github/workflows/{release.yml,ci.yml}`, `scripts/bump-version.mjs`, `RELEASING.md`, `src/lib/api.ts` (tip ekleme),
  `src/lib/components/UpdateBanner.svelte` (yeni), `src/lib/components/Preferences.svelte` (yalnızca "Güncellemeler" bölümü),
  `src/routes/+page.svelte` (yalnızca banner'ı yerleştirmek), `src/lib/i18n/index.ts` (yeni anahtarlar).
- Out: `presence/`, `printers/`, `config/`, `runtime/` içleri. Gizli anahtarlar: hiçbir dosyaya yazılmaz (secret'lar zaten GitHub'da).

## Hazır olanlar (lider ekledi, `v2`'de)

- Crate'ler: `tauri-plugin-updater`, `tauri-plugin-process`, `tauri-plugin-notification` (önceden indirildi, `--offline` çalışır).
- npm: `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`.
- `tauri.conf.json`: `bundle.createUpdaterArtifacts: true`, `plugins.updater.pubkey` (açık anahtar), endpoint
  `https://github.com/badblli/3d-printing-discord-presence/releases/latest/download/latest.json`, Windows `installMode: passive`.
- GitHub secret'ları: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

## Tasks

1. **Kurulum paketi** (`tauri.conf.json`): Windows için yalnızca NSIS (`bundle.targets` platforma göre; Windows'ta `["nsis"]`),
   `nsis.installMode: "currentUser"` (yönetici izni yok), `languages: ["Turkish", "English"]` (işletim sistemi diline göre),
   kurulum ikonu `icons/icon.ico`, `publisher`, `shortDescription`, `copyright`, `homepage`. Kaldırırken kullanıcı ayarları silinmez.
2. **Güncelleme servisi** (`updates.rs`, Rust tarafında; arayüz yalnızca komut/olay kullanır): updater + process + notification eklentilerini kaydet.
   Açılıştan 20 sn sonra ve sonra 6 saatte bir sessizce denetle; ağ hatası sessizce yutulur (sadece durum). Durum makinesi:
   `idle | checking | available{version, notes} | downloading{percent} | ready | error{code}` → `update` olayı + `get_update_state` komutu.
   Komutlar: `check_for_updates` (elle), `install_update` (indir → kur → yeniden başlat). Yeni sürüm bulununca:
   tepsi menüsüne "Güncelleme var (vX)" öğesi (tıklayınca pencereyi açar), pencere gizliyse **bir kez** Windows bildirimi (aynı sürüm için tekrar etmez).
   Capabilities yalnızca gerekenler.
3. **Arayüz**: üstte ince `UpdateBanner` ("Yeni sürüm vX hazır · Güncelle ve yeniden başlat", indirme yüzdesi, "Sonra" ile gizleme);
   Ayarlar → "Güncellemeler": mevcut sürüm, son denetim zamanı, "Şimdi denetle" düğmesi. TR/EN i18n, mevcut tasarım dili.
4. **CI** (`.github/workflows/ci.yml`): `v2` ve `main`'e push ile PR'larda `windows-latest` üzerinde `npm ci`, `npm run lint`, `npm test`,
   `cargo test`, `cargo clippy -- -D warnings`. Cargo/npm önbelleği.
5. **Yayın** (`.github/workflows/release.yml`): `v*` etiketi push edilince `windows-latest` üzerinde `tauri-apps/tauri-action` ile derle,
   **yayınlanmış** (taslak değil, ön sürüm değil) release oluştur, `latest.json` ekle (`includeUpdaterJson`). Ortam: `GITHUB_TOKEN`,
   `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `PRINT_PRESENCE_DISCORD_APP_ID: ${{ vars.DISCORD_APP_ID }}`.
   Etiket sürümü `tauri.conf.json` sürümüyle eşleşmiyorsa iş başarısız olsun. `permissions: contents: write`. Action'ları sürümle sabitle.
6. **Sürüm yönetimi**: `scripts/bump-version.mjs <x.y.z>` — `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` (+ `Cargo.lock` paketi)
   sürümünü birlikte günceller; `npm run release:bump`. `RELEASING.md`: Türkçe, adım adım (bump → commit → `git tag vX.Y.Z` → push → Actions → release),
   imza anahtarının yerel konumu (`%USERPROFILE%\.tauri\`) ve yedekleme uyarısı, SmartScreen notu (kod imzalama sertifikası yok).
7. Testler: güncelleme durum makinesi ve "aynı sürüm için tek bildirim" mantığı (Rust birim testi, ağ yok); banner/ayarlar için vitest; bump betiği için küçük test.

## Acceptance criteria

- [ ] `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings`, `npm run lint`, `npm test` temiz.
- [ ] Workflow YAML'ları geçerli (lider `actionlint` veya gh ile kontrol eder); gizli değer içermez.
- [ ] Lider yerelde `npm run tauri build` ile `*-setup.exe` + `.sig` üretildiğini ve kurulumun yönetici izni istemediğini doğrular.

## Notes

- Sandbox'ta ağ yok: `tauri build` (NSIS indirir) ve gerçek güncelleme denetimi liderde. `node_modules` çalışma klasörüne lider tarafından kopyalandı.
- Kod imzalama sertifikası yok; SmartScreen "Bilinmeyen yayıncı" uyarısı beklenir.
