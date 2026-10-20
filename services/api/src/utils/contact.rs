pub(crate) fn ascii_digits(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

pub(crate) fn phone_number(value: &str) -> u64 {
    ascii_digits(value).parse().unwrap_or_default()
}

pub(crate) fn format_address(parts: &[Option<&str>]) -> String {
    parts
        .iter()
        .flatten()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn romanize(value: &str, country: &str) -> String {
    let value = if country == "JP"
        && value
            .chars()
            .any(|c| matches!(c,'\u{3400}'..='\u{9fff}'|'\u{f900}'..='\u{faff}'))
    {
        kakasi::convert(value).romaji
    } else {
        value.into()
    };
    deunicode::deunicode(&value).trim().replace('\'', "")
}

#[cfg(test)]
#[path = "../tests/utils/contact.rs"]
mod tests;
