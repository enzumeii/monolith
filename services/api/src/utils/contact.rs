pub(crate) fn ascii_digits(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

#[cfg(test)]
#[path = "../tests/utils/contact.rs"]
mod tests;
