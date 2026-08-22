#![allow(clippy::single_call_fn)]

use crate::runtime::process::ProcessStatus;
use crate::source::Span;
use crate::symbol::{Name, NameText, QualifiedName, SymbolOwner};
use rustc_hash::FxHashMap;
use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordShape {
    data: Arc<RecordShapeData>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct RecordShapeData {
    names: Box<[Name]>,
    texts: Box<[NameText]>,
    symbols: Option<SymbolOwner>,
}

#[derive(Default)]
struct RuntimeRecordShapes {
    preloaded: FxHashMap<Box<[Name]>, Arc<RecordShapeData>>,
    dynamic: FxHashMap<Box<[Name]>, Weak<RecordShapeData>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeShapeStats {
    pub hits: usize,
    pub misses: usize,
    pub live_shapes: usize,
}

static RUNTIME_SHAPE_HITS: AtomicUsize = AtomicUsize::new(0);
static RUNTIME_SHAPE_MISSES: AtomicUsize = AtomicUsize::new(0);
const MAX_DYNAMIC_RECORD_SHAPES: usize = 256;

impl RecordShape {
    pub fn new(fields: Vec<Arc<str>>) -> Self {
        let names = fields
            .iter()
            .map(|field| Name::intern(field.as_ref()))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            data: intern_record_shape(names),
        }
    }

    fn index_of(&self, key: &str) -> Option<usize> {
        self.data
            .texts
            .iter()
            .position(|field| field.as_str() == key)
    }
}

fn runtime_record_shapes() -> &'static RwLock<RuntimeRecordShapes> {
    static SHAPES: OnceLock<RwLock<RuntimeRecordShapes>> = OnceLock::new();
    SHAPES.get_or_init(|| RwLock::new(RuntimeRecordShapes::default()))
}

fn make_record_shape(fields: Box<[Name]>, symbols: Option<SymbolOwner>) -> Arc<RecordShapeData> {
    let texts = fields.iter().map(|name| name.as_str()).collect();
    Arc::new(RecordShapeData {
        names: fields,
        texts,
        symbols,
    })
}

#[inline]
fn intern_record_shape(fields: Box<[Name]>) -> Arc<RecordShapeData> {
    if fields.iter().all(|name| name.is_preloaded()) {
        return intern_preloaded_record_shape(fields);
    }
    intern_dynamic_record_shape(fields)
}

#[inline]
fn intern_preloaded_record_shape(fields: Box<[Name]>) -> Arc<RecordShapeData> {
    let shapes = runtime_record_shapes();
    if let Some(shape) = shapes
        .read()
        .expect("runtime record shape interner poisoned")
        .preloaded
        .get(fields.as_ref())
    {
        RUNTIME_SHAPE_HITS.fetch_add(1, Ordering::Relaxed);
        return Arc::clone(shape);
    }

    let mut shapes = shapes
        .write()
        .expect("runtime record shape interner poisoned");
    if let Some(shape) = shapes.preloaded.get(fields.as_ref()) {
        RUNTIME_SHAPE_HITS.fetch_add(1, Ordering::Relaxed);
        return Arc::clone(shape);
    }
    RUNTIME_SHAPE_MISSES.fetch_add(1, Ordering::Relaxed);
    let shape = make_record_shape(fields.clone(), None);
    shapes.preloaded.insert(fields, Arc::clone(&shape));
    shape
}

fn intern_dynamic_record_shape(fields: Box<[Name]>) -> Arc<RecordShapeData> {
    let shapes = runtime_record_shapes();
    if let Some(shape) = shapes
        .read()
        .expect("runtime record shape interner poisoned")
        .dynamic
        .get(fields.as_ref())
        .and_then(Weak::upgrade)
    {
        RUNTIME_SHAPE_HITS.fetch_add(1, Ordering::Relaxed);
        return shape;
    }
    let mut shapes = shapes
        .write()
        .expect("runtime record shape interner poisoned");
    if let Some(shape) = shapes.dynamic.get(fields.as_ref()).and_then(Weak::upgrade) {
        RUNTIME_SHAPE_HITS.fetch_add(1, Ordering::Relaxed);
        return shape;
    }
    RUNTIME_SHAPE_MISSES.fetch_add(1, Ordering::Relaxed);
    shapes.dynamic.remove(fields.as_ref());
    if shapes.dynamic.len() >= MAX_DYNAMIC_RECORD_SHAPES {
        shapes.dynamic.retain(|_, shape| shape.strong_count() != 0);
    }
    let symbols =
        SymbolOwner::current().expect("dynamic record shape requires an active symbol owner");
    let shape = make_record_shape(fields.clone(), Some(symbols));
    if shapes.dynamic.len() < MAX_DYNAMIC_RECORD_SHAPES {
        shapes.dynamic.insert(fields, Arc::downgrade(&shape));
    }
    shape
}

pub fn runtime_shape_stats() -> RuntimeShapeStats {
    let live_shapes = {
        let shapes = runtime_record_shapes()
            .read()
            .expect("runtime record shape interner poisoned");
        shapes.preloaded.len()
            + shapes
                .dynamic
                .values()
                .filter(|shape| shape.strong_count() != 0)
                .count()
    };
    RuntimeShapeStats {
        hits: RUNTIME_SHAPE_HITS.load(Ordering::Relaxed),
        misses: RUNTIME_SHAPE_MISSES.load(Ordering::Relaxed),
        live_shapes,
    }
}

#[derive(Clone, Debug)]
pub enum RecordMap {
    Dynamic(BTreeMap<Arc<str>, Value>),
    Shaped {
        shape: Arc<RecordShapeData>,
        // `Arc` so cloning a shaped record (e.g. binding a stream item to a
        // block param and to `.`) is a refcount bump rather than copying the
        // whole value vector. `get_mut`/`insert` copy-on-write via `Arc::make_mut`.
        values: Arc<[Value]>,
    },
    SparseShaped(Arc<SparseRecordMap>),
}

impl PartialEq for RecordMap {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .all(|(key, value)| other.get(key).is_some_and(|other| other == value))
    }
}

impl Eq for RecordMap {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparseRecordMap {
    shape: Arc<RecordShapeData>,
    defaults: &'static [Value],
    overrides: Arc<[(usize, Value)]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsEntryKind {
    Dir,
    File,
    Symlink,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FsEntryValue {
    path: Arc<PathBuf>,
    kind: FsEntryKind,
}

impl FsEntryValue {
    pub fn new(path: PathBuf, file_type: std::fs::FileType) -> Self {
        let kind = if file_type.is_dir() {
            FsEntryKind::Dir
        } else if file_type.is_file() {
            FsEntryKind::File
        } else if file_type.is_symlink() {
            FsEntryKind::Symlink
        } else {
            FsEntryKind::Other
        };
        Self {
            path: Arc::new(path),
            kind,
        }
    }

    pub fn field_value(&self, name: &str) -> Option<Result<Value, RuntimeError>> {
        let value = match name {
            "accessed" | "blocks_512" | "gid" | "mode" | "modified" | "size" | "uid" => {
                return Some(Err(RuntimeError::new(
                    "metadata-unavailable",
                    format!(
                        "filesystem entry field `{name}` requires stat=true; this entry was created with stat=false"
                    ),
                )));
            }
            "path" => PathValue::new(self.path.as_os_str().as_bytes().to_vec()).map(Value::Path),
            "name" => Ok(Value::Str(
                self.path
                    .file_name()
                    .map(|name| Arc::<str>::from(name.to_string_lossy().as_ref()))
                    .unwrap_or_else(|| "".into()),
            )),
            "ext" => Ok(Value::Str(
                self.path
                    .extension()
                    .map(|name| Arc::<str>::from(name.to_string_lossy().as_ref()))
                    .unwrap_or_else(|| "".into()),
            )),
            "kind" => Ok(Value::Str(Arc::from(self.kind.as_str()))),
            "executable" | "group_executable" | "other_executable" | "owner_executable"
            | "setgid" | "setuid" | "sticky" | "world_writable" => {
                return Some(Err(RuntimeError::new(
                    "metadata-unavailable",
                    format!(
                        "filesystem entry field `{name}` requires stat=true; this entry was created with stat=false"
                    ),
                )));
            }
            _ => return None,
        };
        Some(value)
    }

    pub fn to_record_map(&self) -> Result<RecordMap, RuntimeError> {
        let mut fields = Vec::new();
        for name in [
            "accessed",
            "blocks_512",
            "executable",
            "ext",
            "gid",
            "group_executable",
            "kind",
            "mode",
            "modified",
            "name",
            "other_executable",
            "owner_executable",
            "path",
            "setgid",
            "setuid",
            "size",
            "sticky",
            "uid",
            "world_writable",
        ] {
            let value = self
                .field_value(name)
                .expect("fs entry field list is complete")?;
            fields.push((Name::intern(name), value));
        }
        Ok(RecordMap::from_name_values(fields))
    }
}

impl FsEntryKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Dir => "dir",
            Self::File => "file",
            Self::Symlink => "symlink",
            Self::Other => "other",
        }
    }
}

impl Default for RecordMap {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordMap {
    pub fn new() -> Self {
        Self::Dynamic(BTreeMap::new())
    }

    pub fn shaped(shape: &RecordShape, values: Vec<Value>) -> Self {
        assert_eq!(shape.data.names.len(), values.len());
        Self::Shaped {
            shape: Arc::clone(&shape.data),
            values: Arc::from(values),
        }
    }

    #[inline]
    pub fn from_name_values(mut fields: Vec<(Name, Value)>) -> Self {
        fields.sort_by_key(|(name, _)| *name);
        let mut names = Vec::with_capacity(fields.len());
        let mut values = Vec::with_capacity(fields.len());
        for (name, value) in fields {
            if names.last() == Some(&name) {
                *values
                    .last_mut()
                    .expect("record value exists for duplicate name") = value;
            } else {
                names.push(name);
                values.push(value);
            }
        }
        Self::Shaped {
            shape: intern_record_shape(names.into_boxed_slice()),
            values: Arc::from(values),
        }
    }

    pub fn sparse_shaped(
        shape: &RecordShape,
        defaults: &'static [Value],
        overrides: Vec<(&str, Value)>,
    ) -> Self {
        let overrides = overrides
            .into_iter()
            .map(|(key, value)| {
                let index = shape
                    .index_of(key)
                    .unwrap_or_else(|| panic!("unknown sparse record field `{key}`"));
                (index, value)
            })
            .collect();
        Self::sparse_shaped_indices(shape, defaults, overrides)
    }

    pub fn sparse_shaped_indices(
        shape: &RecordShape,
        defaults: &'static [Value],
        overrides: Vec<(usize, Value)>,
    ) -> Self {
        let overrides = Arc::from(overrides.into_boxed_slice());
        Self::sparse_shaped_arc(shape, defaults, overrides)
    }

    pub fn sparse_shaped_array<const N: usize>(
        shape: &RecordShape,
        defaults: &'static [Value],
        overrides: [(usize, Value); N],
    ) -> Self {
        let overrides: Arc<[(usize, Value)]> = Arc::new(overrides);
        Self::sparse_shaped_arc(shape, defaults, overrides)
    }

    fn sparse_shaped_arc(
        shape: &RecordShape,
        defaults: &'static [Value],
        overrides: Arc<[(usize, Value)]>,
    ) -> Self {
        assert_eq!(shape.data.names.len(), defaults.len());
        debug_assert!(overrides.iter().all(|(index, _)| *index < defaults.len()));
        Self::SparseShaped(Arc::new(SparseRecordMap {
            shape: Arc::clone(&shape.data),
            defaults,
            overrides,
        }))
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Dynamic(fields) => fields.get(key),
            Self::Shaped { shape, values, .. } => shape
                .texts
                .iter()
                .position(|field| field.as_str() == key)
                .map(|index| &values[index]),
            Self::SparseShaped(sparse) => sparse
                .shape
                .texts
                .iter()
                .position(|field| field.as_str() == key)
                .map(|index| {
                    sparse_shaped_value(sparse.defaults, sparse.overrides.as_ref(), index)
                }),
        }
    }

    pub fn get_name(&self, key: Name) -> Option<&Value> {
        match self {
            Self::Dynamic(fields) => {
                let key_text = key.as_str();
                fields.get::<str>(key_text.as_str())
            }
            Self::Shaped { shape, values, .. } => shape
                .names
                .iter()
                .position(|field| *field == key)
                .map(|index| &values[index]),
            Self::SparseShaped(sparse) => sparse
                .shape
                .names
                .iter()
                .position(|field| *field == key)
                .map(|index| {
                    sparse_shaped_value(sparse.defaults, sparse.overrides.as_ref(), index)
                }),
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self {
            Self::Dynamic(fields) => fields.get_mut(key),
            Self::Shaped { shape, values, .. } => shape
                .texts
                .iter()
                .position(|field| field.as_str() == key)
                .map(|index| &mut Arc::make_mut(values)[index]),
            Self::SparseShaped(_) => self.ensure_dynamic().get_mut(key),
        }
    }

    pub fn insert(&mut self, key: Arc<str>, value: Value) -> Option<Value> {
        if let Self::Shaped { shape, values, .. } = self
            && let Some(index) = shape
                .texts
                .iter()
                .position(|field| field.as_str() == key.as_ref())
        {
            return Some(std::mem::replace(&mut Arc::make_mut(values)[index], value));
        }
        self.ensure_dynamic().insert(key, value)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Dynamic(fields) => fields.len(),
            Self::Shaped { values, .. } => values.len(),
            Self::SparseShaped(sparse) => sparse.shape.names.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn keys(&self) -> RecordKeys<'_> {
        match self {
            Self::Dynamic(fields) => RecordKeys::Dynamic(fields.keys()),
            Self::Shaped { shape, .. } => RecordKeys::Shaped(shape.texts.iter()),
            Self::SparseShaped(sparse) => RecordKeys::Shaped(sparse.shape.texts.iter()),
        }
    }

    pub fn values(&self) -> RecordValues<'_> {
        match self {
            Self::Dynamic(fields) => RecordValues::Dynamic(fields.values()),
            Self::Shaped { values, .. } => RecordValues::Shaped(values.iter()),
            Self::SparseShaped(sparse) => RecordValues::SparseShaped {
                len: sparse.shape.names.len(),
                defaults: sparse.defaults,
                overrides: sparse.overrides.as_ref(),
                index: 0,
            },
        }
    }

    pub fn iter(&self) -> RecordIter<'_> {
        match self {
            Self::Dynamic(fields) => RecordIter::Dynamic(fields.iter()),
            Self::Shaped { shape, values, .. } => RecordIter::Shaped {
                keys: shape.texts.iter(),
                values: values.iter(),
            },
            Self::SparseShaped(sparse) => RecordIter::SparseShaped {
                keys: sparse.shape.texts.iter(),
                defaults: sparse.defaults,
                overrides: sparse.overrides.as_ref(),
                index: 0,
            },
        }
    }

    pub(crate) fn owned_key_iter(&self) -> RecordOwnedKeyIter<'_> {
        match self {
            Self::Dynamic(fields) => RecordOwnedKeyIter::Dynamic(fields.iter()),
            Self::Shaped { shape, values, .. } => RecordOwnedKeyIter::Shaped {
                keys: shape.texts.iter(),
                values: values.iter(),
            },
            Self::SparseShaped(sparse) => RecordOwnedKeyIter::SparseShaped {
                keys: sparse.shape.texts.iter(),
                defaults: sparse.defaults,
                overrides: sparse.overrides.as_ref(),
                index: 0,
            },
        }
    }

    fn ensure_dynamic(&mut self) -> &mut BTreeMap<Arc<str>, Value> {
        if matches!(self, Self::Shaped { .. } | Self::SparseShaped(_)) {
            let mut fields = BTreeMap::new();
            for (key, value) in self.iter() {
                fields.insert(Arc::from(key), value.clone());
            }
            *self = Self::Dynamic(fields);
        }
        let Self::Dynamic(fields) = self else {
            unreachable!("shaped record converted to dynamic")
        };
        fields
    }
}

impl<const N: usize> From<[(Arc<str>, Value); N]> for RecordMap {
    fn from(fields: [(Arc<str>, Value); N]) -> Self {
        Self::from_name_values(
            fields
                .into_iter()
                .map(|(key, value)| (Name::intern(key.as_ref()), value))
                .collect(),
        )
    }
}

impl From<BTreeMap<Arc<str>, Value>> for RecordMap {
    fn from(fields: BTreeMap<Arc<str>, Value>) -> Self {
        let mut names = Vec::with_capacity(fields.len());
        let mut values = Vec::with_capacity(fields.len());
        for (key, value) in fields {
            names.push(Name::intern(key.as_ref()));
            values.push(value);
        }
        Self::Shaped {
            shape: intern_record_shape(names.into_boxed_slice()),
            values: Arc::from(values),
        }
    }
}

impl FromIterator<(Arc<str>, Value)> for RecordMap {
    fn from_iter<T: IntoIterator<Item = (Arc<str>, Value)>>(iter: T) -> Self {
        Self::from_name_values(
            iter.into_iter()
                .map(|(key, value)| (Name::intern(key.as_ref()), value))
                .collect(),
        )
    }
}

impl IntoIterator for RecordMap {
    type Item = (Arc<str>, Value);
    type IntoIter = std::vec::IntoIter<(Arc<str>, Value)>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::Dynamic(fields) => fields.into_iter().collect::<Vec<_>>().into_iter(),
            Self::Shaped { shape, values, .. } => shape
                .texts
                .iter()
                .map(|key| Arc::from(key.as_str()))
                .zip(values.iter().cloned())
                .collect::<Vec<_>>()
                .into_iter(),
            Self::SparseShaped(sparse) => sparse
                .shape
                .texts
                .iter()
                .enumerate()
                .map(|(index, key)| {
                    (
                        Arc::from(key.as_str()),
                        sparse_shaped_value(sparse.defaults, sparse.overrides.as_ref(), index)
                            .clone(),
                    )
                })
                .collect::<Vec<_>>()
                .into_iter(),
        }
    }
}

impl<'a> IntoIterator for &'a RecordMap {
    type Item = (&'a str, &'a Value);
    type IntoIter = RecordIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub enum RecordIter<'a> {
    Dynamic(std::collections::btree_map::Iter<'a, Arc<str>, Value>),
    Shaped {
        keys: std::slice::Iter<'a, NameText>,
        values: std::slice::Iter<'a, Value>,
    },
    SparseShaped {
        keys: std::slice::Iter<'a, NameText>,
        defaults: &'static [Value],
        overrides: &'a [(usize, Value)],
        index: usize,
    },
}

pub(crate) enum RecordOwnedKeyIter<'a> {
    Dynamic(std::collections::btree_map::Iter<'a, Arc<str>, Value>),
    Shaped {
        keys: std::slice::Iter<'a, NameText>,
        values: std::slice::Iter<'a, Value>,
    },
    SparseShaped {
        keys: std::slice::Iter<'a, NameText>,
        defaults: &'static [Value],
        overrides: &'a [(usize, Value)],
        index: usize,
    },
}

impl<'a> Iterator for RecordOwnedKeyIter<'a> {
    type Item = (NameText, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Dynamic(iter) => iter
                .next()
                .map(|(key, value)| (NameText::Dynamic(Arc::clone(key)), value)),
            Self::Shaped { keys, values } => keys.next().cloned().zip(values.next()),
            Self::SparseShaped {
                keys,
                defaults,
                overrides,
                index,
            } => {
                let key = keys.next()?.clone();
                let value = sparse_shaped_value(defaults, overrides, *index);
                *index += 1;
                Some((key, value))
            }
        }
    }
}

impl<'a> Iterator for RecordIter<'a> {
    type Item = (&'a str, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Dynamic(iter) => iter.next().map(|(key, value)| (key.as_ref(), value)),
            Self::Shaped { keys, values } => keys
                .next()
                .zip(values.next())
                .map(|(key, value)| (key.as_str(), value)),
            Self::SparseShaped {
                keys,
                defaults,
                overrides,
                index,
            } => {
                let key = keys.next()?;
                let value = sparse_shaped_value(defaults, overrides, *index);
                *index += 1;
                Some((key.as_str(), value))
            }
        }
    }
}

pub enum RecordKeys<'a> {
    Dynamic(std::collections::btree_map::Keys<'a, Arc<str>, Value>),
    Shaped(std::slice::Iter<'a, NameText>),
}

impl<'a> Iterator for RecordKeys<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Dynamic(iter) => iter.next().map(AsRef::as_ref),
            Self::Shaped(iter) => iter.next().map(NameText::as_str),
        }
    }
}

pub enum RecordValues<'a> {
    Dynamic(std::collections::btree_map::Values<'a, Arc<str>, Value>),
    Shaped(std::slice::Iter<'a, Value>),
    SparseShaped {
        len: usize,
        defaults: &'static [Value],
        overrides: &'a [(usize, Value)],
        index: usize,
    },
}

impl<'a> Iterator for RecordValues<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Dynamic(iter) => iter.next(),
            Self::Shaped(iter) => iter.next(),
            Self::SparseShaped {
                len,
                defaults,
                overrides,
                index,
            } => {
                if *index >= *len {
                    return None;
                }
                let value = sparse_shaped_value(defaults, overrides, *index);
                *index += 1;
                Some(value)
            }
        }
    }
}

fn sparse_shaped_value<'a>(
    defaults: &'static [Value],
    overrides: &'a [(usize, Value)],
    index: usize,
) -> &'a Value {
    overrides
        .iter()
        .find_map(|(candidate, value)| (*candidate == index).then_some(value))
        .unwrap_or(&defaults[index])
}

const QUALIFIED_FUNCTION_TAG: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct FunctionName(u32);

impl FunctionName {
    pub fn name(name: Name) -> Self {
        let raw = name.symbol().raw();
        debug_assert_eq!(raw & (1 << 31), 0);
        Self(raw << 1)
    }

    pub fn qualified(name: QualifiedName) -> Self {
        let index = qualified_function_names()
            .write()
            .expect("qualified function interner poisoned")
            .intern(name);
        Self((index << 1) | QUALIFIED_FUNCTION_TAG)
    }

    pub fn as_name(self) -> Option<Name> {
        (self.0 & QUALIFIED_FUNCTION_TAG == 0)
            .then(|| Name::from_symbol(crate::symbol::Symbol::from_raw(self.0 >> 1)))
    }

    pub fn as_qualified(self) -> Option<QualifiedName> {
        if self.0 & QUALIFIED_FUNCTION_TAG == 0 {
            return None;
        }
        qualified_function_names()
            .read()
            .expect("qualified function interner poisoned")
            .resolve(self.0 >> 1)
    }

    pub fn display_name(&self) -> String {
        if let Some(name) = self.as_name() {
            name.to_string()
        } else if let Some(name) = self.as_qualified() {
            name.to_string()
        } else {
            "<unknown-function>".to_string()
        }
    }
}

impl From<Name> for FunctionName {
    fn from(value: Name) -> Self {
        Self::name(value)
    }
}

impl From<QualifiedName> for FunctionName {
    fn from(value: QualifiedName) -> Self {
        Self::qualified(value)
    }
}

struct QualifiedFunctionNames {
    by_name: FxHashMap<QualifiedName, u32>,
    names: Vec<QualifiedName>,
}

impl QualifiedFunctionNames {
    fn intern(&mut self, name: QualifiedName) -> u32 {
        if let Some(index) = self.by_name.get(&name) {
            return *index;
        }
        let index = self.names.len() as u32;
        debug_assert_eq!(index & (1 << 31), 0);
        self.names.push(name);
        self.by_name.insert(name, index);
        index
    }

    fn resolve(&self, index: u32) -> Option<QualifiedName> {
        self.names.get(index as usize).copied()
    }
}

fn qualified_function_names() -> &'static RwLock<QualifiedFunctionNames> {
    static NAMES: OnceLock<RwLock<QualifiedFunctionNames>> = OnceLock::new();
    NAMES.get_or_init(|| {
        RwLock::new(QualifiedFunctionNames {
            by_name: FxHashMap::default(),
            names: Vec::new(),
        })
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(FloatValue),
    Duration(DurationValue),
    Str(Arc<str>),
    Bytes(Vec<u8>),
    Digest(Box<DigestValue>),
    Regex(RegexValue),
    Path(PathValue),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
    Stream(Box<StreamValue>),
    Record(RecordMap),
    FsEntry(FsEntryValue),
    Module(RecordMap),
    Result(ResultValue),
    Status(ProcessStatus),
    EnvPathList,
    // Boxed: `RuntimeError` is large (~9 fields); inlining it made every `Value`
    // ~216 bytes, so every move/clone `memmove`d that much. Boxing keeps `Value`
    // small (errors are the cold path).
    Error(Box<RuntimeError>),
    RunError(Box<RunError>),
    Pure(FunctionName),
    Proc(FunctionName),
    Command(Box<CommandPlan>),
    ProcessHandle(Box<ProcessHandleValue>),
    NetJob(Box<NetJobValue>),
    Unit,
    Tag { name: Arc<str>, fields: Vec<Value> },
}

impl Value {
    pub fn digest(digest: DigestValue) -> Self {
        Self::Digest(Box::new(digest))
    }

    pub fn stream(stream: StreamValue) -> Self {
        Self::Stream(Box::new(stream))
    }

    pub fn ok(value: Value) -> Self {
        Self::Result(ResultValue::Ok(Box::new(value)))
    }

    pub fn err(error: Value) -> Self {
        Self::Result(ResultValue::Err(Box::new(error)))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "Null",
            Self::Bool(_) => "Bool",
            Self::Int(_) => "Int",
            Self::Float(_) => "Float",
            Self::Duration(_) => "Duration",
            Self::Str(_) => "Str",
            Self::Bytes(_) => "Bytes",
            Self::Digest(_) => "Digest",
            Self::Regex(_) => "Regex",
            Self::Path(_) => "Path",
            Self::List(_) => "List",
            Self::Map(_) => "Map",
            Self::Stream(_) => "Stream",
            Self::Record(_) | Self::FsEntry(_) => "Record",
            Self::Module(_) => "Module",
            Self::Result(_) => "Result",
            Self::Status(_) => "Status",
            Self::EnvPathList => "EnvPathList",
            Self::Error(_) => "Error",
            Self::RunError(_) => "RunError",
            Self::Pure(_) => "Pure",
            Self::Proc(_) => "Proc",
            Self::Command(_) => "Command",
            Self::ProcessHandle(_) => "ProcessHandle",
            Self::NetJob(_) => "NetJob",
            Self::Unit => "Unit",
            Self::Tag { .. } => "Tag",
        }
    }

    pub fn error_kind(&self) -> Option<&str> {
        match self {
            Self::Error(error) => Some(&error.kind),
            Self::RunError(error) => Some(&error.kind),
            _ => None,
        }
    }

    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Error(error) => Some(&error.message),
            Self::RunError(error) => Some(&error.message),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FloatValue(pub f64);

impl PartialEq for FloatValue {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for FloatValue {}

impl FloatValue {
    pub fn new(value: f64) -> Self {
        Self(value)
    }

    pub fn format(self) -> String {
        if self.0.is_nan() {
            "NaN".to_string()
        } else if self.0 == f64::INFINITY {
            "Infinity".to_string()
        } else if self.0 == f64::NEG_INFINITY {
            "-Infinity".to_string()
        } else {
            self.0.to_string()
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DigestValue {
    pub algorithm: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct RegexValue {
    pub pattern: String,
    pub regex: Arc<regex_lite::Regex>,
}

impl PartialEq for RegexValue {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
    }
}

impl Eq for RegexValue {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurationValue {
    pub millis: u64,
}

impl DurationValue {
    pub fn from_literal(literal: &str) -> Option<Self> {
        let (number, multiplier) = if let Some(number) = literal.strip_suffix("ms") {
            (number, 1)
        } else if let Some(number) = literal.strip_suffix('s') {
            (number, 1_000)
        } else if let Some(number) = literal.strip_suffix('m') {
            (number, 60_000)
        } else {
            let number = literal.strip_suffix('h')?;
            (number, 3_600_000)
        };
        let amount = number.parse::<u64>().ok()?;
        Some(Self {
            millis: amount.checked_mul(multiplier)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPlan {
    pub target: Vec<u8>,
    pub argv: Vec<Vec<u8>>,
    pub cwd: Option<PathValue>,
    pub env: BTreeMap<String, String>,
    pub redirections: Vec<CommandRedirection>,
    pub timeout: Option<DurationValue>,
    pub cpu_max: Option<i64>,
    pub detach: bool,
    pub new_session: bool,
    pub ignore_hup: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandRedirection {
    File {
        stream: CommandRedirectionStream,
        mode: CommandRedirectionMode,
        path: PathValue,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandRedirectionStream {
    Stdin,
    Stdout,
    Stderr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandRedirectionMode {
    Read,
    Write,
    Append,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessHandleValue {
    pub id: u64,
    pub pid: i64,
    pub command: Arc<str>,
    pub argv: Arc<[Arc<str>]>,
    pub detached: bool,
}

/// Opaque evaluator-owned network job identity.
///
/// The evaluator's live-job registry owns the transport task, completion
/// receiver, request metadata, and capacity reservation. Cloning the language
/// value only creates another alias to this ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetJobValue {
    pub id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamValue {
    pub items: Vec<StreamItem>,
    pub(crate) source: Option<StreamSource>,
}

impl StreamValue {
    pub fn from_items(items: Vec<StreamItem>) -> Self {
        Self {
            items,
            source: None,
        }
    }

    pub fn from_values(values: Vec<Value>) -> Self {
        Self {
            items: values
                .into_iter()
                .enumerate()
                .map(|(index, value)| StreamItem {
                    value,
                    index,
                    source_span: None,
                })
                .collect(),
            source: None,
        }
    }

    pub(crate) fn from_values_live(name: &'static str, values: Vec<Value>) -> Self {
        Self::from_live(
            name,
            ValuesStream {
                values: values.into_iter(),
            },
        )
    }

    pub(crate) fn from_live(name: &'static str, source: impl LiveStream + 'static) -> Self {
        Self {
            items: Vec::new(),
            source: Some(StreamSource::new(name, source)),
        }
    }

    pub(crate) fn next_live(&self, span: Span) -> Result<Option<Value>, RuntimeError> {
        match &self.source {
            Some(source) => source.next(span),
            None => Ok(None),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamItem {
    pub value: Value,
    pub index: usize,
    pub source_span: Option<Span>,
}

pub(crate) trait LiveStream: Send + Any {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError>;
}

struct ValuesStream {
    values: std::vec::IntoIter<Value>,
}

impl LiveStream for ValuesStream {
    fn next(&mut self, _span: Span) -> Result<Option<Value>, RuntimeError> {
        Ok(self.values.next())
    }
}

#[derive(Clone)]
pub(crate) struct StreamSource {
    name: &'static str,
    state: Arc<Mutex<Box<dyn LiveStream>>>,
}

impl StreamSource {
    fn new(name: &'static str, source: impl LiveStream + 'static) -> Self {
        Self {
            name,
            state: Arc::new(Mutex::new(Box::new(source))),
        }
    }

    fn next(&self, span: Span) -> Result<Option<Value>, RuntimeError> {
        let mut source = self.state.lock().map_err(|_| {
            RuntimeError::new(
                "stream-state",
                format!("{} stream state is poisoned", self.name),
            )
            .with_span(span)
        })?;
        source.next(span)
    }
}

impl fmt::Debug for StreamSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamSource")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl PartialEq for StreamSource {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && Arc::ptr_eq(&self.state, &other.state)
    }
}

impl Eq for StreamSource {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResultValue {
    Ok(Box<Value>),
    Err(Box<Value>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathValue {
    pub bytes: Vec<u8>,
}

impl PathValue {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, RuntimeError> {
        let bytes = bytes.into();
        if bytes.contains(&0) {
            return Err(RuntimeError::new(
                "nul-path",
                "paths cannot contain NUL bytes",
            ));
        }
        Ok(Self { bytes })
    }

    pub fn from_text(text: impl AsRef<str>) -> Result<Self, RuntimeError> {
        Self::new(text.as_ref().as_bytes().to_vec())
    }

    pub fn join_text(&self, rhs: &str) -> Result<Self, RuntimeError> {
        self.join_bytes(rhs.as_bytes())
    }

    pub fn join_path(&self, rhs: &Self) -> Result<Self, RuntimeError> {
        self.join_bytes(&rhs.bytes)
    }

    fn join_bytes(&self, rhs: &[u8]) -> Result<Self, RuntimeError> {
        if rhs.starts_with(b"/") {
            return Self::new(rhs.to_vec());
        }

        let mut bytes = self.bytes.clone();
        if !bytes.is_empty() && !bytes.ends_with(b"/") {
            bytes.push(b'/');
        }
        bytes.extend_from_slice(rhs);
        Self::new(bytes)
    }

    pub fn display(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeError {
    pub family: String,
    pub variant: String,
    pub kind: String,
    pub message: String,
    pub payload: RecordMap,
    pub facets: Vec<String>,
    pub span: Option<Span>,
    pub contexts: Vec<ErrorContext>,
    pub abort: Option<AbortSignal>,
    pub(crate) family_name: Name,
    pub(crate) variant_name: Name,
    pub(crate) _symbols: SymbolOwner,
}

impl RuntimeError {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        let kind = kind.into();
        let symbols = SymbolOwner::current().unwrap_or_default();
        let variant_name = symbols.intern(&kind);
        Self {
            family: "Error".to_string(),
            variant: kind.clone(),
            kind,
            message: message.into(),
            payload: RecordMap::new(),
            facets: Vec::new(),
            span: None,
            contexts: Vec::new(),
            abort: None,
            family_name: Name::ERROR,
            variant_name,
            _symbols: symbols,
        }
    }

    pub fn structured(
        family: impl Into<String>,
        variant: impl Into<String>,
        payload: RecordMap,
        facets: Vec<String>,
        message: impl Into<String>,
    ) -> Self {
        let family = family.into();
        let variant = variant.into();
        let symbols = SymbolOwner::current().unwrap_or_default();
        let family_name = symbols.intern(&family);
        let variant_name = symbols.intern(&variant);
        let kind = payload
            .get("kind")
            .and_then(|value| match value {
                Value::Str(kind) => Some(kind.to_string()),
                _ => None,
            })
            .unwrap_or_else(|| format!("{family}.{variant}"));
        Self {
            kind,
            family,
            variant,
            message: message.into(),
            payload,
            facets,
            span: None,
            contexts: Vec::new(),
            abort: None,
            family_name,
            variant_name,
            _symbols: symbols,
        }
    }

    pub fn abort(status: u8, force: bool) -> Self {
        let symbols = SymbolOwner::current().unwrap_or_default();
        Self {
            family: "Error".to_string(),
            variant: "Abort".to_string(),
            kind: "abort".to_string(),
            message: format!("script aborted with status {status}"),
            payload: RecordMap::new(),
            facets: Vec::new(),
            span: None,
            contexts: Vec::new(),
            abort: Some(AbortSignal { status, force }),
            family_name: Name::ERROR,
            variant_name: symbols.intern("Abort"),
            _symbols: symbols,
        }
    }

    pub fn family_name(&self) -> Name {
        self.family_name
    }

    pub fn variant_name(&self) -> Name {
        self.variant_name
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_context(mut self, context: ErrorContext) -> Self {
        self.contexts.push(context);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AbortSignal {
    pub status: u8,
    pub force: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorContext {
    pub kind: String,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunError {
    pub kind: String,
    pub message: String,
    pub span: Option<Span>,
    pub status: Option<Box<ProcessStatus>>,
    pub contexts: Vec<ErrorContext>,
}

impl RunError {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            span: None,
            status: None,
            contexts: Vec::new(),
        }
    }

    pub fn canceled(signal: i32, status: Option<ProcessStatus>) -> Self {
        Self {
            kind: "canceled".to_string(),
            message: format!("process work was canceled by signal {signal}"),
            span: None,
            status: status.map(Box::new),
            contexts: Vec::new(),
        }
    }

    pub fn from_status(status: ProcessStatus) -> Self {
        let (kind, message) = run_error_status_summary(&status);
        Self {
            kind,
            message,
            span: None,
            status: Some(Box::new(status)),
            contexts: Vec::new(),
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_status(mut self, status: ProcessStatus) -> Self {
        self.status = Some(Box::new(status));
        self
    }

    pub fn with_context(mut self, context: ErrorContext) -> Self {
        self.contexts.push(context);
        self
    }

    pub fn variant_name(&self) -> &'static str {
        match self.kind.as_str() {
            "not-found" => "NotFound",
            "permission-denied" => "PermissionDenied",
            "nonzero-exit" => "NonzeroExit",
            "signal" => "Signal",
            "timeout" => "Timeout",
            "canceled" => "Canceled",
            "capture-limit" => "CaptureLimit",
            "invalid-utf8" => "InvalidUtf8",
            "pipeline-failure" => "PipelineFailure",
            "exec-failure" | "exec-format" => "ExecFailure",
            "spawn" => "Spawn",
            "io" => "Io",
            "redirection" => "Redirection",
            "nul-target" => "InvalidTarget",
            _ => "Unknown",
        }
    }

    pub fn facets(&self) -> Vec<String> {
        match self.variant_name() {
            "NotFound" => vec!["NotFound".to_string()],
            "PermissionDenied" => vec!["PermissionDenied".to_string()],
            "NonzeroExit" => vec!["NonzeroExit".to_string()],
            "Signal" => vec!["Signal".to_string()],
            "Timeout" => vec!["Timeout".to_string()],
            "Canceled" => vec!["Canceled".to_string()],
            "CaptureLimit" => vec!["CaptureLimit".to_string()],
            "InvalidUtf8" | "InvalidTarget" => vec!["InvalidData".to_string()],
            "Io" | "Redirection" => vec!["HostIo".to_string()],
            "PipelineFailure" | "ExecFailure" | "Spawn" => vec!["ProcessFailure".to_string()],
            _ => Vec::new(),
        }
    }

    pub fn payload(&self) -> RecordMap {
        RecordMap::from([
            ("message".into(), Value::Str(self.message.as_str().into())),
            (
                "status".into(),
                self.status
                    .as_ref()
                    .map(|status| Value::Status((**status).clone()))
                    .unwrap_or(Value::Null),
            ),
        ])
    }
}

pub fn error_constructor(kind: impl Into<String>, message: impl Into<String>) -> Value {
    Value::Error(Box::new(RuntimeError::new(kind, message)))
}

pub fn structured_error_constructor(
    family: impl Into<String>,
    variant: impl Into<String>,
    payload: RecordMap,
    facets: Vec<String>,
    message: impl Into<String>,
) -> Value {
    Value::Error(Box::new(RuntimeError::structured(
        family, variant, payload, facets, message,
    )))
}

pub fn run_error_constructor(kind: impl Into<String>, message: impl Into<String>) -> Value {
    Value::RunError(Box::new(RunError::new(kind, message)))
}

pub fn run_error_from_status(status: ProcessStatus) -> Value {
    Value::RunError(Box::new(RunError::from_status(status)))
}

fn run_error_status_summary(status: &ProcessStatus) -> (String, String) {
    let Some(segment) = status.segments.iter().find(|segment| !segment.success) else {
        return (
            "process-success".to_string(),
            "process completed unsuccessfully".to_string(),
        );
    };

    if let Some(kind) = &segment.error_kind {
        let message = segment
            .error_message
            .clone()
            .unwrap_or_else(|| "process execution failed".to_string());
        return (
            kind.clone(),
            format!(
                "pipeline segment {} failed to execute: {message}",
                segment.index
            ),
        );
    }

    let target = String::from_utf8_lossy(&segment.target);
    let status_text = match segment.kind {
        crate::runtime::process::ProcessSegmentStatusKind::Exit => segment.code.map_or_else(
            || "exited unsuccessfully".to_string(),
            |code| format!("exited with status {code}"),
        ),
        crate::runtime::process::ProcessSegmentStatusKind::Signal => segment.code.map_or_else(
            || "was signaled".to_string(),
            |signal| format!("was terminated by signal {signal}"),
        ),
        crate::runtime::process::ProcessSegmentStatusKind::Exec => "failed to execute".to_string(),
    };
    let kind = if status.segments.len() > 1 {
        "pipeline-failure"
    } else {
        match segment.kind {
            crate::runtime::process::ProcessSegmentStatusKind::Exit => "nonzero-exit",
            crate::runtime::process::ProcessSegmentStatusKind::Signal => "signal",
            crate::runtime::process::ProcessSegmentStatusKind::Exec => "exec-failure",
        }
    };
    (
        kind.to_string(),
        format!(
            "pipeline segment {} `{}` {}",
            segment.index, target, status_text
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::{RecordMap, RecordShape, RuntimeError, Value};
    use crate::symbol::{Name, SymbolOwner};
    use std::collections::BTreeMap;
    use std::mem::size_of;
    use std::sync::Arc;

    #[test]
    fn shaped_records_support_name_lookup() {
        let owner = SymbolOwner::new();
        let (shape, name) = owner.with_current(|| {
            let shape = RecordShape::new(
                (0..8)
                    .map(|index| Arc::<str>::from(format!("field{index}")))
                    .collect(),
            );
            let name = crate::symbol::Name::intern("field7");
            (shape, name)
        });
        let record = RecordMap::shaped(&shape, (0..8).map(Value::Int).collect());

        assert_eq!(record.get_name(name), Some(&Value::Int(7)));
    }

    #[test]
    fn dynamic_record_shapes_are_reclaimed_after_each_owner_drops() {
        for iteration in 0..8 {
            let owner = SymbolOwner::new();
            let shape = owner.with_current(|| {
                let shape = RecordShape::new(vec![Arc::from(format!("field_{iteration}"))]);
                let _record = RecordMap::shaped(&shape, vec![Value::Int(iteration)]);
                shape
            });
            let weak = Arc::downgrade(&shape.data);
            drop(shape);
            drop(owner);
            assert!(weak.upgrade().is_none());
        }
    }

    #[test]
    fn preloaded_record_shapes_remain_interned_after_records_drop() {
        let first = RecordMap::from_name_values(vec![(Name::BOOL, Value::Bool(true))]);
        let RecordMap::Shaped {
            shape: first_shape, ..
        } = &first
        else {
            panic!("preloaded fixed record must use a dense shape");
        };
        let weak = Arc::downgrade(first_shape);
        drop(first);

        let second = RecordMap::from_name_values(vec![(Name::BOOL, Value::Bool(false))]);
        let RecordMap::Shaped {
            shape: second_shape,
            ..
        } = &second
        else {
            panic!("preloaded fixed record must use a dense shape");
        };
        let retained = weak
            .upgrade()
            .expect("preloaded record shapes stay cached for steady-state reuse");
        assert!(Arc::ptr_eq(&retained, second_shape));
    }

    #[test]
    fn record_maps_intern_sorted_shapes_and_keep_dense_fields_on_mutation() {
        SymbolOwner::new().with_current(|| {
            let first = RecordMap::from(BTreeMap::from([
                (Arc::from("z"), Value::Int(3)),
                (Arc::from("a"), Value::Int(1)),
            ]));
            let mut second = RecordMap::from([
                (Arc::from("a"), Value::Int(1)),
                (Arc::from("z"), Value::Int(3)),
            ]);

            let (
                RecordMap::Shaped {
                    shape: first_shape, ..
                },
                RecordMap::Shaped {
                    shape: second_shape,
                    ..
                },
            ) = (&first, &second)
            else {
                panic!("fixed records must use dense shapes");
            };
            assert!(Arc::ptr_eq(first_shape, second_shape));
            assert_eq!(
                first.keys().collect::<Vec<_>>(),
                vec!["a", "z"],
                "record rendering order remains deterministic",
            );

            *second.get_mut("a").expect("field exists") = Value::Int(2);
            assert_eq!(first.get("a"), Some(&Value::Int(1)));
            assert_eq!(second.get("a"), Some(&Value::Int(2)));
            assert!(matches!(second, RecordMap::Shaped { .. }));
        });
    }

    #[test]
    fn extending_a_record_uses_the_dynamic_path() {
        SymbolOwner::new().with_current(|| {
            let mut record = RecordMap::from([(Arc::from("a"), Value::Int(1))]);
            assert_eq!(
                record.insert(Arc::from("a"), Value::Int(2)),
                Some(Value::Int(1))
            );
            assert!(matches!(record, RecordMap::Shaped { .. }));
            record.insert(Arc::from("b"), Value::Int(2));

            assert!(matches!(record, RecordMap::Dynamic(_)));
            assert_eq!(record.keys().collect::<Vec<_>>(), vec!["a", "b"]);
            assert_eq!(
                record,
                RecordMap::from([
                    (Arc::from("a"), Value::Int(2)),
                    (Arc::from("b"), Value::Int(2)),
                ]),
                "record equality ignores the internal storage choice",
            );
        });
    }

    #[test]
    fn value_layout_stays_within_the_compact_runtime_budget() {
        assert_eq!(size_of::<Value>(), 48);
    }

    #[test]
    fn runtime_errors_cache_compact_family_and_variant_names() {
        SymbolOwner::new().with_current(|| {
            let error = RuntimeError::structured(
                "FsError",
                "NotFound",
                RecordMap::new(),
                Vec::new(),
                "missing",
            );

            assert_eq!(error.family_name(), crate::symbol::Name::intern("FsError"));
            assert_eq!(
                error.variant_name(),
                crate::symbol::Name::intern("NotFound")
            );
        });
    }

    #[test]
    fn runtime_errors_own_dynamic_names_without_an_active_symbol_owner() {
        let error = RuntimeError::new("dynamic-session-error", "session helper failed");

        assert_eq!(error.variant_name().as_str(), "dynamic-session-error");
    }
}
