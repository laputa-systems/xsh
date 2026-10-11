use super::{LoweredValue, MapKey, ResultValue, RuntimeError, Type, Value};
use crate::map_key::MapKeyRef;
use crate::runtime::value::FsEntryValue;
use crate::sema::types::ModuleExportType;
use crate::sema::validated::{Validation, is_rel_path};
use crate::symbol::Name;
use std::collections::{BTreeMap, BTreeSet};

/// Type checks borrow stored children and expose computed fields without
/// materializing a record, cloning a value, or pulling a stream.
#[derive(Clone, Copy)]
pub(super) enum ValueView<'a> {
    Runtime(&'a Value),
    Lowered(&'a LoweredValue),
    Key(MapKeyRef<'a>),
    Int(i64),
    Str,
    Path(&'a [u8]),
    Map(&'a BTreeMap<MapKey, LoweredValue>),
    EmptyMap,
}

impl<'a> ValueView<'a> {
    fn scalar_type(self) -> Option<Type> {
        Some(match self {
            Self::Int(_) | Self::Key(MapKeyRef::Int(_)) => Type::Int,
            Self::Str | Self::Key(MapKeyRef::Str(_)) => Type::Str,
            Self::Path(_) | Self::Key(MapKeyRef::Path(_)) => Type::Path,
            Self::Key(MapKeyRef::Bool(_)) => Type::Bool,
            Self::Key(MapKeyRef::Bytes(_)) => Type::Bytes,
            Self::Key(MapKeyRef::Duration(_)) => Type::Duration,
            Self::Runtime(value) => match value {
                Value::Null => Type::Null,
                Value::Bool(_) => Type::Bool,
                Value::Int(_) => Type::Int,
                Value::Float(_) => Type::Float,
                Value::Duration(_) => Type::Duration,
                Value::Str(_) => Type::Str,
                Value::Bytes(_) => Type::Bytes,
                Value::Digest(_) => Type::Digest,
                Value::Regex(_) => Type::Regex,
                Value::Path(_) => Type::Path,
                Value::Status(_) => Type::Status,
                Value::EnvPathList => Type::EnvPathList,
                Value::Error(_) => Type::Error,
                Value::RunError(_) => Type::ProcessError,
                Value::Pure(_) => Type::Pure,
                Value::Proc(_) => Type::Proc,
                Value::Command(_) => Type::Command,
                Value::ProcessHandle(_) => Type::ProcessHandle,
                Value::NetJob(_) => Type::NetJob,
                Value::FsRoot(_) => Type::FsRoot,
                Value::FsLock(_) => Type::FsLock,
                Value::Unit => Type::Unit,
                Value::Tag { type_name, .. } => Type::Tag(*type_name),
                Value::List(_) | Value::Map(_) | Value::Set(_) | Value::Stream(_)
                | Value::Record(_) | Value::FsEntry(_) | Value::Module(_) | Value::Result(_) => return None,
            },
            Self::Lowered(value) => match value {
                LoweredValue::Null => Type::Null,
                LoweredValue::Bool(_) => Type::Bool,
                LoweredValue::Int(_) => Type::Int,
                LoweredValue::Float(_) => Type::Float,
                LoweredValue::Duration(_) => Type::Duration,
                LoweredValue::Str(_) | LoweredValue::StrView(_) => Type::Str,
                LoweredValue::Bytes(_) | LoweredValue::BytesView(_) => Type::Bytes,
                LoweredValue::Digest(_) => Type::Digest,
                LoweredValue::Regex(_) => Type::Regex,
                LoweredValue::Path(_) => Type::Path,
                LoweredValue::Status(_) => Type::Status,
                LoweredValue::Error(value) => {
                    if matches!(value.as_ref(), Value::RunError(_)) { Type::ProcessError } else { Type::Error }
                }
                LoweredValue::Pure(_) => Type::Pure,
                LoweredValue::Proc(_) => Type::Proc,
                LoweredValue::Command(_) => Type::Command,
                LoweredValue::ProcessHandle(_) => Type::ProcessHandle,
                LoweredValue::NetJob(_) => Type::NetJob,
                LoweredValue::FsRoot(_) => Type::FsRoot,
                LoweredValue::FsLock(_) => Type::FsLock,
                LoweredValue::Unit => Type::Unit,
                LoweredValue::Tag(tag) => Type::Tag(tag.type_name),
                LoweredValue::OmittedArgument | LoweredValue::List(_) | LoweredValue::SharedList(_)
                | LoweredValue::Map(_) | LoweredValue::Set(_) | LoweredValue::Stream(_)
                | LoweredValue::Record(_) | LoweredValue::RecordVec(_) | LoweredValue::FsEntry(_)
                | LoweredValue::Stats { .. } | LoweredValue::StatsBlob(_) | LoweredValue::Module(_)
                | LoweredValue::ResultOk(_) | LoweredValue::ResultErr(_) => return None,
            },
            Self::Map(_) | Self::EmptyMap => return None,
        })
    }

    fn integer(self) -> Option<i64> {
        match self {
            Self::Runtime(Value::Int(value)) | Self::Lowered(LoweredValue::Int(value)) => Some(*value),
            Self::Int(value) | Self::Key(MapKeyRef::Int(value)) => Some(value),
            _ => None,
        }
    }

    fn path_bytes(self) -> Option<&'a [u8]> {
        match self {
            Self::Runtime(Value::Path(path)) | Self::Lowered(LoweredValue::Path(path)) => Some(&path.bytes),
            Self::Path(bytes) | Self::Key(MapKeyRef::Path(bytes)) => Some(bytes),
            _ => None,
        }
    }

    fn list_len(self) -> Option<usize> {
        match self {
            Self::Runtime(Value::List(items)) => Some(items.len()),
            Self::Lowered(LoweredValue::List(items)) => Some(items.len()),
            Self::Lowered(LoweredValue::SharedList(items)) => Some(items.len()),
            _ => None,
        }
    }

    fn all_list(self, mut accepts: impl FnMut(Self) -> bool) -> bool {
        match self {
            Self::Runtime(Value::List(items)) => items.iter().all(|item| accepts(Self::Runtime(item))),
            Self::Lowered(LoweredValue::List(items)) => items.iter().all(|item| accepts(Self::Lowered(item))),
            Self::Lowered(LoweredValue::SharedList(items)) => items.iter().all(|item| accepts(Self::Lowered(item))),
            _ => false,
        }
    }

    fn set(self) -> Option<&'a BTreeSet<MapKey>> {
        match self {
            Self::Runtime(Value::Set(items)) => Some(items),
            Self::Lowered(LoweredValue::Set(items)) => Some(items),
            _ => None,
        }
    }

    fn map_fields(self) -> bool {
        matches!(self, Self::Runtime(Value::Record(_))
            | Self::Lowered(LoweredValue::Record(_) | LoweredValue::RecordVec(_)))
    }

    fn all_map(self, mut accepts: impl FnMut(MapKeyRef<'_>, Self) -> bool) -> bool {
        match self {
            Self::Runtime(Value::Map(items)) => items.iter().all(|(key, item)| accepts(key.as_ref(), Self::Runtime(item))),
            Self::Lowered(LoweredValue::Map(items)) => items.iter().all(|(key, item)| accepts(key.as_ref(), Self::Lowered(item))),
            Self::Map(items) => items.iter().all(|(key, item)| accepts(key.as_ref(), Self::Lowered(item))),
            Self::EmptyMap => true,
            // An entry cannot materialize metadata it was created without.
            Self::Runtime(Value::FsEntry(_)) | Self::Lowered(LoweredValue::FsEntry(_)) => false,
            _ => false,
        }
    }

    fn all_record_values(self, mut accepts: impl FnMut(Self) -> bool) -> bool {
        match self {
            Self::Runtime(Value::Record(fields)) => fields.values().all(|value| accepts(Self::Runtime(value))),
            Self::Lowered(LoweredValue::Record(fields)) => fields.values().all(|value| accepts(Self::Lowered(value))),
            Self::Lowered(LoweredValue::RecordVec(fields)) => fields.iter().all(|(_, value)| accepts(Self::Lowered(value))),
            _ => false,
        }
    }

    fn is_record(self, erased: bool) -> bool {
        match self {
            Self::Runtime(Value::Record(_) | Value::FsEntry(_))
            | Self::Lowered(LoweredValue::Record(_) | LoweredValue::RecordVec(_) | LoweredValue::FsEntry(_)) => true,
            Self::Lowered(LoweredValue::Stats { .. } | LoweredValue::StatsBlob(_)) => !erased,
            _ => false,
        }
    }

    fn field(self, name: Name) -> Option<Self> {
        match self {
            Self::Runtime(Value::Record(fields) | Value::Module(fields)) => name.with_str(|name| fields.get(name).map(Self::Runtime)),
            Self::Lowered(LoweredValue::Record(fields) | LoweredValue::Module(fields)) => name.with_str(|name| fields.get::<str>(name).map(Self::Lowered)),
            Self::Lowered(LoweredValue::RecordVec(fields)) => fields.iter().find(|(key, _)| *key == name).map(|(_, value)| Self::Lowered(value)),
            Self::Runtime(Value::FsEntry(entry)) | Self::Lowered(LoweredValue::FsEntry(entry)) => name.with_str(|name| Self::entry_field(entry, name)),
            Self::Lowered(LoweredValue::Stats { blanks, code, comments }) => name.with_str(|name| match name {
                "blanks" => Some(Self::Int(*blanks)),
                "blobs" => Some(Self::EmptyMap),
                "code" => Some(Self::Int(*code)),
                "comments" => Some(Self::Int(*comments)),
                _ => None,
            }),
            Self::Lowered(LoweredValue::StatsBlob(stats)) => name.with_str(|name| match name {
                "blanks" => Some(Self::Int(stats.blanks)),
                "blobs" => Some(Self::Map(&stats.blobs)),
                "code" => Some(Self::Int(stats.code)),
                "comments" => Some(Self::Int(stats.comments)),
                _ => None,
            }),
            _ => None,
        }
    }

    fn entry_field(entry: &'a FsEntryValue, name: &str) -> Option<Self> {
        match name {
            "path" if !entry.path_bytes().contains(&0) => Some(Self::Path(entry.path_bytes())),
            "name" | "ext" | "kind" => Some(Self::Str),
            _ => None,
        }
    }

    fn module_len(self) -> Option<usize> {
        match self {
            Self::Runtime(Value::Module(fields)) => Some(fields.len()),
            Self::Lowered(LoweredValue::Module(fields)) => Some(fields.len()),
            _ => None,
        }
    }

    fn result(self) -> Option<(bool, Self)> {
        match self {
            Self::Runtime(Value::Result(ResultValue::Ok(value))) => Some((true, Self::Runtime(value))),
            Self::Runtime(Value::Result(ResultValue::Err(value))) => Some((false, Self::Runtime(value))),
            Self::Lowered(LoweredValue::ResultOk(value)) => Some((true, Self::Lowered(value))),
            Self::Lowered(LoweredValue::ResultErr(value)) => Some((false, Self::Runtime(value))),
            _ => None,
        }
    }

    fn error(self) -> Option<&'a RuntimeError> {
        match self {
            Self::Runtime(Value::Error(error)) => Some(error),
            Self::Lowered(LoweredValue::Error(value)) => Self::Runtime(value).error(),
            _ => None,
        }
    }

    pub(super) fn passes(self, validation: Validation) -> bool {
        match validation {
            Validation::NonEmpty => self.list_len().is_some_and(|len| len > 0),
            Validation::RelPath => self.path_bytes().is_some_and(is_rel_path),
            // Nominal identity has no runtime representation; the checker
            // restricts identity tests to values whose static type vouches for it.
            Validation::Nominal(_) => true,
            Validation::Range(range) => self.integer().is_some_and(|value| range.contains(value)),
        }
    }
}

pub(super) fn value_matches_static_type(value: ValueView<'_>, ty: &Type) -> bool {
    // Stored key domains never widen to container or optional types.
    if matches!(value, ValueView::Key(_)) && !matches!(ty,
        Type::Str | Type::Int | Type::UInt | Type::Bool | Type::Bytes
        | Type::Path | Type::Duration | Type::Any | Type::Unknown | Type::Invalid)
    {
        return false;
    }
    match ty {
        Type::BuiltinParameter(_) | Type::Inference(_) => false,
        Type::Any | Type::Unknown | Type::Invalid => true,
        Type::UInt => value.integer().is_some_and(|value| value >= 0),
        Type::Null | Type::Bool | Type::Int | Type::Float | Type::Duration
        | Type::Str | Type::Bytes | Type::Digest | Type::Regex | Type::Path
        | Type::Status | Type::EnvPathList | Type::ProcessError | Type::Pure
        | Type::Proc | Type::Command | Type::ProcessHandle | Type::NetJob
        | Type::FsRoot | Type::FsLock | Type::Unit | Type::Tag(_) => value.scalar_type().as_ref() == Some(ty),
        Type::List(item) => value.all_list(|value| value_matches_static_type(value, item)),
        Type::Map(key, item) => {
            if value.map_fields() {
                key.as_ref() == &Type::Str && value.all_record_values(|value| value_matches_static_type(value, item))
            } else {
                value.all_map(|found_key, value| value_matches_static_type(ValueView::Key(found_key), key) && value_matches_static_type(value, item))
            }
        }
        Type::Set(item) => value.set().is_some_and(|items| items.iter().all(|key| value_matches_static_type(ValueView::Key(key.as_ref()), item))),
        // A type test cannot inspect a stream's items or start its producer.
        Type::Stream(_) => matches!(value, ValueView::Runtime(Value::Stream(_)) | ValueView::Lowered(LoweredValue::Stream(_))),
        Type::ErasedRecord => value.is_record(true),
        Type::Record(fields) => value.is_record(false) && fields.iter().all(|(name, ty)| value.field(*name).is_some_and(|field| value_matches_static_type(field, ty))),
        // Shape tests check export presence and kind. Full callable signatures
        // remain the responsibility of the module validation boundary.
        Type::Module(contract) => {
            let Some(len) = value.module_len() else { return false; };
            let mut present = 0;
            contract.iter().all(|(name, export)| match value.field(*name) {
                Some(field) => {
                    present += 1;
                    match export {
                        ModuleExportType::Value { ty, .. } => value_matches_static_type(field, ty),
                        ModuleExportType::Proc { .. } => field.scalar_type() == Some(Type::Proc),
                        ModuleExportType::Pure { .. } => field.scalar_type() == Some(Type::Pure),
                    }
                }
                None => export.optional(),
            }) && (!contract.exact || present == len)
        }
        Type::DynamicModule => value.module_len().is_some(),
        Type::Result(ok, error) => value.result().is_some_and(|(success, value)| value_matches_static_type(value, if success { ok } else { error })),
        Type::Error => matches!(value.scalar_type(), Some(Type::Error | Type::ProcessError)),
        Type::ErrorFamily(family) => value.error().is_some_and(|error| crate::runtime::value::error_family_matches(error.family_name(), *family)),
        Type::ErrorVariant { family, variant } => value.error().is_some_and(|error| crate::runtime::value::error_family_matches(error.family_name(), *family) && error.variant_name() == *variant),
        Type::ErrorFacet(facet) => value.error().is_some_and(|error| facet.with_str(|facet| error.facets.iter().any(|value| value == facet))),
        // Callable handles carry their kind, not their checked signature.
        Type::Callable(callable) => value.scalar_type() == Some(if callable.pure { Type::Pure } else { Type::Proc }),
        Type::Optional(inner) => value.scalar_type() == Some(Type::Null) || value_matches_static_type(value, inner),
        Type::Union(members) => crate::sema::types::first_accepting_union_member(members, |ty| value_matches_static_type(value, ty)).is_some(),
        Type::Validated(validated) => value_matches_static_type(value, validated.base()) && value.passes(validated.validation()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_track::{CountingAllocator, begin_stage, enable_tracking, end_stage};
    use crate::runtime::value::RecordMap;
    use crate::sema::types::ModuleType;
    use crate::symbol::SymbolOwner;
    use std::sync::Arc;

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator::new(std::alloc::System);

    // Allocation accounting needs the host allocator boundary; native tests
    // cover the same shapes and their visible type-test outcomes.
    #[test]
    fn borrowed_type_checks_allocate_nothing_for_nested_maps_records_and_modules() {
        SymbolOwner::new().with_current(|| {
            let items = Name::intern("items");
            let item_type = Type::List(Box::new(Type::Optional(Box::new(Type::UInt))));
            let record_type = Type::Record(BTreeMap::from([(items, item_type)]));
            let runtime = Value::Record(RecordMap::from_name_values(vec![
                (items, Value::List(vec![Value::Int(7), Value::Null])),
            ]));
            let lowered = LoweredValue::RecordVec(Arc::new(vec![
                (items, LoweredValue::SharedList(Arc::new(vec![LoweredValue::Int(7), LoweredValue::Null]))),
            ]));
            let fields = Value::Record(RecordMap::from_name_values(vec![(items, Value::Int(7))]));
            let map_type = Type::Map(Box::new(Type::Str), Box::new(Type::UInt));
            let value = Name::intern("value");
            let runtime_module = Value::Module(RecordMap::from_name_values(vec![
                (value, runtime),
            ]));
            let lowered_module = LoweredValue::Module(Arc::new(BTreeMap::from([
                (Arc::from("value"), lowered),
            ])));
            let module_type = Type::Module(Arc::new(ModuleType::exact(BTreeMap::from([
                (value, ModuleExportType::Value { ty: record_type, optional: false }),
            ]))));
            let stats = LoweredValue::Stats { blanks: 1, code: 2, comments: 3 };
            let stats_blob = LoweredValue::StatsBlob(Box::new(super::super::LoweredStatsValue {
                blanks: 1,
                blobs: BTreeMap::from([(MapKey::Str(Arc::from("text")), LoweredValue::Int(4))]),
                code: 2,
                comments: 3,
            }));
            let blobs = Name::intern("blobs");
            // Another owner releasing slots invalidates the thread's spelling
            // cache. Borrowed field access must stay allocation-free afterward.
            let _ = blobs.as_str();
            let released = SymbolOwner::new();
            released.with_current(|| { Name::intern("released allocation fixture symbol"); });
            drop(released);
            let stats_type = Type::Record(BTreeMap::from([
                (blobs, Type::Map(Box::new(Type::Str), Box::new(Type::UInt))),
            ]));
            let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/value.rs");
            let entry = Value::FsEntry(FsEntryValue::new(source.clone(), std::fs::metadata(source).unwrap().file_type()));
            let entry_type = Type::Record(BTreeMap::from([
                (Name::intern("path"), Type::Path),
                (Name::intern("name"), Type::Str),
                (Name::intern("ext"), Type::Str),
                (Name::intern("kind"), Type::Str),
            ]));

            enable_tracking();
            begin_stage();
            let matched = [
                value_matches_static_type(ValueView::Runtime(&runtime_module), &module_type),
                value_matches_static_type(ValueView::Lowered(&lowered_module), &module_type),
                value_matches_static_type(ValueView::Runtime(&fields), &map_type),
                value_matches_static_type(ValueView::Lowered(&stats), &stats_type),
                value_matches_static_type(ValueView::Lowered(&stats_blob), &stats_type),
                value_matches_static_type(ValueView::Runtime(&entry), &entry_type),
            ];
            let traffic = end_stage();
            assert!(matched.into_iter().all(|matched| matched));
            assert!(traffic.tracking_active);
            assert_eq!(traffic.alloc_count, 0);
            assert_eq!(traffic.alloc_bytes, 0);
        });
    }

    // The host constructor can receive a path the kernel could never list;
    // native scripts cannot create a directory entry whose name contains NUL.
    #[test]
    fn filesystem_entry_path_type_checks_keep_path_construction_failures() {
        use std::os::unix::ffi::OsStringExt;
        SymbolOwner::new().with_current(|| {
            let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/value.rs");
            let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"invalid\0path".to_vec()));
            let entry = Value::FsEntry(FsEntryValue::new(path, std::fs::metadata(source).unwrap().file_type()));
            let ty = Type::Record(BTreeMap::from([(Name::intern("path"), Type::Any)]));
            assert!(!value_matches_static_type(ValueView::Runtime(&entry), &ty));
        });
    }
}
