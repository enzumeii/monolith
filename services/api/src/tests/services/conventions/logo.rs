#![allow(clippy::unwrap_used)]
use super::*;
use crate::{services::files::upload, validations::files::image};
use image::{DynamicImage, ImageFormat};
use std::io::Cursor;
#[tokio::test]
async fn upload_attachment_and_preview_use_the_explicit_location_bucket() {
    let fixture = crate::s3_fixture::Fixture::new().await;
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(1, 1)
        .write_to(&mut encoded, ImageFormat::Png)
        .unwrap();
    let bytes = encoded.into_inner();
    let uploaded = upload(
        &fixture.client,
        "conventions/ax-2026",
        image(bytes.clone(), "image/png").await.unwrap(),
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

#[tokio::test]
async fn attachment_rejects_another_convention_before_contacting_storage() {
    let fixture = crate::s3_fixture::Fixture::new().await;
    let error = attach(
        &fixture.client,
        "conventions/fair/550e8400-e29b-41d4-a716-446655440000.png",
        "another",
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Logo location must belong to the convention slug"
    );
    assert_eq!(fixture.data.lock().unwrap().calls, Vec::<String>::new());
}
