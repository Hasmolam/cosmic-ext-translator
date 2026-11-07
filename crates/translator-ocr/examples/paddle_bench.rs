//! Runs PaddleOCR models over images and prints text and timing.
//!
//! Usage: `paddle_bench <det.onnx> <rec.onnx> <dict.txt> <image>...`

use std::path::Path;
use std::time::Instant;

use translator_ocr::paddle::PaddleOcrBackend;
use translator_ocr::OcrBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [det, rec, dict, images @ ..] = args.as_slice() else {
        anyhow::bail!("usage: paddle_bench <det> <rec> <dict> <image>...");
    };
    let start = Instant::now();
    let backend = PaddleOcrBackend::load_blocking(Path::new(det), Path::new(rec), Path::new(dict))?;
    println!("load: {:?}", start.elapsed());
    for path in images {
        let image = image::open(path)?;
        // Warm-up run, then timed runs.
        backend.recognize_detailed(&image, None).await?;
        let runs = 5;
        let t = Instant::now();
        let mut result = None;
        for _ in 0..runs {
            result = Some(backend.recognize_detailed(&image, None).await?);
        }
        let per_run = t.elapsed() / runs;
        if let Some(r) = result {
            println!(
                "{path}: {per_run:?} {:?} boxes={} text={}",
                r.orientation,
                r.boxes.len(),
                r.text
            );
        }
    }
    Ok(())
}
