//! Stage-split frontend retained-byte and allocation-peak measurements.
//!
//! Stage retained totals come from owned structures. Stage peak live bytes and
//! allocation traffic come from [`crate::mem_track`] when the stats binary
//! installs the counting allocator.

use crate::loader::parse_load_check_text;
use crate::mem_track::{self, AllocTraffic};
use crate::runtime::eval::Evaluator;
use crate::sema::check::CheckOptions;
use crate::source::{SourceId, SourceMap};
use crate::symbol;
use crate::syntax::cst::SyntaxTree;
use crate::syntax::lexer::Lexer;
use std::fs;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StageMetrics {
    pub name: &'static str,
    pub retained_bytes: usize,
    pub item_count: usize,
    pub peak_bytes: usize,
    pub alloc_count: usize,
    pub alloc_bytes: usize,
    pub tracking_active: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileFrontendStats {
    pub path: String,
    pub source_bytes: usize,
    pub stages: Vec<StageMetrics>,
    pub token_count: usize,
    pub token_retained_bytes: usize,
    pub cst_node_count: usize,
    pub cst_retained_bytes: usize,
    pub ast_stmt_count: usize,
    pub ast_expr_count: usize,
    pub ast_pattern_count: usize,
    pub ast_type_count: usize,
    pub ast_extra_items: usize,
    pub ast_retained_bytes: usize,
    pub semantic_type_count: usize,
    pub semantic_retained_bytes: usize,
    pub lowered_function_count: usize,
    pub lowered_constructed_functions: usize,
    pub lowered_statement_count: usize,
    pub lowered_expression_count: usize,
    pub lowered_pattern_count: usize,
    pub lowered_retained_bytes: usize,
    pub lowered_blocker_events: u64,
    pub retained_after_drop_bytes: usize,
    pub dynamic_symbol_count: usize,
    pub dynamic_symbol_bytes: usize,
    pub diagnostics: usize,
    pub components_sum: usize,
    pub reconcile_delta: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CorpusFrontendStats {
    pub roots: Vec<String>,
    pub files: Vec<FileFrontendStats>,
    pub totals: FileFrontendStats,
    pub maxima: FileFrontendStats,
    pub tracking_active: bool,
}

fn stage_from_traffic(
    name: &'static str,
    retained_bytes: usize,
    item_count: usize,
    traffic: AllocTraffic,
) -> StageMetrics {
    StageMetrics {
        name,
        retained_bytes,
        item_count,
        peak_bytes: traffic.peak_bytes,
        alloc_count: traffic.alloc_count,
        alloc_bytes: traffic.alloc_bytes,
        tracking_active: traffic.tracking_active,
    }
}

fn measure_expr_types(
    types: &std::collections::BTreeMap<crate::source::Span, crate::sema::types::Type>,
) -> (usize, usize) {
    let mut bytes = types.len() * size_of::<(crate::source::Span, crate::sema::types::Type)>();
    for ty in types.values() {
        bytes = bytes.saturating_add(ty.retained_bytes());
    }
    (types.len(), bytes)
}

fn source_map_retained_bytes(sources: &SourceMap) -> usize {
    let mut total = size_of::<SourceMap>();
    for file in sources.files() {
        total = total.saturating_add(file.text().len());
        total = total.saturating_add(file.name().len());
    }
    total
}

/// Measure one in-memory source through tokens, CST, AST/check, and lower.
pub fn measure_source(path: &str, source: &str) -> FileFrontendStats {
    let source_bytes = source.len();
    let mut stages = Vec言い聞かせwith_capacity(5);
