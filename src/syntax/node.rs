//! Leaf syntax types shared across the compact arena, checker, lowering, and
//! tooling. These carry no recursive children, so they survive independently of
//! the old recursive AST: the arena stores them directly (ops, literals, format
//! specs) and the checker/linter reuse the operator and effect vocabularies.

use crate::source::Span;
use crate::symbol::Name;
use std::str::FromStr;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Effect {
    Fs,
    Net,
    Process,
    Env,
    Time,
    Error,
    Io,
}

impl Effect {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fs => "fs",
            Self::Net => "net",
            Self::Process => "process",
            Self::Env => "env",
            Self::Time => "time",
            Self::Error => "error",
            Self::Io => "io",
        }
    }

    /// Map a standard-library module call to the effect it requires, or `None`
    /// if the specific function is pure. Used by both the linter (inference) and
    /// the checker (enforcement) so the two stay in sync.
    pub fn from_module_call(module: &str, function: &str) -> Option<Self> {
        match module {
            "fs" | "archive" | "diff" | "elf" | "patch" | "user" | "group" | "module" => {
                Some(Self::Fs)
            }
            "io" => Some(Self::Io),
            "tui" => match function {
                "read_secret" => Some(Self::Io),
                _ => None,
            },
            "net" | "dns" => Some(Self::Net),
            "env" => Some(Self::Env),
            "error" => Some(Self::Error),
            "time" => match function {
                // Pure Duration constructors: no clock access, usable anywhere.
                "millis" | "seconds" => None,
                _ => Some(Self::Time),
            },
            "system" => Some(Self::Env),
            "applet" => Some(Self::Process),
            "process" | "unix" | "linux" => match function {
                "command_argv" | "argv_words" => None,
                _ => Some(Self::Process),
            },
            "json" => match function {
                "read" | "write" => Some(Self::Fs),
                _ => None,
            },
            "ini" => match function {
                "read" | "write" => Some(Self::Fs),
                _ => None,
            },
            "mime" => match function {
                "lookup_ext" | "lookup_path" => Some(Self::Fs),
                _ => None,
            },
            "path" => match function {
                "resolve" => Some(Self::Fs),
                _ => None,
            },
            "hash" => match function {
                "verify_file" => Some(Self::Fs),
                _ => None,
            },
            _ => None,
        }
    }
}

impl FromStr for Effect {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fs" => Ok(Self::Fs),
            "net" => Ok(Self::Net),
            "process" => Ok(Self::Process),
            "env" => Ok(Self::Env),
            "time" => Ok(Self::Time),
            "error" => Ok(Self::Error),
            "io" => Ok(Self::Io),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntLiteral {
    value: Option<i64>,
    raw: Option<Arc<str>>,
}

impl IntLiteral {
    pub fn from_text(text: &str) -> Self {
        if let Some(octal) = text.strip_prefix("0o") {
            return Self {
                value: i64::from_str_radix(octal, 8).ok(),
                raw: Some(Arc::from(text)),
            };
        }
        match text.parse::<i64>() {
            Ok(value) => Self {
                value: Some(value),
                raw: None,
            },
            Err(_) => Self {
                value: None,
                raw: Some(Arc::from(text)),
            },
        }
    }

    pub fn value(&self) -> Option<i64> {
        self.value
    }

    pub fn write(&self, output: &mut String) {
        if let Some(raw) = &self.raw {
            output.push_str(raw);
        } else if let Some(value) = self.value {
            output.push_str(&value.to_string());
        }
    }

    pub fn to_text(&self) -> String {
        if let Some(raw) = &self.raw {
            raw.to_string()
        } else {
            self.value.unwrap_or_default().to_string()
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FloatLiteral {
    value_bits: Option<u64>,
    raw: Arc<str>,
}

impl FloatLiteral {
    pub fn from_text(text: &str) -> Self {
        Self {
            value_bits: text.parse::<f64>().ok().map(f64::to_bits),
            raw: Arc::from(text),
        }
    }

    pub fn value(&self) -> Option<f64> {
        self.value_bits.map(f64::from_bits)
    }

    pub fn write(&self, output: &mut String) {
        output.push_str(&self.raw);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurationUnit {
    Millis,
    Seconds,
    Minutes,
    Hours,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurationLiteral {
    amount: Option<u64>,
    unit: DurationUnit,
    raw: Option<Arc<str>>,
}

impl DurationLiteral {
    pub fn from_text(text: &str) -> Self {
        let (number, unit) = if let Some(number) = text.strip_suffix("ms") {
            (number, DurationUnit::Millis)
        } else if let Some(number) = text.strip_suffix('s') {
            (number, DurationUnit::Seconds)
        } else if let Some(number) = text.strip_suffix('m') {
            (number, DurationUnit::Minutes)
        } else if let Some(number) = text.strip_suffix('h') {
            (number, DurationUnit::Hours)
        } else {
            return Self {
                amount: None,
                unit: DurationUnit::Millis,
                raw: Some(Arc::from(text)),
            };
        };
        match number.parse::<u64>() {
            Ok(amount) => Self {
                amount: Some(amount),
                unit,
                raw: None,
            },
            Err(_) => Self {
                amount: None,
                unit,
                raw: Some(Arc::from(text)),
            },
        }
    }

    pub fn millis(&self) -> Option<u64> {
        let multiplier = match self.unit {
            DurationUnit::Millis => 1,
            DurationUnit::Seconds => 1_000,
            DurationUnit::Minutes => 60_000,
            DurationUnit::Hours => 3_600_000,
        };
        self.amount?.checked_mul(multiplier)
    }

    pub fn write(&self, output: &mut String) {
        if let Some(raw) = &self.raw {
            output.push_str(raw);
            return;
        }
        let amount = self.amount.unwrap_or_default();
        output.push_str(&amount.to_string());
        output.push_str(match self.unit {
            DurationUnit::Millis => "ms",
            DurationUnit::Seconds => "s",
            DurationUnit::Minutes => "m",
            DurationUnit::Hours => "h",
        });
    }

    pub fn to_text(&self) -> String {
        let mut text = String::new();
        self.write(&mut text);
        text
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnvGetKind {
    Str,
    Path,
    PathList,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SignalHookOptions {
    pub pre_cancel: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockParam {
    pub name: Name,
    pub span: Span,
}

/// Format specifier for an f-string interpolation: `${expr:spec}`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatSpec {
    pub kind: FormatSpecKind,
    /// Minimum field width in characters.
    pub width: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatSpecKind {
    /// `>N` — right-align, space-padded.
    RightAlign,
    /// `<N` — left-align, space-padded.
    LeftAlign,
    /// `0N` — right-align, zero-padded (integers only).
    ZeroPad,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamStageKind {
    Where,
    Map,
    ParMap,
    Each,
    Batch,
    Sort,
    SortBy,
    Take,
    Drop,
    First,
    Last,
    UniqueBy,
    Enumerate,
    Zip,
    Range,
    Repeat,
    Tee,
    Sum,
    Min,
    Max,
    GroupBy,
    Fold,
    Reduce,
    FlatMap,
    Any,
    All,
    Shuffle,
    TablePrint,
    TextStreamLines,
    BytesChunks,
    JsonLines,
    JsonStream,
    Count,
    Collect,
    ReduceBy,
}

impl StreamStageKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Where => "where",
            Self::Map => "map",
            Self::ParMap => "par-map",
            Self::Each => "each",
            Self::Batch => "batch",
            Self::Sort => "sort",
            Self::SortBy => "sort-by",
            Self::Take => "take",
            Self::Drop => "drop",
            Self::First => "first",
            Self::Last => "last",
            Self::UniqueBy => "unique-by",
            Self::Enumerate => "enumerate",
            Self::Zip => "zip",
            Self::Range => "range",
            Self::Repeat => "repeat",
            Self::Tee => "tee",
            Self::Sum => "sum",
            Self::Min => "min",
            Self::Max => "max",
            Self::GroupBy => "group-by",
            Self::Fold => "fold",
            Self::Reduce => "reduce",
            Self::FlatMap => "flat-map",
            Self::Any => "any",
            Self::All => "all",
            Self::Shuffle => "shuffle",
            Self::TablePrint => "table.print",
            Self::TextStreamLines => "text.lines",
            Self::BytesChunks => "bytes.chunks",
            Self::JsonLines => "json.lines",
            Self::JsonStream => "json.stream",
            Self::Count => "count",
            Self::Collect => "collect",
            Self::ReduceBy => "reduce-by",
        }
    }

    pub const fn is_adapter(&self) -> bool {
        matches!(
            self,
            Self::TextStreamLines | Self::BytesChunks | Self::JsonLines | Self::JsonStream
        )
    }

    /// Whether this stage uses `()` when it has no args and no block. Stages
    /// that look like function calls (`count()`, `first()`) are in this set;
    /// stages that read as qualifiers (`sort`, `sum`, `min`) are not.
    pub const fn canonical_parens_when_empty(&self) -> bool {
        matches!(
            self,
            Self::Count
                | Self::Collect
                | Self::Take
                | Self::Drop
                | Self::First
                | Self::Last
                | Self::Enumerate
                | Self::Zip
                | Self::Range
                | Self::Repeat
                | Self::Fold
                | Self::Reduce
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryOp {
    ResultFallback,
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    NotIn,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreCommand {
    Print,
    Eprint,
    Cd,
    Env,
}

impl CoreCommand {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "print" => Self::Print,
            "eprint" => Self::Eprint,
            "cd" => Self::Cd,
            "env" => Self::Env,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Eprint => "eprint",
            Self::Cd => "cd",
            Self::Env => "env",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunKind {
    Plain,
    Status,
    CaptureText,
    CaptureBytes,
    CaptureTextRecord,
    CaptureBytesRecord,
    StreamText,
    StreamBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedirectionKind {
    StdoutWrite,
    StdoutAppend,
    StdinRead,
    StderrWrite,
    StderrAppend,
    StdoutDup,
    StdinDup,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandWordRefSegment {
    Field(Name),
    Index(i64),
}

pub fn parse_command_word_reference(text: &str) -> Option<(&str, Vec<CommandWordRefSegment>)> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let start = index;
    if !bytes.get(index).is_some_and(|byte| is_ident_start(*byte)) {
        return None;
    }
    index += 1;
    while bytes
        .get(index)
        .is_some_and(|byte| is_ident_continue(*byte))
    {
        index += 1;
    }
    let root = &text[start..index];
    let mut segments = Vec::new();
    while index < bytes.len() {
        match bytes[index] {
            b'.' => {
                index += 1;
                let start = index;
                if !bytes.get(index).is_some_and(|byte| is_ident_start(*byte)) {
                    return None;
                }
                index += 1;
                while bytes
                    .get(index)
                    .is_some_and(|byte| is_ident_continue(*byte))
                {
                    index += 1;
                }
                segments.push(CommandWordRefSegment::Field(Name::intern(
                    &text[start..index],
                )));
            }
            b'[' => {
                index += 1;
                let start = index;
                while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
                    index += 1;
                }
                if start == index || !matches!(bytes.get(index), Some(b']')) {
                    return None;
                }
                let value = text[start..index].parse::<i64>().ok()?;
                index += 1;
                segments.push(CommandWordRefSegment::Index(value));
            }
            _ => return None,
        }
    }
    Some((root, segments))
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
