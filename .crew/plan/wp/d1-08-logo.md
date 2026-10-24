# d1-08-logo — Yeni logo fikirleri

- Summary: Uygulama için 6 farklı yeni logo konsepti; kullanıcı aralarından seçecek.
- Engine: codex
- Model: standard
- Skill: frontend-design
- Phase: D1
- Branch: `codex/d1-08-logo` (base: `origin/v2`)
- Depends on: none
- Role: implementer

## Goal

Ürün sahibi mevcut iki ikonu (`assets/app-icon-maker-pixel-v2.png` piksel robot maskot, `assets/app-icon-3d-printing-v1.png`
nozül + yeşil katmanlar) beğenmedi ve **yeni fikirler** istiyor. Birbirinden gerçekten farklı 6 konsept üret. Use your frontend-design skill.

## Scope

- In: yalnızca yeni klasör `assets/concepts/` (görseller + `README.md`).
- Out: diğer tüm dosyalar; mevcut ikonları silme veya değiştirme.

## Tasks

1. 6 konsept, her biri ayrı bir fikir (aynı fikrin renk varyasyonu değil). Yön örnekleri (bunlarla sınırlı değil):
   katman çizgilerinden oluşan monogram/harf işareti; ilerleme halkası + nozül; Discord'un "oynuyor" durum noktasını
   çağrıştıran bir baskı ucu; tek çizgiyle çizilmiş filament yolu; izometrik voxel; sevimli ama olgun bir maskot.
2. Her konsept için **1024×1024 PNG** (`assets/concepts/<n>-<kısa-ad>.png`). Kare, kenara taşmayan, dairesel kırpmada da çalışan
   %12 güvenli alan; 32 px'te okunabilir; metin/harf yazısı, Discord logosu, yazıcı markası logosu **yok**.
   Görsel üretim aracın (imagegen) varsa onu kullan; yoksa elle hazırlanmış SVG'den PNG'ye dönüştür (SVG'yi de ekle).
3. Her konseptin 32 px ve 128 px küçültülmüş hali (`<n>-<kısa-ad>-32.png`, `-128.png`) — küçük boyutta okunabilirliği göstermek için.
4. `assets/concepts/README.md`: her konsept için bir cümle fikir, renk paleti (hex) ve kullanılan üretim komutu/istemi.
   Koyu (#0d1117) ve açık (#ffffff) zeminde nasıl durduğuna dair kısa not.

## Acceptance criteria

- [ ] `assets/concepts/` altında 6 konsept × (1024, 128, 32) PNG ve README.
- [ ] Hiçbir konseptte metin veya marka logosu yok.
- [ ] Diff yalnızca `assets/concepts/**`.

## Notes

- Uygulamanın kimliği: global, birçok marka yazıcı (Bambu, Klipper, OctoPrint, Prusa), Discord profil kartı.
  Mevcut arayüz paleti: zemin #0d1117, vurgu #52dda0 — yeni logo bu paleti değiştirebilir, öner.
