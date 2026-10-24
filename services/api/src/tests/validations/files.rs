#![allow(clippy::unwrap_used)]
use super::*;
use image::{DynamicImage, ImageBuffer, Rgba};
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(ImageBuffer::<Rgba<u8>, _>::new(width, height))
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
#[tokio::test]
async fn decodes_complete_images_and_rejects_mime_size_and_dimension_violations() {
    let bytes = png(1, 1);
    assert_eq!(
        image(bytes.clone(), "image/png").await.unwrap().bytes,
        bytes
    );
    for (bytes, mime) in [
        (bytes.clone(), "image/jpeg"),
        (bytes[..bytes.len() / 2].to_vec(), "image/png"),
        (bytes[..bytes.len() - 1].to_vec(), "image/png"),
        (vec![], "image/png"),
        (vec![0; MAX_BYTES + 1], "image/png"),
        (png(4097, 1), "image/png"),
        (b"<svg/>".to_vec(), "image/svg+xml"),
    ] {
        assert!(image(bytes, mime).await.is_err());
    }
    assert!(image(png(4096, 1), "image/png").await.is_ok());
}
#[tokio::test]
async fn jpeg_and_encoded_size_boundaries_preserve_original_bytes() {
    let mut jpeg = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(1, 1)
        .write_to(&mut jpeg, ImageFormat::Jpeg)
        .unwrap();
    let jpeg = jpeg.into_inner();
    assert_eq!(image(jpeg.clone(), "image/jpeg").await.unwrap().bytes, jpeg);
    let mut bytes = png(1, 1);
    let tail = bytes.split_off(bytes.len() - 12);
    let padding = vec![b'a'; MAX_BYTES - bytes.len() - tail.len() - 12];
    bytes.extend_from_slice(&u32::try_from(padding.len()).unwrap().to_be_bytes());
    let start = bytes.len();
    bytes.extend_from_slice(b"npAD");
    bytes.extend_from_slice(&padding);
    let mut crc = u32::MAX;
    for byte in &bytes[start..] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
    bytes.extend_from_slice(&tail);
    assert_eq!(bytes.len(), MAX_BYTES);
    assert!(image(bytes, "image/png").await.is_ok());
    assert!(
        image(jpeg[..jpeg.len() - 1].to_vec(), "image/jpeg")
            .await
            .is_err()
    );
}

#[test]
fn upload_directories_keep_the_existing_namespace_and_slug_rules() {
    assert!(folder("conventions/fair").is_ok());
    for bad in [
        "conventions/api",
        "reports/fair",
        "conventions/fair/",
        "conventions/fair/a.png",
        "conventions/../fair",
        "/conventions/fair",
        "conventions/Fair",
        "conventions/fair?x=1",
    ] {
        assert!(folder(bad).is_err(), "{bad}");
    }
}
