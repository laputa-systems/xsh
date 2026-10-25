use crate::diagnostic::{Diagnostic, DiagnosticCode, Label, Severity};
use crate::sema::constants::LiteralConstant;
use crate::sema::check::CheckBundleView;
use crate::sema::types::Type;
use crate::symbol::Name;
use crate::syntax::arena::{
    ArenaParam, ArenaStmtKind, ExprId, FunctionDefId, StmtId, TypeExprId,
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
    pub metadata: CliParameterMetadata,
}

/// Entry metadata carries prepared data and concrete bindings independently
/// of the callable declaration tables.
#[derive(Clone, Debug)]
pub(crate) struct CliEntryPlan {
    pub statement: StmtId,
    pub definition: FunctionDefId,
    /// The subcommand words as typed on the command line, in kebab case.
    pub path: Vec<String>,
    pub parameters: Vec<CliEntryParameter>,
    pub metadata: CliEntryMetadata,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CliEntryMetadata {
    pub name: Option<String>,
    pub help: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CliParameterMetadata {
    pub form: Option<String>,
    pub min: Option<i64>,
}

/// The command-line spelling of a subcommand word or option name.
pub(crate) fn cli_word(identifier: &str) -> String {
    identifier.replace('_', "-")
}

pub(crate) fn validate_cli_entry(
    view: &CheckBundleView<'_>,
    mut type_of: impl FnMut(&ArenaParam) -> Type,
    mut parser_type: impl FnMut(TypeExprId) -> Option<String>,
    mut default_value: impl FnMut(ExprId) -> Option<LiteralConstant>,
) -> (Vec<CliEntryPlan>, Vec<Diagnostic>) {
    let program = view.program();
    let roots = view.statement_ids().collect::<Vec<_>>();
    // A workspace arena can hold other entry scripts that this program neither
    // is nor imports; only the entry source and its modules are validated.
    let sources = roots
        .iter()
        .copied()
        .chain(
            view.modules()
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
        if entries.iter().any(|other| other.path == path) {
            error(statement.span, "this subcommand path is already declared; subcommand words must be unique after snake_case maps to kebab-case");
        }
        let (metadata, parameter_metadata) = match function.cli_metadata {
            None => (CliEntryMetadata::default(), std::collections::BTreeMap::new()),
            Some(expression) => match default_value(expression).as_ref().map(read_metadata) {
                Some(Ok(value)) => value,
                Some(Err(message)) => {
                    error(statement.span, &message);
                    continue;
                }
                None => {
                    error(statement.span, "CLI metadata must be a compile-time literal record");
                    continue;
                }
            },
        };
        for name in parameter_metadata.keys() {
            if !program.arena.params(function.params).iter().any(|parameter| parameter.name == *name) {
                error(statement.span, &format!("CLI metadata names unknown parameter `{name}`"));
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
            let metadata = parameter_metadata.get(&parameter.name).cloned().unwrap_or_default();
            if let Some(form) = &metadata.form {
                if let Err(message) = validate_form(form) { error(span, &message); }
                if parameter.rest && form.starts_with('-') { error(span, "a CLI rest parameter must be positional"); }
            }
            if metadata.min.is_some() && !matches!(ty, Type::Int | Type::UInt)
                && !matches!(&ty, Type::List(item) if matches!(item.as_ref(), Type::Int | Type::UInt))
            {
                error(span, "CLI minimum requires an Int or UInt parameter");
            }
            let positional = metadata.form.as_ref().is_none_or(|form| !form.starts_with('-'));
            if !parameter.rest && parameter.default.is_none() && default_seen && positional {
                error(
                    span,
                    "required CLI positionals must precede defaulted options",
                );
            }
            default_seen |= parameter.default.is_some();
            let parser_type = match parameter.annotation {
                Some(annotation) => parser_type(annotation).unwrap_or_default(),
                None => parameter
                    .default
                    .and_then(|_| inferred_parser_type(&ty))
                    .unwrap_or_default(),
            };
            let scalar = |ty: &Type| {
                matches!(
                    ty,
                    Type::Str | Type::Int | Type::UInt | Type::Bool | Type::Path | Type::Duration
                )
            };
            let supported = if parameter.rest {
                matches!(&ty, Type::List(item) if matches!(item.as_ref(), Type::Str | Type::Path))
            } else if parameter.default.is_some() || metadata.form.as_ref().is_some_and(|form| form.starts_with('-')) {
                scalar(&ty) || matches!(&ty, Type::List(item) if scalar(item))
            } else {
                scalar(&ty)
            };
            if let Some(form) = &metadata.form {
                let option = form.starts_with('-');
                let metavar = form.split_whitespace().any(|word| !word.starts_with('-'));
                if option && !metavar && !matches!(ty, Type::Bool) {
                    error(span, "a value option form requires a metavar");
                }
                if form.starts_with("...") && !matches!(ty, Type::List(_)) {
                    error(span, "a repeated CLI form requires a List parameter");
                }
            }
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
                metadata,
            });
        }
        entries.push(CliEntryPlan {
            statement: id,
            definition,
            path,
            parameters,
            metadata,
        });
    }
    for entry in &entries {
        let span = program.arena.stmt(entry.statement).span;
        if entries.iter().any(|other| other.path.len() > entry.path.len() && other.path.starts_with(&entry.path))
            && !(entry.parameters.is_empty() || entry.parameters.len() == 1 && entry.parameters[0].rest && entry.parameters[0].ty == Type::List(Box::new(Type::Str)))
        {
            diagnostics.push(Diagnostic::new(Severity::Error, "a CLI group entry requires no parameters or one rest List[Str]")
                .with_code(DiagnosticCode::CheckCliEntry).with_label(Label::primary(span, "invalid CLI group signature")));
        }
        let parameters = entry.parameters.iter().map(|parameter| {
            use crate::modules::cli::{SignatureDefault, SignatureParameter};
            let default = match parameter.default.as_ref() {
                None => None,
                Some(CliEntryDefault::Constant(value)) => {
                    let value = crate::sema::constants::cli_constant_value(value, &Default::default())
                        .ok_or_else(|| format!("CLI default for `{}` cannot be represented as {}", parameter.name, parameter.parser_type))?;
                    Some(SignatureDefault::Value(value))
                }
                Some(CliEntryDefault::Computed(_)) => Some(SignatureDefault::Computed(String::new())),
            };
            Ok(SignatureParameter { name: parameter.name.to_string(), type_name: parameter.parser_type.clone(), default, rest: parameter.rest, metadata: parameter.metadata.clone() })
        }).collect::<Result<Vec<_>, String>>();
        let parameters = match parameters {
            Ok(parameters) => parameters,
            Err(message) => {
                diagnostics.push(Diagnostic::new(Severity::Error, &message).with_code(DiagnosticCode::CheckCliEntry)
                    .with_label(Label::primary(span, message)));
                continue;
            }
        };
        if let Err(error) = crate::modules::cli::PreparedSignatureCli::prepare(parameters, String::new(), &entry.metadata, span) {
            diagnostics.push(Diagnostic::new(Severity::Error, &error.message).with_code(DiagnosticCode::CheckCliEntry)
                .with_label(Label::primary(span, error.message)));
        }
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

fn read_metadata(value: &LiteralConstant) -> Result<(CliEntryMetadata, std::collections::BTreeMap<Name, CliParameterMetadata>), String> {
    let LiteralConstant::Record(fields) = value else { return Err("CLI metadata must be a literal record".into()); };
    let mut metadata = CliEntryMetadata::default();
    let mut parameters = std::collections::BTreeMap::new();
    for (name, value) in fields.iter() {
        match name.as_str().as_str() {
            "name" | "help" => {
                let LiteralConstant::Str(value) = value else { return Err(format!("CLI metadata `{name}` must be Str")); };
                if *name == "name" {
                    if value.is_empty() || value.contains(['\n', '\r']) { return Err("CLI display name must be nonempty and contain no newline".into()); }
                    metadata.name = Some(value.to_string());
                } else { metadata.help = Some(value.to_string()); }
            }
            "parameters" => {
                let LiteralConstant::Record(fields) = value else { return Err("CLI parameter metadata must be a record".into()); };
                for (parameter, value) in fields.iter() {
                    let LiteralConstant::Record(fields) = value else { return Err(format!("CLI parameter metadata `{parameter}` must be a record")); };
                    let mut metadata = CliParameterMetadata::default();
                    for (field, value) in fields.iter() {
                        match (field.as_str().as_str(), value) {
                            ("form", LiteralConstant::Str(value)) => metadata.form = Some(value.to_string()),
                            ("min", LiteralConstant::Int(value)) => metadata.min = Some(*value),
                            _ => return Err(format!("unknown or malformed CLI parameter metadata `{field}`")),
                        }
                    }
                    parameters.insert(*parameter, metadata);
                }
            }
            _ => return Err(format!("unknown CLI metadata `{name}`")),
        }
    }
    Ok((metadata, parameters))
}

fn validate_form(form: &str) -> Result<(), String> {
    let words = form.split_whitespace().collect::<Vec<_>>();
    let valid_metavar = |word: &str| !word.is_empty() && word.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    let valid_option = |word: &str| word.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
        && word.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)
        && word.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    let valid = if form.starts_with('-') {
        let options = words.iter().take_while(|word| word.starts_with('-')).count();
        options > 0 && words.len() <= options + 1 && words[..options].iter().all(|word| {
            if let Some(long) = word.strip_prefix("--") { valid_option(long) }
            else { word.len() == 2 && word.as_bytes()[1].is_ascii_alphabetic() }
        }) && words.get(options).is_none_or(|word| valid_metavar(word))
    } else {
        words.len() == 1 && valid_metavar(words[0].trim_start_matches("..."))
    };
    if valid { Ok(()) } else { Err(format!("malformed CLI form `{form}`")) }
}
