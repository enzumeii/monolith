#![allow(clippy::unwrap_used)]
use super::CountryCode;
use serde_json::json;

const ASSIGNED_COUNTRIES: &str = "AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS YE YT ZA ZM ZW";

#[test]
fn every_previously_assigned_country_round_trips_and_normalizes() {
    let countries: Vec<_> = ASSIGNED_COUNTRIES.split_whitespace().collect();
    assert_eq!(countries.len(), 249);
    for code in countries {
        let country = CountryCode::from(code.to_owned());
        assert!(!matches!(country, CountryCode::Other(_)), "{code}");
        assert_eq!(country.as_str(), code);
        assert_eq!(String::from(country.clone()), code);
        assert_eq!(CountryCode::from(code.to_ascii_lowercase()), country);
        assert_eq!(serde_json::to_value(&country).unwrap(), json!(code));
        assert_eq!(
            serde_json::from_value::<CountryCode>(json!(code.to_ascii_lowercase())).unwrap(),
            country
        );
    }
    // Scan all alpha-2 possibilities so unassigned codes cannot silently become accepted.
    for first in b'A'..=b'Z' {
        for second in b'A'..=b'Z' {
            let code = String::from_utf8(vec![first, second]).unwrap();
            assert_eq!(
                !matches!(CountryCode::from(code.clone()), CountryCode::Other(_)),
                ASSIGNED_COUNTRIES
                    .split_whitespace()
                    .any(|assigned| assigned == code),
                "{code}"
            );
        }
    }
}

#[test]
fn unknown_country_codes_preserve_distinct_normalized_identity() {
    for raw in ["zz", "xx", "", " TH", "TH ", "THA", "ก", "1A"] {
        let normalized = raw.to_ascii_uppercase();
        let country = CountryCode::from(raw.to_owned());
        assert_eq!(country, CountryCode::Other(normalized.clone()));
        assert_eq!(country.as_str(), normalized);
        assert_eq!(serde_json::to_value(&country).unwrap(), json!(normalized));
        assert_eq!(
            serde_json::from_value::<CountryCode>(json!(raw)).unwrap(),
            country
        );
    }
    assert_ne!(
        CountryCode::from("ZZ".to_owned()),
        CountryCode::from("XX".to_owned())
    );
    assert_eq!(
        serde_json::to_value(CountryCode::Other("zz".into())).unwrap(),
        json!("ZZ")
    );
}
