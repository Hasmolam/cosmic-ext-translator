# Otomatize Gerçek Hayat Test ve Doğrulama Planı

Bu doküman, `cosmic-ext-lingua` projesinin Wayland, PipeWire, OCR ve katman arayüzü (HUD) bileşenlerini insan müdahalesine gerek kalmadan, sürekli entegrasyon (CI) ve yerel geliştirme ortamlarında uçtan uca (E2E) doğrulamak için kurulacak otomatize test sisteminin mimarisini ve uygulama adımlarını tanımlar.

---

## 1. Test Mimarisi ve Bileşenler

```
+-----------------------------------------------------------------------------------+
|                            Otomatize Test Koşucusu                                |
|                                                                                   |
|  +--------------------+   +---------------------+   +--------------------------+  |
|  | 1. Headless         |   | 2. Sentetik Oyun    |   | 3. Wayland Doğrulayıcı   |  |
|  |    Compositor       |   |    Diyalog Üreteci  |   |    (SCTK / Virtual Ptr)  |  |
|  | (cosmic-comp/weston)|   | (Typewriter Emulatör|   | (Click-Through Testi)    |  |
|  +---------+----------+   +----------+----------+   +------------+-------------+  |
|            |                         |                           |                |
|            v                         v                           v                |
|     Wayland Socket           PipeWire Test Düğümü         IPC Soket Denetimi      |
|    (WAYLAND_DISPLAY)          (Virtual PW Source)      (HUD Durum Doğrulama)      |
|            |                         |                           |                |
|            +------------+------------+---------------------------+                |
|                         |                                                         |
|                         v                                                         |
|           [cosmic-applet-translator / cosmic-translator-overlay]                  |
+-----------------------------------------------------------------------------------+
```

---

## 2. Alt Sistemler ve Uygulama Adımları

### 2.1. Headless Wayland ve PipeWire Test Ortamı (Fixture)
Fiziksel ekran ve XDG screencast kullanıcı diyaloğu olmadan testlerin çalışabilmesi için sanal ortam sağlanır:

- **Araçlar:**
  - `cosmic-comp --headless` veya `weston --backend=headless`
  - PipeWire sanal video kaynağı (dummy driver) veya `spa-v4l2` sanal cihazı.
- **Hedef:** CI sunucusunda veya yerel terminalde fiziksel ekran kartı olmadan `WAYLAND_DISPLAY` ve `PIPEWIRE_REMOTE` soketlerini ayağa kaldırmak.

### 2.2. Sentetik Oyun ve Daktilo (Typewriter) Simülatörü
Oyunlardaki harf harf akan metinleri deterministik olarak taklit eden Rust tabanlı bir test ikilisi (`test-fixtures/game-simulator`):

- **İşleyiş:**
  1. Verilen bir cümle ("The ancient dragon awakened in the fiery depths.") font ve arka plan ile rasterize edilir.
  2. Her `N` milisaniyede (örneğin 30ms) metne bir harf eklenerek yeni bir kare üretilir.
  3. Üretilen kareler doğrudan bir PipeWire test akışına (veya `translator-core` frame kanalına) 60 FPS hızla aktarılır.
- **Doğrulama Kriterleri:**
  - Metin yazılırken debouncer'ın `Animating` durumunda kalması.
  - Metin bittiğinde `settle_duration` (250ms) sonra tek bir `SettledFrame` üretilmesi.
  - OCR motorunun 100 karenin aktığı bir diyalog satırında **yalnızca 1 kez** tetiklendiğinin doğrulanması.

### 2.3. Click-Through ve Layer-Shell Otomasyonu
HUD penceresinin şeffaflığını ve tıklama geçirgenliğini doğrulayan Wayland istemci testi:

- **İşleyiş:**
  1. Test koşucusu arka planda basit bir Wayland penceresi açar ve fare tıklama olaylarını dinler.
  2. `cosmic-translator-overlay --hud` başlatılır ve test penceresinin üzerine binen bir altyazı gösterilir.
  3. `zwp_virtual_pointer_v1` protokolü kullanılarak altyazı kutusunun tam ortasına yapay fare tıklaması gönderilir.
- **Doğrulama Kriterleri:**
  - Tıklamanın HUD tarafından yutulmadan doğrudan arkadaki test penceresine ulaştığının teyit edilmesi (`assert_eq!(target_window.clicks_received, 1)`).
  - Altyazı katmanının `zwlr_layer_surface_v1` üzerinde `Layer::Overlay` olarak konumlandığının kompozitör sorgusu ile doğrulanması.

### 2.4. IPC Protokol ve Yarış Durumu (Stress/Race Condition) Testi
HUD IPC soketine yönelik yük ve sıra testi:

- **İşleyiş:**
  - 1 saniye içinde 50 farklı `ShowSubtitle` mesajı gönderilir.
  - Altyazı süre sayaçları (timer) simüle edilir.
- **Doğrulama Kriterleri:**
  - `subtitle_seq` sayacının doğru ilerlediği, eski mesajların yeni gelen altyazıyı erken kapatmadığı.
  - Soketin çökmediği ve applet kapandığında soket dosyasının temizlendiği.

### 2.5. OCR ve Çeviri Doğruluk Kıyaslama (Benchmark) Paketi
Görüntü ön işleme ve OCR kalitesini regresyona karşı koruyan test kümesi:

- **Veri Seti:** 20 adet sentetik/gerçek oyun ekran görüntüsü (koyu arka plan, açık arka plan, yarı saydam diyalog kutusu, piksel font, gölgeli font).
- **Metrik:** Karakter Hata Oranı (CER) ve Kelime Hata Oranı (WER).
- **Doğrulama:** CI her çalıştığında `CER < %3` ve `WER < %5` eşiklerinin sağlandığının otomatik onaylanması.

---

## 3. Uygulama ve Entegrasyon Yol Haritası

| Aşama | Yapılacak İşlem | Çıktı / Dosya |
| :--- | :--- | :--- |
| **Aşama 1** | Sentetik kare üreteci ve debouncer simülasyon testi | `crates/translator-core/tests/e2e_debouncer_sim.rs` |
| **Aşama 2** | Mock PipeWire loopback ve yakalama entegrasyon testi | `crates/translator-core/tests/e2e_capture_pipeline.rs` |
| **Aşama 3** | Virtual pointer ile HUD click-through entegrasyon testi | `crates/cosmic-translator-overlay/tests/e2e_clickthrough.rs` |
| **Aşama 4** | OCR altın veri seti (golden dataset) kıyaslama testi | `crates/translator-core/tests/e2e_ocr_accuracy.rs` |
| **Aşama 5** | Test koşucu komutu ve CI entegrasyonu | `justfile` içine `test-e2e` tarifi |

---

## 4. Çalıştırma Komutları (Hedef)

Tüm otomatik test hattı tek bir komutla çalıştırılabilir olacaktır:

```sh
# Tüm birim ve entegrasyon testlerini çalıştır
just test

# Headless Wayland ortamında uçtan uca daktilo, click-through ve OCR testlerini çalıştır
just test-e2e
```
