use crate::diagnostic::{Diagnostic, DiagnosticCode, Label, Severity};
use crate::sema::constants::LiteralConstant;
use crate::sema::types::Type;
use crate::symbol::Name;
use crate::syntax::arena::{
    ArenaParam, ArenaProgram, ArenaStmtKind, ExprId, FunctionDefId, StmtId, TypeExprId,
};

/// What an option holds when the command line does not give it.
#[derive(Clone, Debug)]
pub(crate) enum CliEntryDefault {
    /// A prepared constant: reading it runs no code, and help shows its value.
    Constant(LiteralConstant),
    /// Any other expression. The entry evaluates it as its own parameter
    /// default after the command line is parsed, and only when the option is
    /// absent; help shows its source text.
    Computed(ExprId),
}

#[derive(Clone, Debug)]
pub(crate) struct CliEntryParameter {
    pub name: Name,
    pub ty: Type,
    pub parser_type: String,
    pub default: Option<CliEntryDefault>,
    pub rest: bool,
}

/// Entry metadata carries prepared data and concrete bindings independently
/// of the callable declaration tables.
#[derive(Clone, Debug)]
pub(crate) struct CliEntryPlan {
    pub statement: StmtId,
    pub definition: FunctionDefId,
    /// The subcommand words as typed on the command line, in kebab case.
    /// Empty for a bare `cli main`, which is then the only entry.
    pub path: Vec<String>,
    pub parameters: Vec<CliEntryParameter>,
}

/// The command-line spelling of a subcommand word or option name.
pub(crate) fn cli_word(identifier: &str) -> String {
    identifier.replace('_', "-")
}

pub(crate) fn validate_cli_entry(
    program: &ArenaProgram,
    mut type_of: impl FnMut(&ArenaParam) -> Type,
    mut parser_type: impl FnMut(TypeExprId) -> Option<String>,
    mut default_value: impl FnMut(ExprId) -> Option<LiteralConstant>,
) -> (Vec<CliEntryPlan>, Vec<Diagnostic>) {
    let roots = program.statement_ids().collect::<Vec<_>>();
    // A workspace arena can hold other entry scripts that this program neither
    // is nor imports; only the entry source and its modules are validated.
    let sources = roots
        .iter()
        .copied()
        .chain(
            program
                .modules
                .iter()
                .flat_map(|module| program.module_statements(module)),
        )
        .map(|id| program.arena.stmt(id).span.source_id)
        .collect::<std::collections::BTreeSet<_>>();
    let mut diagnostics = Vec::new();
    let mut entries = Vec::<CliEntryPlan>::new();
    // Nested `cli main` is a parse error (`parse.cli-entry-scope`), and the
    // arena may hold other modules' statements (`xsht lint` shares one arena
    // across entry bundles), so only this program's top level is an entry.
    for &id in &roots {
        let statement = program.arena.stmt(id);
        let ArenaStmtKind::CliMain(definition) = statement.kind else {
            continue;
        };
        if !sources.contains(&statement.span.source_id) {
            continue;
        }
        let mut error = |span, message: &str| {
            diagnostics.push(
                Diagnostic::new(Severity::Error, message)
                    .with_code(DiagnosticCode::CheckCliEntry)
                    .with_label(Label::primary(span, message)),
            )
        };
        let function = program.arena.function_def(definition);
        // The parser joins the entry's words with single spaces.
        let name = function.name.as_str();
        let mut words = name.split(' ');
        if words.next() != Some("main") {
            error(statement.span, "a signature CLI entry must be named `main`");
        }
        let path = words.map(cli_word).collect::<Vec<_>>();
        if let Some(other) = entries.first() {
            if path.is_empty() || other.path.is_empty() {
                error(
                    statement.span,
                    "a bare `cli main` cannot be declared beside another CLI entry; name every entry by a subcommand path, as in `cli main build(...)`",
                );
            } else if entries.iter().any(|other| other.path == path) {
                error(
                    statement.span,
                    "this subcommand path is already declared; subcommand words must be unique after snake_case maps to kebab-case",
                );
            } else if entries
                .iter()
                .any(|other| other.path.starts_with(&path) || path.starts_with(&other.path))
            {
                error(
                    statement.span,
                    "a subcommand path cannot continue another entry's path: a word is either an entry or a group of entries",
                );
            }
        }
        if path.iter().any(|word| word == "help") {
            error(statement.span, "`help` is reserved by the CLI parser");
        }
        let another_main = roots.iter().any(|root| {
            let kind = match program.arena.stmt(*root).kind {
                ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind,
                kind => kind,
            };
            match kind {
                ArenaStmtKind::ProcDef(def)
                | ArenaStmtKind::PureDef(def)
                | ArenaStmtKind::StreamDef(def) => program.arena.function_def(def).name == "main",
                _ => false,
            }
        });
        if another_main {
            error(
                statement.span,
                "a signature CLI entry cannot coexist with another `main` declaration",
            );
        }
        let mut parameters = Vec::new();
        let mut names = std::collections::BTreeSet::new();
        let mut default_seen = false;
        let parameter_count = program.arena.params(function.params).len();
        for (index, parameter) in program.arena.params(function.params).iter().enumerate() {
            let span = program.arena.span(parameter.span);
            let ty = type_of(parameter);
            if !names.insert(cli_word(&parameter.name.as_str())) {
                error(
                    span,
                    "CLI parameter spellings must be unique after snake_case maps to kebab-case",
                );
            }
            if parameter.rest && index + 1 != parameter_count {
                error(span, "a CLI rest parameter must be last");
            }
            if parameter.rest && parameter.default.is_some() {
                error(span, "a CLI rest parameter cannot have a default");
            }
            if !parameter.rest && parameter.default.is_none() && default_seen {
                error(
                    span,
                    "required CLI positionals must precede defaulted options",
                );
            }
            default_seen |= parameter.default.is_some();
            let parser_type = if parameter.ty_defaulted {
                inferred_parser_type(&ty)
            } else {
                parser_type(parameter.ty)
            }
            .unwrap_or_default();
            let scalar = |ty: &Type| {
                matches!(
                    ty,
                    Type::Str | Type::Int | Type::UInt | Type::Bool | Type::Path | Type::Duration
                )
            };
            let supported = if parameter.rest {
                matches!(&ty, Type::List(item) if matches!(item.as_ref(), Type::Str | Type::Path))
            } else if parameter.default.is_some() {
                scalar(&ty) || matches!(&ty, Type::List(item) if scalar(item))
            } else {
                scalar(&ty)
            };
            if !supported || parser_type.is_empty() {
                error(
                    span,
                    "CLI parameters require a supported scalar, a defaulted scalar List, or a final rest List[Str]/List[Path]",
                );
            }
            if parameter.name == "help" {
                error(span, "`--help` is reserved by the CLI parser");
            }
            // A default that is not a prepared constant is the entry's own
            // parameter default: the function check types it against the
            // parameter and charges its effects to the entry.
            let constant = parameter.default.and_then(&mut default_value);
            if constant
                .as_ref()
                .is_some_and(|value| !value.matches_data_type(&ty))
            {
                error(span, "a CLI default must match its parameter type");
            }
            if parser_type == "UInt"
                && matches!(constant, Some(LiteralConstant::Int(value)) if value < 0)
            {
                error(span, "a UInt CLI default must be nonnegative");
            }
            if parser_type == "List[UInt]"
                && matches!(&constant, Some(LiteralConstant::List(values)) if values.iter().any(|value| matches!(value, LiteralConstant::Int(value) if *value < 0)))
            {
                error(span, "UInt CLI list defaults must be nonnegative");
            }
            let default = match (constant, parameter.default) {
                (Some(constant), _) => Some(CliEntryDefault::Constant(constant)),
                (None, Some(expression)) => Some(CliEntryDefault::Computed(expression)),
                (None, None) => None,
            };
            parameters.push(CliEntryParameter {
                name: parameter.name,
                ty,
                parser_type,
                default,
                rest: parameter.rest,
            });
        }
        entries.push(CliEntryPlan {
            statement: id,
            definition,
            path,
            parameters,
        });
    }
    if diagnostics.is_empty() {
        (entries, diagnostics)
    } else {
        (Vec::new(), diagnostics)
    }
}

// Inferred annotations carry no parser spelling. Only concrete supported CLI
// domains can supply one; explicit aliases retain their declared parser.
fn inferred_parser_type(ty: &Type) -> Option<String> {
    match ty {
        Type::Str | Type::Int | Type::UInt | Type::Bool | Type::Path | Type::Duration => {
            Some(ty.to_string())
        }
        Type::List(item) => Some(format!("List[{}]", inferred_parser_type(item)?)),
        _ => None,
    }
}
