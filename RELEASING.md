# Windows sürümü yayınlama

Önce bir kez: GitHub Actions için `TAURI_SIGNING_PRIVATE_KEY` ve `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secret'larını, ortak Discord Application ID için `DISCORD_APP_ID` değişkenini yapılandır. Özel anahtarı repoya ekleme. Yerel imza anahtarları `%USERPROFILE%\.tauri\` altında tutulur; kaybolursa mevcut kurulumlar yeni güncellemelerin imzasını doğrulayamaz. Anahtarın ve parolasının güvenli, erişimi kısıtlı bir yedeğini ayrı yerde sakla.

## Nasıl yayınlanır (Release düğmesi)

Yayın her merge'de değil, yalnızca elle başlatılınca çıkar. Yayınlanan sürüm, uygulama içi güncellemeyle tüm kullanıcılara ulaşır.

1. GitHub → **Actions → Release → Run workflow**. Branch olarak `main` seçili kalsın (başka branch'ten başlatılırsa iş durur).
2. **Version bump** için `patch` / `minor` / `major` seç ya da **Exact version** alanına `X.Y.Z` yaz (doluysa bump yok sayılır). **Run workflow** de.
3. İş sırasıyla: yeni sürümü hesaplar (mevcut sürümden yeni değilse ya da `vX.Y.Z` etiketi zaten varsa hemen durur), `npm run release:bump` çalıştırır, `npm run lint`, `npm test`, `cargo test` ve `cargo clippy` denetimlerini yapar, `chore(release): vX.Y.Z` commit'ini ve `vX.Y.Z` etiketini `main`'e push eder, ardından aynı çalıştırmada NSIS kurulum dosyasını derleyip GitHub Release'i yayınlar.
4. Release'te `Plycast_X.Y.Z_x64-setup.exe`, imzalı güncelleme dosyası (`.sig`), `latest.json` ve sabit adlı `Plycast-setup.exe` olduğunu doğrula. Temiz bir Windows kullanıcı hesabında kurulumun yönetici izni istemediğini ve uygulama içi güncellemenin yeniden açıldığını dene. Kaldırmada “Uygulama verilerini sil” seçeneğini işaretlemeden ayarların kaldığını doğrula.

## Web sitesi indirme bağlantısı

Her yayına kurulum dosyasının `Plycast-setup.exe` adlı bir kopyası da eklenir. Bu bağlantı her zaman son sürümü doğrudan indirir, GitHub sayfası açılmaz:

```
https://github.com/badblli/plycast/releases/latest/download/Plycast-setup.exe
```

plycast.badblli.dev'deki indirme düğmesi bu adrese bağlanmalı. Uygulama içi güncelleme bu dosyayı kullanmaz; `latest.json` ve `.sig` sürüm numaralı dosyayı göstermeye devam eder.

## Elle yayın (yedek yol)

Düğme kullanılamazsa: `npm run release:bump -- X.Y.Z` çalıştır, yukarıdaki denetimleri yerelde çalıştır (`cargo test --offline --manifest-path src-tauri/Cargo.toml`, `cargo clippy --offline --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, `npm run lint && npm test`), sürüm değişikliklerini commit et, `git tag vX.Y.Z` oluştur ve branch ile etiketi kendi hesabınla push et. Etiket push'u **Release build** işini başlatır; etiket sürümü uygulama sürümüyle uyuşmazsa iş durur. Yerel Windows paket denemesinde imza anahtarı ortam değişkenleri yüklüyken `npm run tauri build -- --bundles nsis` kullan.

NSIS kurulum dosyası için ayrıca bir Windows kod imzalama sertifikası yoktur. Bu nedenle SmartScreen “Bilinmeyen yayıncı” uyarısı gösterebilir. Updater'ın `.sig` imzası paket bütünlüğünü doğrular; Windows'un kod imzalama uyarısını kaldırmaz.
