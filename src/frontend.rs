//! The `libxsh` frontend contract.
//!
//! The submodules below are the canonical import paths for source loading,
//! syntax representations, semantic checking, and diagnostics. AST/CST and
//! checker items are first-party tooling APIs; their current representations
//! remain coupled to the compiler pipeline.

pub mod check {
    pub use crate::sema::check::{
        AnnotationFact, AnnotationFactKind, CheckOptions, CheckOutput, Checker,
        CompactBodyProbeOutput, CompactDeclOutput, CompactFunctionSig, CompactTypeDefInfo,
        ErrorFamilyInfo, ErrorVariantInfo, TagVariantInfo,
    };
    pub use crate::sema::records::record_schemas;
    pub use crate::sema::types::{CallableParamType, CallableType, ModuleExportType, Type};
}

pub mod load {
    pub use crate::loader::{
        CheckedEntry, CompactFileDeclarationSummary, CompactFileExport, CompactFileImport,
        CompactFileUnit, CompactModuleGraph, CompactModuleImportEdge, EntrySource,
        entry_source_from_bytes, entry_source_from_text, module_key, parse_load_check_bytes,
        parse_load_check_entry_source, parse_load_check_entry_source_with_token_table,
        parse_load_check_file, parse_load_check_text, parse_load_entry_source_arena_only,
        parse_load_entry_source_compact_file_unit, parse_load_entry_source_shared_arena_only,
        parse_script, parse_script_with_module_roots, resolve_user_module,
    };
}

pub mod source {
    pub use crate::source::{
        SourceFile, SourceId, SourceLoadError, SourceLocation, SourceMap, Span,
    };
}

pub mod symbols {
    pub use crate::symbol::{
        Name, NameText, QualifiedName, Symbol, SymbolOwner, SymbolOwnerGuard, dynamic_symbol_stats,
    };
}

pub mod syntax {
    // These representations are intentionally exposed as a tooling tier. The
    // façade makes their ownership explicit without promising arena-layout
    // stability to arbitrary host applications.
    pub use crate::syntax::{arena, cst, lexer, literal, node, parser, token};
}
