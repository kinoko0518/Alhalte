//! Image decoding, deterministic downscaling, and WebP re-encoding.

use oxideav_webp::EncodeOptions;

use crate::error::{CannerError, Result};

/// Maximum allowed length of the longest side for encoded images (1920 px).
pub const MAX_IMAGE_SIDE: u32 = 1920;

/// Quality level used for lossy WebP encoding (quality 80).
pub const WEBP_LOSSY_QUALITY: f32 = 80.0;

/// Decodes an input image (PNG, JPEG, WebP, GIF, etc.), scales it down preserving
/// aspect ratio if its longest side exceeds 1920px using a deterministic fixed-point algorithm,
/// and re-encodes it as lossy WebP (quality 80) with device-independent identity.
///
/// // spec: alhalte-canner/encode-image
pub fn encode_image(input: &[u8]) -> Result<Vec<u8>> {
    let dynamic_img = image::load_from_memory(input)?;
    let (width, height) = (dynamic_img.width(), dynamic_img.height());

    if width == 0 || height == 0 {
        return Err(CannerError::InvalidImageDimensions { width, height });
    }

    let rgba_img = dynamic_img.to_rgba8();
    let src_bytes = rgba_img.as_raw();

    let max_dim = width.max(height);
    let (target_w, target_h, target_rgba) = if max_dim > MAX_IMAGE_SIDE {
        let (new_w, new_h) = if width >= height {
            let nw = MAX_IMAGE_SIDE;
            let nh = ((height as u64 * MAX_IMAGE_SIDE as u64 + (width as u64 / 2)) / width as u64)
                .max(1) as u32;
            (nw, nh)
        } else {
            let nh = MAX_IMAGE_SIDE;
            let nw = ((width as u64 * MAX_IMAGE_SIDE as u64 + (height as u64 / 2)) / height as u64)
                .max(1) as u32;
            (nw, nh)
        };
        let resized = resize_rgba_deterministic(src_bytes, width, height, new_w, new_h);
        (new_w, new_h, resized)
    } else {
        (width, height, src_bytes.to_vec())
    };

    let opts = EncodeOptions::default().with_quality(WEBP_LOSSY_QUALITY);

    let webp_bytes = oxideav_webp::encode_rgba8(target_w, target_h, &target_rgba, &opts)?;
    Ok(webp_bytes)
}

/// Deterministic 16.16 fixed-point bilinear image resizing algorithm.
///
/// Operates entirely with integer and fixed-point arithmetic without any floating-point
/// discrepancies, guaranteeing device-independent identical output bit for bit.
fn resize_rgba_deterministic(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> Vec<u8> {
    let mut dst = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return dst;
    }
    if dst_w == 1 && dst_h == 1 {
        dst[0..4].copy_from_slice(&src[0..4]);
        return dst;
    }

    let scale_x = ((src_w as u64) << 16) / dst_w as u64;
    let scale_y = ((src_h as u64) << 16) / dst_h as u64;
    let max_src_x = ((src_w - 1) as u64) << 16;
    let max_src_y = ((src_h - 1) as u64) << 16;

    for y in 0..dst_h {
        let center_src_y = (y as u64 * scale_y) + (scale_y / 2);
        let src_y_fp = center_src_y.saturating_sub(1 << 15).min(max_src_y);
        let y0 = (src_y_fp >> 16) as usize;
        let y1 = (y0 + 1).min((src_h - 1) as usize);
        let wy = (src_y_fp & 0xffff) as u32;
        let wy_inv = 0x10000 - wy;

        let dst_row_offset = (y as usize * dst_w as usize) * 4;
        let src_row0_offset = y0 * (src_w as usize) * 4;
        let src_row1_offset = y1 * (src_w as usize) * 4;

        for x in 0..dst_w {
            let center_src_x = (x as u64 * scale_x) + (scale_x / 2);
            let src_x_fp = center_src_x.saturating_sub(1 << 15).min(max_src_x);
            let x0 = (src_x_fp >> 16) as usize;
            let x1 = (x0 + 1).min((src_w - 1) as usize);
            let wx = (src_x_fp & 0xffff) as u32;
            let wx_inv = 0x10000 - wx;

            let p00_idx = src_row0_offset + x0 * 4;
            let p10_idx = src_row0_offset + x1 * 4;
            let p01_idx = src_row1_offset + x0 * 4;
            let p11_idx = src_row1_offset + x1 * 4;

            let dst_pixel_idx = dst_row_offset + (x as usize) * 4;

            for c in 0..4 {
                let v00 = src[p00_idx + c] as u32;
                let v10 = src[p10_idx + c] as u32;
                let v01 = src[p01_idx + c] as u32;
                let v11 = src[p11_idx + c] as u32;

                let top = (v00 * wx_inv + v10 * wx + 0x8000) >> 16;
                let bot = (v01 * wx_inv + v11 * wx + 0x8000) >> 16;
                let val = (top * wy_inv + bot * wy + 0x8000) >> 16;
                dst[dst_pixel_idx + c] = val.min(255) as u8;
            }
        }
    }
    dst
}
