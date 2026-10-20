use super::*;
#[test]
fn digits_preserve_zeroes_and_ignore_non_ascii_digits() {
    assert_eq!(ascii_digits("+66 (02) 123-４٥"), "6602123");
    assert_eq!(ascii_digits("00123"), "00123");
    assert_eq!(ascii_digits(""), "");
}

#[test]
fn generic_address_and_phone_formatting_preserve_report_rules() {
    assert_eq!(
        format_address(&[
            Some(" บ้าน 1 "),
            None,
            Some(""),
            Some(" กรุงเทพ "),
            Some("00100")
        ]),
        "บ้าน 1, กรุงเทพ, 00100"
    );
    assert_eq!(format_address(&[None, Some(" ")]), "");
    assert_eq!(phone_number("00 (081) ٢3-45"), 81345);
    assert_eq!(phone_number("18446744073709551616"), 0);
    assert_eq!(phone_number("---"), 0);
}

#[test]
fn japanese_kanji_uses_japanese_readings_and_other_addresses_transliterate() {
    assert_eq!(romanize("東京都", "JP").to_lowercase(), "toukyouto");
    assert_eq!(romanize("L'été", "FR"), "Lete");
}
