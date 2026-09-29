//! Ordered scalar map keys and allocation-free lookup views.

use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Runtime key domains preserve scalar identity. UInt shares the existing Int
/// representation; its nonnegative constraint is checked at typed boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MapKey {
    Str(Arc<str>),
    Int(i64),
    Bool(bool),
    Bytes(Arc<[u8]>),
    Path(Arc<[u8]>),
    Duration(u64),
}

/// Borrowed key comparison never clones or decodes text or native bytes.
/// Variant order gives malformed heterogeneous storage a deterministic order;
/// checked language maps have one homogeneous key domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum MapKeyRef<'a> {
    Str(&'a str),
    Int(i64),
    Bool(bool),
    Bytes(&'a [u8]),
    Path(&'a [u8]),
    Duration(u64),
}

trait KeyQuery {
    fn key_ref(&self) -> MapKeyRef<'_>;
}

impl KeyQuery for MapKey {
    fn key_ref(&self) -> MapKeyRef<'_> { self.as_ref() }
}

impl KeyQuery for MapKeyRef<'_> {
    fn key_ref(&self) -> MapKeyRef<'_> { *self }
}

impl PartialEq for dyn KeyQuery + '_ {
    fn eq(&self, other: &Self) -> bool { self.key_ref() == other.key_ref() }
}
impl Eq for dyn KeyQuery + '_ {}
impl PartialOrd for dyn KeyQuery + '_ {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for dyn KeyQuery + '_ {
    fn cmp(&self, other: &Self) -> Ordering { self.key_ref().cmp(&other.key_ref()) }
}
impl<'a> Borrow<dyn KeyQuery + 'a> for MapKey {
    fn borrow(&self) -> &(dyn KeyQuery + 'a) { self }
}

impl Ord for MapKey {
    fn cmp(&self, other: &Self) -> Ordering { self.as_ref().cmp(&other.as_ref()) }
}
impl PartialOrd for MapKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

impl MapKey {
    pub fn as_ref(&self) -> MapKeyRef<'_> {
        match self {
            Self::Str(value) => MapKeyRef::Str(value),
            Self::Int(value) => MapKeyRef::Int(*value),
            Self::Bool(value) => MapKeyRef::Bool(*value),
            Self::Bytes(value) => MapKeyRef::Bytes(value),
            Self::Path(value) => MapKeyRef::Path(value),
            Self::Duration(value) => MapKeyRef::Duration(*value),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self { Self::Str(value) => Some(value), _ => None }
    }


}

impl<'a> MapKeyRef<'a> {
    pub fn same_domain(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }

    pub fn to_owned(self) -> MapKey {
        match self {
            Self::Str(value) => MapKey::Str(Arc::from(value)),
            Self::Int(value) => MapKey::Int(value),
            Self::Bool(value) => MapKey::Bool(value),
            Self::Bytes(value) => MapKey::Bytes(Arc::from(value)),
            Self::Path(value) => MapKey::Path(Arc::from(value)),
            Self::Duration(value) => MapKey::Duration(value),
        }
    }

    pub fn get<V>(self, entries: &BTreeMap<MapKey, V>) -> Option<&V> {
        entries.get::<dyn KeyQuery>(&self)
    }

    pub fn get_mut<V>(self, entries: &mut BTreeMap<MapKey, V>) -> Option<&mut V> {
        entries.get_mut::<dyn KeyQuery>(&self)
    }

    pub fn contains_key<V>(self, entries: &BTreeMap<MapKey, V>) -> bool {
        entries.contains_key::<dyn KeyQuery>(&self)
    }

    pub fn remove<V>(self, entries: &mut BTreeMap<MapKey, V>) -> Option<V> {
        entries.remove::<dyn KeyQuery>(&self)
    }
}

impl From<String> for MapKey {
    fn from(value: String) -> Self { Self::Str(Arc::from(value)) }
}
impl From<&str> for MapKey {
    fn from(value: &str) -> Self { Self::Str(Arc::from(value)) }
}
impl From<Arc<str>> for MapKey {
    fn from(value: Arc<str>) -> Self { Self::Str(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_map_keys_keep_domains_and_canonical_order() {
        let integers = BTreeMap::from([(MapKey::Int(20), "twenty"), (MapKey::Int(-2), "negative"), (MapKey::Int(3), "three")]);
        assert_eq!(integers.keys().cloned().collect::<Vec<_>>(), [MapKey::Int(-2), MapKey::Int(3), MapKey::Int(20)]);
        assert_eq!(MapKeyRef::Int(3).get(&integers), Some(&"three"));
        assert_eq!(MapKeyRef::Str("3").get(&integers), None);
        let mut flags = BTreeMap::from([(MapKey::Bool(true), 1), (MapKey::Bool(false), 0)]);
        assert_eq!(flags.keys().next(), Some(&MapKey::Bool(false)));
        *MapKeyRef::Bool(true).get_mut(&mut flags).unwrap() = 2;
        assert_eq!(MapKeyRef::Bool(true).remove(&mut flags), Some(2));
        let durations = BTreeMap::from([(MapKey::Duration(1000), 1), (MapKey::Duration(2), 2)]);
        assert_eq!(durations.keys().next(), Some(&MapKey::Duration(2)));
        assert_eq!(MapKeyRef::Int(2).get(&durations), None);
    }

    #[test]
    fn scalar_map_native_bytes_are_lossless_and_borrowed() {
        let first: Arc<[u8]> = Arc::from([0x61, 0x80]);
        let second: Arc<[u8]> = Arc::from([0x61, 0xff]);
        let entries = BTreeMap::from([(MapKey::Path(second.clone()), 2), (MapKey::Path(first.clone()), 1)]);
        let query = [0x61, 0x80];
        assert_eq!(MapKeyRef::Path(&query).get(&entries), Some(&1));
        assert_eq!(MapKeyRef::Bytes(&query).get(&entries), None);
        assert_eq!(entries.keys().next(), Some(&MapKey::Path(first.clone())));
        assert_eq!(Arc::strong_count(&first), 2);
        let bytes = BTreeMap::from([(MapKey::Bytes(second), 2), (MapKey::Bytes(first), 1)]);
        assert_eq!(MapKeyRef::Bytes(&query).get(&bytes), Some(&1));
    }
}
