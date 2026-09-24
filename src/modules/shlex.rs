/// Quote a shell word using the stable ASCII safe set below. Every non-ASCII
/// byte requires quotes; character iteration preserves the original UTF-8
/// while escaping apostrophes inside those quotes.
pub(crate) fn quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value.bytes().all(is_safe_shell_word_byte) {
        return value.to_string();
    }

    let mut output = String::from("'");
    for ch in value.chars() {
        if ch == '\'' {
            output.push_str("'\\''");
        } else {
            output.push(ch);
        }
    }
    output.push('\'');
    output
}

pub(crate) fn join(argv: &[String]) -> String {
    argv.iter()
        .map(|word| quote(word))
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_safe_shell_word_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'a'..=b'z'
            | b'A'..=b'Z'
            | b'0'..=b'9'
            | b'_'
            | b'@'
            | b'%'
            | b'+'
            | b'='
            | b':'
            | b','
            | b'.'
            | b'/'
            | b'-'
    )
}
