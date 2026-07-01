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

#[cfg(test)]
mod tests {
    use super::{join, quote};

    #[test]
    fn quote_keeps_safe_words_unquoted() {
        assert_eq!(quote("install"), "install");
        assert_eq!(quote("-m"), "-m");
        assert_eq!(quote("a/b_c-1.2"), "a/b_c-1.2");
    }

    #[test]
    fn quote_handles_empty_spaces_quotes_and_newlines() {
        assert_eq!(quote(""), "''");
        assert_eq!(quote("two words"), "'two words'");
        assert_eq!(quote("can't"), "'can'\\''t'");
        assert_eq!(quote("a\nb"), "'a\nb'");
    }

    #[test]
    fn join_quotes_each_word_independently() {
        assert_eq!(
            join(&[
                "install".to_string(),
                "two words".to_string(),
                "can't".to_string()
            ]),
            "install 'two words' 'can'\\''t'"
        );
    }
}
