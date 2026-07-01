#![allow(clippy::single_call_fn)]

use super::app::expand_word_to_string;
use super::session::Session;
use super::shell::ShellParser;
use rustc_hash::FxHashMap;
use std::path::Path;
use std::path::PathBuf;

pub(super) fn select(session: &Session, query: &str) -> Result<PathBuf, String> {
    let direct = PathBuf::from(query);
    let direct = if direct.is_absolute() {
        direct
    } else {
        session.cwd.join(direct)
    };
    if direct.is_dir() {
        return Ok(direct);
    }

    let terms = query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return Err("z: expected query".to_string());
    }

    let mut visits: FxHashMap<PathBuf, VisitScore> = FxHashMap::default();
    for index in 0..session.history.len() {
        let entry = session.history.get(index);
        let Some(path) = history_jump_path(session, entry) else {
            continue;
        };
        if !path.is_dir() {
            continue;
        }
        let score = visits.entry(path).or_default();
        score.count += 1;
        score.recent = index;
    }

    visits
        .into_iter()
        .filter_map(|(path, visit)| {
            let match_score = path_match_score(&path, &terms)?;
            let recency = visit.recent as i64 * 1000;
            let frequency = visit.count as i64 * 25;
            Some((path, match_score + recency + frequency))
        })
        .max_by(|(left_path, left_score), (right_path, right_score)| {
            left_score
                .cmp(right_score)
                .then_with(|| {
                    right_path
                        .components()
                        .count()
                        .cmp(&left_path.components().count())
                })
                .then_with(|| right_path.cmp(left_path))
        })
        .map(|(path, _)| path)
        .ok_or_else(|| format!("z: no match for '{query}'"))
}

#[derive(Default)]
struct VisitScore {
    count: usize,
    recent: usize,
}

fn history_jump_path(session: &Session, entry: &str) -> Option<PathBuf> {
    let line = ShellParser::new(entry).parse_line().ok()?;
    if line.chains.len() != 1 {
        return None;
    }
    let command = line.chains[0].pipeline.commands.first()?;
    if command.words.len() != 2 || !command.redirections.is_empty() {
        return None;
    }
    let name = command.words[0].text();
    if name != "cd" && name != "z" {
        return None;
    }
    let path = expand_word_to_string(session, &command.words[1]).ok()?;
    let path = PathBuf::from(path);
    let path = if path.is_absolute() {
        path
    } else {
        session.cwd.join(path)
    };
    Some(path)
}

fn path_match_score(path: &Path, terms: &[String]) -> Option<i64> {
    let components = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect::<Vec<_>>();
    for term in terms {
        if !components
            .iter()
            .any(|component| component.contains(term.as_str()))
        {
            return None;
        }
    }
    let basename = components.last().cloned().unwrap_or_default();
    let exact = terms.iter().filter(|term| **term == basename).count() as i64;
    Some(terms.len() as i64 * 100 + exact * 500)
}
