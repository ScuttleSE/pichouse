//! Real ONNX inference verification for the face pipeline.
//!
//! This test is ignored by default. It needs the ONNX Runtime library and the
//! two model files in the local data folder. CI has neither, so it stays out
//! of the normal suite.
//!
//! Run it by hand:
//!   cargo test face::inference_test -- --ignored --nocapture
//!
//! It needs these files present:
//!   ~/.local/share/pichouse/runtime/libonnxruntime.so.1.22.0
//!   ~/.local/share/pichouse/models/face_detection_yunet_2023mar.onnx
//!   ~/.local/share/pichouse/models/face_recognition_sface_2021dec.onnx
//! and three test JPEGs under target/face_test_data/ (face1, face1b, face2).

use super::detector::Detector;
use super::embedder::Embedder;

/// Load a JPEG and return tightly-packed RGB8 plus size.
fn load_rgb(path: &str) -> (Vec<u8>, u32, u32) {
    let img = image::open(path).expect("open image").to_rgb8();
    let (w, h) = (img.width(), img.height());
    (img.into_raw(), w, h)
}

fn data_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap();
    std::path::PathBuf::from(home).join(".local/share/pichouse")
}

fn init() {
    let so = data_dir().join("runtime/libonnxruntime.so.1.22.0");
    std::env::set_var("ORT_DYLIB_PATH", &so);
    // The test inits ort directly. The app uses runtime::init_runtime.
    let _ = ort::init().with_name("pichouse-face-test").commit();
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[test]
#[ignore = "needs local ONNX runtime + models; run with --ignored"]
fn face_pipeline_real_inference() {
    init();

    let models = data_dir().join("models");
    let det = Detector::load(
        models
            .join("face_detection_yunet_2023mar.onnx")
            .to_str()
            .unwrap(),
    )
    .expect("load detector");
    let emb = Embedder::load(
        models
            .join("face_recognition_sface_2021dec.onnx")
            .to_str()
            .unwrap(),
    )
    .expect("load embedder");
    assert_eq!(emb.embedding_dim(), 128);

    let base = "target/face_test_data";
    let (rgb1, w1, h1) = load_rgb(&format!("{base}/face1.jpg"));
    let (rgb1b, w1b, h1b) = load_rgb(&format!("{base}/face1b.jpg"));
    let (rgb2, w2, h2) = load_rgb(&format!("{base}/face2.jpg"));

    // 1. Detect on image 1.
    let faces1 = det.detect(&rgb1, w1, h1, 0.6).expect("detect");
    println!("image1: {} faces", faces1.len());
    assert!(!faces1.is_empty(), "must find at least one face");
    let f = faces1[0].clone();
    println!(
        "  box permille x={} y={} w={} h={} score={:.3}",
        f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h, f.det_score
    );
    println!("  landmarks permille = {:?}", f.landmarks);
    assert!(f.det_score > 0.6, "score must exceed 0.6");
    assert!(f.bbox_x >= 0 && f.bbox_y >= 0);
    assert!(f.bbox_x + f.bbox_w <= 1000);
    assert!(f.bbox_y + f.bbox_h <= 1000);
    assert!(f.bbox_w > 0 && f.bbox_h > 0);
    assert_eq!(f.landmarks.len(), 10);

    // 2. Embed the first face. Check length and normalization.
    let e1 = emb.embed(&rgb1, w1, h1, &f.landmarks).expect("embed");
    assert_eq!(e1.len(), 128);
    let norm: f32 = e1.iter().map(|v| v * v).sum::<f32>().sqrt();
    println!("  embedding norm = {norm:.6}");
    assert!((norm - 1.0).abs() < 1e-3, "must be L2-normalized");

    // 3. Embed the same face twice. Cosine must be ~1.
    let e1_again = emb.embed(&rgb1, w1, h1, &f.landmarks).expect("embed again");
    let cos_same_run = cosine(&e1, &e1_again);
    println!("  cosine same input twice = {cos_same_run:.6}");
    assert!(cos_same_run > 0.999);

    // 4. Same person, different photo. Higher than a different person.
    let faces1b = det.detect(&rgb1b, w1b, h1b, 0.6).expect("detect 1b");
    assert!(!faces1b.is_empty());
    let e1b = emb
        .embed(&rgb1b, w1b, h1b, &faces1b[0].landmarks)
        .expect("embed 1b");

    let faces2 = det.detect(&rgb2, w2, h2, 0.6).expect("detect 2");
    assert!(!faces2.is_empty());
    let e2 = emb
        .embed(&rgb2, w2, h2, &faces2[0].landmarks)
        .expect("embed 2");

    let cos_same_person = cosine(&e1, &e1b);
    let cos_diff_person = cosine(&e1, &e2);
    println!("  cosine same person (2 photos) = {cos_same_person:.4}");
    println!("  cosine different person       = {cos_diff_person:.4}");
    assert!(
        cos_same_person > cos_diff_person,
        "same person must be more similar than different person"
    );
}
