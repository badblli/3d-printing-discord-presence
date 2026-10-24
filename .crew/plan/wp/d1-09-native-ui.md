# d1-09-native-ui — Gerçek bir masaüstü uygulaması görünümü + kart ayarları

- Summary: Windows'un başlık çubuğu kalkar, yerine uygulamanın kendi başlık çubuğu gelir; arayüz web sayfası değil gerçek bir uygulama gibi davranır; Discord kartı ayarları eklenir.
- Engine: codex
- Model: standard
- Skill: frontend-design
- Phase: D1
- Branch: `codex/d1-09-native-ui` (base: `origin/v2`)
- Depends on: d1-06-ui, d1-07-card
- Role: implementer

## Goal

Ürün sahibi ekran görüntüsünde Windows başlık çubuğunu ("3D Printing" yazısı ve sistem küçült/büyüt/kapat düğmeleri)
işaretledi: "Bunlar olmasın, gerçek bir uygulama gibi görünsün." Discord / Spotify / Linear masaüstü uygulamaları gibi:
pencere çerçevesi uygulamanın kendi tasarımının parçası. Use your frontend-design skill.

## Scope

- In: `src/**`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/**`, `src-tauri/src/lib.rs` (yalnızca pencere gösterimi için gerekirse).
- Out: `src-tauri/src/{presence,printers,config,runtime}/**`.

## Tasks

1. **Çerçevesiz pencere**: `decorations: false`, gölge açık (Windows 11 yuvarlak köşe ve gölge korunur), `resizable: false` (sabit boyutlu araç penceresi; 760×640 civarı, içerik kaydırmasız).
   Üst kısım (mevcut logo + "3D Printing" + Durum/Ayarlar sekmeleri) sürüklenebilir başlık alanı olur (`data-tauri-drag-region`; düğmeler sürüklemeyi engellemez).
   Sağ üstte uygulamanın kendi tasarımında **küçült** ve **kapat** düğmeleri (ikon, hover durumları; kapat hover'da kırmızı değil, uygulamanın paletine uygun).
   Kapat = pencereyi tepsiye gizle (mevcut `CloseRequested` davranışı); küçült = görev çubuğuna. İzinleri capabilities'te yalnızca gerekenlere aç.
2. **Uygulama hissi**: arayüz metinlerinde seçim yok (`user-select: none`; form alanları hariç), varsayılan imleç, sağ tık tarayıcı menüsü kapalı,
   fareyle tıklamada odak çerçevesi yok ama klavyede (`:focus-visible`) var, sayfa kaydırma/lastik bant yok, ilk açılışta beyaz flaş yok
   (pencere içerik hazır olunca gösterilsin; gerekiyorsa `visible:false` + hazır olunca `show`).
   Alt bilgideki "Son bir adım: Discord" gibi kurulum sonrası anlamsız kalan metinleri kaldır ya da duruma göre göster.
3. **Discord önizlemesi gerçek kart gibi**: önizlemede `snapshot.activity.assets.large_image` (yazıcı fotoğrafı URL'si) ve
   `small_image` (durum rozeti) gösterilsin; kalan süre için `timestamps.end`'den geri sayım. CSP'de yalnızca
   `https://raw.githubusercontent.com` görselleri için `img-src` izni ekle. Profil avatarı yerine nötr bir avatar.
4. **Kart ayarları** (Ayarlar'da "Discord kartı" bölümü, d1-07 alanları): üst/alt satır şablonları (yer tutucu çipleriyle ekleme,
   canlı önizleme, "varsayılana dön"), durum rozeti aç/kapa, en fazla 2 bağlantı düğmesi (etiket + https URL; "düğmeleri sadece başkaları görür" notu).
   Tüm metinler TR/EN i18n'de.
5. Testler (vitest): şablon önizleme biçimlendirmesi, düğme doğrulaması, i18n anahtarlarının iki dilde eksiksizliği.

## Acceptance criteria

- [ ] `npm run lint` ve `npm test` temiz; `cargo clippy --offline ... -- -D warnings` temiz (Rust'a dokunulduysa).
- [ ] Windows'un başlık çubuğu yok; pencere başlık alanından sürüklenebiliyor; küçült/kapat çalışıyor (lider elle dener).
- [ ] Önizlemede yazıcı fotoğrafı ve durum rozeti görünüyor.
- [ ] Kart ayarları kaydedilince Discord kartı değişiyor (lider gerçek yazıcıyla dener).

## Notes

- Logo: `src/lib/assets/logo.png` (Katman adaları; palet #123D3A · #52DDA0 · #DDF8A8). Zemin #0d1117.
- API: `src/lib/api.ts` (d1-07 ile `DiscordConfig` genişledi: `details_template`, `state_template`, `show_status_badge`, `buttons`).
- Sandbox'ta ağ yok, yeni npm paketi ekleme. `node_modules`'ı ana klasörden **kopyala, taşıma** (önceki seferde ana klasörün `node_modules`'ı boşaldı).
