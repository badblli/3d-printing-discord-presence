<div align="center">

# 3D Printing

### Baskı ilerlemen Discord’da. Yazıcın kendi LAN’ında kalır.

**Bambu Lab A1 Combo için Windows Rich Presence uygulaması**

![Windows](https://img.shields.io/badge/platform-Windows-34435e?style=flat-square)
![A1 Combo](https://img.shields.io/badge/printer-A1%20Combo-52dda0?style=flat-square)
![MIT](https://img.shields.io/github/license/badblli/3d-printing-discord-presence?style=flat-square)

<img src="assets/app-icon-maker-pixel-v2.gif" alt="Göz kırpan piksel art maker maskotu" width="190">

</div>

## Ne işe yarar?

Baskı sürerken Discord profilinde dosya adını (isteğe bağlı), ilerleme yüzdesini ve kalan süre tahminini gösterir. Baskı aktif değilse durum kartı gizlenir.

Uygulama yazıcı durumunu yerel ağdan okur. Bambu hesabına giriş yapmaz ve yazıcıya başlatma/durdurma komutu göndermez.

## Kurulum

Windows bilgisayar, A1 Combo ve Discord masaüstü uygulaması aynı yerel ağda olmalı.

1. Uygulamayı açın ve bağlantı bilgilerini girin.
2. Discord’da **Kullanıcı Ayarları → Etkinlik Gizliliği** bölümünden etkinlik paylaşımını açın.
3. **Kaydet ve başlat**’a basın.

### Bağlantı bilgileri

| Alan | Nereden bulunur? |
| --- | --- |
| Yazıcı IP adresi | Yazıcı ekranı → **Ayarlar → WLAN / Ağ** |
| Yazıcı seri numarası | **Ayarlar → Cihaz bilgisi**. AMS seri numarası değil. |
| LAN erişim kodu | **Ayarlar → WLAN / Ağ → LAN erişim kodu**. Bambu hesabı parolanız değil. |
| Discord Application ID | [Developer Portal](https://discord.com/developers/applications) → uygulamanız → **General Information → Application ID** |

Yeni Discord uygulaması oluşturmanız gerekmez; Rich Presence için kullandığınız uygulamanın Application ID’si yeterlidir. Bot oluşturmayın ve Discord token’ı girmeyin.

Pencerenin **X** düğmesi uygulamayı bildirim alanına küçültür; RPC çalışmayı sürdürür. Tamamen çıkarsanız Discord durumu da kapanır. Uygulamayı yeniden açtığınızda ayarlarınız Windows hesabınız için şifrelenmiş olarak yüklenir.

## Görsel kimlik

Discord uygulama kartı için [animasyonlu PNG/GIF simgesi](assets/app-icon-maker-pixel-v2.gif), Windows uygulaması için statik [PNG](assets/app-icon-maker-pixel-v2.png) ve [ICO](assets/app-icon-maker-pixel-v2.ico) bulunur. GIF; 1024×1024, 3 kare ve 10 MB sınırının altındadır. Developer Portal’da **General Information → App Icon** alanına yükleyebilirsiniz.

## Gizlilik ve ağ

- LAN erişim kodu `%LOCALAPPDATA%/BambuDiscord/settings.json` içinde Windows DPAPI ile şifrelenir.
- Yazıcı IP’si, seri numarası ve Application ID yalnızca bu bilgisayarda tutulur.
- Discord’a baskı durumu, yüzde, süre tahmini ve dosya adı paylaşımı açıksa dosya adı gönderilir.
- TLS bağlantısı Bambu CA sertifikasını doğrular. Uygulama yazıcıyı kontrol etmez.
- Ağ yalıtımı, değişen IP adresi, kapalı LAN erişimi veya firmware farkları bağlantıyı engelleyebilir. **LAN Only** seçeneği Bambu Handy/bulut bağlantısını kapatabilir; uygulama bu modu değiştirmez.

Paket içindeki `bambu.cert`, [HA-Bambulab](https://github.com/greghesp/ha-bambulab) projesinin dağıttığı Bambu CA sertifikasıdır. Bağlantı ve komut uygulaması için kaynaklar README’nin İngilizce bölümünde listelenmiştir.

## Kaynaktan çalıştırma

Python 3.12 ve Windows:

```powershell
python -m venv .venv
.venv/Scripts/python -m pip install -r requirements.txt
.venv/Scripts/python app.py
```

Tek dosya EXE oluşturmak için:

```powershell
.venv/Scripts/python -m pip install pyinstaller==6.16.0
.venv/Scripts/python -m PyInstaller --noconfirm --onefile --windowed --name 3DPrinting --icon assets/app-icon-maker-pixel-v2.ico --add-data "bambu.cert;." --add-data "assets/app-icon-maker-pixel-v2.png;assets" app.py
```

## English

<details>
<summary>Read the English overview</summary>

Show Bambu Lab A1 Combo print progress as Discord Rich Presence on Windows. The app reads telemetry over your local network, does not sign in to Bambu Cloud, and sends no printer control commands.

Enter the printer IP, printer serial number (not the AMS serial), LAN access code and Discord Application ID. Enable activity sharing in Discord under **User Settings → Activity Privacy**, then press **Save and start**. You can reuse an existing Discord application; no bot or token is needed.

Closing the window minimizes the app to the notification area and keeps RPC running. Choosing **Quit completely** stops the Discord activity. The LAN code is encrypted with Windows DPAPI for the current Windows account. Only print status, progress, estimated time and (if enabled) the file name are sent to Discord.

The app requests a printer status snapshot once per minute and validates TLS with the bundled Bambu CA certificate. It never sends start/stop commands. Firmware settings and network isolation can affect local connectivity.

The bundled `bambu.cert` is distributed by [HA-Bambulab](https://github.com/greghesp/ha-bambulab/blob/main/custom_components/bambu_lab/pybambu/certs/bambu.cert). Implementation references: [MQTT client](https://github.com/greghesp/ha-bambulab/blob/main/custom_components/bambu_lab/pybambu/bambu_client.py), [command definitions](https://github.com/greghesp/ha-bambulab/blob/main/custom_components/bambu_lab/pybambu/commands.py), and [pypresence](https://github.com/qwertyquerty/pypresence).

This project is licensed under the MIT License. Third-party dependencies and the Bambu CA certificate retain their respective licenses and ownership.

</details>
