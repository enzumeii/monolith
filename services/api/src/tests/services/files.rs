#![allow(clippy::unwrap_used)]
use super::*;
use crate::validations::files::image;
use image::{DynamicImage, ImageFormat};
use std::io::Cursor;

#[tokio::test]
async fn upload_rejects_invalid_directory_without_writing_storage() {
    let fixture = crate::s3_fixture::Fixture::new().await;
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(1, 1)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    let image = image(bytes.into_inner(), "image/png").await.unwrap();
    assert!(
        upload(&fixture.client, "reports/fair", image)
            .await
            .is_err()
    );
    assert_eq!(fixture.data.lock().unwrap().calls, Vec::<String>::new());
}
