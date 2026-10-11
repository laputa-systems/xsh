use rustc_hash::FxHashMap;

/// Maps alias names to their expansion as raw shell words.
#[derive(Clone, Default)]
pub(super) struct AliasMap {
    map: FxHashMap<String, Vec<String>>,
}

impl AliasMap {
    pub(super) fn set(&mut self, name: String, expansion: Vec<String>) {
        self.map.insert(name, expansion);
    }

    pub(super) fn get(&self, name: &str) -> Option<&[String]> {
        self.map.get(name).map(Vec::as_slice)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Expands the command name at the start of a command line. Returns the
    /// expanded line, or the original if no alias matched. Non-recursive: only
    /// the first word is checked.
    pub(super) fn expand_line<'a>(&self, line: &'a str) -> std::borrow::Cow<'a, str> {
        let trimmed = line.trim_start();
        let Some(command_name) = trimmed.split_whitespace().next() else {
            return std::borrow::Cow::Borrowed(line);
        };
        if let Some(expansion) = self.get(command_name) {
            let leading_ws = &line[..line.len() - trimmed.len()];
            let rest = &trimmed[command_name.len()..];
            let expanded = expansion.join(" ");
            std::borrow::Cow::Owned(format!("{leading_ws}{expanded}{rest}"))
        } else {
            std::borrow::Cow::Borrowed(line)
        }
    }
}

/// Lexes a shell fragment into its raw word tokens, preserving quoting and
/// substitutions in the returned strings. Lexing stops at the first operator.
pub(super) fn lex_words(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut words = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if matches!(bytes[i], b'|' | b'&' | b';' | b'<' | b'>' | b'(' | b')') {
            break;
        }
        let start = i;
        i = scan_word(bytes, i);
        words.push(source[start..i].to_string());
    }
    words
}

/// Returns the index just past the word beginning at `i`.
fn scan_word(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        match bytes[i] {
            b if b.is_ascii_whitespace() => break,
            b'|' | b'&' | b';' | b'<' | b'>' | b'(' | b')' => break,
            b'\\' => i = (i + 2).min(bytes.len()),
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    match bytes[i] {
                        b'\\' => i = (i + 2).min(bytes.len()),
                        b'$' if bytes.get(i + 1) == Some(&b'(') => {
                            i = scan_substitution(bytes, i + 1)
                        }
                        b'`' => i = scan_backtick(bytes, i),
                        _ => i += 1,
                    }
                }
                i = (i + 1).min(bytes.len());
            }
            b'$' if bytes.get(i + 1) == Some(&b'(') => i = scan_substitution(bytes, i + 1),
            b'`' => i = scan_backtick(bytes, i),
            _ => i += 1,
        }
    }
    i
}

/// `i` is at the opening `(`; returns the index just past the matching `)`.
fn scan_substitution(bytes: &[u8], mut i: usize) -> usize {
    let mut depth = 0_usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            b'\\' => i += 1,
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

fn scan_backtick(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() && bytes[i] != b'`' {
        if bytes[i] == b'\\' {
            i += 1;
        }
        i += 1;
    }
    (i + 1).min(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_and_override() {
        let mut aliases = AliasMap::default();
        aliases.set("ll".into(), vec!["ls".into(), "-l".into()]);
        assert_eq!(
            aliases.get("ll"),
            Some(&["ls".to_string(), "-l".to_string()][..])
        );
        aliases.set("ll".into(), vec!["l".into()]);
        assert_eq!(aliases.get("ll"), Some(&["l".to_string()][..]));
        assert!(aliases.get("missing").is_none());
    }

    #[test]
    fn expansion_is_first_word_only_and_non_recursive() {
        let mut aliases = AliasMap::default();
        aliases.set("gs".into(), vec!["git".into(), "status".into()]);
        aliases.set("a".into(), vec!["a".into(), "-x".into()]);
        assert_eq!(aliases.expand_line("gs -sb"), "git status -sb");
        assert_eq!(aliases.expand_line("  gs"), "  git status");
        assert_eq!(aliases.expand_line("echo gs"), "echo gs");
        assert_eq!(aliases.expand_line("a b"), "a -x b");
    }

    #[test]
    fn lex_words_preserves_quoting_and_substitutions() {
        assert_eq!(lex_words("git status -sb"), ["git", "status", "-sb"]);
        assert_eq!(
            lex_words("echo 'a b' \"c d\""),
            ["echo", "'a b'", "\"c d\""]
        );
        assert_eq!(
            lex_words("echo $(date +%s) `x y`"),
            ["echo", "$(date +%s)", "`x y`"]
        );
        assert_eq!(lex_words("ls -l | more"), ["ls", "-l"]);
    }
}
