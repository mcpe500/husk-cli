//! Build script: embeds an optional GGUF payload into the binary.
//!
//! HUSK_EMBED_MODEL=/path/to/MiniCPM5-2B-Q4.gguf cargo build --features local,embed-model
//!
//! The payload is split into 32MB chunk files included via include_bytes! so
//! rustc never materializes one giant array. The first worker use streams
//! the chunks to the OS cache dir and llama.cpp mmaps from there — the
//! binary is single-file, resident memory stays small.

use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::PathBuf;

const CHUNK: usize = 32 * 1024 * 1024;

fn main() {
    println!("cargo:rerun-if-env-changed=HUSK_EMBED_MODEL");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    let generated = match env::var("HUSK_EMBED_MODEL").ok().filter(|s| !s.is_empty()) {
        Some(path) => {
            let data = fs::read(&path).unwrap_or_else(|e| panic!("HUSK_EMBED_MODEL unreadable ({path}): {e}"));
            let sha = format!("{:x}", Sha256::digest(&data));
            let name = PathBuf::from(&path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "model.gguf".to_string());

            let mut src = String::new();
            src.push_str(&format!("pub const NAME: &str = {name:?};\n"));
            src.push_str(&format!("pub const SHA256: &str = {sha:?};\n"));
            let n_chunks = data.len().div_ceil(CHUNK).max(1);
            src.push_str(&format!("pub static CHUNKS: [&[u8]; {n_chunks}] = [\n"));

            let mut written = 0usize;
            let mut idx = 0usize;
            while written < data.len() {
                let end = (written + CHUNK).min(data.len());
                let chunk_path = out.join(format!("embed_chunk_{idx}.bin"));
                fs::write(&chunk_path, &data[written..end])
                    .unwrap_or_else(|e| panic!("write chunk {idx}: {e}"));
                src.push_str(&format!("    include_bytes!({:?}),\n", chunk_path));
                written = end;
                idx += 1;
            }
            if data.is_empty() {
                src.push_str("    &[],\n");
            }
            src.push_str("];\n");
            src
        }
        None => String::from(
            "pub const NAME: &str = \"\";\n\
             pub const SHA256: &str = \"\";\n\
             pub static CHUNKS: [&[u8]; 0] = [];\n",
        ),
    };

    fs::write(out.join("embedded_model.rs"), generated).expect("write embedded_model.rs");
}
