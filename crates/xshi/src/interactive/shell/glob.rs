#![allow(clippy::single_call_fn)]

use crate::xshi::interactive::app::ExpansionError;
use crate::xshi::interactive::session::Session;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn has_glob_meta(text: &str) -> bool {
    text.as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'*' | b'?'))
}

pub(crate) fn expand_glob(session: &Session, pattern: &str) -> Result<Vec<String>, ExpansionError> {
    if pattern.as_bytes().contains(&0) {
        return Err(ExpansionError::usage("glob patterns cannot contain NUL"));
    }
    let absolute = pattern.starts_with('/');
    let components = pattern.split('/').collect::<Vec<_>>();
    let mut matches = Vec::new();
    let base = if absolute {
        PathBuf::from("/")
    } else {
        session.cwd.clone()
    };
    let output = if absolute {
        PathBuf::from("/")
    } else {
        PathBuf::new()
    };
    let start = usize::from(absolute);
    expand_glob_components(&base, &output, &components, start, &mut matches)
        .map_err(|message| ExpansionError::usage(format!("glob: {message}")))?;
    matches.sort_unstable();
    matches.dedup();
    if matches.is_empty() {
        return Err(ExpansionError::usage(format!(
            "glob pattern matched no paths: {pattern}"
        )));
    }
    Ok(matches)
}

fn expand_glob_components(
    base: &Path,
    output: &Path,
    components: &[&str],
    index: usize,
    matches: &mut Vec<String>,
) -> Result<(), String> {
    if index >= components.len() {
        if !output.as_os_str().is_empty() {
            matches.push(output.to_string_lossy().into_owned());
        }
        return Ok(());
    }

    let component = components[index];
    if component.is_empty() {
        return expand_glob_components(base, output, components, index + 1, matches);
    }

    if component == "**" {
        expand_glob_components(base, output, components, index + 1, matches)?;
        for entry in read_sorted_dir(base)? {
            if !entry.is_dir || entry.name.starts_with('.') {
                continue;
            }
            expand_glob_components(
                &entry.path,
                &output.join(&entry.name),
                components,
                index,
                matches,
            )?;
        }
        return Ok(());
    }

    if has_glob_meta(component) {
        let allow_dot = component.starts_with('.');
        for entry in read_sorted_dir(base)? {
            if !allow_dot && entry.name.starts_with('.') {
                continue;
            }
            if glob_component_matches(component, &entry.name) {
                expand_glob_components(
                    &entry.path,
                    &output.join(&entry.name),
                    components,
                    index + 1,
                    matches,
                )?;
            }
        }
        return Ok(());
    }

    let next = base.join(component);
    if next.exists() {
        expand_glob_components(
            &next,
            &output.join(component),
            components,
            index + 1,
            matches,
        )?;
    }
    Ok(())
}

struct GlobEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

fn read_sorted_dir(path: &Path) -> Result<Vec<GlobEntry>, String> {
    let mut entries = Vec::new();
    let read = fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))?;
    for entry in read {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        entries.push(GlobEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: entry.path(),
            is_dir: file_type.is_dir(),
        });
    }
    entries.sort_unstable_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

fn glob_component_matches(pattern: &str, name: &str) -> bool {
    glob_component_matches_inner(pattern.as_bytes(), name.as_bytes())
}

fn glob_component_matches_inner(pattern: &[u8], name: &[u8]) -> bool {
    match pattern.first().copied() {
        None => name.is_empty(),
        Some(b'*') => {
            glob_component_matches_inner(&pattern[1..], name)
                || (!name.is_empty() && glob_component_matches_inner(pattern, &name[1..]))
        }
        Some(b'?') => !name.is_empty() && glob_component_matches_inner(&pattern[1..], &name[1..]),
        Some(byte) => {
            name.first().copied() == Some(byte)
                && glob_component_matches_inner(&pattern[1..], &name[1..])
        }
    }
}
