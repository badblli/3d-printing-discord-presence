# d2-01-discovery — Yazıcıyı ağda otomatik bul, erişim kodunu Bambu Studio'dan al

- Summary: Kurulum sihirbazı yerel ağdaki Bambu yazıcıları kendisi bulur; kullanıcı listeden seçince ad, IP ve seri numarası dolar, Bambu Studio yüklüyse erişim kodu da tek tıkla oradan alınır.
- Engine: codex
- Model: deep
- Skill: frontend-design
- Phase: D2
- Branch: `codex/d2-01-discovery` (base: `origin/main`)
- Depends on: none
- Role: implementer

## Goal

Ürün sahibi: "IP, seri no, erişim kodu… kullanıcıyı uğraştırmadan otomatik alıp doldurmaz mıyız?"
Sihirbazın 2. adımı ("Yazıcınla tanışalım") açılınca ağ taranır; bulunan Bambu yazıcılar kart olarak listelenir.
Birine tıklanınca form dolar. Bambu Studio bu bilgisayarda o yazıcının erişim kodunu biliyorsa
"Erişim kodunu Bambu Studio'dan al" seçeneği çıkar; seçilirse kullanıcı kod yazmadan kaydedebilir.
Elle giriş her zaman çalışmaya devam eder (keşif başarısızsa veya yazıcı listede yoksa).

## Scope

- In: yeni `src-tauri/src/discovery/{mod.rs,ssdp.rs,studio.rs}` (+ testler), `src-tauri/src/lib.rs` (modül + komut kaydı),
  `src-tauri/src/commands.rs`, `src-tauri/src/runtime/settings.rs` (yalnızca Studio kodunu kaydetme yolu),
  `src-tauri/Cargo.toml` (yalnızca `socket2 = "0.6"` ve `dirs = "6"`; ikisi de Cargo.lock'ta var, offline derlenir),
  `src/lib/api.ts`, `src/lib/forms.ts`, `src/lib/components/PrinterWizard.svelte`, `src/lib/i18n/index.ts`, `src/lib/theme.css`, vitest dosyaları.
- Out: `model.rs`, `printers/**`, Moonraker/OctoPrint/PrusaLink keşfi (mDNS), Orca Slicer, Bambu bulut hesabı girişi,
  Windows güvenlik duvarı kuralı / yükleyici değişikliği.

## Tasks

1. **SSDP keşfi** (`discovery/ssdp.rs`). Bambu yazıcılar `239.255.255.250` adresine **2021** (ve bazı modeller 1990) portundan
   birkaç saniyede bir `NOTIFY` yayar. Başlıklar (anahtarlar büyük/küçük harf duyarsız):
   `Location: <ip>`, `USN: <seri>`, `NT`/`ST: urn:bambulab-com:device:3dprinter:1`, `DevModel.bambu.com: N2S`,
   `DevName.bambu.com: <kullanıcının verdiği ad>`, `DevConnect.bambu.com`, `DevBind.bambu.com`, `DevVersion.bambu.com`.
   - 1990 ve 2021 portlarına `SO_REUSEADDR` ile bağlan (socket2; Bambu Studio aynı portları dinliyor olabilir), multicast grubuna katıl,
     ayrıca her iki porta `M-SEARCH` (`ST: urn:bambulab-com:device:3dprinter:1`, `MX: 1`) gönder. Bir port bağlanamazsa diğeriyle devam et.
   - Varsayılan ~6 sn dinle (komut parametresiyle 1–15 sn arası sınırlı), sonuçları seriye göre tekilleştir.
   - Paket ayrıştırma saf bir fonksiyon olsun (`parse(packet: &[u8], from: Ipv4Addr) -> Option<Discovered>`): 16 KB üstü paketleri yok say,
     yalnızca Bambu ST/NT'li olanları al, `Location` geçerli bir özel IPv4 değilse kaynak adresi kullan, seriyi mevcut
     `validate` kurallarıyla doğrula, adı 80 karakterle sınırla ve kontrol karakterlerinden arındır.
   - `Discovered { host, serial, name, model: String /* models::resolve(None, serial) */, studio_code: bool }`.
   - Loglarda IP/seri/ad yazma (AGENTS.md).
2. **Bambu Studio erişim kodu** (`discovery/studio.rs`). Dosya: Windows `%APPDATA%\BambuStudio\BambuStudio.conf`,
   macOS `~/Library/Application Support/BambuStudio/BambuStudio.conf`, Linux `~/.config/BambuStudio/BambuStudio.conf` (`dirs::config_dir()`).
   İçerik JSON + sonunda `# MD5 checksum ...` satırı olabilir: ilk JSON değerini `serde_json::Deserializer::from_str(..).into_iter::<Value>()` ile oku.
   Kod `access_code[<seri>]`, yoksa `user_access_code[<seri>]`; `validate_access_code` ile doğrula. Dosya > 4 MB veya bozuksa sessizce "yok" de.
   Fonksiyon kökü parametre alsın ki testler `tempfile` ile çalışsın.
3. **Komutlar** (`commands.rs` + `lib.rs`):
   - `discover_printers(seconds: Option<u8>) -> Vec<Discovered>`: engelleyici iş `spawn_blocking`'de; `studio_code` alanı
     yalnızca **bool**. Kodun kendisi **asla** IPC'den web arayüzüne gitmez.
   - Studio kodunu kaydetme: `save_config` girdisine `studio_secrets: Vec<Uuid>` (serde default, geriye uyumlu) ekle; arka uç
     her id için o yazıcının seri numarasıyla Studio'dan kodu okuyup mevcut `SecretUpdate` yoluna ekler (aynı doğrulama, aynı geri alma).
     Kod bulunamazsa `studio_code_missing` hatası; aynı id hem `secrets` hem `studio_secrets`'ta ise `duplicate_secret_printer`.
4. **Sihirbaz** (`PrinterWizard.svelte`, use your frontend-design skill). 2. adım açılınca (düzenleme modunda değilse) taramayı başlat:
   - Üstte "Ağda bulunan yazıcılar" bölümü: taranırken ince bir yükleniyor durumu, sonra kartlar (ad, model, IP; seri gösterme ya da son 4 hane).
     Kart tıklanınca form dolar (ad boşsa DevName, IP, seri; model alanı boş = otomatik). Seçili kart vurgulanır. "Yeniden tara" düğmesi.
   - Hiç bulunamazsa sakin bir not: "Yazıcı bulunamadı. Aynı ağda ve açık olduğundan emin ol ya da bilgileri elle gir." (TR/EN).
   - Seçilen yazıcı için `studio_code` true ise erişim kodu alanının yerinde işaretli bir seçenek: "Erişim kodunu Bambu Studio'dan al".
     İşaretliyken parola alanı gizlenir ve `validatePrinter` kodu zorunlu tutmaz; kaldırılırsa elle giriş alanı geri gelir.
     Kaydet → `saveConfig(config, [], [printerId])`.
   - Mevcut elle giriş, yardım metinleri ve hata gösterimi aynen kalır; pencere boyutu (sabit, kaydırmasız) aşılmasın — gerekirse
     yardım `<details>` bölümlerini sadeleştir, kart listesi yatay/kompakt olsun.
   - Tüm yeni metinler `i18n` TR+EN; `studio_code_missing` için `errors.ts` mesajı.
5. **Testler**:
   - Rust: `parse` için Bambu NOTIFY, M-SEARCH yanıtı, büyük/küçük harf başlıklar, Bambu olmayan SSDP, geçersiz seri, aşırı uzun/kontrol karakterli ad,
     `Location` boş/genel IP; Studio dosyası için checksum satırlı JSON, `user_access_code` yedeği, geçersiz kod, bozuk/eksik dosya;
     `Settings::save` ile `studio_secrets` (başarılı, eksik kod, çakışan id). Sahte değerler: `192.168.1.50`, `00M00A000000000`, `12345678`.
   - vitest: Studio seçiliyken form doğrulaması, i18n anahtarlarının iki dilde eksiksizliği (mevcut test genişletilir).

## Acceptance criteria

- [ ] `cargo test --offline`, `cargo clippy --offline ... --all-targets -- -D warnings`, `npm run lint`, `npm test` temiz.
- [ ] Erişim kodu hiçbir IPC yanıtında, olayda veya logda yok (lider `discover_printers` yanıtını ve kodu inceler).
- [ ] Gerçek ağda (lider dener): sihirbaz 2. adımında yazıcı ~6 sn içinde listelenir; tıklayınca ad/IP/seri dolar;
      "Bambu Studio'dan al" ile kod yazmadan kaydedip bağlantı testi başarılı olur.
- [ ] Keşif başarısız/boşken elle giriş eskisi gibi çalışır; düzenleme modunda tarama otomatik başlamaz.

## Notes

- Eski Python sürümünde aynı mantık var: `discover.py` (yalnızca referans; Python'a dokunma).
- Seri öneki → model: `src-tauri/src/printers/bambu/models.rs::resolve`.
- Erişim kodu doğrulaması: `src-tauri/src/config/validate.rs::validate_access_code`.
- Sandbox'ta multicast/ağ olmayabilir: ağ kodunu ayrıştırmadan ayrı tut, testler yalnızca saf fonksiyonları ve dosya okumayı sınasın.
- `node_modules`'ı ana klasörden **kopyala, taşıma**. Yeni npm paketi ekleme.
