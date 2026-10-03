//! Failure minimization.
//!
//! Generated programs shrink structurally: statements are deleted, compound
//! statements are replaced by their bodies, and unused declarations are
//! dropped, keeping each candidate the predicate still reports as failing.
//! Text inputs (mutated corpus files) shrink by line deletion.

use crate::ast::{Block, Program, Stmt};

/// Every statement position, as a path of statement indices through nested
/// blocks. Function bodies come first, then the entry body.
#[derive(Clone, Debug)]
struct Site {
    function: Option<usize>,
    path: Vec<(usize, usize)>,
}

fn child_blocks(stmt: &Stmt) -> Vec<&Block> {
    match stmt {
        Stmt::If { then, otherwise, .. } => {
            let mut blocks = vec![then];
            if let Some(otherwise) = otherwise {
                blocks.push(otherwise);
            }
            blocks
        }
        Stmt::For { body, .. } | Stmt::While { body, .. } | Stmt::Block(body) => vec![body],
        Stmt::Match { arms, .. } => arms.iter().map(|(_, body)| body).collect(),
        _ => Vec::new(),
    }
}

fn child_blocks_mut(stmt: &mut Stmt) -> Vec<&mut Block> {
    match stmt {
        Stmt::If { then, otherwise, .. } => {
            let mut blocks = vec![then];
            if let Some(otherwise) = otherwise {
                blocks.push(otherwise);
            }
            blocks
        }
        Stmt::For { body, .. } | Stmt::While { body, .. } | Stmt::Block(body) => vec![body],
        Stmt::Match { arms, .. } => arms.iter_mut().map(|(_, body)| body).collect(),
        _ => Vec::new(),
    }
}

fn collect_sites(block: &Block, function: Option<usize>, prefix: &[(usize, usize)], out: &mut Vec<Site>) {
    for (index, stmt) in block.stmts.iter().enumerate() {
        let mut path = prefix.to_vec();
        path.push((0, index));
        out.push(Site { function, path });
        for (child, nested) in child_blocks(stmt).into_iter().enumerate() {
            let mut nested_prefix = prefix.to_vec();
            nested_prefix.push((child, index));
            collect_sites(nested, function, &nested_prefix, out);
        }
    }
}

fn block_at<'a>(program: &'a mut Program, site: &Site) -> Option<&'a mut Block> {
    let mut block = match site.function {
        Some(index) => &mut program.functions.get_mut(index)?.body,
        None => &mut program.body,
    };
    let (last, parents) = site.path.split_last()?;
    let _ = last;
    for (child, index) in parents {
        let stmt = block.stmts.get_mut(*index)?;
        block = child_blocks_mut(stmt).into_iter().nth(*child)?;
    }
    Some(block)
}

enum Edit {
    Delete,
    Inline(usize),
    DropElse,
}

fn apply(program: &Program, site: &Site, edit: &Edit) -> Option<Program> {
    let mut candidate = program.clone();
    let block = block_at(&mut candidate, site)?;
    let index = site.path.last()?.1;
    match edit {
        Edit::Delete => {
            block.stmts.remove(index);
        }
        Edit::Inline(child) => {
            let stmt = block.stmts.get(index)?.clone();
            let inner = child_blocks(&stmt).into_iter().nth(*child)?.clone();
            if inner.tail.is_some() {
                return None;
            }
            block.stmts.splice(index..=index, inner.stmts);
        }
        Edit::DropElse => match block.stmts.get_mut(index)? {
            Stmt::If { otherwise, .. } if otherwise.is_some() => *otherwise = None,
            _ => return None,
        },
    }
    Some(candidate)
}

/// Shrinks `program` while `still_fails` holds, within `budget` predicate
/// evaluations.
pub fn shrink_program(program: &Program, mut still_fails: impl FnMut(&Program) -> bool, budget: usize) -> Program {
    let mut current = program.clone();
    let mut spent = 0;
    loop {
        let mut improved = false;
        // Whole functions first. Calls name functions by index, so only the
        // last one can go; a candidate that still calls it is rejected by the
        // predicate.
        while !current.functions.is_empty() && spent < budget {
            let mut candidate = current.clone();
            candidate.functions.pop();
            spent += 1;
            if !still_fails(&candidate) {
                break;
            }
            current = candidate;
            improved = true;
        }
        let mut sites = Vec::new();
        for (index, function) in current.functions.iter().enumerate() {
            collect_sites(&function.body, Some(index), &[], &mut sites);
        }
        collect_sites(&current.body, None, &[], &mut sites);
        // Later statements first, so deleting one keeps earlier sites valid.
        for site in sites.iter().rev() {
            for edit in [Edit::Delete, Edit::Inline(0), Edit::Inline(1), Edit::DropElse] {
                if spent >= budget {
                    return current;
                }
                let Some(candidate) = apply(&current, site, &edit) else { continue };
                spent += 1;
                if still_fails(&candidate) {
                    current = candidate;
                    improved = true;
                    break;
                }
            }
        }
        if !improved {
            return current;
        }
    }
}

/// Shrinks text by deleting line ranges while `still_fails` holds.
pub fn shrink_text(text: &str, mut still_fails: impl FnMut(&str) -> bool, budget: usize) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    let mut chunk = lines.len().div_ceil(2).max(1);
    let mut spent = 0;
    while chunk >= 1 && spent < budget {
        let mut start = 0;
        let mut improved = false;
        while start < lines.len() && spent < budget {
            let end = (start + chunk).min(lines.len());
            let mut candidate = lines.clone();
            candidate.drain(start..end);
            let joined = candidate.join("\n") + "\n";
            spent += 1;
            if !candidate.is_empty() && still_fails(&joined) {
                lines = candidate;
                improved = true;
            } else {
                start = end;
            }
        }
        if !improved {
            if chunk == 1 {
                break;
            }
            chunk = chunk.div_ceil(2);
        }
    }
    lines.join("\n") + "\n"
}
