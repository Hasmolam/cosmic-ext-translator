# cosmic-ext-lingua

**COSMIC Masaüstü Ortamı için Gerçek Zamanlı Ekran ve Oyun Çevirmeni**  
*(Real-time Wayland Screen & Game Subtitle Translator for COSMIC Desktop)*

`cosmic-ext-lingua`, tam ekran oyunlarda, görsel romanlarda (Visual Novel), RPG diyaloglarında veya yabancı dildeki uygulamalarda ekrandaki metinleri gerçek zamanlı yakalayıp çevirerek oyun/uygulama üzerinde tıklamaları engellemeyen (click-through) şeffaf bir altyazı katmanı (HUD) olarak gösteren yerel bir COSMIC panel eklentisidir.

---

## Ne İşe Yarar ve Nasıl Çalışır?

Wayland üzerinde geleneksel X11 ekran yakalama ve pencere üstü çizim yöntemleri çalışmaz veya oyun performansını düşürür. `cosmic-ext-lingua`, doğrudan COSMIC ve Wayland protokollerini kullanarak sıfır odak kaybıyla çalışır:

1. **Kesintisiz Ekran Yakalama (`ashpd` + `PipeWire`):** XDG Desktop Portal Screencast API'si ve `restore_token` desteğiyle ekran izinlerini hatırlar, yalnızca seçili alt bölgeyi (ROI) kırpar.
2. **Akıllı Kare Filtreleme (`dHash FrameDebouncer`):** Oyunlarda harf harf akan (typewriter efekti) diyalogların tamamlanmasını bekler; görüntü sabitlenmeden OCR çalıştırmayarak CPU/GPU tüketimini minimumda tutar.
3. **Saf Rust Yerel OCR (`ocrs` + `rten`):** Harici C++ bağımlılığı olmadan (Tesseract gerektirmez) yerel sinir ağı modelleriyle ekrandaki metni okur.
4. **Sözlük Destekli Çeviri (`Google Translate` & `Ollama`):** Özel oyun terimlerini (`Glossary`) koruyarak hızlı bulut çevirisi veya tamamen çevrimdışı yerel LLM (`Ollama`) ile bağlama duyarlı çeviri yapar ve LRU önbellekte tutar.
5. **Click-Through Layer-Shell HUD (`wlr-layer-shell`):** Çevrilen altyazıyı tam ekran oyunların üzerinde gösteren ama fare tıklamalarını doğrudan arkadaki oyuna geçiren (`wl_region` boş input maskesi) özel Wayland katmanı.

---

## Mimari Yapı (Workspace Crates)

| Crate | Rolü |
| :--- | :--- |
| `crates/translator-core` | PipeWire yakalama, ROI kırpma, dHash debouncer, `ocrs` OCR motoru, Google/Ollama çeviri motorları, LRU önbellek, XDG Global Shortcuts ve Unix Domain Socket IPC protokolü. |
| `crates/cosmic-translator-overlay` | İki modlu Wayland `layer-shell` ikilisi: `--hud` (tıklama geçirgen altyazı katmanı) ve `--selector` (etkileşimli ROI alan seçici). |
| `crates/cosmic-applet-translator` | COSMIC Panel eklentisi (`com.system76.CosmicExtLingua`), hızlı durum açılır menüsü ve 4 sekmeli detaylı ayarlar penceresi. |

---

## Sistem Gereksinimleri

Derleme ve çalışma zamanı için aşağıdaki sistem paketlerinin kurulu olması gerekir (Ubuntu / Pop!_OS 24.04+):

```bash
sudo apt install build-essential pkg-config clang libclang-dev \
    libpipewire-0.3-dev libspa-0.2-dev libwayland-dev libxkbcommon-dev \
    libdbus-1-dev just
```

- **Masaüstü Ortamı:** COSMIC Desktop (`cosmic-comp` ve `xdg-desktop-portal-cosmic`)
- **Opsiyonel (Yerel LLM Çevirisi için):** [Ollama](https://ollama.com/) (`ollama pull qwen2.5:3b`)

---

## Derleme ve Kurulum

Proje kök dizininde `just` komutlarını kullanarak derleyebilir ve sisteme kurabilirsiniz:

```bash
# Tüm çalışma alanını kontrol et ve testleri çalıştır
just check
just test

# Release modunda her iki ikiliyi (applet + overlay) derle
just build-release

# Sisteme kur (/usr/bin altına ikilileri ve .desktop / metainfo / ikon dosyalarını yükler)
sudo just install
```

Geliştirme sırasında doğrudan çalıştırmak için:

```bash
just run-applet
```

Kurulumu kaldırmak için:

```bash
sudo just uninstall
```

---

## Küresel Kısayollar (Global Shortcuts)

XDG Global Shortcuts portalı üzerinden oyun içindeyken pencere değiştirmeden kullanabileceğiniz varsayılan kısayollar:

| Kısayol | İşlev |
| :--- | :--- |
| `Super + Alt + S` | Ekranda çevrilecek özel alanı (ROI) fare ile seç (`--selector`) |
| `Super + Alt + T` | Gerçek zamanlı ekran çevirisini başlat / duraklat |
| `Super + Alt + H` | Altyazı HUD katmanını gizle / göster |

---

## Lisans

Bu proje **GPL-3.0-or-later** lisansı altında sunulmaktadır.
