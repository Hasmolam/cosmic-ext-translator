//! OCR model provisioning: locate, download, validate and atomically install model files.
//!
//! A model file is only placed at its final path after it has been fully written and
//! successfully parsed by `rten`. A file that exists but fails to load is treated as
//! corrupt, moved aside and replaced, so an interrupted download cannot leave the
//! backend permanently broken.
//!
//! `rten` selects the parser from the file extension (`.onnx` or `.rten`), so temporary
//! files keep the real extension: `text-detection.part.onnx`.

use std::path::{Path, PathBuf};

use rten::Model;
use tracing::{info, warn};

use crate::error::OcrInitError;

/// Read-only model directory populated by system packages.
pub const SYSTEM_MODELS_DIR: &str = "/usr/share/cosmic-ext-translator/models";

/// Per-user model directory: `$XDG_DATA_HOME/cosmic-ext-translator/models`, falling back
/// to `~/.local/share/...` and finally the system temporary directory.
#[must_use]
pub fn user_models_dir() -> PathBuf {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        PathBuf::from(data_home).join("cosmic-ext-translator/models")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".local/share/cosmic-ext-translator/models")
    } else {
        std::env::temp_dir().join("cosmic-ext-translator/models")
    }
}

/// Files smaller than this are rejected before attempting to parse them.
pub const MIN_MODEL_SIZE: u64 = 100_000;

/// A model file name paired with its download URL.
#[derive(Debug, Clone, Copy)]
pub struct ModelSource {
    /// File name of the downloaded model (determines the format by extension).
    pub file_name: &'static str,
    pub url: &'static str,
    /// Older file names still accepted if present on disk (e.g. `.rten` exports).
    pub legacy_names: &'static [&'static str],
    /// Expected SHA-256 of the downloaded file (lowercase hex). Checked on download.
    pub sha256: Option<&'static str>,
}

impl ModelSource {
    /// Accepted file names, preferred first.
    pub fn candidates(&self) -> impl Iterator<Item = &'static str> {
        std::iter::once(self.file_name).chain(self.legacy_names.iter().copied())
    }
}

/// Loads a model from a read-only location (e.g. `/usr/share`). Never modifies the file.
pub async fn load_readonly(path: &Path) -> Result<Model, OcrInitError> {
    load_blocking(path.to_path_buf()).await
}

/// Returns the first accepted file name of `source` that exists in `dir`.
pub fn find_existing(dir: &Path, source: &ModelSource) -> Option<PathBuf> {
    source
        .candidates()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
}

/// Returns a valid model from `dir`, installing it from `legacy_dir` or `source.url` if
/// it is missing or corrupt.
pub async fn ensure_model(
    dir: &Path,
    source: ModelSource,
    legacy_dir: Option<&Path>,
) -> Result<Model, OcrInitError> {
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| OcrInitError::ModelDir {
            path: dir.to_path_buf(),
            source: e,
        })?;

    for name in source.candidates() {
        let path = dir.join(name);
        if file_size(&path).await.is_none() {
            continue;
        }
        match load_blocking(path.clone()).await {
            Ok(model) => return Ok(model),
            Err(err) => {
                warn!("Existing OCR model is unusable, replacing it: {err}");
                quarantine(&path).await;
            }
        }
    }

    if let Some(legacy_dir) = legacy_dir {
        for name in source.candidates() {
            let legacy = legacy_dir.join(name);
            if file_size(&legacy).await.unwrap_or(0) < MIN_MODEL_SIZE {
                continue;
            }
            let part = part_path(dir, name);
            match tokio::fs::copy(&legacy, &part).await {
                Ok(_) => match install(&part, &dir.join(name), source.url).await {
                    Ok(model) => {
                        info!("Migrated OCR model from {legacy:?}");
                        return Ok(model);
                    }
                    Err(err) => warn!("Legacy OCR model {legacy:?} rejected: {err}"),
                },
                Err(err) => warn!("Cannot copy legacy OCR model {legacy:?}: {err}"),
            }
        }
    }

    info!("Downloading OCR model {}", source.url);
    let bytes = download(source.url, MIN_MODEL_SIZE).await?;
    verify_sha256(&source, &bytes)?;
    let part = part_path(dir, source.file_name);
    tokio::fs::write(&part, &bytes)
        .await
        .map_err(|e| OcrInitError::ModelWrite {
            path: part.clone(),
            source: e,
        })?;
    install(&part, &dir.join(source.file_name), source.url).await
}

/// Temporary path for `name` that keeps its extension: `a.onnx` -> `a.part.onnx`.
fn part_path(dir: &Path, name: &str) -> PathBuf {
    match name.rsplit_once('.') {
        Some((stem, ext)) => dir.join(format!("{stem}.part.{ext}")),
        None => dir.join(format!("{name}.part")),
    }
}

/// Returns the path of a non-model data file (e.g. a recognition dictionary) in `dir`,
/// downloading it if missing. Downloads must match `source.sha256` when set; an existing
/// file whose hash does not match is replaced.
pub async fn ensure_file(dir: &Path, source: ModelSource) -> Result<PathBuf, OcrInitError> {
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| OcrInitError::ModelDir {
            path: dir.to_path_buf(),
            source: e,
        })?;

    let path = dir.join(source.file_name);
    if let Ok(existing) = tokio::fs::read(&path).await {
        if verify_sha256(&source, &existing).is_ok() {
            return Ok(path);
        }
        warn!("Data file {path:?} does not match its checksum, replacing it");
        quarantine(&path).await;
    }

    info!("Downloading OCR data file {}", source.url);
    let bytes = download(source.url, 1).await?;
    verify_sha256(&source, &bytes)?;
    let part = part_path(dir, source.file_name);
    tokio::fs::write(&part, &bytes)
        .await
        .map_err(|e| OcrInitError::ModelWrite {
            path: part.clone(),
            source: e,
        })?;
    tokio::fs::rename(&part, &path)
        .await
        .map_err(|e| OcrInitError::ModelWrite {
            path: path.clone(),
            source: e,
        })?;
    Ok(path)
}

/// Checks `bytes` against `source.sha256`; sources without a checksum always pass.
fn verify_sha256(source: &ModelSource, bytes: &[u8]) -> Result<(), OcrInitError> {
    use sha2::{Digest, Sha256};
    let Some(expected) = source.sha256 else {
        return Ok(());
    };
    let actual: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(OcrInitError::InvalidModel {
            url: source.url,
            reason: format!("SHA-256 mismatch: expected {expected}, got {actual}"),
        })
    }
}

async fn download(url: &'static str, min_size: u64) -> Result<Vec<u8>, OcrInitError> {
    let fail = |e: reqwest::Error| OcrInitError::Download {
        url,
        reason: e.to_string(),
    };
    let bytes = reqwest::get(url)
        .await
        .map_err(fail)?
        .error_for_status()
        .map_err(fail)?
        .bytes()
        .await
        .map_err(fail)?;

    if (bytes.len() as u64) < min_size {
        return Err(OcrInitError::InvalidModel {
            url,
            reason: format!("response is only {} bytes", bytes.len()),
        });
    }
    Ok(bytes.to_vec())
}

/// Validates a fully written `.part` file and renames it into place.
async fn install(part: &Path, final_path: &Path, url: &'static str) -> Result<Model, OcrInitError> {
    let model = match load_blocking(part.to_path_buf()).await {
        Ok(model) => model,
        Err(err) => {
            let _ = tokio::fs::remove_file(part).await;
            return Err(OcrInitError::InvalidModel {
                url,
                reason: err.to_string(),
            });
        }
    };

    tokio::fs::rename(part, final_path)
        .await
        .map_err(|e| OcrInitError::ModelWrite {
            path: final_path.to_path_buf(),
            source: e,
        })?;
    Ok(model)
}

async fn load_blocking(path: PathBuf) -> Result<Model, OcrInitError> {
    let size = file_size(&path).await.unwrap_or(0);
    if size < MIN_MODEL_SIZE {
        return Err(OcrInitError::ModelLoad {
            path,
            reason: format!("file is {size} bytes, expected at least {MIN_MODEL_SIZE}"),
        });
    }

    if is_onnx(&path) && !has_onnx_header(&path).await {
        return Err(OcrInitError::ModelLoad {
            path,
            reason: "not an ONNX model (missing ir_version header)".to_string(),
        });
    }

    let task_path = path.clone();
    let load = tokio::task::spawn_blocking(move || {
        Model::load_file(&task_path).map_err(|e| OcrInitError::ModelLoad {
            path: task_path.clone(),
            reason: e.to_string(),
        })
    });
    match tokio::time::timeout(LOAD_TIMEOUT, load).await {
        Ok(joined) => joined.map_err(|e| OcrInitError::Task(e.to_string()))?,
        Err(_) => Err(OcrInitError::ModelLoad {
            path,
            reason: format!("parsing did not finish within {}s", LOAD_TIMEOUT.as_secs()),
        }),
    }
}

/// Upper bound for parsing one model. Valid ocrs models load in well under a second;
/// `rten`'s ONNX parser can spin indefinitely on some malformed inputs.
const LOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn is_onnx(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("onnx"))
}

/// ONNX `ModelProto` files start with field 1 (`ir_version`, varint), encoded as `0x08`.
///
/// Rejecting other inputs before parsing avoids handing arbitrary bytes to the parser.
async fn has_onnx_header(path: &Path) -> bool {
    use tokio::io::AsyncReadExt;
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return false;
    };
    let mut first = [0u8; 1];
    file.read_exact(&mut first).await.is_ok() && first[0] == 0x08
}

async fn file_size(path: &Path) -> Option<u64> {
    tokio::fs::metadata(path).await.ok().map(|m| m.len())
}

/// Moves a corrupt model to `<name>.corrupt` for inspection; deletes it if that fails.
async fn quarantine(path: &Path) {
    let mut target = path.as_os_str().to_owned();
    target.push(".corrupt");
    if tokio::fs::rename(path, &target).await.is_err() {
        let _ = tokio::fs::remove_file(path).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cosmic-ext-translator-models-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn truncated_model_is_rejected() {
        let dir = temp_dir("truncated");
        let path = dir.join("m.rten");
        std::fs::write(&path, vec![0u8; 1024]).unwrap();
        let err = load_readonly(&path).await.unwrap_err();
        assert!(matches!(err, OcrInitError::ModelLoad { .. }), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn invalid_part_file_is_removed_and_not_installed() {
        let dir = temp_dir("install");
        let part = part_path(&dir, "m.onnx");
        let final_path = dir.join("m.onnx");
        // Arbitrary bytes without the ONNX header must be rejected before parsing.
        std::fs::write(&part, vec![0xABu8; (MIN_MODEL_SIZE as usize) + 10]).unwrap();

        let err = install(&part, &final_path, "https://example.invalid/m.onnx")
            .await
            .unwrap_err();
        assert!(matches!(err, OcrInitError::InvalidModel { .. }), "{err}");
        assert!(
            !part.exists(),
            ".part file must be removed after failed validation"
        );
        assert!(
            !final_path.exists(),
            "invalid model must not reach the final path"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn zero_filled_onnx_is_rejected() {
        let dir = temp_dir("zeros");
        let path = dir.join("m.onnx");
        std::fs::write(&path, vec![0u8; (MIN_MODEL_SIZE as usize) + 10]).unwrap();
        let err = load_readonly(&path).await.unwrap_err();
        assert!(matches!(err, OcrInitError::ModelLoad { .. }), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn truncated_onnx_with_valid_header_is_rejected() {
        let dir = temp_dir("trunc-onnx");
        let path = dir.join("m.onnx");
        // ir_version = 11, then a length-delimited field whose length exceeds the file.
        let mut bytes = vec![0x08, 0x0B, 0x12, 0xFF, 0xFF, 0x7F];
        bytes.resize((MIN_MODEL_SIZE as usize) + 10, 0x41);
        std::fs::write(&path, bytes).unwrap();
        let err = load_readonly(&path).await.unwrap_err();
        assert!(matches!(err, OcrInitError::ModelLoad { .. }), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn part_path_keeps_extension() {
        let dir = Path::new("/m");
        assert_eq!(
            part_path(dir, "text-detection.onnx"),
            Path::new("/m/text-detection.part.onnx")
        );
        assert_eq!(part_path(dir, "noext"), Path::new("/m/noext.part"));
    }

    #[test]
    fn find_existing_prefers_primary_then_legacy() {
        let dir = temp_dir("find");
        let source = ModelSource {
            file_name: "m.onnx",
            url: "https://example.invalid/m.onnx",
            legacy_names: &["m.rten"],
            sha256: None,
        };
        assert_eq!(find_existing(&dir, &source), None);
        std::fs::write(dir.join("m.rten"), b"x").unwrap();
        assert_eq!(find_existing(&dir, &source), Some(dir.join("m.rten")));
        std::fs::write(dir.join("m.onnx"), b"x").unwrap();
        assert_eq!(find_existing(&dir, &source), Some(dir.join("m.onnx")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sha256_is_checked_when_present() {
        let mut source = ModelSource {
            file_name: "d.txt",
            url: "https://example.invalid/d.txt",
            legacy_names: &[],
            sha256: None,
        };
        assert!(verify_sha256(&source, b"abc").is_ok());
        source.sha256 = Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(verify_sha256(&source, b"abc").is_ok());
        assert!(matches!(
            verify_sha256(&source, b"abd"),
            Err(OcrInitError::InvalidModel { .. })
        ));
    }

    #[tokio::test]
    async fn ensure_file_keeps_matching_file_and_quarantines_mismatch() {
        let dir = temp_dir("ensure-file");
        let source = ModelSource {
            file_name: "d.txt",
            url: "https://example.invalid/d.txt",
            legacy_names: &[],
            sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        };
        std::fs::write(dir.join("d.txt"), b"abc").unwrap();
        assert_eq!(ensure_file(&dir, source).await.unwrap(), dir.join("d.txt"));

        // A mismatching file is moved aside; the download then fails (invalid host).
        std::fs::write(dir.join("d.txt"), b"tampered").unwrap();
        assert!(ensure_file(&dir, source).await.is_err());
        assert!(dir.join("d.txt.corrupt").exists());
        assert!(!dir.join("d.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn quarantine_moves_file_aside() {
        let dir = temp_dir("quarantine");
        let path = dir.join("m.rten");
        std::fs::write(&path, b"broken").unwrap();
        quarantine(&path).await;
        assert!(!path.exists());
        assert!(dir.join("m.rten.corrupt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
