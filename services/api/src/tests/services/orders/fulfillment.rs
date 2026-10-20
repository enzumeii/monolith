use super::*;

#[test]
fn tracking_patterns_keep_last_mile_carrier_precedence_and_escape_urls() {
    for (number, company, url) in [
        ("1ZABC", "UPS", "https://www.ups.com/track?tracknum=1ZABC"),
        (
            "92123456789012345678901234",
            "UPS",
            "https://www.ups.com/track?tracknum=92123456789012345678901234",
        ),
        (
            "1234567890123456789012",
            "USPS",
            "https://tools.usps.com/go/TrackConfirmAction?tLabels=1234567890123456789012",
        ),
        (
            "YT123",
            "YunExpress",
            "https://www.yuntrack.com/parcelTracking?id=YT123",
        ),
        (
            "VP123",
            "Other",
            "https://www.aftership.com/track/800bestex/VP123",
        ),
        ("FDA123", "Other", "https://track.fastship.co/FDA123"),
        ("FXA123", "Other", "https://track.fastship.co/FXA123"),
        ("WG123", "Other", "https://track.fastship.co/WG123"),
        ("LX123", "Other", "https://track.fastship.co/LX123"),
        (
            "RR123TH",
            "Other",
            "https://www.aftership.com/track/thailand-post/RR123TH",
        ),
        (
            "a/b?c#d",
            "Other",
            "https://www.aftership.com/track/a%2Fb%3Fc%23d",
        ),
    ] {
        assert_eq!(
            tracking(number).expect("tracking URL"),
            (company, url.into())
        );
    }
}
