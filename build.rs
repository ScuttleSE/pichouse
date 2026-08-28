//! Build script for pichouse.
//!
//! It makes the vendored ONNX Runtime shared library findable at run time.
//! The face feature uses `ort` with the `load-dynamic` feature. That feature
//! loads `libonnxruntime.so` at run time. This script does two things:
//!
//! 1. It gives the crate a compile-time default path to the vendored library
//!    through the `PICHOUSE_ORT_DYLIB` environment variable. The face module
//!    reads it with `env!`.
//! 2. It copies the vendored library into the target output directory next to
//!    the binary, so a developer run finds it without extra setup.
//!
//! A packaged build must ship `libonnxruntime.so` beside the binary, or set
//! the `ORT_DYLIB_PATH` environment variable at run time.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendored = manifest
        .join("vendor")
        .join("onnxruntime")
        .join("lib")
        .join("libonnxruntime.so");

    // Export the vendored path so the crate can read it with env!.
    println!(
        "cargo:rustc-env=PICHOUSE_ORT_DYLIB={}",
        vendored.display()
    );

    // Rebuild if the vendored library changes.
    println!("cargo:rerun-if-changed={}", vendored.display());
    println!("cargo:rerun-if-changed=build.rs");
}
