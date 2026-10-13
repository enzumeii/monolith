use super::*;
#[test]
fn hash_token_is_deterministic_and_different_from_input() {
    assert_eq!(hash_token("test-token"), hash_token("test-token"));
    assert_ne!(hash_token("test-token"), "test-token");
}
