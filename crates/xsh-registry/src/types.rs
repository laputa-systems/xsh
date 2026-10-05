use std::collections::BTreeMap;

macro_rules! builtin_type_names {
    ($(($variant:ident, $text:literal)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub enum BuiltinTypeName {
            $($variant),+
        }

        impl BuiltinTypeName {
            pub fn parse(text: &str) -> Option<Self> {
                match text {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                }
            }
        }

        pub const BUILTIN_TYPE_NAMES: &[BuiltinTypeName] = &[
            $(BuiltinTypeName::$variant,)+
        ];
    };
}

builtin_type_names!(
    (Unknown, "<unknown>"),
    (Unit, "Unit"),
    (Any, "Any"),
    (Null, "Null"),
    (Bool, "Bool"),
    (Int, "Int"),
    (UInt, "UInt"),
    (Float, "Float"),
    (Duration, "Duration"),
    (Str, "Str"),
    (Bytes, "Bytes"),
    (Digest, "Digest"),
    (Regex, "Regex"),
    (Path, "Path"),
    (Map, "Map"),
    (Module, "Module"),
    (Record, "Record"),
    (Status, "Status"),
    (EnvPathList, "EnvPathList"),
    (Error, "Error"),
    (ProcessError, "ProcessError"),
    (Pure, "Pure"),
    (Proc, "Proc"),
    (Command, "Command"),
    (ProcessHandle, "ProcessHandle"),
    (NetJob, "NetJob"),
    (FsRoot, "FsRoot"),
    (Result, "Result"),
    (RelPath, "RelPath"),
);

/// Fixed relationships used only by builtin signature templates.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BuiltinTypeParameter {
    Receiver,
    Element,
    Key,
    Value,
    Error,
}

impl BuiltinTypeParameter {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Receiver => "Self",
            Self::Element => "T",
            Self::Key => "K",
            Self::Value => "V",
            Self::Error => "E",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Type {
    BuiltinParameter(BuiltinTypeParameter),
    Any,
    Unknown,
    Invalid,
    Null,
    Bool,
    Int,
    Float,
    Duration,
    Str,
    Bytes,
    Digest,
    Regex,
    Path,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Stream(Box<Type>),
    Record(BTreeMap<String, Type>),
    Module(BTreeMap<String, Type>),
    Result(Box<Type>, Box<Type>),
    Status,
    EnvPathList,
    Error,
    ProcessError,
    ErrorFamily(&'static str),
    Pure,
    Proc,
    Command,
    ProcessHandle,
    NetJob,
    FsRoot,
    Unit,
    Optional(Box<Type>),
    /// A closed union of member types, in the order a dynamic value is
    /// tried against them. A signature lists at least two members, none of
    /// which accepts another's values.
    Union(Vec<Type>),
    /// A list of the element type that holds at least one element.
    NonEmpty(Box<Type>),
    /// A path that is not absolute and never climbs above where it starts.
    RelPath,
}

/// The full CLI outcome always contains this envelope, independently of the
/// descriptor that determines the values record's fields.
pub fn cli_full_fields<T>(values: T, sources: T, warnings: T) -> [(&'static str, T); 3] {
    [
        ("values", values),
        ("sources", sources),
        ("warnings", warnings),
    ]
}

#[cfg(test)]
mod tests {
    use super::{BUILTIN_TYPE_NAMES, BuiltinTypeName};
    use crate::CORE_BUILTIN_SYMBOLS;

    #[test]
    fn builtin_type_names_round_trip_through_parser() {
        for name in BUILTIN_TYPE_NAMES {
            assert_eq!(BuiltinTypeName::parse(name.as_str()), Some(*name));
        }
    }

    #[test]
    fn core_builtin_symbol_table_matches_builtin_type_names() {
        let names: Vec<&str> = BUILTIN_TYPE_NAMES
            .iter()
            .map(|name| name.as_str())
            .collect();
        assert_eq!(CORE_BUILTIN_SYMBOLS, names.as_slice());
    }
}
