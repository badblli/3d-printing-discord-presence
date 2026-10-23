# 3D Printing

Show your Bambu Lab A1 Combo print progress as Discord Rich Presence on Windows. The app reads printer telemetry over your local network. It does not sign in to your Bambu account or send printer control commands.

![3D Printing app icon](assets/app-icon-3d-printing-v1.png)

## Features

- Shows the current print, progress and estimated time in Discord.
- Hides the activity when no active print is detected.
- Keeps the LAN access code encrypted with Windows DPAPI.
- Runs in the notification area when the window is closed.
- Uses Discord's local desktop RPC connection; no Discord bot or token is needed.

## Setup

1. Download and open `3DPrinting.exe` from Releases, or follow the developer steps below to run from source.
2. Enter the printer IP address, printer serial number (not the AMS serial), LAN access code and Discord Application ID.
3. In Discord, enable activity sharing under **User Settings → Activity Privacy**.
4. Press **Save and start**. The computer and printer must be on the same local network, and the Discord desktop app must be running.

Create a Discord application in the [Discord Developer Portal](https://discord.com/developers/applications), then copy **General Information → Application ID**. You do not need to create a bot. Set the application's name and upload its image in the portal if you want to customize the Discord profile card; the included icon is used by the Windows app.

The A1's IP, serial number and LAN access code are available in the printer's network and device information settings. The exact menu names may vary by firmware. Never share your LAN access code. Enabling LAN Only can disable Bambu Handy and cloud access; this app does not change the printer's network mode.

## Build from source

Windows and Python 3.12:

```powershell
python -m venv .venv
.venv/Scripts/python -m pip install -r requirements.txt
.venv/Scripts/python app.py
```

Build a standalone executable:

```powershell
.venv/Scripts/python -m pip install pyinstaller==6.16.0
.venv/Scripts/python -m PyInstaller --noconfirm --onefile --windowed --name 3DPrinting --icon assets/app-icon-3d-printing-v1.ico --add-data "bambu.cert;." --add-data "assets/app-icon-3d-printing-v1.png;assets" app.py
```

Start the saved connection automatically with `3DPrinting.exe --start`.

## Privacy and connection notes

The LAN access code is encrypted for the current Windows account in `%LOCALAPPDATA%/BambuDiscord/settings.json`. The printer IP, serial number and Discord Application ID are also stored locally. Discord receives only the print status, progress, time estimate and (when enabled) file name.

The app validates TLS using the bundled Bambu CA certificate. It requests a printer status snapshot once per minute and never sends start/stop commands. A network isolation setting, changed printer IP, unavailable LAN access or incompatible firmware can prevent connection. The Discord card uses Bambu Studio's public A1 preview image; it does not upload a camera feed or print file.

`bambu.cert` is distributed by the [HA-Bambulab project](https://github.com/greghesp/ha-bambulab/blob/main/custom_components/bambu_lab/pybambu/certs/bambu.cert). The implementation references its [MQTT client](https://github.com/greghesp/ha-bambulab/blob/main/custom_components/bambu_lab/pybambu/bambu_client.py) and [command definitions](https://github.com/greghesp/ha-bambulab/blob/main/custom_components/bambu_lab/pybambu/commands.py). Discord Rich Presence uses [pypresence](https://github.com/qwertyquerty/pypresence).

## Türkçe

Bambu Lab A1 Combo baskı ilerlemesini Windows'ta Discord Rich Presence olarak gösterir. Yazıcı telemetrisini yerel ağdan okur; Bambu hesabına giriş yapmaz ve yazıcıya kontrol komutu göndermez.

Kurulumda yazıcının IP adresini, yazıcının seri numarasını (AMS değil), LAN erişim kodunu ve Discord **Application ID** değerini girin. Bilgisayar, yazıcı ve açık Discord masaüstü uygulaması aynı ağda olmalı. Discord **Kullanıcı Ayarları → Etkinlik Gizliliği** bölümünde etkinlik paylaşımını açın.

Discord uygulamasının adını ve profil görselini Developer Portal'dan düzenleyebilirsiniz. Bu depodaki simge Windows uygulama penceresi ve `.exe` içindir. LAN erişim kodu Windows hesabınız için DPAPI ile şifrelenir. LAN Only modu Bambu Handy/bulut erişimini kapatabilir; uygulama yazıcı ayarını değiştirmez.

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE). Third-party dependencies and the Bambu CA certificate retain their respective licenses and ownership.
