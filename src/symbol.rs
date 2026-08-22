use rustc_hash::FxHashMap;
use std::borrow::Borrow;
use std::cell::{Cell, RefCell};
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

/// The spelling of a [`Name`].
///
/// Preloaded spellings borrow generated static storage. Dynamic spellings keep
/// their session-owned allocation alive for the duration of the returned value,
/// rather than exposing an unsound `&'static str` from the process interner.
#[derive(Clone, Debug)]
pub enum NameText {
    Preloaded(&'static str),
    Dynamic(std::sync::Arc<str>),
}

impl NameText {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Preloaded(text) => text,
            Self::Dynamic(text) => text,
        }
    }

    pub(crate) fn into_arc(self) -> std::sync::Arc<str> {
        match self {
            Self::Preloaded(text) => std::sync::Arc::from(text),
            Self::Dynamic(text) => text,
        }
    }
}

impl PartialEq for NameText {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for NameText {}

impl Hash for NameText {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl AsRef<str> for NameText {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<std::path::Path> for NameText {
    fn as_ref(&self) -> &std::path::Path {
        self.as_str().as_ref()
    }
}

impl Borrow<str> for NameText {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Deref for NameText {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Display for NameText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for NameText {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for NameText {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<NameText> for str {
    fn eq(&self, other: &NameText) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<NameText> for &str {
    fn eq(&self, other: &NameText) -> bool {
        *self == other.as_str()
    }
}

impl Ord for NameText {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialOrd for NameText {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Owns dynamic symbols created while a source program or interactive session
/// is live. Dropping the last clone releases their spellings from the process
/// interner and makes their compact IDs available for a later session.
#[derive(Clone, Debug)]
pub struct SymbolOwner(std::sync::Arc<SymbolOwnerData>);

#[derive(Default)]
struct SymbolOwnerData {
    symbols: RwLock<FxHashMap<Symbol, ()>>,
}

impl fmt::Debug for SymbolOwnerData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SymbolOwnerData").finish_non_exhaustive()
    }
}

impl SymbolOwner {
    pub fn new() -> Self {
        Self(std::sync::Arc::new(SymbolOwnerData::default()))
    }

    pub fn intern(&self, text: &str) -> Name {
        let mut symbols = self.0.symbols.write().expect("symbol owner poisoned");
        let mut interner = interner().write().expect("symbol interner poisoned");
        let symbol = interner.intern(text);
        if !symbol_is_preloaded(symbol) && symbols.insert(symbol, ()).is_none() {
            interner.retain(symbol, self.identity());
        }
        Name(symbol)
    }

    fn identity(&self) -> usize {
        std::sync::Arc::as_ptr(&self.0) as usize
    }

    fn intern_existing(&self, text: &str, symbol: Symbol) -> Name {
        if self
            .0
            .symbols
            .read()
            .expect("symbol owner poisoned")
            .contains_key(&symbol)
        {
            return Name(symbol);
        }
        self.intern(text)
    }

    pub fn with_current<R>(&self, work: impl FnOnce() -> R) -> R {
        let _guard = self.enter();
        work()
    }

    pub fn enter(&self) -> SymbolOwnerGuard {
        let previous = ACTIVE_SYMBOL_OWNER.with(|owner| owner.borrow_mut().replace(self.clone()));
        let previous_identity = ACTIVE_SYMBOL_OWNER_ID.with(|owner| owner.replace(self.identity()));
        SymbolOwnerGuard {
            previous,
            previous_identity,
        }
    }

    pub fn current() -> Option<Self> {
        current_symbol_owner()
    }

    pub fn dynamic_stats(&self) -> (usize, usize) {
        let symbols = self.0.symbols.read().expect("symbol owner poisoned");
        let interner = interner().read().expect("symbol interner poisoned");
        let bytes = symbols
            .keys()
            .filter_map(|symbol| interner.dynamic_text_len(*symbol))
            .sum();
        (symbols.len(), bytes)
    }
}

impl Default for SymbolOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for SymbolOwner {
    fn eq(&self, _other: &Self) -> bool {
        // Owners are lifetime bookkeeping, not program semantics.
        true
    }
}

impl Eq for SymbolOwner {}

impl Drop for SymbolOwnerData {
    fn drop(&mut self) {
        let symbols = self.symbols.get_mut().expect("symbol owner poisoned");
        let mut interner = interner().write().expect("symbol interner poisoned");
        for symbol in symbols.keys().copied() {
            interner.release(symbol);
        }
    }
}

thread_local! {
    static ACTIVE_SYMBOL_OWNER: RefCell<Option<SymbolOwner>> = const { RefCell::new(None) };
    static ACTIVE_SYMBOL_OWNER_ID: Cell<usize> = const { Cell::new(0) };
}

pub struct SymbolOwnerGuard {
    previous: Option<SymbolOwner>,
    previous_identity: usize,
}

impl Drop for SymbolOwnerGuard {
    fn drop(&mut self) {
        ACTIVE_SYMBOL_OWNER.with(|owner| {
            *owner.borrow_mut() = self.previous.take();
        });
        ACTIVE_SYMBOL_OWNER_ID.with(|owner| owner.set(self.previous_identity));
    }
}

fn current_symbol_owner() -> Option<SymbolOwner> {
    ACTIVE_SYMBOL_OWNER.with(|owner| owner.borrow().clone())
}

fn with_current_symbol_owner<R>(work: impl FnOnce(Option<&SymbolOwner>) -> R) -> R {
    ACTIVE_SYMBOL_OWNER.with(|owner| {
        let owner = owner.borrow();
        work(owner.as_ref())
    })
}

fn current_symbol_owner_identity() -> usize {
    ACTIVE_SYMBOL_OWNER_ID.with(Cell::get)
}

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
    pub const NET_JOB: Self = Self(Symbol::from_raw(25));
    pub const RESULT: Self = Self(Symbol::from_raw(26));

    pub fn intern(text: impl AsRef<str>) -> Self {
        let text = text.as_ref();
        let existing = {
            let interner = interner().read().expect("symbol interner poisoned");
            interner
                .get(text)
                .map(|symbol| (symbol, interner.sole_owner(symbol)))
        };
        if let Some((symbol, sole_owner)) = existing {
            if !symbol_is_preloaded(symbol) {
                let current_owner = current_symbol_owner_identity();
                if current_owner == 0 {
                    panic!("dynamic symbol `{text}` was interned without a symbol owner");
                }
                if sole_owner == Some(current_owner) {
                    return Self(symbol);
                }
                return with_current_symbol_owner(|owner| {
                    let owner = owner.unwrap_or_else(|| {
                        panic!("dynamic symbol `{text}` was interned without a symbol owner")
                    });
                    owner.intern_existing(text, symbol)
                });
            }
            return Self(symbol);
        }
        with_current_symbol_owner(|owner| {
            owner
                .unwrap_or_else(|| {
                    panic!("dynamic symbol `{text}` was interned without a symbol owner")
                })
                .intern(text)
        })
    }

    pub const fn from_symbol(symbol: Symbol) -> Self {
        Self(symbol)
    }

    pub const fn symbol(self) -> Symbol {
        self.0
    }

    pub fn as_str(self) -> NameText {
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
        self.as_str().as_str() == other
    }
}

impl PartialEq<&str> for Name {
    fn eq(&self, other: &&str) -> bool {
        self.as_str().as_str() == *other
    }
}

impl PartialEq<String> for Name {
    fn eq(&self, other: &String) -> bool {
        self.as_str().as_str() == other.as_str()
    }
}

impl PartialEq<&String> for Name {
    fn eq(&self, other: &&String) -> bool {
        self.as_str().as_str() == other.as_str()
    }
}

impl PartialEq<Name> for String {
    fn eq(&self, other: &Name) -> bool {
        self.as_str() == other.as_str().as_str()
    }
}

impl PartialEq<Name> for &String {
    fn eq(&self, other: &Name) -> bool {
        self.as_str() == other.as_str().as_str()
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
        if self == other {
            return Ordering::Equal;
        }
        let interner = interner().read().expect("symbol interner poisoned");
        interner
            .resolve_ref(self.0)
            .cmp(interner.resolve_ref(other.0))
    }
}

impl PartialOrd for Name {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str().as_str())
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
    by_text: FxHashMap<NameText, Symbol>,
    preloaded_by_text_capacity: usize,
    dynamic: Vec<Option<DynamicSymbol>>,
    free_dynamic: Vec<u32>,
}

struct DynamicSymbol {
    text: std::sync::Arc<str>,
    owners: usize,
    sole_owner: Option<usize>,
}

impl Interner {
    fn with_preloaded() -> Self {
        let mut interner = Self {
            by_text: FxHashMap::with_capacity_and_hasher(
                PRELOADED_SYMBOL_COUNT as usize,
                Default::default(),
            ),
            preloaded_by_text_capacity: 0,
            dynamic: Vec::new(),
            free_dynamic: Vec::new(),
        };
        for index in 0..PRELOADED_SYMBOL_COUNT {
            let symbol = Symbol::from_raw(index);
            let name = resolve_preloaded(symbol).expect("preloaded symbol exists");
            interner.by_text.insert(NameText::Preloaded(name), symbol);
        }
        interner.preloaded_by_text_capacity = interner.by_text.capacity();
        interner
    }

    fn get(&self, text: &str) -> Option<Symbol> {
        self.by_text.get(text).copied()
    }

    fn intern(&mut self, text: &str) -> Symbol {
        if let Some(symbol) = self.get(text) {
            return symbol;
        }
        let text: std::sync::Arc<str> = text.into();
        let index = self.free_dynamic.pop().unwrap_or_else(|| {
            u32::try_from(self.dynamic.len()).expect("too many dynamic symbols")
        });
        let symbol = Symbol::from_raw(
            PRELOADED_SYMBOL_COUNT
                .checked_add(index)
                .expect("too many symbols"),
        );
        let entry = DynamicSymbol {
            text: text.clone(),
            owners: 0,
            sole_owner: None,
        };
        if let Some(slot) = self.dynamic.get_mut(index as usize) {
            debug_assert!(slot.is_none(), "reused dynamic symbol slot was still live");
            *slot = Some(entry);
        } else {
            debug_assert_eq!(index as usize, self.dynamic.len());
            self.dynamic.push(Some(entry));
        }
        self.by_text.insert(NameText::Dynamic(text), symbol);
        symbol
    }

    fn retain(&mut self, symbol: Symbol, owner: usize) {
        let Some(entry) = self.dynamic_entry_mut(symbol) else {
            return;
        };
        if entry.owners == 0 {
            entry.sole_owner = Some(owner);
        } else if entry.sole_owner != Some(owner) {
            entry.sole_owner = None;
        }
        entry.owners = entry
            .owners
            .checked_add(1)
            .expect("too many owners for dynamic symbol");
    }

    fn release(&mut self, symbol: Symbol) {
        let Some(index) = dynamic_index(symbol) else {
            return;
        };
        let Some(entry) = self.dynamic.get_mut(index) else {
            return;
        };
        let Some(entry) = entry.as_mut() else {
            return;
        };
        entry.owners = entry
            .owners
            .checked_sub(1)
            .expect("dynamic symbol owner count underflow");
        if entry.owners != 0 {
            return;
        }
        let text = entry.text.clone();
        self.dynamic[index] = None;
        self.by_text.remove(text.as_ref());
        self.free_dynamic
            .push(u32::try_from(index).expect("dynamic symbol index exceeded u32"));
    }

    fn dynamic_entry_mut(&mut self, symbol: Symbol) -> Option<&mut DynamicSymbol> {
        self.dynamic.get_mut(dynamic_index(symbol)?)?.as_mut()
    }

    fn dynamic_text_len(&self, symbol: Symbol) -> Option<usize> {
        self.dynamic
            .get(dynamic_index(symbol)?)?
            .as_ref()
            .map(|entry| entry.text.len())
    }

    fn sole_owner(&self, symbol: Symbol) -> Option<usize> {
        self.dynamic
            .get(dynamic_index(symbol)?)?
            .as_ref()?
            .sole_owner
    }

    fn resolve(&self, symbol: Symbol) -> NameText {
        let raw = symbol.raw();
        if raw < PRELOADED_SYMBOL_COUNT {
            return NameText::Preloaded(resolve_preloaded(symbol).unwrap_or("<invalid-symbol>"));
        }
        self.dynamic
            .get(dynamic_index(symbol).unwrap_or_default())
            .and_then(|entry| entry.as_ref())
            .map(|entry| NameText::Dynamic(entry.text.clone()))
            .unwrap_or(NameText::Preloaded("<invalid-symbol>"))
    }

    fn resolve_ref(&self, symbol: Symbol) -> &str {
        let raw = symbol.raw();
        if raw < PRELOADED_SYMBOL_COUNT {
            return resolve_preloaded(symbol).unwrap_or("<invalid-symbol>");
        }
        self.dynamic
            .get(dynamic_index(symbol).unwrap_or_default())
            .and_then(|entry| entry.as_ref())
            .map(|entry| entry.text.as_ref())
            .unwrap_or("<invalid-symbol>")
    }
}

fn symbol_is_preloaded(symbol: Symbol) -> bool {
    symbol.raw() < PRELOADED_SYMBOL_COUNT
}

fn dynamic_index(symbol: Symbol) -> Option<usize> {
    symbol
        .raw()
        .checked_sub(PRELOADED_SYMBOL_COUNT)
        .map(|index| index as usize)
}

fn interner() -> &'static RwLock<Interner> {
    static INTERNER: OnceLock<RwLock<Interner>> = OnceLock::new();
    INTERNER.get_or_init(|| RwLock::new(Interner::with_preloaded()))
}

/// Count and retained backing bytes for live dynamic symbols.
pub fn dynamic_symbol_stats() -> (usize, usize) {
    let interner = interner().read().expect("symbol interner poisoned");
    let count = interner.dynamic.iter().flatten().count();
    let bytes = interner
        .dynamic
        .iter()
        .flatten()
        .map(|entry| entry.text.len())
        .sum::<usize>()
        + interner.dynamic.capacity() * std::mem::size_of::<Option<DynamicSymbol>>()
        + interner
            .by_text
            .capacity()
            .saturating_sub(interner.preloaded_by_text_capacity)
            * std::mem::size_of::<(NameText, Symbol)>()
        + interner.free_dynamic.capacity() * std::mem::size_of::<u32>();
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
    use super::{Interner, Name, SymbolOwner, interner};
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
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let first = Name::intern("demo_dynamic_name");
            let second = Name::intern("demo_dynamic_name");
            let other = Name::intern("other_dynamic_name");
            assert_eq!(first, second);
            assert_ne!(first, other);
            assert_eq!(first.as_str(), "demo_dynamic_name");
            assert!(!first.is_preloaded());
        });
    }

    #[test]
    fn oversized_dynamic_symbols_round_trip() {
        let text = "x".repeat(20 * 1024);
        let owner = SymbolOwner::new();
        owner.with_current(|| {
            let name = Name::intern(&text);
            assert_eq!(name.as_str(), text.as_str());
        });
    }

    #[test]
    fn nested_symbol_owner_scope_restores_parent() {
        let outer = SymbolOwner::new();
        let inner = SymbolOwner::new();
        outer.with_current(|| {
            Name::intern("symbol_owner_outer_before");
            inner.with_current(|| {
                Name::intern("symbol_owner_inner");
            });
            Name::intern("symbol_owner_outer_after");
        });
        assert_eq!(outer.dynamic_stats().0, 2);
        assert_eq!(inner.dynamic_stats().0, 1);
    }

    #[test]
    fn dynamic_symbols_return_to_a_stable_plateau_after_owner_drop() {
        let mut local = Interner::with_preloaded();
        for index in 0..8 {
            let text = format!("session_symbol_{index}");
            let symbol = local.intern(&text);
            local.retain(symbol, 0);
            local.release(symbol);
            assert!(local.get(&text).is_none());
            assert_eq!(local.dynamic.len(), 1);
            assert_eq!(local.free_dynamic, vec![0]);
        }

        let text = "phase_eight_owner_drop_probe";
        {
            let owner = SymbolOwner::new();
            owner.with_current(|| {
                let _ = Name::intern(text);
            });
        }
        assert!(
            interner()
                .read()
                .expect("symbol interner poisoned")
                .get(text)
                .is_none()
        );
    }

    #[test]
    fn concurrent_owners_retain_dynamic_symbols_before_their_slots_can_be_reused() {
        const WORKERS: usize = 8;
        const ROUNDS: usize = 32;
        let text = "phase_eight_concurrent_owner_probe";

        for _ in 0..ROUNDS {
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(WORKERS));
            std::thread::scope(|scope| {
                for _ in 0..WORKERS {
                    let barrier = std::sync::Arc::clone(&barrier);
                    scope.spawn(move || {
                        let owner = SymbolOwner::new();
                        barrier.wait();
                        owner.with_current(|| {
                            let name = Name::intern(text);
                            assert_eq!(name.as_str(), text);
                        });
                    });
                }
            });
            assert!(
                interner()
                    .read()
                    .expect("symbol interner poisoned")
                    .get(text)
                    .is_none()
            );
        }
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
