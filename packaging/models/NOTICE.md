# PaddleOCR model files

The files in the `models-v1` release are PaddleOCR PP-OCRv5 mobile models in ONNX format.

| File | Upstream name |
| :--- | :--- |
| `ppocrv5-mobile-det.onnx` | `PP-OCRv5_mobile_det_infer.onnx` |
| `ppocrv5-mobile-rec.onnx` | `PP-OCRv5_mobile_rec_infer.onnx` |
| `ppocrv5-dict.txt` | `ppocrv5_dict.txt` |

- Models and dictionary: PaddleOCR, Copyright (c) PaddlePaddle Authors,
  <https://github.com/PaddlePaddle/PaddleOCR>.
- ONNX conversion: `ppu-paddle-ocr-models`,
  <https://huggingface.co/snowfluke/ppu-paddle-ocr-models>.

Both are licensed under the Apache License, Version 2.0:
<https://www.apache.org/licenses/LICENSE-2.0>. The files are redistributed unmodified
apart from the file names. Checksums are listed in `SHA256SUMS`; the applet verifies
them after download.

These files are not covered by the GPL-3.0-or-later licence of cosmic-ext-translator.
