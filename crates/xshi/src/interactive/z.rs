//! Frecency-based directory jumping (`z` builtin).
//! Derives scores from shell history — no separate database needed.

use super::history::History;
use std::io::Write as _;
use std::path::PathBuf;

/// `z [query]`: picks the directory to jump to and returns it; the caller
/// changes into it. Scores come from `cd`/`z` commands in history, weighted by
/// recency. Failures are reported on `stderr` and returned as the status.
pub(super) fn jump(
    args: &[String],
    history: &History,
    home: &str,
    stderr: &mut Vec<u8>,
    announce: &mut Vec<u8>,
) -> Result<PathBuf, i32> {
    let Some(query) = args.first() else {
        // No arguments: go home.
        return Ok(PathBuf::from(home));
    };

    // An existing directory is a direct jump.
    if std::path::Path::new(query).is_dir() {
        return Ok(PathBuf::from(query));
    }

    // Scan history for cd/z commands, scored by recency.
    let query_lower: Vec<u8> = query.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let len = history.len();
    if len == 0 {
        writeln!(stderr, "xshi: z: no match for '{query}'").ok();
        return Err(1);
    }

    let mut scores: Vec<(String, f64)> = Vec::new();
    for i in 0..len {
        let entry = history.get(i);
        let dir = match extract_cd_target(entry) {
            Some(d) if !d.is_empty() && d != "-" && !d.starts_with('-') => d,
            _ => continue,
        };

        // Resolve ~ to home.
        let resolved = if let Some(rest) = dir.strip_prefix("~/") {
            format!("{home}/{rest}")
        } else if dir == "~" {
            home.to_string()
        } else {
            dir.to_string()
        };

        // Recency weight: later entries score higher (i/len gives 0..1).
        let weight = (i as f64 + 1.0) / len as f64;

        if let Some(entry) = scores.iter_mut().find(|(p, _)| *p == resolved) {
            entry.1 += weight;
        } else {
            scores.push((resolved, weight));
        }
    }

    // Keep entries matching the query (case-insensitive substring of the path).
    let mut matches: Vec<(&str, f64)> = scores
        .iter()
        .filter(|(path, _)| {
            let path_lower: Vec<u8> = path.bytes().map(|b| b.to_ascii_lowercase()).collect();
            path_lower
                .windows(query_lower.len())
                .any(|w| w == query_lower.as_slice())
        })
        .filter(|(path, _)| std::path::Path::new(path).is_dir())
        .map(|(path, score)| (path.as_str(), *score))
        .collect();

    matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    match matches.first() {
        Some((path, _)) => {
            writeln!(announce, "{path}").ok();
            Ok(PathBuf::from(*path))
        }
        None => {
            writeln!(stderr, "xshi: z: no match for '{query}'").ok();
            Err(1)
        }
    }
}

/// Extract the target directory from a cd/z history entry.
fn extract_cd_target(entry: &str) -> Option<&str> {
    let trimmed = entry.trim();
    // Handle: "cd dir", "z dir", "cd dir && ...", "cd dir; ..."
    let rest = trimmed
        .strip_prefix("cd ")
        .or_else(|| trimmed.strip_prefix("z "))?;

    // Take just the first argument (stop at whitespace, &&, ||, ;, |)
    let arg = rest
        .trim_start()
        .split(|c: char| c.is_whitespace() || c == '&' || c == '|' || c == ';')
        .next()?;

    if arg.is_empty() { None } else { Some(arg) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_cd_simple() {
        assert_eq!(extract_cd_target("cd /tmp"), Some("/tmp"));
        assert_eq!(extract_cd_target("cd ~/d/ish"), Some("~/d/ish"));
        assert_eq!(extract_cd_target("z ish"), Some("ish"));
    }

    #[test]
    fn extract_cd_from_command_list() {
        assert_eq!(extract_cd_target("cd /tmp && ls"), Some("/tmp"));
        assert_eq!(extract_cd_target("cd src; make"), Some("src"));
    }

    #[test]
    fn extract_cd_no_match() {
        assert_eq!(extract_cd_target("git commit -m fix"), None);
        assert_eq!(extract_cd_target("echo cd"), None);
    }

    #[test]
    fn extract_cd_dash() {
        assert_eq!(extract_cd_target("cd -"), Some("-"));
        // The caller filters out "-"
    }
}
