//! The `libxsh` frontend contract.
//!
//! The submodules below are the canonical import paths for source loading,
//! syntax representations, semantic checking, and diagnostics. AST/CST and
//! checker items are first-party tooling APIs; their current representations
//! remain coupled to the compiler pipeline.

pub mod check {
    pub use crate::sema::arguments::{
        ArgumentExpansionError, ArgumentValueSource, ExpandedArgument, StaticArgumentBinding,
        bind_static_arguments, expand_named_arguments,
    };
    pub use crate::sema::check::{
        AnnotationFact, AnnotationFactKind, BindingDeclaration, BindingId, BindingKind,
        CheckOptions, CheckOutput, CheckedProjection, ResolvedBindings,
        CheckedStreamStage, Checker, CompactBodyFacts, CompactDeclOutput, CompactFunctionSig,
        CompactTypeDefInfo, Conversion, EffectDeclarationId, ErrorFamilyInfo, ErrorVariantInfo,
        FunctionEffectFact, MessagePayloadConstructor, ProjectionOperation, RecordRequireMigration,
        RequirementTarget, StatementPosition, StaticCallableAlias, TagVariantInfo,
        call_receiver_text,
    };
    pub use crate::sema::constants::{
        CheckedRecordConstructor, LiteralConstant, PreparedConstants, RecordConstructors,
        SchemaComponent, SchemaExpectation, SchemaInstance, SchemaTypeError,
    };
    pub use crate::sema::records::record_schemas;
    pub use crate::sema::types::{
        CallableParamType, CallableType, ModuleExportType, ModuleType, Type, union_member_error,
    };
}

pub mod load {
    pub use crate::loader::{
        CheckedEntry, CompactFileDeclarationSummary, CompactFileExport, CompactFileImport,
        CompactFileUnit, CompactModuleGraph, CompactModuleImportEdge, EntrySource, StdlibLinkage,
        LoadedImport, LoadedModule, ModuleLoader, UserModuleResolution, add_source_bytes,
        entry_source_from_bytes, entry_source_from_text, module_key, parse_load_check_bytes,
        parse_load_check_entry_source, parse_load_check_entry_source_with_token_table,
        parse_load_check_file, parse_load_check_text, parse_load_entry_source_arena_only,
        parse_load_entry_source_arena_only_with_linkage, parse_load_entry_source_compact_file_unit,
        parse_load_entry_source_shared_arena_only, parse_script, parse_script_with_module_roots,
        prepare_stdlib_catalog_module, resolve_user_module,
    };
    pub use crate::project::{
        PROJECT_CONFIG_FILE_NAME, configured_module_path, default_module_path, module_roots,
        nearest_project_config_dir, project_module_roots, read_project_config,
        resolve_project_path,
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
    pub use crate::syntax::{
        arena, cst, grammar, grouping, highlight, lexer, literal, node, parser, token,
    };
}

/// Embedded standard-library preparation hooks used by the architecture tests.
#[cfg(feature = "native-tests")]
pub mod stdlib_preparation {
    pub use crate::stdlib::counters::{parsed_modules, reset};
}
