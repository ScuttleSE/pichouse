//! ONNX Runtime library manager.
//!
//! Face detection is off by default. The ONNX Runtime shared library is not
//! shipped with pichouse and is not built into the binary. The first time the
//! user enables faces, this module downloads the library into the data folder
//! and loads it. Later runs find it there.
//!
//! `ort` with the `load-dynamic` feature loads the library at run time through
//! `ORT_DYLIB_PATH`. `ort::init` runs once per process. This module guards
//! that with a `Once`, and sets the path before the first init.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

use sha2::{Digest, Sha256};

/// The pinned ONNX Runtime version pichouse loads.
pub const ORT_VERSION: &str = "1.22.0";

/// The official Microsoft release archive for Linux x64.
const ORT_URL: &str =
    "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-linux-x64-1.22.0.tgz";

/// The SHA-256 of `lib/libonnxruntime.so.1.22.0` inside that archive.
const ORT_SO_SHA256: &str =
    "3da6146e14e7b8aaec625dde11d6114c7457c87a5f93d744897da8781e35c673";

/// The library file name pichouse writes into the data folder.
const ORT_SO_NAME: &str = "libonnxruntime.so.1.22.0";

static INIT: Once = Once::new();
static INIT_OK: AtomicBool = AtomicBool::new(false);

/// The path where pichouse keeps the ONNX Runtime library.
pub fn runtime_path() -> std::io::Result<PathBuf> {
    Ok(crate::db::data_dir()?.join("runtime").join(ORT_SO_NAME))
}

/// Report whether the ONNX Runtime library is present in the data folder.
pub fn runtime_present() -> bool {
    runtime_path().map(|p| p.exists()).unwrap_or(false)
}

/// Verify a file against a hex SHA-256.
fn verify_sha256(path: &std::path::Path, want_hex: &str) -> std::io::Result<bool> {
    let bytes = std::fs::read(path)?;
    let mut h = Sha256::new();
    h.update(&bytes);
    let got = h.finalize();
    let got_hex: String = got.iter().map(|b| format!("{b:02x}")).collect();
    Ok(got_hex.eq_ignore_ascii_case(want_hex))
}

/// Download the ONNX Runtime library into the data folder if it is absent or
/// its hash does not match. This does blocking network work. Call it off the
/// GTK main thread.
pub fn ensure_runtime() -> Result<PathBuf, String> {
    let dest = runtime_path().map_err(|e| format!("data dir: {e}"))?;
    if dest.exists() {
        match verify_sha256(&dest, ORT_SO_SHA256) {
            Ok(true) => return Ok(dest),
            Ok(false) => {
                log::warn!("ONNX Runtime present but hash mismatch; re-downloading");
            }
            Err(e) => return Err(format!("hash check: {e}")),
        }
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create runtime dir: {e}"))?;
    }

    log::info!("downloading ONNX Runtime {ORT_VERSION}");
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(ORT_URL)
        .send()
        .map_err(|e| format!("download: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download status {}", resp.status()));
    }
    let bytes = resp.bytes().map_err(|e| format!("read body: {e}"))?;

    // Extract the one library file from the gzip tar archive without a heavy
    // tar dependency: pipe the bytes through the system tar. The runner and
    // Debian 13 both ship tar and gzip.
    let so_bytes = extract_so_from_tgz(&bytes)?;

    // Verify before writing to the final path.
    let mut h = Sha256::new();
    h.update(&so_bytes);
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if !got.eq_ignore_ascii_case(ORT_SO_SHA256) {
        return Err(format!(
            "ONNX Runtime hash mismatch: expected {ORT_SO_SHA256}, got {got}"
        ));
    }

    let tmp = dest.with_extension("part");
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| format!("write: {e}"))?;
        f.write_all(&so_bytes).map_err(|e| format!("write: {e}"))?;
    }
    std::fs::rename(&tmp, &dest).map_err(|e| format!("finalize: {e}"))?;
    log::info!("ONNX Runtime ready at {}", dest.display());
    Ok(dest)
}

/// Extract `libonnxruntime.so.<version>` from the release tgz using the system
/// tar. Returns the library bytes.
fn extract_so_from_tgz(tgz: &[u8]) -> Result<Vec<u8>, String> {
    use std::process::{Command, Stdio};

    let member = format!(
        "onnxruntime-linux-x64-{ver}/lib/{name}",
        ver = ORT_VERSION,
        name = ORT_SO_NAME
    );
    let mut child = Command::new("tar")
        .args(["xzO", "-f", "-", &member])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn tar: {e}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("tar stdin")?;
        stdin.write_all(tgz).map_err(|e| format!("feed tar: {e}"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("tar wait: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "tar failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    if out.stdout.is_empty() {
        return Err("tar produced no data".into());
    }
    Ok(out.stdout)
}

/// Initialize ONNX Runtime once, pointing it at the data-folder library.
///
/// Call `ensure_runtime` first so the library is present. This sets
/// `ORT_DYLIB_PATH` and runs `ort::init` a single time for the process.
pub fn init_runtime() -> Result<(), String> {
    if INIT_OK.load(Ordering::Acquire) {
        return Ok(());
    }
    let path = runtime_path().map_err(|e| format!("data dir: {e}"))?;
    if !path.exists() {
        return Err("ONNX Runtime library is not downloaded".into());
    }

    let mut result: Result<(), String> = Ok(());
    INIT.call_once(|| {
        // ort load-dynamic reads this path at first init.
        std::env::set_var("ORT_DYLIB_PATH", &path);
        match ort::init().with_name("pichouse-face").commit() {
            Ok(_) => {
                INIT_OK.store(true, Ordering::Release);
            }
            Err(e) => {
                result = Err(format!("ort init: {e}"));
            }
        }
    });
    if INIT_OK.load(Ordering::Acquire) {
        Ok(())
    } else {
        result
    }
}
