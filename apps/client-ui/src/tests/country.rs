use super::{COUNTRY_NAMES, country_name};

#[test]
fn country_names_cover_supported_codes_with_english_display_names() {
    for (code, expected) in [
        ("TH", "Thailand"),
        ("US", "United States"),
        ("GB", "United Kingdom"),
        ("SZ", "Eswatini"),
        ("TR", "Türkiye"),
        ("TW", "Taiwan"),
        ("CI", "Côte d’Ivoire"),
        ("AX", "Åland Islands"),
        ("RE", "Réunion"),
        ("ST", "São Tomé & Príncipe"),
        ("ZW", "Zimbabwe"),
    ] {
        assert_eq!(country_name(code), expected, "{code}");
    }
}

#[test]
fn country_names_normalize_case_and_surrounding_whitespace() {
    for code in ["th", "Th", "tH", " TH ", "\tth\r\n", "\u{2003}th\u{2003}"] {
        assert_eq!(country_name(code), "Thailand");
    }
}

#[test]
fn country_names_hide_unknown_and_malformed_codes() {
    for code in [
        "", " ", "T", "THA", "T H", "ZZ", "UK", "XK", "01", "é", "ＴＨ",
    ] {
        assert_eq!(country_name(code), "Unknown country", "{code}");
    }
}

#[test]
fn country_table_has_all_249_sorted_unique_uppercase_codes_and_noncode_names() {
    assert_eq!(COUNTRY_NAMES.len(), 249);
    assert!(COUNTRY_NAMES.windows(2).all(|pair| pair[0].0 < pair[1].0));
    for (code, name) in COUNTRY_NAMES {
        assert_eq!(code.len(), 2);
        assert!(code.bytes().all(|byte| byte.is_ascii_uppercase()));
        assert!(!name.trim().is_empty(), "{code}");
        assert_ne!(name, code);
    }
}
