use super::*;
#[test]
fn digits_preserve_zeroes_and_ignore_non_ascii_digits() {
    assert_eq!(ascii_digits("+66 (02) 123-４٥"), "6602123");
    assert_eq!(ascii_digits("00123"), "00123");
    assert_eq!(ascii_digits(""), "");
}
