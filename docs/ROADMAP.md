# Yol haritası

Hedef: herhangi bir 3D yazıcının baskı ilerlemesini Discord profilinde gösteren, Windows/macOS/Linux'ta
tepside sessizce çalışan, kurulumu "indir → yazıcını seç → bitti" kadar kolay bir uygulama.

## Temel kararlar

| Konu | Karar | Neden |
| --- | --- | --- |
| Uygulama çatısı | **Tauri v2** (Rust çekirdek + web arayüz) | Sürekli açık kalan tepsi uygulaması için hafif: ~10 MB kurulum, düşük RAM. Tek kod tabanı ile üç işletim sistemi. |
| Discord bağlantısı | **Rich Presence (yerel IPC)**, bot değil | Profil durumunu yalnızca kullanıcının kendi bilgisayarındaki Discord istemcisi ayarlayabilir. Bot sunucu kanallarına yazar, profil durumu değiştiremez. |
| Application ID | **Uygulamayla birlikte gelen tek ortak ID** | Application ID gizli değildir (OAuth "client id"). Spotify, PreMiD vb. de kendi ID'lerini uygulamaya gömer. Kullanıcı Developer Portal'ı hiç görmez. Gelişmiş ayarlarda "kendi ID'ni kullan" seçeneği kalır. |
| Yazıcı desteği | **Adaptör katmanı** | Her marka/protokol ortak bir `PrintStatus` modeline çevrilir; Discord tarafı markayı bilmez. |
| Gizli bilgiler | **İşletim sistemi anahtarlığı** (`keyring` crate) | Windows Credential Manager, macOS Keychain, Linux Secret Service. DPAPI'nin çapraz platform karşılığı. |
| Ağ | **Yalnızca yerel ağ, yalnızca okuma** | Yazıcıya başlat/durdur komutu gönderilmez; bulut hesabına giriş yok (bulut modu ileride, isteğe bağlı). |

### Ortak Application ID nasıl çalışır?

1. Proje sahibi Developer Portal'da **bir kez** "3D Printing" adlı bir uygulama oluşturur.
2. **Rich Presence → Art Assets** bölümüne marka/model görselleri yüklenir (`bambu_a1`, `klipper`,
   `octoprint`, `prusa_mk4`, …). Uygulama bu anahtar adlarıyla görsel seçer.
3. ID uygulamanın koduna sabit olarak yazılır. Herkes aynı ID'yi kullanır; Discord profilinde
   "**3D Printing** oynuyor" görünür.
4. Bot token, client secret veya OAuth gerekmez; bunlar hiçbir zaman uygulamaya girmez.

Sınırlama: Rich Presence, Discord masaüstü uygulamasının **aynı bilgisayarda** açık olmasını
gerektirir. Bu yüzden servis kullanıcının bilgisayarında çalışır; Raspberry Pi gibi başka bir cihazdan
profil durumu ayarlanamaz.

## Mimari

```
┌──────────────── Tauri uygulaması (tek süreç, tepside) ────────────────┐
│  Web arayüz (kurulum sihirbazı, durum, ayarlar)                        │
│        ▲ Tauri komutları / olaylar                                     │
│  Rust çekirdek                                                         │
│   ├─ printers/  ── trait PrinterSource → PrintStatus                   │
│   │    ├─ bambu      LAN MQTT/TLS (8883), SSDP keşfi                   │
│   │    ├─ moonraker  WebSocket JSON-RPC (Klipper: Creality K1, Voron…) │
│   │    ├─ octoprint  REST + push API, API anahtarı                     │
│   │    └─ prusalink  REST, digest auth / API key                       │
│   ├─ presence/  ── PrintStatus → Discord etkinliği (şablonlar, hız sınırı)│
│   ├─ discovery/ ── SSDP + mDNS ile yazıcıları otomatik bulma            │
│   └─ config/    ── ayarlar (JSON) + gizli bilgiler (OS anahtarlığı)     │
└────────────────────────────────────────────────────────────────────────┘
```

`PrintStatus` (taslak): `state` (idle/preparing/printing/paused/finished/failed), `progress`,
`remaining_secs`, `file_name`, `printer_model`, `layer`/`total_layers`, `nozzle_temp`, `bed_temp`,
`updated_at`.

## Fazlar

### D0 · Kurulum
- Projeyi devlog'dan ayır, agent-crew'i kur. ✅

### D1 · Tauri iskeleti + çekirdek
- Tauri v2 + web arayüz iskeleti, tepsi simgesi, tek örnek kilidi, OS ile başlatma (autostart).
- `PrintStatus` modeli ve `PrinterSource` trait'i.
- Discord IPC istemcisi + presence motoru (mevcut `Telemetry.activity` mantığının Rust karşılığı).
- Bambu LAN adaptörü (mevcut `core.py` portu; Bambu CA doğrulaması korunur, ana makine adı denetimi
  özel doğrulayıcıyla atlanır, doğrulama asla kapatılmaz).
- Ayarlar + OS anahtarlığı.
- Python sürümü, Rust sürümü A1 Combo'da doğrulanınca `legacy/` altına alınır, sonra kaldırılır.

### D2 · Yazıcı adaptörleri
- Moonraker, OctoPrint, PrusaLink adaptörleri; her biri kayıtlı örnek yanıtlarla (fixture) test edilir.
- Otomatik keşif ve "Yazıcını seç" sihirbazı (IP/seri numarası elle girme zorunluluğu kalkar).
- Birden fazla yazıcı: aynı anda basan yazıcılardan biri (ya da en yakın biteni) gösterilir.

### D3 · Dağıtım + ek özellikler
- GitHub Actions ile Windows (MSI/NSIS), macOS (DMG, universal), Linux (AppImage/deb) derlemeleri.
- Tauri updater ile otomatik güncelleme; imzalama (Windows SmartScreen, macOS notarization).
- Durum metni şablonları ("{{file}} · %{{progress}} · {{remaining}} kaldı"), TR/EN arayüz.
- Rich Presence düğmeleri (ör. MakerWorld/Printables model bağlantısı).
- **Baskı bitti bildirimi:** kullanıcının yapıştırdığı Discord webhook URL'sine mesaj (+ isteğe bağlı
  kamera kareleri). Bot ve sunucu barındırma gerektirmez.
- Bambu bulut modu (LAN'a erişimi olmayanlar için), isteğe bağlı ve açıkça onaylanır.

## Riskler / doğrulanacaklar

- Bambu'nun 2025 "Authorization Control" firmware'i: hangi modellerde yerel MQTT okuma için LAN Only /
  Developer Mode gerektiği model bazında doğrulanmalı; arayüz kullanıcıya doğru yönergeyi göstermeli.
- Discord IPC: Flatpak/Snap Discord'da soket yolu farklı (`app/com.discordapp.Discord/`); Linux'ta
  bilinen yollar taranmalı.
- macOS/Windows imzalama sertifikaları ücretli; ilk sürümlerde imzasız derleme + açıklama.
