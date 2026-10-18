# Production Readiness Gaps & Resolution Status (cosmic-ext-lingua)

Bu doküman, `cosmic-ext-lingua` projesinin deneysel durumdan kararlı son kullanıcı (production) dağıtımına geçiş sürecinde tespit edilen teknik eksikleri, kök nedenlerini ve uygulanan çözümleri belgeler.

---

## 1. Görüntü Yakalama ve Koordinat Katmanı (Capture & Geometry)

### 1.1. HiDPI ve Fractional Scaling Koordinat Uyuşmazlığı [TAMAMLANDI]
- **Kök Neden:** `cosmic-translator-overlay --selector`, Wayland mantıksal piksel koordinatları (logical surface coordinates) üretirken, PipeWire akışı monitörün gerçek fiziksel piksel çözünürlüğündedir.
- **Uygulanan Çözüm:**
  - `selector.rs` seçici yüzeyinin `surface_width` ve `surface_height` değerlerini `ROI_SELECTED` JSON çıktısına ekledi.
  - `translator_core::capture::roi::Roi::scale_to_frame(surface_w, surface_h, frame_w, frame_h)` metodu geliştirildi. Mantıksal koordinatlar PipeWire fiziksel tampon çözünürlüğüne orantılanarak ölçeklendi.
  - %125, %150 ve %200 HiDPI/fractional scaling altında piksel doğruluğu birim testlerle (`test_roi_scale_to_frame_hidpi`) garanti altına alındı.

### 1.2. Screencast Restore Token Hata Kurtarma (Fallback) Mekanizması [TAMAMLANDI]
- **Kök Neden:** Monitör topolojisi değiştiğinde veya kompozitör oturumu yenilendiğinde kayıtlı `restore_token` geçersizleşip yakalamanın doğrudan çökmesine neden oluyordu.
- **Uygulanan Çözüm:**
  - `crates/translator-core/src/capture/screencast.rs` içindeki `create_stream` metodu iki aşamalı hale getirildi.
  - Kayıtlı token ile yapılan oturum isteği hata verdiğinde uyarı loglanarak sistem otomatik olarak `try_create_stream(None, persist)` çağrısı ile interaktif portal seçicisine düşmektedir.

### 1.3. PipeWire Format Kapsamı [TAMAMLANDI]
- **Uygulanan Çözüm:** `pipewire_thread.rs` format müzakeresinde `BGRx`, `BGRA`, `RGBx`, `RGBA` raw video formatları ve özel satır hizalama adımları (padded stride) desteklenerek test edildi.

---

## 2. OCR ve Görüntü İşleme Hattı (Pipeline & Engine)

### 2.1. CJK (Japonca, Çince, Korece) Dil Desteği [TAMAMLANDI]
- **Kök Neden:** Saf Rust `ocrs` motoru yalnızca Latin karakter seti içerdiğinden JRPG ve görsel romanların birincil kaynak dillerini okuyamıyordu.
- **Uygulanan Çözüm:**
  - `crates/translator-core/src/ocr/mod.rs` içerisine `TesseractBackend` eklendi.
  - `create_ocr_backend_for_lang(src_lang)` fabrikası kuruldu. Kaynak dil Japonca (`ja`/`jpn`), Çince (`zh`/`chi`) veya Korece (`ko`/`kor`) seçildiğinde ve sistemde Tesseract kurulu olduğunda otomatik olarak CJK backend'ine geçiş yapılması sağlandı.

### 2.2. Katı Otsu Binarizasyonunun OCR Doğruluğuna Etkisi [TAMAMLANDI]
- **Kök Neden:** Tüm görüntüler katı şekilde 1-bit siyah/beyaz eşiklemeye tabi tutulduğunda derin öğrenme OCR modellerinin kenar yumuşatma (anti-aliasing) verisi kayboluyordu.
- **Uygulanan Çözüm:**
  - `PreprocessConfig` ve `Config` içerisine `binarize: bool` konfigürasyonu eklendi; varsayılan değer `false` yapılarak anti-aliased gri tonlama korundu.
  - Ayarlar penceresine kullanıcının dilerse 1-bit eşiklemeyi açabileceği toggler eklendi.

### 2.3. Sabit Görüntüde Tekil Çeviri Tetikleme (One-Shot Trigger) [TAMAMLANDI]
- **Kök Neden:** `FrameDebouncer` sadece pHash değişimi (animasyon) algılandığında tetiklendiği için ekranda zaten duran statik metinler çevrilmiyordu.
- **Uygulanan Çözüm:**
  - `GlobalShortcutsManager`'a `ShortcutAction::TranslateCurrentScreen` (`Super+Alt+C`) eklendi.
  - Applet popup menüsüne "Şu Anki Ekranı Çevir" (`Message::OneShotTranslate`) butonu eklendi.
  - Debouncer beklenmeden en güncel PipeWire karesi doğrudan OCR ve çeviri hattına aktarılarak altyazı katmanına basıldı.

---

## 3. Katman Arayüzü (HUD) ve Süreç Yaşam Döngüsü

### 3.1. Öksüz Süreç (Orphan/Zombie Process) ve IPC Temizliği [TAMAMLANDI]
- **Kök Neden:** Applet kapatıldığında arka plandaki `cosmic-translator-overlay --hud` süreci asılı kalıyordu.
- **Uygulanan Çözüm:**
  - IPC protokolüne `HudMessage::Quit` mesajı eklendi.
  - HUD tarafında `HudMessage::Quit` alındığında `destroy_layer_surface` çağrılıp soket dosyası temizlenerek süreç kontrollü şekilde `exit(0)` ile sonlandırıldı.

### 3.2. Dinamik Metin Boyutu, Metin Sarma ve Dikey Konumlandırma [TAMAMLANDI]
- **Kök Neden:** HUD yüzey boyutu `110px` sabitti ve altyazı ekranın sadece altına çakılıydı.
- **Uygulanan Çözüm:**
  - `hud.rs` içindeki altyazı bileşenine `.wrapping(Wrapping::Word)` ve `max_width(1200.0)` eklenerek uzun metinlerin alt satırlara sarılması sağlandı; yüzey yüksekliği `180px`'e genişletildi.
  - `HudPosition` (`Top`, `Bottom`) enum'ı ve IPC `HudMessage::SetPosition` desteği eklendi.
  - Ayarlar penceresine altyazıyı ekranın altına veya üstüne sabitleme seçeneği yerleştirildi.

---

## 4. Çeviri Motorları ve Dayanıklılık

### 4.1. Hız Sınırı (Rate Limit) ve Hata Görselleştirme [TAMAMLANDI]
- **Kök Neden:** Google Translate web ucu 429 döndürdüğünde veya bağlantı koptuğunda hata sessizce konsola loglanıyor, kullanıcı sistemin donduğunu düşünüyordu.
- **Uygulanan Çözüm:**
  - `crates/translator-core/src/orchestrator/mod.rs` içinde çeviri hatası oluştuğunda HUD'a `⚠️ Hız Sınırı Aşıldı (HTTP 429 Rate Limit)` veya `⚠️ Çeviri Hatası (Bağlantı/Servis)` bildirimi basılması sağlandı.

### 4.2. Ek Çeviri Sağlayıcıları (DeepL & OpenAI/LLM) [TAMAMLANDI]
- **Uygulanan Çözüm:**
  - `DeepLTranslationBackend` (Free/Pro API desteği) geliştirildi.
  - `OpenAiTranslationBackend` (OpenAI, Groq, OpenRouter, yerel vLLM / LM Studio / LocalAI REST desteği) geliştirildi.
  - Ayarlar arayüzünde Google, DeepL, Ollama ve OpenAI motorları arasında dinamik geçiş ve API anahtarı alanları tanımlandı.

---

## 5. Paketleme, Dağıtım ve İlk Kurulum

### 5.1. Sistem Model Arama ve Çevrimdışı Yol [TAMAMLANDI]
- **Uygulanan Çözüm:**
  - `OcrsBackend::load_default_or_download()` öncelikle `/usr/share/cosmic-ext-lingua/models/` sistem dizinini kontrol eder.
  - Modeller sistem paketinde mevcutsa ağa çıkmadan doğrudan bu modeller yüklenir; eksikse `$XDG_DATA_HOME` altına indirilir.
