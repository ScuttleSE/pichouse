//! Phase 0 dependency spike for facial recognition.
//!
//! This program has one job. It proves that the `ort` crate links a working
//! ONNX Runtime on this machine and on the CI runner. It initializes the
//! runtime and prints a confirmation. It needs no model file.
//!
//! A later phase adds the real detect and embed pipeline with the YuNet and
//! SFace models. Run this spike with:
//!
//!     cargo run --example face_spike

fn main() -> ort::Result<()> {
    // Point ort at the vendored ONNX Runtime library. The build script gives
    // the compile-time path through PICHOUSE_ORT_DYLIB. A run-time override
    // through ORT_DYLIB_PATH wins if it is set.
    if std::env::var_os("ORT_DYLIB_PATH").is_none() {
        std::env::set_var("ORT_DYLIB_PATH", env!("PICHOUSE_ORT_DYLIB"));
    }

    // Initialize the ONNX Runtime environment. This call links and loads the
    // native runtime. A failure here means the native dependency is not ready.
    ort::init().with_name("pichouse-face-spike").commit()?;

    println!("ort linked an ONNX Runtime.");
    println!("Phase 0 gate: the native dependency builds and runs on this host.");
    Ok(())
}
