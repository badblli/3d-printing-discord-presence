<div align="center">

<img src="assets/brand/logo.png" alt="Plycast logosu" width="140">

# Plycast

### Baskı ilerlemen Discord profilinde. Yazıcın kendi ağında kalır.

[![Son sürüm](https://img.shields.io/github/v/release/badblli/plycast?style=flat-square&color=52dda0)](https://github.com/badblli/plycast/releases/latest)
![Windows](https://img.shields.io/badge/platform-Windows-34435e?style=flat-square)
![Bambu Lab](https://img.shields.io/badge/yaz%C4%B1c%C4%B1-Bambu%20Lab-52dda0?style=flat-square)
![MIT](https://img.shields.io/github/license/badblli/plycast?style=flat-square)

**[⬇ Windows için indir](https://github.com/badblli/plycast/releases/latest)** · [plycast.badblli.dev](https://plycast.badblli.dev) · [English](#english)

<sub>Discord kartında uygulama adı <b>3D Printing</b> olarak görünür.</sub>

</div>

## Ne işe yarar?

3D yazıcın baskı alırken Discord profilinde canlı bir kart gösterir:

- **Yazıcı modeli ve fotoğrafı** (ör. Bambu Lab A1)
- **İlerleme yüzdesi**, durum rozeti (basılıyor / hazırlanıyor / duraklatıldı)
- **Kalan süre**, Discord'un kendi geri sayımıyla
- İsteğe bağlı **dosya adı**, katman ve sıcaklık bilgisi, en fazla 2 bağlantı düğmesi

Baskı bitince ya da yazıcı boştayken kart kendiliğinden kaybolur. Uygulama arka planda, sistem tepsisinde sessizce çalışır.

## Kurulum

1. [Son sürümü](https://github.com/badblli/plycast/releases/latest) indir: `Plycast_x.y.z_x64-setup.exe`.
2. Kurulum dosyasını aç. Yönetici izni istemez.
   Windows "Bilinmeyen yayıncı" uyarısı gösterebilir; **Ek bilgi → Yine de çalıştır** de (uygulama henüz ücretli bir kod imzalama sertifikasıyla imzalanmadı).
3. Uygulama açılınca kurulum sihirbazı yazıcını ekletir. Eski Python sürümünü kullandıysan ayarlarını tek tıkla içe aktarabilirsin.
4. Discord'da **Kullanıcı Ayarları → Etkinlik Gizliliği** bölümünden etkinlik paylaşımının açık olduğundan emin ol.

Discord Developer Portal'a girmen, bot oluşturman ya da token girmen **gerekmez**.

### Yazıcı bilgileri nerede?

| Alan | Nereden bulunur? |
| --- | --- |
| IP adresi | Yazıcı ekranı → **Ayarlar → WLAN / Ağ** |
| Seri numarası | **Ayarlar → Cihaz bilgisi** (AMS seri numarası değil) |
| LAN erişim kodu | **Ayarlar → WLAN / Ağ → LAN erişim kodu** (Bambu hesap parolan değil) |

Yazıcı ve bilgisayar aynı yerel ağda olmalı. Bazı yeni Bambu firmware'leri üçüncü taraf uygulamalar için **LAN Only** veya **Developer Mode** isteyebilir.

## Özellikler

- **Desteklenen yazıcılar:** Bambu Lab yazıcıları yerel ağ üzerinden (A1, A1 mini, P1P, P1S, X1 Carbon, X1E, H2D…; gerçek cihazda test edilen: A1 Combo). Klipper/Moonraker, OctoPrint ve PrusaLink yakında.
- **Kart ayarları:** üst ve alt satır için şablonlar (`{model} · {status} · %{progress}` gibi), canlı önizleme, Türkçe/İngilizce.
- **Tepside çalışma:** pencereyi kapatınca uygulama tepside çalışmaya devam eder; istersen bilgisayar açılınca otomatik başlar.
- **Otomatik güncelleme:** yeni sürüm çıkınca uygulama içinde (pencere kapalıysa Windows bildirimiyle) haber verir; tek tıkla güncellenir. Güncellemeler imzalıdır, yalnızca bu projenin anahtarıyla imzalanmış sürümler kurulur.

## Gizlilik

- Uygulama yazıcıyı **yalnızca okur**: başlat/durdur gibi hiçbir komut göndermez, Bambu hesabına giriş yapmaz.
- Yazıcıyla bağlantı yerel ağda, Bambu'nun sertifikasıyla doğrulanan TLS üzerinden kurulur.
- LAN erişim kodu Windows Kimlik Bilgisi Yöneticisi'nde saklanır; ayar dosyasına yazılmaz.
- Discord'a yalnızca kartta gördüğün bilgiler (model, durum, yüzde, kalan süre ve açıksa dosya adı) gider. Kart, kendi bilgisayarındaki Discord uygulamasına yerel olarak iletilir; arada sunucu yoktur.
- Güncelleme denetimi için GitHub Releases'a bağlanılır.

## Sık sorulanlar

**Herkes aynı Discord uygulamasını mı kullanıyor, kartlar karışır mı?**
Hayır. Kartı herkesin kendi bilgisayarındaki Discord ayarlar; uygulama kimliği yalnızca kartta görünen ad ve ikonu belirler. Aynı oyunu oynayan binlerce kişi gibi düşün.

**Kart görünmüyor.**
Discord masaüstü uygulaması aynı bilgisayarda açık olmalı ve Etkinlik Gizliliği'nde paylaşım açık olmalı. Uygulamadaki Discord önizlemesi kartın nasıl görüneceğini gösterir.

**Kendi Discord uygulamamın adını kullanmak istiyorum.**
Ayarlar → Gelişmiş → Application ID alanına kendi uygulamanın kimliğini yazabilirsin.

## Geliştirme

Uygulama [Tauri v2](https://tauri.app) ile yazıldı: arka plan servisi Rust, arayüz Svelte.

```powershell
npm ci
npm run tauri dev      # geliştirme
npm run lint; npm test # arayüz kontrolleri
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --bundles nsis   # kurulum dosyası
```

Gereksinimler: Node 22, Rust (stable), Windows'ta Visual Studio Build Tools (C++). Yayın adımları (Actions → Release → Run workflow) için [RELEASING.md](RELEASING.md), yol haritası için [docs/ROADMAP.md](docs/ROADMAP.md).

Eski Python/tkinter sürümü (`app.py`, `core.py`) yeni sürüm yaygınlaşınca kaldırılacak.

## English

<details>
<summary>Read the English overview</summary>

**Plycast** shows your 3D printer's live progress on your Discord profile: printer model and photo, progress, a status badge, Discord's own countdown for the remaining time, and optionally the file name, layer and temperatures. The card disappears when the printer is idle. The app runs quietly in the system tray.

**Install:** download `Plycast_x.y.z_x64-setup.exe` from the [latest release](https://github.com/badblli/plycast/releases/latest) (website: [plycast.badblli.dev](https://plycast.badblli.dev)). No admin rights are needed. Windows may show an "Unknown publisher" warning because the installer is not code-signed yet. The setup wizard adds your printer (IP, serial number and LAN access code from the printer's screen). No Discord developer account, bot or token is required.

**Printers:** Bambu Lab printers over the local network today; Klipper/Moonraker, OctoPrint and PrusaLink are planned.

**Privacy:** read-only, no printer commands, no Bambu cloud login. TLS is verified against Bambu's CA. The LAN access code is stored in Windows Credential Manager. Only what you see on the card is sent to your local Discord client. Updates are signed and fetched from GitHub Releases.

**Development:** Tauri v2 (Rust + Svelte). See the commands above, [RELEASING.md](RELEASING.md) (releases ship from Actions → Release → Run workflow) and [docs/ROADMAP.md](docs/ROADMAP.md).

</details>

## Lisans ve kaynaklar

MIT lisansı. Paketteki `bambu.cert`, [HA-Bambulab](https://github.com/greghesp/ha-bambulab) projesinin dağıttığı Bambu CA sertifikasıdır. Yazıcı görselleri [BambuStudio](https://github.com/bambulab/BambuStudio) deposundan bağlantıyla gösterilir ve sahiplerine aittir. Üçüncü taraf bağımlılıklar kendi lisanslarına tabidir.
