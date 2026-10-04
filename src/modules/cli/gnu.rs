//! GNU `getopt_long` mode of `cli.applet`, `cli.parse`, and `cli.parse_full`.
//!
//! A schema enters this mode by declaring a `gnu` record next to its options.
//! The same option descriptors apply; only the scanning rules and the
//! diagnostics change. Every command-line error becomes a `cli-parse` error
//! whose message is the complete GNU diagnostic and whose payload carries
//! `cli_usage` and `cli_status`, so the evaluator's usage boundary prints it
//! on stderr and exits without a traceback.

use super::{
    ArgParseState, ArgValueType, OptionSpec, ParsePolicy, ParsedValues, cli_error,
    convert_arg_value, defaults, long_option_specs, option_label, optional_value_default,
    positional_specs, push_positional_value, short_option_specs, validate_relationships,
};
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

const DEFAULT_STATUS: i64 = 1;

#[derive(Clone, Debug)]
struct Unsupported {
    long: bool,
    name: String,
    reason: String,
}

#[derive(Clone, Debug)]
pub(super) struct GnuConfig {
    prog: Option<String>,
    status: i64,
    permute: bool,
    unsupported: Vec<Unsupported>,
}

impl GnuConfig {
    pub(super) fn from_value(value: &Value, span: Span) -> Result<Self, RuntimeError> {
        let Value::Record(fields) = value else {
            return Err(cli_error(
                format!("`gnu` must be Record, found {}", value.type_name()),
                span,
            ));
        };
        let mut config = Self {
            prog: None,
            status: DEFAULT_STATUS,
            permute: true,
            unsupported: Vec::new(),
        };
        for (field, value) in fields.iter() {
            match (field.as_ref(), value) {
                ("prog", Value::Str(prog)) => config.prog = Some(prog.to_string()),
                ("status", Value::Int(status)) if (0..=255).contains(status) => {
                    config.status = *status;
                }
                ("permute", Value::Bool(permute)) => config.permute = *permute,
                ("unsupported", Value::Record(entries)) => {
                    config.unsupported = unsupported_entries(entries, span)?;
                }
                ("prog" | "status" | "permute" | "unsupported", value) => {
                    return Err(cli_error(
                        format!(
                            "`gnu` field `{field}` has an invalid {} value",
                            value.type_name()
                        ),
                        span,
                    ));
                }
                _ => {
                    return Err(cli_error(
                        format!(
                            "unknown `gnu` field `{field}`; expected prog, status, permute, or unsupported"
                        ),
                        span,
                    ));
                }
            }
        }
        Ok(config)
    }
}

fn unsupported_entries(entries: &RecordMap, span: Span) -> Result<Vec<Unsupported>, RuntimeError> {
    let mut out = Vec::new();
    for (key, reason) in entries.iter() {
        let Value::Str(reason) = reason else {
            return Err(cli_error(
                format!("`gnu` unsupported reason for `{key}` must be Str"),
                span,
            ));
        };
        let (long, name) = if let Some(name) = key.strip_prefix("--") {
            (true, name)
        } else if let Some(name) = key.strip_prefix('-') {
            (false, name)
        } else {
            (false, "")
        };
        if name.is_empty() || (!long && name.chars().count() != 1) {
            return Err(cli_error(
                format!("`gnu` unsupported key `{key}` must be `--long` or `-s`"),
                span,
            ));
        }
        out.push(Unsupported {
            long,
            name: name.to_string(),
            reason: reason.to_string(),
        });
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Arity {
    /// A switch: `--opt=value` is an error.
    Switch,
    /// A switch that may carry an attached value of the declared type.
    SwitchValue,
    /// A value that may be omitted; long forms attach with `=`, short forms
    /// attach directly.
    Optional,
    Required,
}

fn arity(spec: &OptionSpec) -> Arity {
    if spec.flag {
        if spec.value_ty == ArgValueType::Bool && !spec.optional_value {
            Arity::Switch
        } else {
            Arity::SwitchValue
        }
    } else if spec.optional_value {
        Arity::Optional
    } else {
        Arity::Required
    }
}

struct LongEntry {
    name: String,
    /// The owning option, or the reason a declared-unsupported option fails.
    target: LongTarget,
}

enum LongTarget {
    Option(String),
    Unsupported(String),
}

enum LongMatch<'a> {
    Found(&'a LongEntry),
    Ambiguous(Vec<&'a str>),
    Unknown,
}

fn long_table(specs: &BTreeMap<String, OptionSpec>, config: &GnuConfig) -> Vec<LongEntry> {
    let mut table = Vec::new();
    for (name, spec) in specs {
        for long in &spec.long {
            table.push(LongEntry {
                name: long.replace('_', "-"),
                target: LongTarget::Option(name.clone()),
            });
        }
    }
    for entry in config.unsupported.iter().filter(|entry| entry.long) {
        table.push(LongEntry {
            name: entry.name.clone(),
            target: LongTarget::Unsupported(entry.reason.clone()),
        });
    }
    table.sort_by(|left, right| left.name.cmp(&right.name));
    table
}

/// getopt_long resolution: an exact name always wins; otherwise a unique
/// prefix wins, aliases of one option count once, and any other overlap is
/// ambiguous.
fn find_long<'a>(table: &'a [LongEntry], typed: &str) -> LongMatch<'a> {
    if typed.is_empty() {
        return LongMatch::Unknown;
    }
    if let Some(exact) = table.iter().find(|entry| entry.name == typed) {
        return LongMatch::Found(exact);
    }
    let mut candidates = table.iter().filter(|entry| entry.name.starts_with(typed));
    let Some(first) = candidates.next() else {
        return LongMatch::Unknown;
    };
    let mut listed = vec![first.name.as_str()];
    for entry in candidates {
        let same = matches!(
            (&entry.target, &first.target),
            (LongTarget::Option(left), LongTarget::Option(right)) if left == right
        );
        if !same {
            listed.push(entry.name.as_str());
        }
    }
    if listed.len() == 1 {
        LongMatch::Found(first)
    } else {
        LongMatch::Ambiguous(listed)
    }
}

struct Diag {
    prog: String,
    phrase: String,
    status: i64,
    span: Span,
}

impl Diag {
    fn new(config: &GnuConfig, command: &str, span: Span) -> Self {
        let prog = config.prog.clone().unwrap_or_else(|| {
            Path::new(command)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(command)
                .to_string()
        });
        let phrase = std::env::var("XSH_EXECUTION_PHRASE")
            .ok()
            .filter(|phrase| !phrase.is_empty())
            .unwrap_or_else(|| prog.clone());
        Self {
            prog,
            phrase,
            status: config.status,
            span,
        }
    }

    fn error(&self, detail: impl std::fmt::Display) -> RuntimeError {
        let message = format!(
            "{}: {detail}\nTry '{} --help' for more information.",
            self.prog, self.phrase
        );
        let mut error = RuntimeError::new("cli-parse", message).with_span(self.span);
        error
            .payload
            .insert(Arc::from("cli_usage"), Value::Bool(true));
        error
            .payload
            .insert(Arc::from("cli_status"), Value::Int(self.status));
        error
    }
}

struct Scan<'a> {
    argv: &'a [String],
    specs: &'a BTreeMap<String, OptionSpec>,
    shorts: &'a BTreeMap<String, String>,
    longs: Vec<LongEntry>,
    unsupported_shorts: &'a [Unsupported],
    numeric: Option<&'a str>,
    diag: Diag,
    span: Span,
}

struct Step {
    next: usize,
    stop: bool,
}

pub(super) fn parse_values(
    argv: &[String],
    specs: &BTreeMap<String, OptionSpec>,
    config: &GnuConfig,
    command: &str,
    env: &RecordMap,
    span: Span,
) -> Result<ParsedValues, RuntimeError> {
    let shorts = short_option_specs(specs, span)?;
    long_option_specs(specs, span)?;
    let scan = Scan {
        argv,
        specs,
        shorts: &shorts,
        longs: long_table(specs, config),
        unsupported_shorts: &config.unsupported,
        numeric: specs
            .iter()
            .find(|(_, spec)| spec.numeric && !spec.positional)
            .map(|(name, _)| name.as_str()),
        diag: Diag::new(config, command, span),
        span,
    };
    let mut parsed = defaults(specs, env, span)?;
    let mut state = ArgParseState {
        argv,
        specs,
        short_specs: &shorts,
        output: &mut parsed.values,
        sources: &mut parsed.sources,
        warnings: &mut parsed.warnings,
        present: BTreeMap::new(),
        span,
        // Scalars overwrite and conflicting options reset one another, which
        // is the getopt "last one wins" behavior.
        policy: ParsePolicy::Applet,
    };
    let permute = config.permute && std::env::var_os("POSIXLY_CORRECT").is_none();
    let mut operands: Vec<(usize, &str)> = Vec::new();
    let mut index = 0;
    let mut stopped = false;
    while index < argv.len() {
        let arg = argv[index].as_str();
        let step = if arg == "--" {
            operands.extend(
                argv[index + 1..]
                    .iter()
                    .enumerate()
                    .map(|(offset, arg)| (index + 1 + offset, arg.as_str())),
            );
            break;
        } else if let Some(body) = arg.strip_prefix("--") {
            scan.long(&mut state, index, body)?
        } else if arg.len() > 1 && arg.starts_with('-') {
            scan.short(&mut state, index, &arg[1..])?
        } else {
            operands.push((index, arg));
            if !permute {
                operands.extend(
                    argv[index + 1..]
                        .iter()
                        .enumerate()
                        .map(|(offset, arg)| (index + 1 + offset, arg.as_str())),
                );
                break;
            }
            index += 1;
            continue;
        };
        index = step.next;
        if step.stop {
            stopped = true;
            break;
        }
    }
    if !stopped {
        scan.finish(&mut state, &operands)?;
    }
    drop(state);
    Ok(parsed)
}

impl Scan<'_> {
    fn long(
        &self,
        state: &mut ArgParseState<'_>,
        index: usize,
        body: &str,
    ) -> Result<Step, RuntimeError> {
        let (typed, inline) = match body.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (body, None),
        };
        let entry = match find_long(&self.longs, typed) {
            LongMatch::Found(entry) => entry,
            LongMatch::Ambiguous(names) => {
                let listed = names
                    .iter()
                    .map(|name| format!("'--{name}'"))
                    .collect::<Vec<_>>()
                    .join(" ");
                return Err(self.diag.error(format!(
                    "option '--{body}' is ambiguous; possibilities: {listed}"
                )));
            }
            LongMatch::Unknown => {
                return Err(self.diag.error(format!("unrecognized option '--{body}'")));
            }
        };
        let name = match &entry.target {
            LongTarget::Unsupported(reason) => {
                return Err(self.diag.error(format!(
                    "option '--{}' is not supported: {reason}",
                    entry.name
                )));
            }
            LongTarget::Option(name) => name,
        };
        let spec = &self.specs[name];
        let mut next = index + 1;
        let raw = match (arity(spec), inline) {
            (Arity::Switch, Some(_)) => {
                return Err(self.diag.error(format!(
                    "option '--{}' doesn't allow an argument",
                    entry.name
                )));
            }
            (Arity::Switch, None) | (Arity::SwitchValue | Arity::Optional, None) => None,
            (Arity::SwitchValue | Arity::Optional, Some(value)) | (Arity::Required, Some(value)) => {
                Some(value)
            }
            (Arity::Required, None) => {
                let Some(value) = self.argv.get(next) else {
                    return Err(self
                        .diag
                        .error(format!("option '--{}' requires an argument", entry.name)));
                };
                next += 1;
                Some(value.as_str())
            }
        };
        self.assign(state, name, spec, raw, index)?;
        Ok(Step {
            next,
            stop: spec.stop,
        })
    }

    fn short(
        &self,
        state: &mut ArgParseState<'_>,
        index: usize,
        cluster: &str,
    ) -> Result<Step, RuntimeError> {
        let mut pos = 0;
        let mut stop = false;
        while pos < cluster.len() {
            let ch = cluster[pos..].chars().next().expect("cluster offset is valid");
            let rest = &cluster[pos + ch.len_utf8()..];
            if let Some(name) = self.shorts.get(ch.to_string().as_str()) {
                let spec = &self.specs[name];
                let (raw, next, ends) = match arity(spec) {
                    Arity::Switch => (None, index + 1, false),
                    Arity::SwitchValue | Arity::Optional => {
                        ((!rest.is_empty()).then_some(rest), index + 1, true)
                    }
                    Arity::Required if !rest.is_empty() => (Some(rest), index + 1, true),
                    Arity::Required => {
                        let Some(value) = self.argv.get(index + 1) else {
                            return Err(self
                                .diag
                                .error(format!("option requires an argument -- '{ch}'")));
                        };
                        (Some(value.as_str()), index + 2, true)
                    }
                };
                self.assign(state, name, spec, raw, index)?;
                if spec.stop || ends {
                    return Ok(Step {
                        next,
                        stop: spec.stop,
                    });
                }
                pos += ch.len_utf8();
            } else if let Some(entry) = self
                .unsupported_shorts
                .iter()
                .find(|entry| !entry.long && entry.name == ch.to_string())
            {
                return Err(self.diag.error(format!(
                    "option '-{ch}' is not supported: {}",
                    entry.reason
                )));
            } else if let (true, Some(name)) = (ch.is_ascii_digit(), self.numeric) {
                let digits = cluster[pos..]
                    .find(|ch: char| !ch.is_ascii_digit())
                    .map_or(cluster.len() - pos, |len| len);
                let spec = &self.specs[name];
                self.assign(state, name, spec, Some(&cluster[pos..pos + digits]), index)?;
                stop = stop || spec.stop;
                pos += digits;
            } else {
                return Err(self.diag.error(format!("invalid option -- '{ch}'")));
            }
        }
        Ok(Step {
            next: index + 1,
            stop,
        })
    }

    fn assign(
        &self,
        state: &mut ArgParseState<'_>,
        name: &str,
        spec: &OptionSpec,
        raw: Option<&str>,
        index: usize,
    ) -> Result<(), RuntimeError> {
        let invalid = |raw: &str| {
            self.diag.error(format!(
                "invalid argument '{raw}' for '{}'",
                option_label(name, spec)
            ))
        };
        let value = match raw {
            Some(raw) => convert_arg_value(name, raw, &spec.value_ty, index, self.span)
                .map_err(|_| invalid(raw))?,
            None if arity(spec) == Arity::Optional => {
                optional_value_default(name, spec, self.span).map_err(|_| {
                    self.diag.error(format!(
                        "option '{}' requires an argument",
                        option_label(name, spec)
                    ))
                })?
            }
            None => Value::Bool(true),
        };
        let shown = raw.unwrap_or_default();
        state
            .set_option_value(name, spec, value, index)
            .map_err(|_| invalid(shown))
    }

    fn finish(
        &self,
        state: &mut ArgParseState<'_>,
        operands: &[(usize, &str)],
    ) -> Result<(), RuntimeError> {
        let positionals = positional_specs(self.specs);
        let mut next = 0;
        for &(index, raw) in operands {
            let Some((name, spec)) = positionals.get(next) else {
                return Err(self.diag.error(format!("extra operand '{raw}'")));
            };
            push_positional_value(
                state.output,
                state.sources,
                name,
                spec,
                raw,
                index,
                self.span,
            )
            .map_err(|_| self.diag.error(format!("invalid argument '{raw}'")))?;
            state.present.insert((*name).clone(), true);
            if !spec.repeated {
                next += 1;
            }
        }
        for (name, spec) in self.specs {
            if spec.required && !state.present.contains_key(name.as_str()) {
                return Err(self.diag.error(if spec.positional {
                    "missing operand".to_string()
                } else {
                    format!("missing required argument {}", option_label(name, spec))
                }));
            }
        }
        validate_relationships(self.specs, &state.present, self.span)
            .map_err(|error| self.diag.error(error.message))
    }
}
