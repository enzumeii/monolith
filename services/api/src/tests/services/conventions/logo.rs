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
        validate(bytes.clone(), "image/png").await.unwrap().bytes,
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
        assert!(validate(bytes, mime).await.is_err());
    }
    assert!(validate(png(4096, 1), "image/png").await.is_ok());
}
#[test]
fn locations_are_canonical_and_owned_by_the_slug() {
    let key = "conventions/fair/550e8400-e29b-41d4-a716-446655440000.png";
    assert_eq!(location(key, Some("fair")).unwrap(), "image/png");
    assert!(location(key, Some("another")).is_err());
    for bad in [
        "reports/fair/a.png",
        "conventions/fair/../a.png",
        "conventions/fair/a.png",
        "https://storage.test/a.png",
        "conventions/fair/550e8400-e29b-41d4-a716-446655440000.svg",
        "conventions/fair/550E8400-E29B-41D4-A716-446655440000.png",
        "conventions/fair/550e8400-e29b-41d4-0716-446655440000.png",
    ] {
        assert!(location(bad, None).is_err(), "{bad}");
    }
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

#[tokio::test]
async fn jpeg_and_encoded_size_boundaries_preserve_original_bytes() {
    let mut jpeg = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(1, 1)
        .write_to(&mut jpeg, ImageFormat::Jpeg)
        .unwrap();
    let jpeg = jpeg.into_inner();
    assert_eq!(
        validate(jpeg.clone(), "image/jpeg").await.unwrap().bytes,
        jpeg
    );
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
    assert!(validate(bytes, "image/png").await.is_ok());
    assert!(
        validate(jpeg[..jpeg.len() - 1].to_vec(), "image/jpeg")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn upload_attachment_and_preview_use_the_explicit_location_bucket() {
    let fixture = crate::s3_fixture::Fixture::new().await;
    let bytes = png(1, 1);
    let uploaded = upload(
        &fixture.client,
        "conventions/ax-2026",
        validate(bytes.clone(), "image/png").await.unwrap(),
    )
    .await
    .unwrap();
    let (bucket, key) = uploaded.location.split_once('/').unwrap();
    assert_eq!(bucket, "conventions");
    assert!(key.starts_with("ax-2026/"));
    let signed = reqwest::Url::parse(&uploaded.url).unwrap();
    assert_eq!(signed.path(), format!("/{}", uploaded.location));
    assert!(!signed.path().starts_with("/reports/"));
    attach(&fixture.client, &uploaded.location, "ax-2026")
        .await
        .unwrap();
    let preview = preview(&fixture.client, &uploaded.location).await.unwrap();
    assert_eq!(
        reqwest::Url::parse(&preview.url).unwrap().path(),
        signed.path()
    );
    let response = reqwest::get(preview.url).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.bytes().await.unwrap().as_ref(), bytes);
    let data = fixture.data.lock().unwrap();
    let destination = (bucket.to_owned(), key.to_owned());
    assert_eq!(data.puts, vec![destination.clone()]);
    assert_eq!(data.objects.get(&destination), Some(&bytes));
    assert_eq!(
        data.destinations,
        vec![(bucket.to_owned(), key.to_owned()); 4]
    );
    assert_eq!(
        data.calls,
        vec!["upload", "metadata", "download", "download"]
    );
}
