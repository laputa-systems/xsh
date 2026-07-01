#![allow(clippy::single_call_fn)]

use crate::runtime::value::{DurationValue, PathValue, RecordMap, RuntimeError, Value};
use crate::source::Span;
use rustc_hash::FxHashSet;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Debug)]
struct OptionSpec {
    value_ty: ArgValueType,
    repeated: bool,
    required: bool,
    flag: bool,
    positional: bool,
    long: Vec<String>,
    short: Vec<String>,
    form: Option<String>,
    help: Option<String>,
    hidden: bool,
    deprecated: Option<String>,
    optional_value: bool,
    optional_default: Option<Value>,
    choices: Vec<String>,
    conflicts: Vec<String>,
    requires: Vec<String>,
    required_group: Option<String>,
    env: Option<String>,
    min: Option<i64>,
    max: Option<i64>,
    positive: bool,
    nonzero: bool,
    exists: bool,
    file: bool,
    dir: bool,
    default: Option<Value>,
}

#[derive(Clone, Debug, Default)]
struct FormSpec {
    raw: Option<String>,
    positional: bool,
    repeated: bool,
    optional_value: bool,
    long: Vec<String>,
    short: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ArgValueType {
    Str,
    Int,
    UInt,
    Bool,
    Path,
    Duration,
}

impl ArgValueType {
    fn parse_scalar(name: &str) -> Option<Self> {
        match name.trim() {
            "Str" => Some(Self::Str),
            "Int" => Some(Self::Int),
            "UInt" => Some(Self::UInt),
            "Bool" => Some(Self::Bool),
            "Path" => Some(Self::Path),
            "Duration" => Some(Self::Duration),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ArgTokenKind {
    Long,
    Short,
    Operand,
}

impl ArgTokenKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
            Self::Operand => "operand",
        }
    }
}

#[derive(Clone, Debug)]
struct CommandSpec {
    canonical: String,
    aliases: Vec<String>,
    positionals: Vec<String>,
    types: BTreeMap<String, ArgValueType>,
    options: BTreeMap<String, OptionSpec>,
    rest: Option<String>,
    min_rest: usize,
    command_like: bool,
}

#[derive(Clone, Debug)]
struct ParsedValues {
    values: RecordMap,
    sources: RecordMap,
    warnings: Vec<String>,
}

pub(crate) fn parse_cli(
    argv: Vec<String>,
    schema: RecordMap,
    command: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    let specs = parse_schema(schema, span)?;
    if argv_requests_help(&argv) {
        return Err(cli_help_error(usage_text(&specs, command), span));
    }
    let parsed = parse_values(&argv, &specs, &RecordMap::new(), span)
        .map_err(|error| cli_usage_error(error, usage_text(&specs, command)))?;
    Ok(Value::ok(Value::Record(parsed.values)))
}

pub(crate) fn parse_cli_full(
    argv: Vec<String>,
    schema: RecordMap,
    env: RecordMap,
    command: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    let specs = parse_schema(schema, span)?;
    if argv_requests_help(&argv) {
        return Err(cli_help_error(usage_text(&specs, command), span));
    }
    let parsed = parse_values(&argv, &specs, &env, span)
        .map_err(|error| cli_usage_error(error, usage_text(&specs, command)))?;
    Ok(Value::ok(Value::Record(RecordMap::from([
        (Arc::from("values"), Value::Record(parsed.values)),
        (Arc::from("sources"), Value::Record(parsed.sources)),
        (
            Arc::from("warnings"),
            Value::List(
                parsed
                    .warnings
                    .into_iter()
                    .map(|warning| Value::Str(warning.into()))
                    .collect(),
            ),
        ),
    ]))))
}

pub(crate) fn render_usage(
    schema: RecordMap,
    command: String,
    span: Span,
) -> Result<Value, RuntimeError> {
    let specs = parse_schema(schema, span)?;
    Ok(Value::Str(usage_text(&specs, &command).into()))
}

pub(crate) fn parse_commands(
    argv: Vec<String>,
    rootless_default: String,
    commands: RecordMap,
    fallback_command: Option<RecordMap>,
    span: Span,
) -> Result<Value, RuntimeError> {
    let specs = parse_command_schema(commands, span)?;
    let fallback = fallback_command
        .map(|descriptor| {
            parse_command_descriptor("fallback_command", Value::Record(descriptor), span)
        })
        .transpose()?;
    let parsed = parse_command_values(&argv, &rootless_default, &specs, fallback.as_ref(), span)?;
    Ok(Value::ok(Value::Record(parsed)))
}

pub(crate) fn tokenize_flags(
    argv: Vec<String>,
    value_flags: Vec<String>,
    span: Span,
) -> Result<Value, RuntimeError> {
    let value_flags = value_flags.into_iter().collect::<FxHashSet<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut operands_only = false;

    while index < argv.len() {
        let arg = &argv[index];
        if operands_only || arg == "-" || !arg.starts_with('-') || looks_negative_number(arg) {
            tokens.push(arg_token(ArgTokenKind::Operand, arg, ""));
            index += 1;
            continue;
        }

        if arg == "--" {
            operands_only = true;
            index += 1;
            continue;
        }

        if let Some(long) = arg.strip_prefix("--") {
            let (name, attached) = long
                .split_once('=')
                .map_or((long, None), |(name, value)| (name, Some(value)));
            if name.is_empty() {
                tokens.push(arg_token(ArgTokenKind::Operand, arg, ""));
            } else if let Some(value) = attached {
                tokens.push(arg_token(ArgTokenKind::Long, name, value));
            } else if value_flags.contains(name) {
                index += 1;
                let value = argv
                    .get(index)
                    .ok_or_else(|| cli_error(format!("option `--{name}` expects a value"), span))?;
                tokens.push(arg_token(ArgTokenKind::Long, name, value));
            } else {
                tokens.push(arg_token(ArgTokenKind::Long, name, ""));
            }
            index += 1;
            continue;
        }

        let short = arg.trim_start_matches('-');
        let chars = short.char_indices().collect::<Vec<_>>();
        let mut consumed_value = false;
        for (pos, &(byte_index, ch)) in chars.iter().enumerate() {
            let name = ch.to_string();
            if value_flags.contains(&name) {
                let value = if pos + 1 < chars.len() {
                    let next_byte = chars[pos + 1].0;
                    consumed_value = true;
                    &short[next_byte..]
                } else {
                    index += 1;
                    consumed_value = true;
                    argv.get(index).ok_or_else(|| {
                        cli_error(format!("option `-{name}` expects a value"), span)
                    })?
                };
                tokens.push(arg_token(ArgTokenKind::Short, &name, value));
                break;
            }
            let next_byte = byte_index + ch.len_utf8();
            if next_byte <= short.len() {
                tokens.push(arg_token(ArgTokenKind::Short, &name, ""));
            }
        }
        index += 1;
        if consumed_value {
            continue;
        }
    }

    Ok(Value::ok(Value::List(tokens)))
}

fn looks_negative_number(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('-') else {
        return false;
    };
    let mut chars = rest.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_digit())
}

fn argv_requests_help(argv: &[String]) -> bool {
    for arg in argv {
        if arg == "--" {
            return false;
        }
        if arg == "--help" {
            return true;
        }
        if let Some(shorts) = arg.strip_prefix('-')
            && !shorts.starts_with('-')
            && shorts.len() > 1
            && !looks_negative_number(arg)
            && shorts.chars().any(|ch| ch == 'h')
        {
            return true;
        }
    }
    false
}

fn option_label(name: &str, spec: &OptionSpec) -> String {
    if spec.positional {
        return spec
            .form
            .as_deref()
            .map(|form| form.trim_start_matches("...").to_string())
            .unwrap_or_else(|| name.to_string());
    }
    if let Some(long) = spec.long.first() {
        return format!("--{}", long.replace('_', "-"));
    }
    if let Some(short) = spec.short.first() {
        return format!("-{short}");
    }
    format!("--{}", name.replace('_', "-"))
}

fn validate_relationships(
    specs: &BTreeMap<String, OptionSpec>,
    present: &BTreeMap<String, bool>,
    span: Span,
) -> Result<(), RuntimeError> {
    let mut groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (name, spec) in specs {
        if let Some(group) = &spec.required_group {
            groups.entry(group).or_default().push(name);
        }
        if !present.contains_key(name) {
            continue;
        }
        for conflict in &spec.conflicts {
            let conflict = normalize_arg_name(conflict);
            if present.contains_key(&conflict) {
                let conflict_spec = specs.get(&conflict).unwrap_or(spec);
                return Err(cli_error(
                    format!(
                        "{} conflicts with {}",
                        option_label(name, spec),
                        option_label(&conflict, conflict_spec)
                    ),
                    span,
                ));
            }
        }
        for required in &spec.requires {
            let required = normalize_arg_name(required);
            if !present.contains_key(&required) {
                let required_spec = specs.get(&required).unwrap_or(spec);
                return Err(cli_error(
                    format!(
                        "{} requires {}",
                        option_label(name, spec),
                        option_label(&required, required_spec)
                    ),
                    span,
                ));
            }
        }
    }
    for (group, names) in groups {
        if !names.iter().any(|name| present.contains_key(*name)) {
            let labels = names
                .iter()
                .map(|name| option_label(name, specs.get(*name).expect("group field exists")))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(cli_error(
                format!("one of required group `{group}` is required: {labels}"),
                span,
            ));
        }
    }
    Ok(())
}

fn arg_token(kind: ArgTokenKind, name: &str, value: &str) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("kind"), Value::Str(kind.as_str().into())),
        (Arc::from("name"), Value::Str(name.into())),
        (Arc::from("value"), Value::Str(value.into())),
    ]))
}

fn parse_schema(
    schema: RecordMap,
    span: Span,
) -> Result<BTreeMap<String, OptionSpec>, RuntimeError> {
    let mut specs = BTreeMap::new();
    for (name, descriptor) in schema {
        let spec = parse_descriptor(&name, descriptor, span)?;
        validate_not_reserved_help(&name, &spec, span)?;
        specs.insert(name.to_string(), spec);
    }
    Ok(specs)
}

fn validate_not_reserved_help(
    name: &str,
    spec: &OptionSpec,
    span: Span,
) -> Result<(), RuntimeError> {
    if normalize_arg_name(name) == "help" || spec.long.iter().any(|long| long == "help") {
        return Err(cli_error("`--help` is reserved by cli.parse", span));
    }
    if spec.short.iter().any(|short| short == "h") {
        return Err(cli_error("`-h` is reserved by cli.parse", span));
    }
    Ok(())
}

fn parse_command_schema(
    schema: RecordMap,
    span: Span,
) -> Result<BTreeMap<String, CommandSpec>, RuntimeError> {
    let mut specs = BTreeMap::new();
    for (name, descriptor) in schema {
        let spec = parse_command_descriptor(&name, descriptor, span)?;
        for alias in &spec.aliases {
            let key = command_key(alias);
            if specs.insert(key, spec.clone()).is_some() {
                return Err(cli_commands_error(
                    format!("duplicate command alias `{alias}`"),
                    span,
                ));
            }
        }
        specs.insert(name.to_string(), spec);
    }
    Ok(specs)
}

fn parse_descriptor(name: &str, descriptor: Value, span: Span) -> Result<OptionSpec, RuntimeError> {
    match descriptor {
        Value::Str(type_name) => {
            let (value_ty, repeated) = parse_type_name(&type_name, span)?;
            Ok(OptionSpec {
                flag: matches!(value_ty, ArgValueType::Bool),
                value_ty,
                repeated,
                required: false,
                positional: false,
                long: Vec::new(),
                short: Vec::new(),
                form: None,
                help: None,
                hidden: false,
                deprecated: None,
                optional_value: false,
                optional_default: None,
                choices: Vec::new(),
                conflicts: Vec::new(),
                requires: Vec::new(),
                required_group: None,
                env: None,
                min: None,
                max: None,
                positive: false,
                nonzero: false,
                exists: false,
                file: false,
                dir: false,
                default: None,
            })
        }
        Value::Record(fields) => {
            let form = parse_form_descriptor(name, &fields, span)?;
            let explicit_type = fields.get("kind").or_else(|| fields.get("type"));
            let value_ty = match explicit_type {
                Some(Value::Str(type_name)) => type_name,
                Some(value) => {
                    return Err(cli_error(
                        format!(
                            "option `{name}` kind must be Str, found {}",
                            value.type_name()
                        ),
                        span,
                    ));
                }
                None => "",
            };
            let (value_ty, type_repeated) = if value_ty.is_empty() {
                infer_descriptor_type(name, fields.get("default"), span)?
            } else {
                parse_type_name(value_ty, span)?
            };
            let repeated = descriptor_bool(name, &fields, "repeated", false, span)?
                || type_repeated
                || form.repeated;
            let positional =
                descriptor_bool(name, &fields, "positional", false, span)? || form.positional;
            let flag = descriptor_bool(
                name,
                &fields,
                "flag",
                matches!(value_ty, ArgValueType::Bool) && !repeated && !positional,
                span,
            )?;
            let required = descriptor_bool(name, &fields, "required", false, span)?
                || (positional && !repeated);
            let long = merge_arg_names(
                form.long,
                descriptor_arg_names(name, &fields, "long", span)?,
            );
            let short = merge_arg_names(
                form.short,
                descriptor_arg_names(name, &fields, "short", span)?,
            );
            let help = descriptor_args_string(name, &fields, "help", span)?;
            let hidden = descriptor_bool(name, &fields, "hidden", false, span)?;
            let deprecated = descriptor_deprecated(name, &fields, span)?;
            let optional_value =
                descriptor_bool(name, &fields, "optional_value", form.optional_value, span)?;
            let choices = descriptor_args_string_list(name, &fields, "choices", span)?;
            let conflicts = descriptor_args_string_list(name, &fields, "conflicts", span)?;
            let requires = descriptor_args_string_list(name, &fields, "requires", span)?;
            let required_group = descriptor_args_string(name, &fields, "required_group", span)?;
            let env = descriptor_args_string(name, &fields, "env", span)?;
            let min = descriptor_optional_i64(name, &fields, "min", span)?;
            let max = descriptor_optional_i64(name, &fields, "max", span)?;
            let positive = descriptor_bool(name, &fields, "positive", false, span)?;
            let nonzero = descriptor_bool(name, &fields, "nonzero", false, span)?;
            let exists = descriptor_bool(name, &fields, "exists", false, span)?;
            let file = descriptor_bool(name, &fields, "file", false, span)?;
            let dir = descriptor_bool(name, &fields, "dir", false, span)?;
            let default = normalize_default(
                name,
                fields.get("default").cloned(),
                &value_ty,
                repeated,
                span,
            )?;
            let optional_default = normalize_default(
                name,
                fields.get("optional_default").cloned(),
                &value_ty,
                false,
                span,
            )?;
            Ok(OptionSpec {
                value_ty,
                repeated,
                required,
                positional,
                flag,
                long,
                short,
                form: form.raw,
                help,
                hidden,
                deprecated,
                optional_value,
                optional_default,
                choices,
                conflicts,
                requires,
                required_group,
                env,
                min,
                max,
                positive,
                nonzero,
                exists,
                file,
                dir,
                default,
            })
        }
        value => Err(cli_error(
            format!(
                "option `{name}` descriptor must be Str or Record, found {}",
                value.type_name()
            ),
            span,
        )),
    }
}

fn infer_descriptor_type(
    name: &str,
    default: Option<&Value>,
    span: Span,
) -> Result<(ArgValueType, bool), RuntimeError> {
    let Some(default) = default else {
        return Ok((ArgValueType::Str, false));
    };
    match default {
        Value::Str(_) => Ok((ArgValueType::Str, false)),
        Value::Int(_) => Ok((ArgValueType::Int, false)),
        Value::Bool(_) => Ok((ArgValueType::Bool, false)),
        Value::Path(_) => Ok((ArgValueType::Path, false)),
        Value::Duration(_) => Ok((ArgValueType::Duration, false)),
        Value::List(items) => {
            let Some(first) = items.first() else {
                return Ok((ArgValueType::Str, true));
            };
            let first_ty = infer_value_type(first).ok_or_else(|| {
                cli_error(
                    format!("option `{name}` default list contains unsupported values"),
                    span,
                )
            })?;
            for item in items.iter().skip(1) {
                let Some(item_ty) = infer_value_type(item) else {
                    return Err(cli_error(
                        format!("option `{name}` default list contains unsupported values"),
                        span,
                    ));
                };
                if item_ty != first_ty {
                    return Err(cli_error(
                        format!("option `{name}` default list must contain one scalar type"),
                        span,
                    ));
                }
            }
            Ok((first_ty, true))
        }
        value => Err(cli_error(
            format!(
                "option `{name}` default cannot infer type from {}",
                value.type_name()
            ),
            span,
        )),
    }
}

fn infer_value_type(value: &Value) -> Option<ArgValueType> {
    match value {
        Value::Str(_) => Some(ArgValueType::Str),
        Value::Int(_) => Some(ArgValueType::Int),
        Value::Bool(_) => Some(ArgValueType::Bool),
        Value::Path(_) => Some(ArgValueType::Path),
        Value::Duration(_) => Some(ArgValueType::Duration),
        _ => None,
    }
}

fn parse_form_descriptor(
    name: &str,
    fields: &RecordMap,
    span: Span,
) -> Result<FormSpec, RuntimeError> {
    let value = fields.get("form").or_else(|| fields.get("use"));
    let Some(value) = value else {
        return Ok(FormSpec::default());
    };
    let Value::Str(form) = value else {
        return Err(cli_error(
            format!(
                "option `{name}` descriptor field `form` must be Str, found {}",
                value.type_name()
            ),
            span,
        ));
    };
    let mut parsed = FormSpec {
        raw: Some(form.to_string()),
        ..FormSpec::default()
    };
    let mut has_option = false;
    for token in form.split_whitespace() {
        if let Some(long) = token.strip_prefix("--") {
            let (name, optional_value) = if let Some(name) = long
                .strip_suffix(']')
                .and_then(|value| value.split_once("[="))
                .map(|(name, _)| name)
            {
                (name, true)
            } else {
                (long.split_once('=').map_or(long, |(name, _)| name), false)
            };
            if name.is_empty() {
                return Err(cli_error("empty long option in cli form", span));
            }
            parsed.long.push(normalize_arg_name(name));
            parsed.optional_value = parsed.optional_value || optional_value;
            has_option = true;
        } else if let Some(short) = token.strip_prefix('-') {
            if short.is_empty() {
                return Err(cli_error("empty short option in cli form", span));
            }
            for ch in short.chars() {
                parsed.short.push(ch.to_string());
            }
            has_option = true;
        } else if !has_option {
            parsed.positional = true;
            parsed.repeated = token.starts_with("...");
        }
    }
    Ok(parsed)
}

fn merge_arg_names(mut first: Vec<String>, second: Vec<String>) -> Vec<String> {
    for name in second {
        if !first.contains(&name) {
            first.push(name);
        }
    }
    first
}

fn descriptor_arg_names(
    option: &str,
    fields: &RecordMap,
    name: &str,
    span: Span,
) -> Result<Vec<String>, RuntimeError> {
    match fields.get(name) {
        Some(Value::Str(value)) => Ok(vec![normalize_arg_name(value)]),
        Some(Value::List(items)) => {
            let mut names = Vec::new();
            for item in items {
                let Value::Str(value) = item else {
                    return Err(cli_error(
                        format!(
                            "option `{option}` descriptor field `{name}` must be Str or List[Str]"
                        ),
                        span,
                    ));
                };
                names.push(normalize_arg_name(value));
            }
            Ok(names)
        }
        Some(value) => Err(cli_error(
            format!(
                "option `{option}` descriptor field `{name}` must be Str or List[Str], found {}",
                value.type_name()
            ),
            span,
        )),
        None => Ok(Vec::new()),
    }
}

fn normalize_arg_name(name: &str) -> String {
    name.trim_start_matches('-').replace('-', "_")
}

fn parse_command_descriptor(
    name: &str,
    descriptor: Value,
    span: Span,
) -> Result<CommandSpec, RuntimeError> {
    let Value::Record(fields) = descriptor else {
        return Err(cli_commands_error(
            format!("command `{name}` descriptor must be Record"),
            span,
        ));
    };
    let mut positionals = descriptor_string_list(name, &fields, "positionals", span)?;
    let mut rest = descriptor_string(name, &fields, "rest", span)?;
    if let Some(form) = descriptor_string(name, &fields, "form", span)? {
        parse_command_form(&form, &mut positionals, &mut rest);
    }
    let min_rest = descriptor_usize(name, &fields, "min_rest", 0, span)?;
    let command_like = descriptor_command_bool(name, &fields, "command_like", false, span)?;
    let aliases = descriptor_string_list(name, &fields, "aliases", span)?;
    let types = parse_command_types(name, fields.get("types"), span)?;
    let options = match fields.get("options") {
        Some(Value::Record(options)) => parse_schema(options.clone(), span)?,
        Some(value) => {
            return Err(cli_commands_error(
                format!(
                    "command `{name}` descriptor field `options` must be Record, found {}",
                    value.type_name()
                ),
                span,
            ));
        }
        None => BTreeMap::new(),
    };
    Ok(CommandSpec {
        canonical: name.to_string(),
        aliases,
        positionals,
        types,
        options,
        rest,
        min_rest,
        command_like,
    })
}

fn parse_command_form(form: &str, positionals: &mut Vec<String>, rest: &mut Option<String>) {
    for token in form.split_whitespace().skip(1) {
        if token.starts_with('-') {
            continue;
        }
        if let Some(name) = token.strip_prefix("...") {
            *rest = Some(name.to_ascii_lowercase());
        } else {
            positionals.push(token.to_ascii_lowercase());
        }
    }
}

fn parse_command_types(
    command: &str,
    types: Option<&Value>,
    span: Span,
) -> Result<BTreeMap<String, ArgValueType>, RuntimeError> {
    let Some(types) = types else {
        return Ok(BTreeMap::new());
    };
    let Value::Record(fields) = types else {
        return Err(cli_commands_error(
            format!("command `{command}` field `types` must be Record"),
            span,
        ));
    };
    let mut parsed = BTreeMap::new();
    for (field, value) in fields {
        let Value::Str(type_name) = value else {
            return Err(cli_commands_error(
                format!("command `{command}` type for `{field}` must be Str"),
                span,
            ));
        };
        let value_ty = parse_command_type_name(type_name, command, field, span)?;
        parsed.insert(field.to_string(), value_ty);
    }
    Ok(parsed)
}

fn parse_command_type_name(
    name: &str,
    command: &str,
    field: &str,
    span: Span,
) -> Result<ArgValueType, RuntimeError> {
    let trimmed = name.trim();
    if trimmed.starts_with("List[") {
        return Err(cli_commands_error(
            format!("command `{command}` type for `{field}` cannot be List"),
            span,
        ));
    }
    ArgValueType::parse_scalar(trimmed).ok_or_else(|| {
        cli_commands_error(
            format!("unsupported command positional type `{trimmed}`"),
            span,
        )
    })
}

fn descriptor_bool(
    option: &str,
    fields: &RecordMap,
    name: &str,
    default: bool,
    span: Span,
) -> Result<bool, RuntimeError> {
    match fields.get(name) {
        Some(Value::Bool(value)) => Ok(*value),
        Some(value) => Err(cli_error(
            format!(
                "option `{option}` descriptor field `{name}` must be Bool, found {}",
                value.type_name()
            ),
            span,
        )),
        None => Ok(default),
    }
}

fn descriptor_deprecated(
    option: &str,
    fields: &RecordMap,
    span: Span,
) -> Result<Option<String>, RuntimeError> {
    match fields.get("deprecated") {
        Some(Value::Bool(true)) => Ok(Some(format!("option `{option}` is deprecated"))),
        Some(Value::Bool(false)) => Ok(None),
        Some(Value::Str(value)) => Ok(Some(value.to_string())),
        Some(value) => Err(cli_error(
            format!(
                "option `{option}` descriptor field `deprecated` must be Bool or Str, found {}",
                value.type_name()
            ),
            span,
        )),
        None => Ok(None),
    }
}

fn descriptor_args_string(
    option: &str,
    fields: &RecordMap,
    name: &str,
    span: Span,
) -> Result<Option<String>, RuntimeError> {
    match fields.get(name) {
        Some(Value::Str(value)) => Ok(Some(value.to_string())),
        Some(value) => Err(cli_error(
            format!(
                "option `{option}` descriptor field `{name}` must be Str, found {}",
                value.type_name()
            ),
            span,
        )),
        None => Ok(None),
    }
}

fn descriptor_args_string_list(
    option: &str,
    fields: &RecordMap,
    name: &str,
    span: Span,
) -> Result<Vec<String>, RuntimeError> {
    match fields.get(name) {
        Some(Value::Str(value)) => Ok(vec![value.to_string()]),
        Some(Value::List(items)) => {
            let mut values = Vec::new();
            for item in items {
                let Value::Str(value) = item else {
                    return Err(cli_error(
                        format!(
                            "option `{option}` descriptor field `{name}` must be Str or List[Str]"
                        ),
                        span,
                    ));
                };
                values.push(value.to_string());
            }
            Ok(values)
        }
        Some(value) => Err(cli_error(
            format!(
                "option `{option}` descriptor field `{name}` must be Str or List[Str], found {}",
                value.type_name()
            ),
            span,
        )),
        None => Ok(Vec::new()),
    }
}

fn descriptor_optional_i64(
    option: &str,
    fields: &RecordMap,
    name: &str,
    span: Span,
) -> Result<Option<i64>, RuntimeError> {
    match fields.get(name) {
        Some(Value::Int(value)) => Ok(Some(*value)),
        Some(value) => Err(cli_error(
            format!(
                "option `{option}` descriptor field `{name}` must be Int, found {}",
                value.type_name()
            ),
            span,
        )),
        None => Ok(None),
    }
}

fn descriptor_command_bool(
    command: &str,
    fields: &RecordMap,
    name: &str,
    default: bool,
    span: Span,
) -> Result<bool, RuntimeError> {
    match fields.get(name) {
        Some(Value::Bool(value)) => Ok(*value),
        Some(value) => Err(cli_commands_error(
            format!(
                "command `{command}` descriptor field `{name}` must be Bool, found {}",
                value.type_name()
            ),
            span,
        )),
        None => Ok(default),
    }
}

fn descriptor_string(
    owner: &str,
    fields: &RecordMap,
    name: &str,
    span: Span,
) -> Result<Option<String>, RuntimeError> {
    match fields.get(name) {
        Some(Value::Str(value)) => Ok(Some(value.to_string())),
        Some(value) => Err(cli_commands_error(
            format!(
                "command `{owner}` descriptor field `{name}` must be Str, found {}",
                value.type_name()
            ),
            span,
        )),
        None => Ok(None),
    }
}

fn descriptor_string_list(
    owner: &str,
    fields: &RecordMap,
    name: &str,
    span: Span,
) -> Result<Vec<String>, RuntimeError> {
    let Some(value) = fields.get(name) else {
        return Ok(Vec::new());
    };
    let Value::List(items) = value else {
        return Err(cli_commands_error(
            format!("command `{owner}` descriptor field `{name}` must be List[Str]"),
            span,
        ));
    };
    let mut strings = Vec::new();
    for item in items {
        let Value::Str(value) = item else {
            return Err(cli_commands_error(
                format!("command `{owner}` descriptor field `{name}` must be List[Str]"),
                span,
            ));
        };
        strings.push(value.to_string());
    }
    Ok(strings)
}

fn descriptor_usize(
    owner: &str,
    fields: &RecordMap,
    name: &str,
    default: usize,
    span: Span,
) -> Result<usize, RuntimeError> {
    match fields.get(name) {
        Some(Value::Int(value)) if *value >= 0 => Ok(*value as usize),
        Some(Value::Int(_)) => Err(cli_commands_error(
            format!("command `{owner}` descriptor field `{name}` cannot be negative"),
            span,
        )),
        Some(value) => Err(cli_commands_error(
            format!(
                "command `{owner}` descriptor field `{name}` must be Int, found {}",
                value.type_name()
            ),
            span,
        )),
        None => Ok(default),
    }
}

fn parse_type_name(name: &str, span: Span) -> Result<(ArgValueType, bool), RuntimeError> {
    let trimmed = name.trim();
    if let Some(inner) = trimmed
        .strip_prefix("List[")
        .and_then(|value| value.strip_suffix(']'))
    {
        let (inner, inner_repeated) = parse_type_name(inner, span)?;
        if inner_repeated {
            return Err(cli_error(
                "nested List option types are not supported",
                span,
            ));
        }
        return Ok((inner, true));
    }
    ArgValueType::parse_scalar(trimmed)
        .map(|ty| (ty, false))
        .ok_or_else(|| cli_error(format!("unsupported option type `{trimmed}`"), span))
}

fn normalize_default(
    name: &str,
    default: Option<Value>,
    value_ty: &ArgValueType,
    repeated: bool,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    let Some(default) = default else {
        return Ok(None);
    };
    if repeated {
        let Value::List(items) = default else {
            return Err(cli_error(
                format!("option `{name}` default must be List for repeated options"),
                span,
            ));
        };
        let mut converted = Vec::new();
        for item in items {
            converted.push(convert_default_value(name, item, value_ty, span)?);
        }
        return Ok(Some(Value::List(converted)));
    }
    convert_default_value(name, default, value_ty, span).map(Some)
}

fn convert_default_value(
    name: &str,
    value: Value,
    value_ty: &ArgValueType,
    span: Span,
) -> Result<Value, RuntimeError> {
    match (value_ty, value) {
        (ArgValueType::Str, Value::Str(value)) => Ok(Value::Str(value)),
        (ArgValueType::Int, Value::Int(value)) => Ok(Value::Int(value)),
        (ArgValueType::UInt, Value::Int(value)) if value >= 0 => Ok(Value::Int(value)),
        (ArgValueType::UInt, Value::Int(_)) => Err(cli_error(
            format!("option `{name}` default does not match declared type UInt"),
            span,
        )),
        (ArgValueType::Bool, Value::Bool(value)) => Ok(Value::Bool(value)),
        (ArgValueType::Path, Value::Path(value)) => Ok(Value::Path(value)),
        (ArgValueType::Path, Value::Str(value)) => PathValue::from_text(value)
            .map(Value::Path)
            .map_err(|error| error.with_span(span)),
        (ArgValueType::Duration, Value::Duration(value)) => Ok(Value::Duration(value)),
        (ArgValueType::Duration, Value::Str(value)) => DurationValue::from_literal(&value)
            .map(Value::Duration)
            .ok_or_else(|| {
                cli_error(
                    format!("option `{name}` default does not parse as Duration"),
                    span,
                )
            }),
        (_, value) => Err(cli_error(
            format!(
                "option `{name}` default does not match declared type, found {}",
                value.type_name()
            ),
            span,
        )),
    }
}

fn parse_values(
    argv: &[String],
    specs: &BTreeMap<String, OptionSpec>,
    env: &RecordMap,
    span: Span,
) -> Result<ParsedValues, RuntimeError> {
    let positionals: Vec<(&String, &OptionSpec)> =
        specs.iter().filter(|(_, spec)| spec.positional).collect();
    let long_specs = long_option_specs(specs, span)?;
    let short_specs = short_option_specs(specs, span)?;
    let mut positional_index = 0usize;
    let mut parsed = defaults(specs, env, span)?;
    let mut state = ArgParseState {
        argv,
        specs,
        short_specs: &short_specs,
        output: &mut parsed.values,
        sources: &mut parsed.sources,
        warnings: &mut parsed.warnings,
        present: BTreeMap::new(),
        span,
    };
    let mut index = 0;
    while index < argv.len() {
        let token = &argv[index];
        if token == "--" {
            // Everything after -- goes to the positional field (if any)
            for (offset, raw) in argv[index + 1..].iter().enumerate() {
                let Some((name, spec)) = positionals.get(positional_index) else {
                    return Err(cli_error(
                        format!(
                            "unexpected positional argument at argv[{}]: {raw}",
                            index + 1 + offset
                        ),
                        span,
                    ));
                };
                if !spec.repeated && state.present.contains_key(*name) {
                    return Err(cli_error(
                        format!(
                            "duplicate positional argument at argv[{}]: {raw}",
                            index + 1 + offset
                        ),
                        span,
                    ));
                }
                push_positional_value(
                    state.output,
                    state.sources,
                    name,
                    spec,
                    raw,
                    index + 1 + offset,
                    span,
                )?;
                state.present.insert((*name).clone(), true);
                if !spec.repeated {
                    positional_index += 1;
                }
            }
            break;
        }
        if let Some(option) = token.strip_prefix("--") {
            if option.is_empty() {
                return Err(cli_error(format!("empty option at argv[{index}]"), span));
            }
            let (raw_name, inline_value) = match option.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (option, None),
            };
            let key = normalize_arg_name(raw_name);
            let name = long_specs.get(&key).cloned().unwrap_or_else(|| key.clone());
            let Some(spec) = specs.get(&name) else {
                return Err(cli_error(
                    format!("unknown argument at argv[{index}]: --{raw_name}"),
                    span,
                ));
            };
            if !spec.repeated && state.present.contains_key(&name) {
                return Err(cli_error(
                    format!("duplicate argument at argv[{index}]: --{raw_name}"),
                    span,
                ));
            }

            let (value, consumed) = if spec.flag && inline_value.is_none() {
                (Value::Bool(true), 0)
            } else if let Some(raw_value) = inline_value {
                (
                    convert_arg_value(&name, &raw_value, &spec.value_ty, index, span)?,
                    0,
                )
            } else if spec.optional_value
                && argv
                    .get(index + 1)
                    .is_none_or(|raw_value| raw_value.starts_with('-'))
            {
                (optional_value_default(&name, spec, span)?, 0)
            } else {
                let Some(raw_value) = argv.get(index + 1) else {
                    return Err(cli_error(
                        format!("missing value for --{raw_name} at argv[{index}]"),
                        span,
                    ));
                };
                if raw_value.starts_with("--") {
                    return Err(cli_error(
                        format!("missing value for --{raw_name} at argv[{index}]"),
                        span,
                    ));
                }
                (
                    convert_arg_value(&name, raw_value, &spec.value_ty, index + 1, span)?,
                    1,
                )
            };

            state.set_option_value(&name, spec, value, index)?;
            if let Some(message) = &spec.deprecated {
                state.warnings.push(message.clone());
            }
            index += consumed + 1;
            continue;
        }
        if token.starts_with('-') && token.len() > 1 && !looks_negative_number(token) {
            state.parse_short_options(&mut index)?;
            continue;
        }
        if let Some((name, spec)) = positionals.get(positional_index) {
            if !spec.repeated && state.present.contains_key(*name) {
                return Err(cli_error(
                    format!("duplicate positional argument at argv[{index}]: {token}"),
                    span,
                ));
            }
            push_positional_value(state.output, state.sources, name, spec, token, index, span)?;
            state.present.insert((*name).clone(), true);
            if !spec.repeated {
                positional_index += 1;
            }
            index += 1;
            continue;
        }
        return Err(cli_error(
            format!("unexpected positional argument at argv[{index}]: {token}"),
            span,
        ));
    }

    for (name, spec) in specs {
        if spec.required && !state.present.contains_key(name.as_str()) {
            return Err(cli_error(
                format!("missing required argument {}", option_label(name, spec)),
                span,
            ));
        }
    }
    validate_relationships(specs, &state.present, span)?;
    drop(state);
    Ok(parsed)
}

struct ArgParseState<'a> {
    argv: &'a [String],
    specs: &'a BTreeMap<String, OptionSpec>,
    short_specs: &'a BTreeMap<String, String>,
    output: &'a mut RecordMap,
    sources: &'a mut RecordMap,
    warnings: &'a mut Vec<String>,
    present: BTreeMap<String, bool>,
    span: Span,
}

impl ArgParseState<'_> {
    fn parse_short_options(&mut self, index: &mut usize) -> Result<(), RuntimeError> {
        let token = &self.argv[*index];
        let short = token.trim_start_matches('-');
        let chars = short.char_indices().collect::<Vec<_>>();
        for (pos, &(_, ch)) in chars.iter().enumerate() {
            let short_name = ch.to_string();
            let Some(name) = self.short_specs.get(&short_name) else {
                return Err(cli_error(
                    format!("unknown argument at argv[{}]: -{short_name}", *index),
                    self.span,
                ));
            };
            let spec = self
                .specs
                .get(name)
                .expect("short option map points at an existing spec");
            if !spec.repeated && self.present.contains_key(name) {
                return Err(cli_error(
                    format!("duplicate argument at argv[{}]: -{short_name}", *index),
                    self.span,
                ));
            }
            if spec.flag {
                self.set_option_value(name, spec, Value::Bool(true), *index)?;
                if let Some(message) = &spec.deprecated {
                    self.warnings.push(message.clone());
                }
                continue;
            }

            let (raw_value, raw_index, consumed) = if pos + 1 < chars.len() {
                let next_byte = chars[pos + 1].0;
                (&short[next_byte..], *index, 0)
            } else {
                let Some(raw_value) = self.argv.get(*index + 1) else {
                    return Err(cli_error(
                        format!("missing value for -{short_name} at argv[{}]", *index),
                        self.span,
                    ));
                };
                if raw_value.starts_with("--") {
                    return Err(cli_error(
                        format!("missing value for -{short_name} at argv[{}]", *index),
                        self.span,
                    ));
                }
                (raw_value.as_str(), *index + 1, 1)
            };
            let value = convert_arg_value(name, raw_value, &spec.value_ty, raw_index, self.span)?;
            self.set_option_value(name, spec, value, *index)?;
            if let Some(message) = &spec.deprecated {
                self.warnings.push(message.clone());
            }
            *index += consumed + 1;
            return Ok(());
        }
        *index += 1;
        Ok(())
    }

    fn set_option_value(
        &mut self,
        name: &str,
        spec: &OptionSpec,
        value: Value,
        index: usize,
    ) -> Result<(), RuntimeError> {
        if !spec.repeated && self.present.contains_key(name) {
            return Err(cli_error(
                format!("duplicate argument at argv[{index}]: --{name}"),
                self.span,
            ));
        }
        validate_option_value(name, spec, &value, index, self.span)?;
        if spec.repeated {
            match self.output.get_mut(name) {
                Some(Value::List(items)) => items.push(value),
                _ => {
                    self.output
                        .insert(Arc::from(name), Value::List(vec![value]));
                }
            }
        } else {
            self.output.insert(Arc::from(name), value);
        }
        self.sources
            .insert(Arc::from(name), Value::Str("argv".into()));
        self.present.insert(name.to_string(), true);
        Ok(())
    }
}

fn long_option_specs(
    specs: &BTreeMap<String, OptionSpec>,
    span: Span,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let mut long_specs = BTreeMap::new();
    for (name, spec) in specs {
        for long in &spec.long {
            if long.is_empty() {
                return Err(cli_error(
                    format!("option `{name}` has an empty long name"),
                    span,
                ));
            }
            if long_specs.insert(long.clone(), name.clone()).is_some() {
                return Err(cli_error(format!("duplicate long option `--{long}`"), span));
            }
        }
    }
    Ok(long_specs)
}

fn short_option_specs(
    specs: &BTreeMap<String, OptionSpec>,
    span: Span,
) -> Result<BTreeMap<String, String>, RuntimeError> {
    let mut short_specs = BTreeMap::new();
    for (name, spec) in specs {
        for short in &spec.short {
            if short.is_empty() {
                return Err(cli_error(
                    format!("option `{name}` has an empty short name"),
                    span,
                ));
            }
            if short_specs.insert(short.clone(), name.clone()).is_some() {
                return Err(cli_error(
                    format!("duplicate short option `-{short}`"),
                    span,
                ));
            }
        }
    }
    Ok(short_specs)
}

fn push_positional_value(
    output: &mut RecordMap,
    sources: &mut RecordMap,
    name: &str,
    spec: &OptionSpec,
    raw: &str,
    index: usize,
    span: Span,
) -> Result<(), RuntimeError> {
    let value = convert_arg_value(name, raw, &spec.value_ty, index, span)?;
    validate_option_value(name, spec, &value, index, span)?;
    if spec.repeated {
        match output.get_mut(name) {
            Some(Value::List(items)) => items.push(value),
            _ => {
                output.insert(Arc::from(name), Value::List(vec![value]));
            }
        }
        sources.insert(Arc::from(name), Value::Str("argv".into()));
        return Ok(());
    }
    output.insert(Arc::from(name), value);
    sources.insert(Arc::from(name), Value::Str("argv".into()));
    Ok(())
}

fn parse_command_values(
    argv: &[String],
    rootless_default: &str,
    specs: &BTreeMap<String, CommandSpec>,
    fallback: Option<&CommandSpec>,
    span: Span,
) -> Result<RecordMap, RuntimeError> {
    if let Some(token) = argv.first() {
        let key = command_key(token);
        if let Some(spec) = specs.get(&key) {
            return parse_command_record(token, &argv[1..], spec, span);
        }
        if let Some(spec) = fallback
            && (!spec.command_like || command_like(token))
        {
            return parse_command_record(token, argv, spec, span);
        }
    }
    if !rootless_default.is_empty() {
        let key = command_key(rootless_default);
        let Some(spec) = specs.get(&key) else {
            return Err(cli_commands_error(
                format!("unknown rootless default command `{rootless_default}`"),
                span,
            ));
        };
        return parse_command_record(rootless_default, argv, spec, span);
    }
    let message = match argv.first() {
        Some(token) => format!("unknown command `{token}`"),
        None => "missing command".to_string(),
    };
    Err(cli_commands_error(message, span))
}

fn parse_command_record(
    command: &str,
    values: &[String],
    spec: &CommandSpec,
    span: Span,
) -> Result<RecordMap, RuntimeError> {
    let canonical = if spec.canonical == "fallback_command" {
        command
    } else {
        &spec.canonical
    };
    let mut output = RecordMap::from([
        (Arc::from("command"), Value::Str(canonical.into())),
        (Arc::from("action"), Value::Str(command.into())),
    ]);
    let mut option_argv = Vec::new();
    let mut positional_values = Vec::new();
    split_command_options(
        values,
        &spec.options,
        &mut option_argv,
        &mut positional_values,
        span,
    )?;
    if !spec.options.is_empty() {
        let options = parse_values(&option_argv, &spec.options, &RecordMap::new(), span)?;
        for (key, value) in options.values.iter() {
            output.insert(key.clone(), value.clone());
        }
    }
    let mut index = 0;
    for name in &spec.positionals {
        let Some(raw) = positional_values.get(index) else {
            return Err(cli_commands_error(
                format!("missing positional `{name}` for command `{command}`"),
                span,
            ));
        };
        let value_ty = spec.types.get(name).unwrap_or(&ArgValueType::Str);
        let value = convert_command_arg_value(name, raw, value_ty, index, span)?;
        output.insert(Arc::from(name.as_str()), value);
        index += 1;
    }
    let rest_values = positional_values[index..]
        .iter()
        .cloned()
        .map(|s| Value::Str(s.into()))
        .collect::<Vec<_>>();
    if rest_values.len() < spec.min_rest {
        return Err(cli_commands_error(
            format!(
                "command `{command}` expects at least {} rest arguments",
                spec.min_rest
            ),
            span,
        ));
    }
    if let Some(rest) = &spec.rest {
        output.insert(Arc::from(rest.as_str()), Value::List(rest_values));
    } else if let Some(extra) = positional_values.get(index) {
        return Err(cli_commands_error(
            format!("unexpected positional argument for command `{command}`: {extra}"),
            span,
        ));
    }
    Ok(output)
}

fn split_command_options(
    values: &[String],
    specs: &BTreeMap<String, OptionSpec>,
    option_argv: &mut Vec<String>,
    positionals: &mut Vec<String>,
    span: Span,
) -> Result<(), RuntimeError> {
    if specs.is_empty() {
        positionals.extend(values.iter().cloned());
        return Ok(());
    }
    let long_specs = long_option_specs(specs, span)?;
    let short_specs = short_option_specs(specs, span)?;
    let mut index = 0;
    let mut operands_only = false;
    while index < values.len() {
        let token = &values[index];
        if operands_only {
            positionals.push(token.clone());
            index += 1;
            continue;
        }
        if token == "--" {
            operands_only = true;
            index += 1;
            continue;
        }
        if let Some(raw) = token.strip_prefix("--") {
            let name = raw.split_once('=').map_or(raw, |(name, _)| name);
            let key = normalize_arg_name(name);
            if let Some(field) = long_specs
                .get(&key)
                .or_else(|| specs.get_key_value(&key).map(|(name, _)| name))
            {
                option_argv.push(token.clone());
                let spec = specs.get(field).expect("known option field");
                if !spec.flag
                    && !raw.contains('=')
                    && !spec.optional_value
                    && let Some(next) = values.get(index + 1)
                {
                    option_argv.push(next.clone());
                    index += 1;
                }
                index += 1;
                continue;
            }
        } else if token.starts_with('-') && token.len() > 1 && !looks_negative_number(token) {
            let mut known = false;
            for ch in token.trim_start_matches('-').chars() {
                if short_specs.contains_key(&ch.to_string()) {
                    known = true;
                    break;
                }
            }
            if known {
                option_argv.push(token.clone());
                index += 1;
                continue;
            }
        }
        positionals.push(token.clone());
        index += 1;
    }
    Ok(())
}

fn command_key(command: &str) -> String {
    command.replace('-', "_")
}

fn command_like(command: &str) -> bool {
    !command.starts_with('/') && !command.starts_with('.') && !command.contains('/')
}

fn defaults(
    specs: &BTreeMap<String, OptionSpec>,
    env: &RecordMap,
    span: Span,
) -> Result<ParsedValues, RuntimeError> {
    let mut values = RecordMap::new();
    let mut sources = RecordMap::new();
    for (name, spec) in specs {
        if spec.repeated {
            values.insert(
                Arc::from(name.as_str()),
                spec.default
                    .clone()
                    .unwrap_or_else(|| Value::List(Vec::new())),
            );
            sources.insert(
                Arc::from(name.as_str()),
                Value::Str(
                    if spec.default.is_some() {
                        "default"
                    } else {
                        "absent"
                    }
                    .into(),
                ),
            );
        } else if let Some(env_name) = &spec.env
            && let Some(value) = env.get(env_name)
        {
            let raw = value_to_env_text(name, value, span)?;
            let value = convert_arg_value(name, &raw, &spec.value_ty, 0, span)?;
            validate_option_value(name, spec, &value, 0, span)?;
            values.insert(Arc::from(name.as_str()), value);
            sources.insert(Arc::from(name.as_str()), Value::Str("env".into()));
        } else if let Some(default) = &spec.default {
            validate_option_value(name, spec, default, 0, span)?;
            values.insert(Arc::from(name.as_str()), default.clone());
            sources.insert(Arc::from(name.as_str()), Value::Str("default".into()));
        } else if spec.flag {
            values.insert(Arc::from(name.as_str()), Value::Bool(false));
            sources.insert(Arc::from(name.as_str()), Value::Str("default".into()));
        } else if !spec.required {
            values.insert(Arc::from(name.as_str()), Value::Null);
            sources.insert(Arc::from(name.as_str()), Value::Str("absent".into()));
        }
    }
    Ok(ParsedValues {
        values,
        sources,
        warnings: Vec::new(),
    })
}

fn value_to_env_text(name: &str, value: &Value, span: Span) -> Result<String, RuntimeError> {
    match value {
        Value::Str(value) => Ok(value.to_string()),
        Value::Int(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Path(value) => Ok(value.display()),
        Value::Duration(value) => Ok(format!("{}ms", value.millis)),
        value => Err(cli_error(
            format!(
                "env fallback for option `{name}` must be scalar, found {}",
                value.type_name()
            ),
            span,
        )),
    }
}

fn optional_value_default(
    name: &str,
    spec: &OptionSpec,
    span: Span,
) -> Result<Value, RuntimeError> {
    if let Some(value) = &spec.optional_default {
        return Ok(value.clone());
    }
    if matches!(spec.value_ty, ArgValueType::Bool) {
        return Ok(Value::Bool(true));
    }
    if let Some(default) = &spec.default {
        return Ok(default.clone());
    }
    Err(cli_error(
        format!("option {} expects a value", option_label(name, spec)),
        span,
    ))
}

fn usage_text(specs: &BTreeMap<String, OptionSpec>, command: &str) -> String {
    let mut usage = format!("usage: {command}");
    for (name, spec) in specs {
        if spec.hidden {
            continue;
        }
        if spec.positional {
            let label = spec
                .form
                .as_deref()
                .map(|form| form.to_string())
                .unwrap_or_else(|| name.to_ascii_uppercase());
            if spec.required {
                usage.push(' ');
                usage.push_str(&label);
            } else {
                usage.push_str(" [");
                usage.push_str(&label);
                usage.push(']');
            }
        }
    }
    let visible_options = specs
        .iter()
        .filter(|(_, spec)| !spec.hidden && !spec.positional)
        .collect::<Vec<_>>();
    let visible_positionals = specs
        .iter()
        .filter(|(_, spec)| !spec.hidden && spec.positional && spec.help.is_some())
        .collect::<Vec<_>>();
    usage.push_str(" [OPTIONS]");
    let mut output = usage;
    if !visible_positionals.is_empty() {
        output.push_str("\n\narguments:");
        for (name, spec) in visible_positionals {
            output.push('\n');
            output.push_str("  ");
            output.push_str(&usage_metavar(name, spec));
            if let Some(help) = &spec.help {
                output.push_str("  ");
                output.push_str(help);
            }
        }
    }
    output.push_str("\n\noptions:");
    for (name, spec) in visible_options {
        output.push('\n');
        output.push_str("  ");
        output.push_str(&usage_option_names(name, spec));
        if let Some(help) = &spec.help {
            output.push_str("  ");
            output.push_str(help);
        }
        if let Some(message) = &spec.deprecated {
            output.push_str("  deprecated");
            if message != &format!("option `{name}` is deprecated") {
                output.push_str(": ");
                output.push_str(message);
            }
        }
    }
    output.push('\n');
    output.push_str("  -h, --help  show this help");
    output
}

fn usage_option_names(name: &str, spec: &OptionSpec) -> String {
    let mut names = Vec::new();
    for short in &spec.short {
        names.push(format!("-{short}"));
    }
    if spec.long.is_empty() {
        names.push(format!("--{}", name.replace('_', "-")));
    } else {
        for long in &spec.long {
            names.push(format!("--{}", long.replace('_', "-")));
        }
    }
    let mut text = names.join(", ");
    if !spec.flag {
        let metavar = usage_metavar(name, spec);
        if spec.optional_value {
            text.push_str(&format!("[={metavar}]"));
        } else {
            text.push(' ');
            text.push_str(&metavar);
        }
    }
    text
}

fn usage_metavar(name: &str, spec: &OptionSpec) -> String {
    if let Some(form) = &spec.form {
        for token in form.split_whitespace().rev() {
            if !token.starts_with('-') {
                return token.trim_start_matches("...").to_string();
            }
            if let Some((_, value)) = token.split_once("[=") {
                return value.trim_end_matches(']').to_string();
            }
            if let Some((_, value)) = token.split_once('=') {
                return value.to_string();
            }
        }
    }
    name.to_ascii_uppercase()
}

fn validate_option_value(
    name: &str,
    spec: &OptionSpec,
    value: &Value,
    index: usize,
    span: Span,
) -> Result<(), RuntimeError> {
    if !spec.choices.is_empty() {
        let Some(text) = value_choice_text(value) else {
            return Err(cli_error(
                format!(
                    "option {} cannot use `choices` with {}",
                    option_label(name, spec),
                    value.type_name()
                ),
                span,
            ));
        };
        if !spec.choices.contains(&text) {
            return Err(cli_error(
                format!(
                    "option {} expects one of {}, got `{text}` at argv[{index}]",
                    option_label(name, spec),
                    spec.choices.join("|")
                ),
                span,
            ));
        }
    }
    if let Value::Int(value) = value {
        if spec.positive && *value <= 0 {
            return Err(cli_error(
                format!(
                    "option {} expects a positive integer",
                    option_label(name, spec)
                ),
                span,
            ));
        }
        if spec.nonzero && *value == 0 {
            return Err(cli_error(
                format!(
                    "option {} expects a non-zero integer",
                    option_label(name, spec)
                ),
                span,
            ));
        }
        if let Some(min) = spec.min
            && *value < min
        {
            return Err(cli_error(
                format!("option {} expects value >= {min}", option_label(name, spec)),
                span,
            ));
        }
        if let Some(max) = spec.max
            && *value > max
        {
            return Err(cli_error(
                format!("option {} expects value <= {max}", option_label(name, spec)),
                span,
            ));
        }
    }
    if let Value::Duration(value) = value
        && spec.positive
        && value.millis == 0
    {
        return Err(cli_error(
            format!(
                "option {} expects a positive duration",
                option_label(name, spec)
            ),
            span,
        ));
    }
    if let Value::Path(path) = value {
        validate_path_option(name, spec, path, span)?;
    }
    Ok(())
}

fn validate_path_option(
    name: &str,
    spec: &OptionSpec,
    value: &PathValue,
    span: Span,
) -> Result<(), RuntimeError> {
    if !(spec.exists || spec.file || spec.dir) {
        return Ok(());
    }
    let display = value.display();
    let path = Path::new(&display);
    if spec.exists && !path.exists() {
        return Err(cli_error(
            format!(
                "option {} expects an existing path: {display}",
                option_label(name, spec)
            ),
            span,
        ));
    }
    if spec.file && !path.is_file() {
        return Err(cli_error(
            format!(
                "option {} expects a file path: {display}",
                option_label(name, spec)
            ),
            span,
        ));
    }
    if spec.dir && !path.is_dir() {
        return Err(cli_error(
            format!(
                "option {} expects a directory path: {display}",
                option_label(name, spec)
            ),
            span,
        ));
    }
    Ok(())
}

fn value_choice_text(value: &Value) -> Option<String> {
    match value {
        Value::Str(value) => Some(value.to_string()),
        Value::Int(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Path(value) => Some(value.display()),
        Value::Duration(value) => Some(format!("{}ms", value.millis)),
        _ => None,
    }
}

fn convert_arg_value(
    name: &str,
    raw: &str,
    value_ty: &ArgValueType,
    index: usize,
    span: Span,
) -> Result<Value, RuntimeError> {
    match value_ty {
        ArgValueType::Str => Ok(Value::Str(raw.into())),
        ArgValueType::Int => raw.parse::<i64>().map(Value::Int).map_err(|_| {
            cli_error(
                format!("option --{name} expects Int at argv[{index}], got `{raw}`"),
                span,
            )
        }),
        ArgValueType::UInt => parse_uint(raw).map(Value::Int).ok_or_else(|| {
            cli_error(
                format!("option --{name} expects UInt at argv[{index}], got `{raw}`"),
                span,
            )
        }),
        ArgValueType::Bool => parse_bool(raw).map(Value::Bool).ok_or_else(|| {
            cli_error(
                format!("option --{name} expects Bool at argv[{index}], got `{raw}`"),
                span,
            )
        }),
        ArgValueType::Path => PathValue::from_text(raw)
            .map(Value::Path)
            .map_err(|error| error.with_span(span)),
        ArgValueType::Duration => DurationValue::from_literal(raw)
            .map(Value::Duration)
            .ok_or_else(|| {
                cli_error(
                    format!("option --{name} expects Duration at argv[{index}], got `{raw}`"),
                    span,
                )
            }),
    }
}

fn convert_command_arg_value(
    name: &str,
    raw: &str,
    value_ty: &ArgValueType,
    index: usize,
    span: Span,
) -> Result<Value, RuntimeError> {
    match value_ty {
        ArgValueType::Str => Ok(Value::Str(raw.into())),
        ArgValueType::Int => raw.parse::<i64>().map(Value::Int).map_err(|_| {
            cli_commands_error(
                format!("positional `{name}` expects Int at argv[{index}], got `{raw}`"),
                span,
            )
        }),
        ArgValueType::UInt => parse_uint(raw).map(Value::Int).ok_or_else(|| {
            cli_commands_error(
                format!("positional `{name}` expects UInt at argv[{index}], got `{raw}`"),
                span,
            )
        }),
        ArgValueType::Bool => parse_bool(raw).map(Value::Bool).ok_or_else(|| {
            cli_commands_error(
                format!("positional `{name}` expects Bool at argv[{index}], got `{raw}`"),
                span,
            )
        }),
        ArgValueType::Path => {
            let path = PathValue::from_text(raw).map_err(|error| error.with_span(span))?;
            Ok(Value::Path(path))
        }
        ArgValueType::Duration => DurationValue::from_literal(raw)
            .map(Value::Duration)
            .ok_or_else(|| {
                cli_commands_error(
                    format!("positional `{name}` expects Duration at argv[{index}], got `{raw}`"),
                    span,
                )
            }),
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_uint(raw: &str) -> Option<i64> {
    let value = raw.parse::<i64>().ok()?;
    (value >= 0).then_some(value)
}

fn cli_error(message: impl Into<String>, span: Span) -> RuntimeError {
    RuntimeError::new("cli-parse", message.into()).with_span(span)
}

fn cli_help_error(message: impl Into<String>, span: Span) -> RuntimeError {
    RuntimeError::new("cli-help", message.into()).with_span(span)
}

fn cli_usage_error(mut error: RuntimeError, usage: String) -> RuntimeError {
    error
        .payload
        .insert(Arc::from("cli_usage"), Value::Bool(true));
    error.message = format!("{}\n\n{usage}", error.message);
    error
}

fn cli_commands_error(message: impl Into<String>, span: Span) -> RuntimeError {
    RuntimeError::new("cli-commands", message.into()).with_span(span)
}

#[cfg(test)]
mod tests {
    use super::{ArgTokenKind, ArgValueType, parse_type_name};
    use crate::source::{SourceId, Span};

    #[test]
    fn parse_scalar_arg_types_is_centralized() {
        assert!(matches!(
            ArgValueType::parse_scalar("Str"),
            Some(ArgValueType::Str)
        ));
        assert!(matches!(
            ArgValueType::parse_scalar(" Int "),
            Some(ArgValueType::Int)
        ));
        assert!(matches!(
            ArgValueType::parse_scalar("UInt"),
            Some(ArgValueType::UInt)
        ));
        assert!(matches!(
            ArgValueType::parse_scalar("Bool"),
            Some(ArgValueType::Bool)
        ));
        assert!(matches!(
            ArgValueType::parse_scalar("Path"),
            Some(ArgValueType::Path)
        ));
        assert!(ArgValueType::parse_scalar("Uint").is_none());
        assert!(ArgValueType::parse_scalar("File").is_none());
        assert!(ArgValueType::parse_scalar("Dir").is_none());
        assert!(ArgValueType::parse_scalar("List[Str]").is_none());
        assert!(ArgValueType::parse_scalar("Nope").is_none());
    }

    #[test]
    fn parse_type_name_still_handles_list_wrappers() {
        let span = Span::new(SourceId::new(0), 0, 0);
        let (ty, repeated) = parse_type_name(" List[Path] ", span).expect("parse List[Path]");
        assert!(matches!(ty, ArgValueType::Path));
        assert!(repeated);
    }

    #[test]
    fn arg_token_kind_strings_match_public_contract() {
        assert_eq!(ArgTokenKind::Long.as_str(), "long");
        assert_eq!(ArgTokenKind::Short.as_str(), "short");
        assert_eq!(ArgTokenKind::Operand.as_str(), "operand");
    }
}
