#![allow(clippy::single_call_fn)]

use super::app::{valid_env_name, validate_alias_source};
use super::session::{Session, set_env_bytes};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use xsh::runtime::value::Value;
use xsh::source::{SourceId, Span};

const CONFIG_PATH: &str = ".config/xshi/config.ini";
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
    if name == "PATH" {
        session.refresh_path_commands();
    }
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

pub(super) fn load_config(session: &mut Session, stderr: &mut dyn Write) {
    let Some(home) = &session.home else {
        return;
    };
    let path = home.join(CONFIG_PATH);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return,
        Err(err) => {
            writeln!(
                stderr,
                "xshi: failed to read config {}: {err}",
                path.display()
            )
            .ok();
            return;
        }
    };
    let span = Span::new(SourceId::new(0), 0, 0);
    let value = match xsh::modules::ini::decode(&text, span) {
        Ok(value) => value,
        Err(err) => {
            writeln!(stderr, "xshi: failed to parse config: {}", err.message).ok();
            return;
        }
    };
    let Value::Record(fields) = value else {
        writeln!(stderr, "xshi: config must be an INI record").ok();
        return;
    };
    apply_config_record(session, &fields, stderr);
}

fn apply_config_record(
    session: &mut Session,
    fields: &xsh::runtime::value::RecordMap,
    stderr: &mut dyn Write,
) {
    for (name, value) in fields {
        match name {
            "env" => apply_config_env(session, value, stderr),
            "aliases" => apply_config_aliases(session, value, stderr),
            other => {
                writeln!(stderr, "xshi: ignoring unknown config field '{other}'").ok();
            }
        }
    }
}

fn apply_config_env(session: &mut Session, value: &Value, stderr: &mut dyn Write) {
    let Value::Record(env_section) = value else {
        writeln!(stderr, "xshi: config [env] must be a section").ok();
        return;
    };
    for (name, value) in env_section {
        let Value::Str(value) = value else {
            writeln!(stderr, "xshi: skipping invalid env entry").ok();
            continue;
        };
        let name = name.to_ascii_uppercase();
        if !valid_env_name(&name) {
            writeln!(stderr, "xshi: skipping invalid env entry").ok();
            continue;
        }
        let expanded = expand_config_vars(session, value);
        set_env_bytes(&mut session.env, name.as_bytes(), expanded.as_bytes());
        if name == "PATH" {
            session.refresh_path_commands();
        }
    }
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

fn apply_config_aliases(session: &mut Session, value: &Value, stderr: &mut dyn Write) {
    let Value::Record(aliases_section) = value else {
        writeln!(stderr, "xshi: config [aliases] must be a section").ok();
        return;
    };
    for (name, source) in aliases_section {
        let Value::Str(source) = source else {
            writeln!(stderr, "xshi: skipping invalid alias entry").ok();
            continue;
        };
        let name = name.to_string();
        let source = source.to_string();
        if validate_alias_source(&name, &source).is_ok() {
            session.aliases.insert(name, source);
        } else {
            writeln!(stderr, "xshi: skipping invalid alias entry").ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::session::set_env_bytes;
    use super::*;
    use std::sync::Arc;
    use xsh::runtime::value::RecordMap;

    #[test]
    fn apply_config_env_uppercases_lowered_ini_keys() {
        let mut session = Session::new();
        set_env_bytes(&mut session.env, b"PATH", b"/usr/bin");
        set_env_bytes(&mut session.env, b"HOME", b"/home/user");

        let mut section = RecordMap::new();
        section.insert(
            Arc::from("path"),
            Value::Str("$HOME/.cargo/bin:$PATH".into()),
        );
        let value = Value::Record(section);

        apply_config_env(&mut session, &value, &mut Vec::new());

        assert_eq!(
            session
                .env
                .get(b"PATH".as_slice())
                .map(|v| String::from_utf8_lossy(v).to_string()),
            Some("/home/user/.cargo/bin:/usr/bin".to_string()),
            "config key 'path' must be stored as uppercase 'PATH' \
             so standard tools and $PATH expansion see it",
        );
    }
}
