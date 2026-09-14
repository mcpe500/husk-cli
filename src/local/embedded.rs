//! Embedded GGUF payload handling.
//!
//! When the binary is built with `HUSK_EMBED_MODEL=/path/to/model.gguf`
//! (optionally together with `--features embed-model`), build.rs chunks the
//! weights into `OUT_DIR` and this module carries them as static data — the
//! model is truly inside the executable. At first worker use it is extracted
//! **streaming** (chunk by chunk, never fully resident) into the OS cache
//! dir and sha256-verified; llama.cpp then mmaps the extracted file so
//! weight pages stay file-backed and evictable under memory pressure.

/// (chunks, file name, sha256 hex) when a payload is baked in.
pub fn payload() -> Option<(&'static [&'static [u8]], &'static str, &'static str)> {
    #[cfg(feature = "embed-model")]
    {
        let chunks = crate::local::embedded_payload::CHUNKS.as_slice();
        let sha = crate::local::embedded_payload::SHA256;
        if sha.is_empty() {
            None
        } else {
            Some((chunks, crate::local::embedded_payload::NAME, sha))
        }
    }
    #[cfg(not(feature = "embed-model"))]
    {
        None
    }
}

/// Stream-write `chunks` into `dest`, verifying sha256 while writing.
pub fn extract_chunks(chunks: &[&[u8]], expected_sha: &str, dest: &Path) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};
    use std::io::Write;

    let sidecar = dest.with_extension("sha");
    if dest.exists() {
        // Trust an existing extraction only when its recorded checksum matches.
        if std::fs::read_to_string(&sidecar)
            .map(|s| s.trim().eq_ignore_ascii_case(expected_sha))
            .unwrap_or(false)
        {
            return Ok(());
        }
        std::fs::remove_file(dest).ok();
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut hasher = Sha256::new();
    let mut file = std::io::BufWriter::new(
        std::fs::File::create(dest).with_context(|| format!("create {}", dest.display()))?,
    );
    for chunk in chunks {
        hasher.update(chunk);
        file.write_all(chunk)?;
    }
    file.flush()?;
    let digest = format!("{:x}", hasher.finalize());
    if !digest.eq_ignore_ascii_case(expected_sha) {
        std::fs::remove_file(dest).ok();
        anyhow::bail!("embedded model checksum mismatch: {digest} != {expected_sha}");
    }
    std::fs::write(&sidecar, &digest)?;
    Ok(())
}

/// Resolve the GGUF path for the worker: explicit config path, or the
/// embedded payload extracted into the cache dir.
pub fn ensure_model_available(config: &LocalConfig) -> anyhow::Result<PathBuf> {
    if let Some(path) = &config.model_path {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
        anyhow::bail!("local.model_path does not exist: {}", path.display());
    }
    match payload() {
        Some((chunks, name, sha)) => {
            let cache = dirs::cache_dir()
                .ok_or_else(|| anyhow::anyhow!("no OS cache dir available"))?
                .join("husk")
                .join("models");
            let prefix_len = sha.len().min(12);
            let dest = cache.join(format!("{}-{}", &sha[..prefix_len], name));
            extract_chunks(chunks, sha, &dest)?;
            Ok(dest)
        }
        None => anyhow::bail!(
            "no local model available: set local.model_path in .husk/config.toml, \
             or build with HUSK_EMBED_MODEL=<path to gguf>"
        ),
    }
}

use anyhow::Context as _;
use std::path::{Path, PathBuf};

use super::LocalConfig;

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    fn extraction_verifies_sha_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("model.gguf");
        let chunk_a = b"hello ".to_vec();
        let chunk_b = b"world".to_vec();
        let mut hasher = sha2::Sha256::new();
        hasher.update(&chunk_a);
        hasher.update(&chunk_b);
        let sha = format!("{:x}", hasher.finalize());

        extract_chunks(&[&chunk_a, &chunk_b], &sha, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"hello world");

        // Second run short-circuits on matching size.
        extract_chunks(&[&chunk_a, &chunk_b], &sha, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"hello world");

        // Corrupt checksum → file removed and error.
        let bad = extract_chunks(&[&chunk_a, &chunk_b], "deadbeef", &dest);
        assert!(bad.is_err());
        assert!(!dest.exists(), "failed extraction must not leave a partial file");
    }

    #[test]
    fn no_payload_and_no_path_is_a_clean_error() {
        let cfg = LocalConfig::default();
        if payload().is_none() {
            let err = ensure_model_available(&cfg).unwrap_err().to_string();
            assert!(err.contains("local.model_path"), "{err}");
        }
    }
}
