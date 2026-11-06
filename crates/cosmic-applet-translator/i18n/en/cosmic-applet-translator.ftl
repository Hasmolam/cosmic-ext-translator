app-title = Screen Translator
app-comment = Real-time Screen and Game Translator for COSMIC
app-keywords = translate;translator;screen;game;cosmic;ocr;
about = About
status = Status
active = Active
paused = Paused
enable-translation = Enable Translation
preset = Preset
active-region = Active Region
custom-roi-unset = No Custom Area Selected
select-roi = Select Capture Area
select-target = Select Window / Screen
settings = Settings
translate-now = Translate Screen Now
profile = Game Profile
example-row = Enable Translation
region = Region

# Tabs
tab-capture = Capture & Area
tab-engine = Translation Engine
tab-display = Subtitle HUD
tab-glossary = Game Glossary
close-window = Close Window

# Presets
preset-bottom25-full = Bottom 25% (RPG)
preset-bottom33-full = Bottom 33% (VN)
preset-fullscreen-full = Full Screen
preset-custom-full = Custom Area
preset-bottom25-short = Bottom 25%
preset-bottom33-short = Bottom 33%
preset-fullscreen-short = Full Screen

# Capture Settings
section-capture-title = Dialogue and Capture Area
preset-dialog-template = Dialogue Template
custom-coords = Custom Coordinates
draw-new-area = Draw New Area
no-custom-area = No custom area defined
otsu-binarization = Otsu Binarization (1-bit)
blur-subtraction = High-Pass Blur Subtraction
settle-threshold = Settle Duration Threshold

# Engine Settings
engine-selection = Translation Engine Selection
active-engine = Active Engine
target-language = Target Language
source-language = Source Language
engine-info = Engine Info
engine-info-google = Google Translate active. Fast, keyless, and requires no local model downloads.
deepl-api-key = DeepL API Key
api-endpoint = API Endpoint
model-name = Model Name
api-key = API Key
ollama-endpoint = Ollama Endpoint
ollama-model = Ollama Model
connection-test = Connection Test
test-button = Test Connection
testing-connection = Testing connection...
status-unknown = Status unknown
connection-success = Connection successful ({ $count } models installed)
connection-error = Error: { $error }

# Display & HUD
hud-layer-title = Subtitle HUD Layer
hud-position = Subtitle Position
hud-vertical-offset = Vertical Offset
font-size = Font Size
background-opacity = Background Opacity
hud-persistent = Keep Subtitle on Screen (Persistent)
hud-auto-hide-time = Auto-Hide Duration
interactive-positioning = Interactive HUD Positioning
positioning-button = Calibrate Screen Position
live-preview = Live Preview
show-test-hud-message = Show Test Message on HUD
test-subtitle-content = Subtitle Overlay Active
pos-bottom = Screen Bottom
pos-top = Screen Top
pos-center = Screen Center

# Glossary
glossary-title = In-Game Terminology Glossary
glossary-source-placeholder = Original Term (e.g. Potion)
glossary-target-placeholder = Translation (e.g. Elixir)
add-button = Add
delete-button = Delete
add-new-term = Add New Term
saved-terms = Saved Terms:
no-saved-terms = No glossary terms defined yet.
duration-seconds = { $sec } s

# Live Inspection Dashboard
live-inspection-title = Live Capture Area & Inspection
preview-mode = Preview Mode
raw-crop = Raw Cropped
ocr-binarize = OCR Binarization
last-detected-ocr = Last Detected Text (OCR)
translation-output = Translation Output
prompt-select-target = Select a window or start capture for live image inspection

# History & Ignore Area
tab-history = Dialogue History
ignore-roi = Ignore ROI (Bust/Portrait)
no-ignore-area = No ignore area defined
draw-ignore-area = Draw Ignore Area
clear-ignore-area = Clear
history-search-placeholder = Search dialogue history...
clear-history = Clear History
copy = Copy
copied = Copied
no-history-entries = No dialogue history recorded yet

ocr-status = OCR
ocr-loading = Loading text recognition engine...
ocr-unavailable = Text recognition unavailable
deepl-api-key-placeholder = DeepL API key (free :fx or pro)
lang-auto = Automatic (Latin script)
lang-en = English
lang-ja = Japanese (requires Tesseract)
lang-zh = Chinese, Simplified (requires Tesseract)
lang-ko = Korean (requires Tesseract)
lang-de = German
lang-fr = French
lang-es = Spanish
lang-it = Italian
lang-pt = Portuguese
lang-tr = Turkish
ocr-engine = OCR engine
ocr-engine-auto = Automatic (PaddleOCR, then Tesseract)
ocr-engine-paddle = PaddleOCR (downloads about 21 MB on first use)
ocr-engine-tesseract = Tesseract (system package)
text-direction = Text direction
text-direction-auto = Automatic
text-direction-horizontal = Horizontal
text-direction-vertical = Vertical (top to bottom, right to left)
