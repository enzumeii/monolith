use super::encoded_bytes;
#[test]
fn binary_bridge_carries_bytes_as_base64_text() {
    assert_eq!(encoded_bytes(&[0, 1, 127, 128, 255]), "AAF/gP8=");
    assert_eq!(encoded_bytes(b"PK\x03\x04"), "UEsDBA==");
}
