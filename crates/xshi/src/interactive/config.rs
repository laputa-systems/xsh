#![allow(clippy::single_call_fn)]

use super::alias;
use super::app::{expand_word_to_string, valid_env_name};
use super::session::{Session, set_env_bytes};
use super::shell::{ShellToken, lex_shell};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const CONFIG_PATH: &str = ".config/xshi/config.ish";
const PROFILE_PATH: &str = "/etc/profile";

pub(super) fn load_profile(session: &mut Session, stderr: &mut dyn Write) {
    let path = env::var_os("XSHI_PROFILE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(PROFILE_PATH));
    load_profile_path(session, &path, stderr);
}

fn load_profile_path(session: &mut Session, path: &Path, stderr: &mut dyn Write) {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return,
        Err(err) => {
            writeln!(
                stderr,
                "xshi: failed to read profile {}: {err}",
                path.display()
            )
            .ok();
            return;
        }
    };

    for line in text.lines() {
        apply_profile_line(session, line);
    }
}

fn apply_profile_line(session: &mut Session, line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return;
    }

    let assignment = trimmed
        .strip_prefix("export ")
        .map(str::trim)
        .unwrap_or(trimmed);
    let Some((name, value)) = assignment.split_once('=') else {
        return;
    };
    let name = name.trim();
    if !valid_env_name(name) {
        return;
    }

    let value = expand_config_vars(session, unquote_profile_value(value.trim()));
    set_env_bytes(&mut session.env, name.as_bytes(), value.as_bytes());
}

fn unquote_profile_value(value: &str) -> &str {
    if value.len() >= 2 {
        if let Some(stripped) = value
            .strip_prefix('"')
            .and_then(|item| item.strip_suffix('"'))
        {
            return stripped;
        }

        if let Some(stripped) = value
            .strip_prefix('\'')
            .and_then(|item| item.strip_suffix('\''))
        {
            return stripped;
        }
    }

    value
}

fn expand_config_vars(session: &Session, value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            out.push(ch);
            continue;
        }
        let mut name = String::new();
        if chars.peek() == Some(&'{') {
            chars.next();
            while chars.peek().is_some_and(|ch| *ch != '}') {
                name.push(chars.next().unwrap());
            }
            if chars.peek() == Some(&'}') {
                chars.next();
            }
        } else {
            while chars
                .peek()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            {
                name.push(chars.next().unwrap());
            }
        }
        if name.is_empty() {
            out.push('$');
        } else if let Some(value) = session.env.get(name.as_bytes()) {
            out.push_str(&String::from_utf8_lossy(value));
        }
    }
    out
}

/// Loads `~/.config/xshi/config.ish`, or `path` when one is given explicitly.
///
/// Two directives are understood: `set NAME value` and `alias name word...`.
/// Values expand like any command word (tilde, variables, substitutions).
/// Blank lines and `#` comments are skipped; a bad line warns and loading
/// continues. A missing default config is not an error; a missing explicit one
/// is.
pub(super) fn load_config(session: &mut Session, stderr: &mut dyn Write) {
    let Some(home) = &session.home else {
        return;
    };
    let path = config_path_for(home);
    load_config_path(session, &path, false, stderr);
}

fn config_path_for(home: &Path) -> PathBuf {
    home.join(CONFIG_PATH)
}

pub(super) fn load_config_path(
    session: &mut Session,
    path: &Path,
    explicit: bool,
    stderr: &mut dyn Write,
) {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            if explicit || err.kind() != io::ErrorKind::NotFound {
                writeln!(stderr, "xshi: {}: {err}", path.display()).ok();
            }
            return;
        }
    };

    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lineno = index + 1;
        if let Some(rest) = line.strip_prefix("set ") {
            apply_set(session, rest.trim(), lineno, path, stderr);
        } else if let Some(rest) = line.strip_prefix("alias ") {
            apply_alias(session, rest.trim(), lineno, path, stderr);
        } else {
            writeln!(
                stderr,
                "xshi: {}:{lineno}: unrecognized directive: {line}",
                path.display()
            )
            .ok();
        }
    }
}

fn apply_set(
    session: &mut Session,
    rest: &str,
    lineno: usize,
    path: &Path,
    stderr: &mut dyn Write,
) {
    let (name, value_source) = match rest.split_once(char::is_whitespace) {
        Some((name, value)) => (name.trim(), value.trim()),
        None => (rest, ""),
    };
    if name.is_empty() {
        writeln!(
            stderr,
            "xshi: {}:{lineno}: set: missing variable name",
            path.display()
        )
        .ok();
        return;
    }

    let expanded = if value_source.is_empty() {
        String::new()
    } else {
        // The value is one shell word: quotes, tilde, variables, and
        // substitutions behave as they do at the prompt.
        let word = match lex_shell(value_source) {
            Ok(tokens) => match tokens.into_iter().next() {
                Some(ShellToken::Word(word)) => Some(word),
                _ => None,
            },
            Err(_) => None,
        };
        match word {
            Some(word) => match expand_word_to_string(session, &word) {
                Ok(text) => text,
                Err(error) => {
                    writeln!(
                        stderr,
                        "xshi: {}:{lineno}: set {name}: expansion error: {}",
                        path.display(),
                        error.message()
                    )
                    .ok();
                    return;
                }
            },
            None => value_source.to_string(),
        }
    };

    set_env_bytes(&mut session.env, name.as_bytes(), expanded.as_bytes());
    if name == "HOME" || name == "USER" {
        session.sync_prompt_identity();
    }
}

fn apply_alias(
    session: &mut Session,
    rest: &str,
    lineno: usize,
    path: &Path,
    stderr: &mut dyn Write,
) {
    let mut words = alias::lex_words(rest);
    if words.is_empty() {
        writeln!(
            stderr,
            "xshi: {}:{lineno}: alias: missing name",
            path.display()
        )
        .ok();
        return;
    }
    let name = words.remove(0);
    if words.is_empty() {
        writeln!(
            stderr,
            "xshi: {}:{lineno}: alias: missing expansion for '{name}'",
            path.display()
        )
        .ok();
        return;
    }
    session.aliases.set(name, words);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(text: &str) -> (Session, String) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.ish");
        fs::write(&path, text).expect("write config");
        let mut session = Session::for_test();
        set_env_bytes(&mut session.env, b"HOME", b"/home/user");
        let mut stderr = Vec::new();
        load_config_path(&mut session, &path, true, &mut stderr);
        (
            session,
            String::from_utf8(stderr).expect("utf8 diagnostics"),
        )
    }

    #[test]
    fn set_and_alias_directives_expand_values_and_keep_words() {
        let (session, stderr) = load(
            "# comment\n\nset EDITOR nvim\nset PAGER \"less -R\"\nset BIN $HOME/bin\nalias ll l\nalias gs git status -sb\n",
        );
        assert_eq!(stderr, "");
        let get = |name: &str| {
            session
                .env
                .get(name.as_bytes())
                .map(|value| String::from_utf8_lossy(value).into_owned())
        };
        assert_eq!(get("EDITOR").as_deref(), Some("nvim"));
        assert_eq!(get("PAGER").as_deref(), Some("less -R"));
        assert_eq!(get("BIN").as_deref(), Some("/home/user/bin"));
        assert_eq!(session.aliases.get("ll"), Some(&["l".to_string()][..]));
        assert_eq!(
            session.aliases.get("gs"),
            Some(&["git".to_string(), "status".to_string(), "-sb".to_string()][..])
        );
    }

    #[test]
    fn config_path_uses_non_utf8_home() {
        use std::os::unix::ffi::OsStringExt;
        let raw = std::ffi::OsString::from_vec(vec![
            b'/', b't', b'm', b'p', b'/', 0xf0, 0x80, 0x80, b'x',
        ]);
        let path = config_path_for(Path::new(&raw));
        assert_eq!(path, PathBuf::from(raw).join(".config/xshi/config.ish"));
    }

    #[test]
    fn bad_lines_warn_with_position_and_loading_continues() {
        let (session, stderr) = load("bogus line\nalias\nalias lonely\nset\nset OK yes\n");
        assert!(
            stderr.contains(":1: unrecognized directive: bogus line"),
            "{stderr}"
        );
        assert!(stderr.contains("unrecognized directive: alias"), "{stderr}");
        assert!(
            stderr.contains(":3: alias: missing expansion for 'lonely'"),
            "{stderr}"
        );
        assert!(stderr.contains("unrecognized directive: set"), "{stderr}");
        assert_eq!(
            session.env.get(b"OK".as_slice()).map(Vec::as_slice),
            Some(&b"yes"[..])
        );
    }
}
