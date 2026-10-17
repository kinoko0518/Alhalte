//! Tests for deterministic image encoding in `alhalte-canner`.

use std::io::Cursor;

use alhalte_canner::encode_image;
use image::{ImageFormat, Rgba, RgbaImage};

fn create_test_png(width: u32, height: u32) -> Vec<u8> {
    let mut img = RgbaImage::new(width, height);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let r = ((x * 255) / width.max(1)) as u8;
        let g = ((y * 255) / height.max(1)) as u8;
        let b = 128u8;
        *pixel = Rgba([r, g, b, 255]);
    }
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png)
        .expect("PNG encoding should succeed");
    buf.into_inner()
}

// spec: alhalte-canner/encode-image
#[test]
fn test_encode_image_standard() {
    let png_bytes = create_test_png(200, 100);
    let webp_bytes = encode_image(&png_bytes).expect("encode_image should succeed");

    // Must be valid WebP (RIFF....WEBP)
    assert!(webp_bytes.len() > 12);
    assert_eq!(&webp_bytes[0..4], b"RIFF");
    assert_eq!(&webp_bytes[8..12], b"WEBP");

    // Decode to check dimensions are preserved (200x100 <= 1920)
    let decoded = image::load_from_memory(&webp_bytes).expect("decoding encoded WebP should work");
    assert_eq!(decoded.width(), 200);
    assert_eq!(decoded.height(), 100);
}

// spec: alhalte-canner/encode-image
#[test]
fn test_encode_image_downscaling() {
    // 2400x1200 image has longest side > 1920.
    // It should be scaled down to 1920x960 preserving 2:1 aspect ratio.
    let png_bytes = create_test_png(2400, 1200);
    let webp_bytes = encode_image(&png_bytes).expect("encode_image should succeed");

    let decoded = image::load_from_memory(&webp_bytes).expect("decoding WebP should work");
    assert_eq!(decoded.width(), 1920);
    assert_eq!(decoded.height(), 960);
}

// spec: alhalte-canner/encode-image
#[test]
fn test_encode_image_deterministic_identity() {
    let png_bytes = create_test_png(500, 400);

    let enc1 = encode_image(&png_bytes).expect("first encode");
    let enc2 = encode_image(&png_bytes).expect("second encode");

    // Device-independent identical bit-for-bit output guaranteed
    assert_eq!(enc1, enc2);
}

// spec: alhalte-canner/encode-image
#[test]
fn test_encode_image_invalid_input() {
    let invalid = b"this is completely invalid image data";
    assert!(encode_image(invalid).is_err());
}
