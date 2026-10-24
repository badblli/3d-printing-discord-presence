# d1-03-bambu — Bambu Lab LAN adaptörü (Rust)

- Summary: Bambu Lab yazıcılarının (A1, A1 mini, P1, X1, H2D) baskı durumunu yerel ağdan MQTT ile okuyan Rust adaptörü.
- Engine: codex
- Model: deep
- Phase: D1
- Branch: `codex/d1-03-bambu` (base: `main`)
- Depends on: d1-01-skeleton (lead)
- Role: implementer

## Goal

Python `core.py`'deki Bambu bağlantısını ve telemetri birleştirmesini Rust'a taşı; `PrinterSource`
trait'ini uygulayan, yalnızca okuyan, TLS doğrulamasını kapatmayan bir adaptör.

## Scope

- In: `src-tauri/src/printers/bambu/**`, `src-tauri/certs/` (yalnızca okuma; sertifika zaten orada),
  `src-tauri/examples/bambu_probe.rs`.
- Out: `model.rs`, `printers/mod.rs` (trait; değişiklik gerekiyorsa raporda iste), keşif (SSDP, D2), ayarlar, arayüz.

## Tasks

1. `bambu/telemetry.rs`: `Telemetry` birleştirici — A1 delta raporlarını biriktirir; `subtask_id`/`task_id`/`subtask_name`
   değişince veya `FINISH|FAILED|IDLE` → `RUNNING|PREPARE` geçişinde sıfırlar; `mc_remaining_time` (dakika) ve
   `mc_percent`, `layer_num`, `total_layer_num`, `nozzle_temper`, `bed_temper`, `gcode_state`, `subtask_name`/`gcode_file` alanlarını
   okur; sayıları sınırla doğrular (Python `number()`); 1 MB'tan büyük yükleri yok sayar. → `PrintStatus` üretir
   (`gcode_state` eşlemesi: RUNNING→Printing, PREPARE/SLICING→Preparing, PAUSE→Paused, FINISH→Finished, FAILED→Failed, IDLE→Idle).
   90 sn'den eski veri → `None`.
2. `bambu/tls.rs`: rustls `ClientConfig` — yalnızca gömülü `bambu.cert` CA'sına güvenir; zincir ve geçerlilik
   doğrulanır. Yazıcı sertifikası IP'yi değil cihazı tanımladığından ana makine adı denetimi yerine sertifika
   CN/SAN'ın **seri numarasıyla eşleşmesi** kontrol edilsin; eşleşme bilgisi bulunamazsa yalnızca ad denetimi atlanır,
   zincir doğrulaması **asla** kapatılmaz. TLS 1.2. Bambu sertifikaları katı X.509 kurallarına uymayabilir (Python'da
   `VERIFY_X509_STRICT` kapatılmıştı): bunu webpki ile nasıl karşıladığını raporda açıkla.
3. `bambu/mod.rs`: `BambuSource { host, serial, access_code, model: Option<String> }` → `PrinterSource`.
   rumqttc ile `host:8883`, kullanıcı `bblp`, parola erişim kodu; `device/{serial}/report` abone ol;
   abonelikten sonra ve bağlıyken 60 sn'de bir `device/{serial}/request`'e yalnızca `{"pushing":{"sequence_id":"0","command":"pushall"}}`.
   Başka hiçbir komut yayınlanmaz (testle kanıtla: yayınlanan konu/yük tek fonksiyondan geçsin).
   Yeniden bağlanma 2→30 sn geri çekilme. `SourceEvent::Connection` ile Türkçe/İngilizce bağımsız hata *kodları* gönder
   (`AuthRejected`, `Unreachable`, `SubscribeDenied`, `Stale`) — mesaj metni arayüzün işi.
   Hata/log içinde erişim kodu, seri numarası veya IP **yazdırılmaz**.
4. `examples/bambu_probe.rs`: `BAMBU_HOST`, `BAMBU_SERIAL`, `BAMBU_CODE` ortam değişkenleriyle bağlanıp 2 dakika boyunca
   gelen `PrintStatus`'u yazdırır (kimlik bilgilerini yazdırmadan). Lider gerçek A1 Combo ile bunu çalıştıracak.
5. Testler: `test_bridge.py`'deki Telemetry testlerinin karşılıkları + kayıtlı örnek JSON yükleri (sahte seri `00M00A000000000`)
   ile en az 12 test; TLS yapılandırmasının gömülü CA'yı yüklediğine dair test; "pushall dışında yayın yok" testi.

## Acceptance criteria

- [ ] `cargo test --offline --manifest-path src-tauri/Cargo.toml bambu` geçer; en az 14 test.
- [ ] `cargo clippy --offline --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` temiz.
- [ ] `cargo build --offline --manifest-path src-tauri/Cargo.toml --example bambu_probe` derlenir.
- [ ] TLS doğrulaması kapatılmamış (`dangerous()` yalnızca ad denetimini özel doğrulayıcıyla değiştirmek için, zincir doğrulaması webpki'ye devredilmiş).
- [ ] Gerçek kimlik bilgisi, IP veya seri numarası hiçbir dosyada yok.

## Notes

- Kaynak: `core.py` (`Telemetry`, `Bridge`), `test_bridge.py`, sertifika `src-tauri/certs/bambu.cert` (HA-Bambulab).
- Bambu'nun 2025 "Authorization Control" firmware'i bazı modellerde üçüncü taraf MQTT için LAN Only + Developer Mode isteyebilir; bağlantı reddinde `AuthRejected` yeterli, kullanıcı yönlendirmesi arayüzde.
