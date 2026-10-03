use crate::diagnostic::{Diagnostic, Label, Severity};
use crate::sema::constants::LiteralConstant;
use crate::sema::types::Type;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaParam, ArenaProgram, ArenaStmtKind, ExprId, FunctionDefId, StmtId, TypeExprId};

#[derive(Clone, Debug)]
pub(crate) struct CliEntryParameter {
    pub name: Name,
    pub ty: Type,
    pub parser_type: String,
    pub default: Option<LiteralConstant>,
    pub rest: bool,
}

/// Entry metadata carries prepared data and concrete bindings independently
/// of the callable declaration tables.
#[derive(Clone, Debug)]
pub(crate) struct CliEntryPlan {
    pub statement: StmtId,
    pub definition: FunctionDefId,
    pub parameters: Vec<CliEntryParameter>,
}

pub(crate) fn validate_cli_entry(
    program: &ArenaProgram,
    mut type_of: impl FnMut(&ArenaParam) -> Type,
    mut parser_type: impl FnMut(TypeExprId) -> Option<String>,
    mut default_value: impl FnMut(ExprId) -> Option<LiteralConstant>,
) -> (Option<CliEntryPlan>, Vec<Diagnostic>) {
    let roots = program.statement_ids().collect::<Vec<_>>();
    let mut diagnostics = Vec::new();
    let mut entry = None;
    // Nested `cli main` is a parse error (`parse.cli-entry-scope`), and the
    // arena may hold other modules' statements (`xsht lint` shares one arena
    // across entry bundles), so only this program's top level is an entry.
    for &id in &roots {
        let statement = program.arena.stmt(id);
        let ArenaStmtKind::CliMain(definition) = statement.kind else { continue; };
        let mut error = |span, message: &str| diagnostics.push(Diagnostic::new(Severity::Error, message)
            .with_code("check.cli-entry").with_label(Label::primary(span, message)));
        if entry.is_some() { error(statement.span, "only one signature CLI entry may be declared"); }
        let function = program.arena.function_def(definition);
        if function.name != "main" { error(statement.span, "a signature CLI entry must be named `main`"); }
        let another_main = roots.iter().any(|root| {
            let kind = match program.arena.stmt(*root).kind { ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind, kind => kind };
            match kind {
                ArenaStmtKind::ProcDef(def) | ArenaStmtKind::PureDef(def) | ArenaStmtKind::StreamDef(def) => program.arena.function_def(def).name == "main",
                _ => false,
            }
        });
        if another_main { error(statement.span, "a signature CLI entry cannot coexist with another `main` declaration"); }
        let mut parameters = Vec::new();
        let mut names = std::collections::BTreeSet::new();
        let mut default_seen = false;
        let parameter_count = program.arena.params(function.params).len();
        for (index, parameter) in program.arena.params(function.params).iter().enumerate() {
            let span = program.arena.span(parameter.span);
            let ty = type_of(parameter);
            if !names.insert(parameter.name.to_string().replace('_', "-")) { error(span, "CLI parameter spellings must be unique after snake_case maps to kebab-case"); }
            if parameter.rest && index + 1 != parameter_count { error(span, "a CLI rest parameter must be last"); }
            if parameter.rest && parameter.default.is_some() { error(span, "a CLI rest parameter cannot have a default"); }
            if !parameter.rest && parameter.default.is_none() && default_seen { error(span, "required CLI positionals must precede defaulted options"); }
            default_seen |= parameter.default.is_some();
            let parser_type = if parameter.ty_defaulted { inferred_parser_type(&ty) } else { parser_type(parameter.ty) }.unwrap_or_default();
            let scalar = |ty: &Type| matches!(ty, Type::Str | Type::Int | Type::UInt | Type::Bool | Type::Path | Type::Duration);
            let supported = if parameter.rest {
                matches!(&ty, Type::List(item) if matches!(item.as_ref(), Type::Str | Type::Path))
            } else if parameter.default.is_some() {
                scalar(&ty) || matches!(&ty, Type::List(item) if scalar(item))
            } else { scalar(&ty) };
            if !supported || parser_type.is_empty() { error(span, "CLI parameters require a supported scalar, a defaulted scalar List, or a final rest List[Str]/List[Path]"); }
            if parameter.name == "help" { error(span, "`--help` is reserved by the CLI parser"); }
            let default = parameter.default.and_then(&mut default_value);
            if parameter.default.is_some() && default.is_none() { error(span, "CLI defaults must be prepared constants and cannot execute source code"); }
            if default.as_ref().is_some_and(|value| !value.matches_data_type(&ty)) { error(span, "a CLI default must match its parameter type"); }
            if parser_type == "UInt" && matches!(default, Some(LiteralConstant::Int(value)) if value < 0) {
                error(span, "a UInt CLI default must be nonnegative");
            }
            if parser_type == "List[UInt]" && matches!(&default, Some(LiteralConstant::List(values)) if values.iter().any(|value| matches!(value, LiteralConstant::Int(value) if *value < 0))) {
                error(span, "UInt CLI list defaults must be nonnegative");
            }
            parameters.push(CliEntryParameter { name: parameter.name, ty, parser_type, default, rest: parameter.rest });
        }
        entry = Some(CliEntryPlan { statement: id, definition, parameters });
    }
    if diagnostics.is_empty() { (entry, diagnostics) } else { (None, diagnostics) }
}

// Inferred annotations carry no parser spelling. Only concrete supported CLI
// domains can supply one; explicit aliases retain their declared parser.
fn inferred_parser_type(ty: &Type) -> Option<String> {
    match ty {
        Type::Str | Type::Int | Type::UInt | Type::Bool | Type::Path | Type::Duration => Some(ty.to_string()),
        Type::List(item) => Some(format!("List[{}]", inferred_parser_type(item)?)),
        _ => None,
    }
}
