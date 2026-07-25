use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::str;
use std::sync::{OnceLock, RwLock};

include!(concat!(env!("OUT_DIR"), "/preloaded_symbols.rs"));

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Symbol(u32);

impl Symbol {
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq)]
pub struct Name(Symbol);

impl Name {
    pub const UNKNOWN: Self = Self(Symbol::from_raw(0));
    pub const UNIT: Self = Self(Symbol::from_raw(1));
    pub const ANY: Self = Self(Symbol::from_raw(2));
    pub const NULL: Self = Self(Symbol::from_raw(3));
    pub const BOOL: Self = Self(Symbol::from_raw(4));
    pub const INT: Self = Self(Symbol::from_raw(5));
    pub const UINT: Self = Self(Symbol::from_raw(6));
    pub const FLOAT: Self = Self(Symbol::from_raw(7));
    pub const DURATION: Self = Self(Symbol::from_raw(8));
    pub const STR: Self = Self(Symbol::from_raw(9));
    pub const BYTES: Self = Self(Symbol::from_raw(10));
    pub const DIGEST: Self = Self(Symbol::from_raw(11));
    pub const REGEX: Self = Self(Symbol::from_raw(12));
    pub const PATH: Self = Self(Symbol::from_raw(13));
    pub const MAP: Self = Self(Symbol::from_raw(14));
    pub const MODULE: Self = Self(Symbol::from_raw(15));
    pub const RECORD: Self = Self(Symbol::from_raw(16));
    pub const STATUS: Self = Self(Symbol::from_raw(17));
    pub const ENV_PATH_LIST: Self = Self(Symbol::from_raw(18));
    pub const ERROR: Self = Self(Symbol::from_raw(19));
    pub const PROCESS_ERROR: Self = Self(Symbol::from_raw(20));
    pub const PURE: Self = Self(Symbol::from_raw(21));
    pub const PROC: Self = Self(Symbol::from_raw(22));
    pub const COMMAND: Self = Self(Symbol::from_raw(23));
    pub const PROCESS_HANDLE: Self = Self(Symbol::from_raw(24));
    pub const RESULT: Self = Self(Symbol::from_raw(25));

    pub fn intern(text: impl AsRef<str>) -> Self {
        let text = text.as_ref();
        if let Some(symbol) = interner()
            .read()
            .expect("symbol interner poisoned")
            .get(text)
        {
            return Self(symbol);
        }
        let mut interner = interner().write().expect("symbol interner poisoned");
        Self(interner.intern(text))
    }

    pub const fn from_symbol(symbol: Symbol) -> Self {
        Self(symbol)
    }

    pub const fn symbol(self) -> Symbol {
        self.0
    }

    pub fn as_str(self) -> &'static str {
        interner()
            .read()
            .expect("symbol interner poisoned")
            .resolve(self.0)
    }

    pub const fn is_builtin(self) -> bool {
        self.0.raw() < CORE_BUILTIN_COUNT
    }

    pub const fn is_preloaded(self) -> bool {
        self.0.raw() < PRELOADED_SYMBOL_COUNT
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<str> for Name {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Name {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for Name {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<&String> for Name {
    fn eq(&self, other: &&String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<Name> for String {
    fn eq(&self, other: &Name) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<Name> for &String {
    fn eq(&self, other: &Name) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<Name> for str {
    fn eq(&self, other: &Name) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Name> for &str {
    fn eq(&self, other: &Name) -> bool {
        *self == other.as_str()
    }
}

impl Hash for Name {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl Ord for Name {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialOrd for Name {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Name {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl From<Name> for String {
    fn from(value: Name) -> Self {
        value.as_str().to_string()
    }
}

impl From<&Name> for String {
    fn from(value: &Name) -> Self {
        value.as_str().to_string()
    }
}

impl From<&str> for Name {
    fn from(value: &str) -> Self {
        Self::intern(value)
    }
}

impl From<String> for Name {
    fn from(value: String) -> Self {
        Self::intern(value)
    }
}

impl From<&String> for Name {
    fn from(value: &String) -> Self {
        Self::intern(value.as_str())
    }
}

impl From<&Name> for Name {
    fn from(value: &Name) -> Self {
        *value
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct QualifiedName {
    pub namespace: Name,
    pub member: Name,
}

impl QualifiedName {
    pub fn new(namespace: Name, member: Name) -> Self {
        Self { namespace, member }
    }
}

impl fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.namespace, self.member)
    }
}

struct Interner {
    by_text: FxHashMap<&'static str, Symbol>,
    dynamic: Vec<&'static str>,
}

impl Interner {
    fn with_preloaded() -> Self {
        let mut interner = Self {
            by_text: FxHashMap::with_capacity_and_hasher(
                PRELOADED_SYMBOL_COUNT as usize,
                Default::default(),
            ),
            dynamic: Vec::new(),
        };
        for index in 0..PRELOADED_SYMBOL_COUNT {
            let symbol = Symbol::from_raw(index);
            let name = resolve_preloaded(symbol).expect("preloaded symbol exists");
            interner.by_text.insert(name, symbol);
        }
        interner
    }

    fn get(&self, text: &str) -> Option<Symbol> {
        self.by_text.get(text).copied()
    }

    fn intern(&mut self, text: &str) -> Symbol {
        if let Some(symbol) = self.get(text) {
            return symbol;
        }
        let text: &'static str = Box::leak(text.to_owned().into_boxed_str());
        let symbol = Symbol::from_raw(
            PRELOADED_SYMBOL_COUNT
                .checked_add(u32::try_from(self.dynamic.len()).expect("too many dynamic symbols"))
                .expect("too many symbols"),
        );
        self.dynamic.push(text);
        self.by_text.insert(text, symbol);
        symbol
    }

    fn resolve(&self, symbol: Symbol) -> &'static str {
        let raw = symbol.raw();
        if raw < PRELOADED_SYMBOL_COUNT {
            return resolve_preloaded(symbol).unwrap_or("<invalid-symbol>");
        }
        let dynamic_index = raw - PRELOADED_SYMBOL_COUNT;
        self.dynamic
            .get(dynamic_index as usize)
            .copied()
            .unwrap_or("<invalid-symbol>")
    }
}

fn interner() -> &'static RwLock<Interner> {
    static INTERNER: OnceLock<RwLock<Interner>> = OnceLock::new();
    INTERNER.get_or_init(|| RwLock::new(Interner::with_preloaded()))
}

/// Count and leaked backing bytes for symbols interned after the preloaded set.
pub fn dynamic_symbol_stats() -> (usize, usize) {
    let interner = interner().read().expect("symbol interner poisoned");
    let count = interner.dynamic.len();
    let bytes = interner.dynamic.iter().map(|text| text.len()).sum::<usize>()
        + interner.dynamic.capacity() * std::mem::size_of::<&'static str>();
    (count, bytes)
}

fn resolve_preloaded(symbol: Symbol) -> Option<&'static str> {
    let (start, len) = *PRELOADED_SYMBOL_RANGES.get(symbol.raw() as usize)?;
    let start = start as usize;
    let end = start.checked_add(len as usize)?;
    let bytes = PRELOADED_SYMBOL_TEXT.0.get(start..end)?;
    debug_assert!(bytes.is_ascii());
    str::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::Name;
    use crate::modules::api_spec;
    use crate::sema::records::record_schemas;
    use crate::sema::types::Type;
    use xsh_registry::types::BUILTIN_TYPE_NAMES;
    use xsh_registry::{CORE_BUILTIN_SYMBOLS, FIXED_SEMANTIC_SYMBOLS};

    #[test]
    fn builtin_symbols_are_stable() {
        assert_eq!(Name::INT, Name::intern("Int"));
        assert_eq!(Name::INT.as_str(), "Int");
        assert!(Name::INT.is_builtin());
        assert!(Name::INT.is_preloaded());
        assert_eq!(
            CORE_BUILTIN_SYMBOLS[Name::INT.symbol().raw() as usize],
            "Int"
        );
        assert_eq!(
            Name::RESULT.symbol().raw() as usize,
            CORE_BUILTIN_SYMBOLS.len() - 1
        );
    }

    #[test]
    fn dynamic_symbols_round_trip() {
        let first = Name::intern("demo_dynamic_name");
        let second = Name::intern("demo_dynamic_name");
        let other = Name::intern("other_dynamic_name");
        assert_eq!(first, second);
        assert_ne!(first, other);
        assert_eq!(first.as_str(), "demo_dynamic_name");
        assert!(!first.is_preloaded());
    }

    #[test]
    fn oversized_dynamic_symbols_round_trip() {
        let text = "x".repeat(20 * 1024);
        let name = Name::intern(&text);
        assert_eq!(name.as_str(), text);
    }

    #[test]
    fn preloaded_symbols_cover_standard_api_registry() {
        let spec = api_spec();
        for (module_name, module) in spec.module_entries() {
            assert_preloaded(module_name);
            for function in &module.functions {
                assert_preloaded(function.name);
                for overload in &function.overloads {
                    for param in &overload.params {
                        assert_preloaded(param.name);
                    }
                }
            }
        }
        for (_receiver, methods) in spec.method_entries() {
            for method in methods {
                assert_preloaded(method.name);
                for overload in &method.overloads {
                    for param in &overload.sig.params {
                        assert_preloaded(param.name);
                    }
                }
            }
        }
    }

    #[test]
    fn preloaded_symbols_cover_standard_record_registry() {
        for (schema, ty) in record_schemas() {
            assert_preloaded(schema);
            assert_record_type_fields_preloaded(&ty);
        }
    }

    #[test]
    fn preloaded_symbols_cover_builtin_checker_names() {
        for name in FIXED_SEMANTIC_SYMBOLS {
            assert_preloaded(name);
        }
        for name in BUILTIN_TYPE_NAMES {
            assert_preloaded(name.as_str());
        }
        for family in xsh_registry::errors::builtin_error_families() {
            assert_preloaded(family.name);
            for field in family.fields {
                assert_preloaded(field.name);
            }
            for variant in family.variants {
                assert_preloaded(variant.name);
                for facet in variant.facets {
                    assert_preloaded(facet);
                }
            }
        }
    }

    fn assert_preloaded(name: &str) {
        let interned = Name::intern(name);
        assert!(
            interned.is_preloaded(),
            "`{name}` was not in the generated preloaded symbol table"
        );
    }

    fn assert_record_type_fields_preloaded(ty: &Type) {
        match ty {
            Type::Record(fields) => {
                for (name, ty) in fields {
                    assert!(
                        name.is_preloaded(),
                        "`{name}` was not in the generated preloaded symbol table"
                    );
                    assert_record_type_fields_preloaded(ty);
                }
            }
            Type::List(inner) | Type::Map(inner) | Type::Stream(inner) | Type::Optional(inner) => {
                assert_record_type_fields_preloaded(inner)
            }
            Type::Result(ok, err) => {
                assert_record_type_fields_preloaded(ok);
                assert_record_type_fields_preloaded(err);
            }
            _ => {}
        }
    }
}
