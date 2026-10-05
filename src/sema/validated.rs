//! Validated types: a base type narrowed by a validation the checker tracks.
//!
//! A validated type has the representation of its base. The validation is a
//! checked fact about every value that reaches the type, established in one of
//! three ways: a literal the checker can judge, an explicit conversion that
//! tests the value once, or one of the few operations declared to preserve it.
//! Every other operation sees the base type and returns what the base returns.
//!
//! An instance is one `Validation` variant. Everything the checker and the
//! runtime need to know about it is asked through this type, so an instance
//! is declared by filling in each `match` on `Validation`: there is no second
//! table to keep in step.

use super::types::Type;
use crate::modules::MethodReceiver;
use crate::symbol::Name;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Validation {
    /// `NonEmpty[T]`: a `List[T]` that holds at least one element.
    NonEmpty,
    /// `RelPath`: a `Path` that is not absolute and whose `..` components
    /// never climb above where the path starts.
    RelPath,
    /// `nominal type Name = {...}`: a record that came from the type's
    /// constructor or from `.require(Name)`. The payload is the identity of
    /// the declaration. Unlike the others this is not a property of the
    /// value: nothing at run time tells such a record from another with the
    /// same fields, so the checker alone decides where one is accepted.
    Nominal(Name),
    /// `Int range LOW..=HIGH`: an `Int` or `UInt` between two constant
    /// bounds. The payload is the bounds, so two declarations with the same
    /// bounds over the same base are one type.
    Range(IntRange),
}

/// The inclusive bounds of a bounded integer type. `low <= high` always: a
/// declaration that admits no value is rejected where it is written.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IntRange {
    low: i64,
    high: i64,
}

impl IntRange {
    /// The range `low..=high`, or `None` when it holds no value.
    pub fn new(low: i64, high: i64) -> Option<Self> {
        (low <= high).then_some(Self { low, high })
    }

    pub fn low(self) -> i64 {
        self.low
    }

    pub fn high(self) -> i64 {
        self.high
    }

    /// Whether `value` lies within the bounds. This is the whole definition,
    /// read by the checker for a literal and a constant and by the runtime
    /// for a value, so the three cannot disagree.
    pub fn contains(self, value: i64) -> bool {
        self.low <= value && value <= self.high
    }
}

impl fmt::Display for IntRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..={}", self.low, self.high)
    }
}

/// Whether the native bytes of a path are a `RelPath`. This is the whole
/// definition, read by the checker for a literal and a constant and by the
/// runtime for a value, so the three cannot disagree.
///
/// The path is not empty and does not start with `/`. Its components are the
/// pieces between `/` separators: an empty piece (a repeated or trailing
/// separator) and `.` stay where they are, a name goes one level down, and
/// `..` goes one level up and must have a level to leave. The test is
/// lexical: it reads no filesystem and knows nothing of symlinks.
pub fn is_rel_path(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes[0] == b'/' {
        return false;
    }
    let mut depth = 0usize;
    for component in bytes.split(|byte| *byte == b'/') {
        match component {
            b"" | b"." => {}
            b".." => match depth.checked_sub(1) {
                Some(above) => depth = above,
                None => return false,
            },
            _ => depth += 1,
        }
    }
    true
}

/// Why `bytes` is not a `RelPath`, for a literal the checker rejects.
pub fn rel_path_failure(bytes: &[u8]) -> Option<&'static str> {
    if is_rel_path(bytes) {
        None
    } else if bytes.is_empty() {
        Some("is empty")
    } else if bytes[0] == b'/' {
        Some("is absolute")
    } else {
        Some("climbs above where it starts")
    }
}

impl Validation {
    /// The validations that are a property of a value without a payload,
    /// which a code alone identifies. A nominal identity is named by its
    /// declaration instead, and a range by its bounds.
    pub const ALL: [Self; 2] = [Self::NonEmpty, Self::RelPath];

    /// The identity a lowered program stores for a validation a code alone
    /// identifies, and `None` for a nominal identity, which a lowered
    /// program stores by name, and for a range, which it stores with its
    /// bounds.
    pub const fn code(self) -> Option<u32> {
        match self {
            Self::NonEmpty => Some(1),
            Self::RelPath => Some(2),
            Self::Nominal(_) => None,
            Self::Range(_) => None,
        }
    }

    pub fn from_code(code: u32) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|validation| validation.code() == Some(code))
    }

    /// Why `base` cannot carry this validation, or `None` when it can.
    pub fn base_error(self, base: &Type) -> Option<String> {
        match self {
            Self::NonEmpty => (!matches!(base, Type::List(_)))
                .then(|| format!("`NonEmpty` validates a List, not {base}")),
            Self::RelPath => (!matches!(base, Type::Path))
                .then(|| format!("`RelPath` validates a Path, not {base}")),
            Self::Nominal(name) => (!matches!(base, Type::Record(_)))
                .then(|| format!("nominal type `{name}` is a record, not {base}")),
            Self::Range(range) => match base {
                Type::Int => None,
                Type::UInt => (range.low() < 0).then(|| {
                    format!("a UInt is never negative, so `{range}` is not a range of UInt")
                }),
                _ => Some(format!("a range bounds an Int or a UInt, not {base}")),
            },
        }
    }

    /// The registry receiver that lists the operations this validation
    /// guarantees or survives. A method found there is checked against the
    /// validated type; every other method is the base type's.
    pub fn method_receiver(self) -> Option<MethodReceiver> {
        match self {
            Self::NonEmpty => Some(MethodReceiver::NonEmpty),
            Self::RelPath => Some(MethodReceiver::RelPath),
            Self::Nominal(_) => None,
            Self::Range(_) => None,
        }
    }

    /// Whether `left + right` keeps the validation when either operand
    /// carries it.
    pub fn survives_concatenation(self) -> bool {
        match self {
            Self::NonEmpty => true,
            Self::RelPath | Self::Nominal(_) | Self::Range(_) => false,
        }
    }

    /// Whether a comprehension with one `for` clause and no filter keeps the
    /// validation of the list it iterates.
    pub fn survives_mapping(self) -> bool {
        match self {
            Self::NonEmpty => true,
            Self::RelPath | Self::Nominal(_) | Self::Range(_) => false,
        }
    }

    /// Whether every value that passes `self` passes `other`.
    pub fn implies(self, other: Self) -> bool {
        self == other
    }

    /// What a value of the base type that fails the validation is, for a
    /// message that reads "expected NonEmpty[Str], found ...".
    pub fn failure(self) -> &'static str {
        match self {
            Self::NonEmpty => "an empty list",
            Self::RelPath => "a path that is empty, absolute, or climbs above where it starts",
            Self::Nominal(_) => "a record that did not come from the type's constructor",
            Self::Range(_) => "an integer outside the range",
        }
    }

    /// How to obtain the validated type from a value of the base type.
    pub fn conversion_note(self, ty: &Type) -> String {
        match self {
            Self::NonEmpty => {
                let written = ty
                    .annotation_source()
                    .unwrap_or_else(|| "NonEmpty[T]".to_string());
                format!(
                    "a list is a {ty} only once it is known to hold an element: validate it with `.require({written})?`, or write a list literal with at least one element where {ty} is expected"
                )
            }
            Self::RelPath => "a path is a RelPath only once it is known to stay beneath where it starts: validate it with `.require(RelPath)?`, or write a path literal where RelPath is expected".to_string(),
            Self::Nominal(name) => {
                let name = name.as_str();
                let name = name.rsplit('.').next().unwrap_or(&name);
                format!(
                    "`{ty}` is a nominal type, so a record with the same fields is not one: build the value with the constructor `{name}(...)`, or convert a record with `.require({name})?`"
                )
            }
            Self::Range(range) => format!(
                "an integer has a bounded type only once it is known to lie in {range}: validate it with `.require(...)?` or `as`, naming the type, or write an integer literal in the range; arithmetic on a bounded value returns its base type"
            ),
        }
    }

    fn fmt_type(self, base: &Type, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self, base) {
            (Self::NonEmpty, Type::List(item)) => write!(f, "NonEmpty[{item}]"),
            (Self::NonEmpty, base) => write!(f, "NonEmpty<{base}>"),
            (Self::RelPath, _) => f.write_str("RelPath"),
            (Self::Nominal(name), _) => write!(f, "{name}"),
            (Self::Range(range), base) => write!(f, "{base} range {range}"),
        }
    }

    fn annotation_source(self, base: &Type) -> Option<String> {
        match (self, base) {
            (Self::NonEmpty, Type::List(item)) => {
                Some(format!("NonEmpty[{}]", item.annotation_source()?))
            }
            (Self::NonEmpty, _) => None,
            (Self::RelPath, _) => Some("RelPath".to_string()),
            (Self::Nominal(name), _) => Some(name.to_string()),
            // The bounds are written only in a `type` declaration, which an
            // annotation then names.
            (Self::Range(_), _) => None,
        }
    }
}

/// A base type and the validation its values have passed. The fields are
/// private so that the pair is always one `Validation::base_error` accepts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedType {
    validation: Validation,
    base: Type,
}

impl ValidatedType {
    pub fn new(validation: Validation, base: Type) -> Result<Self, String> {
        match validation.base_error(&base) {
            Some(reason) => Err(reason),
            None => Ok(Self { validation, base }),
        }
    }

    pub fn validation(&self) -> Validation {
        self.validation
    }

    pub fn base(&self) -> &Type {
        &self.base
    }

    pub fn into_base(self) -> Type {
        self.base
    }

    /// Rewrites the base in place, for substitution of type variables inside
    /// it. `rewrite` must keep the outer form of the base.
    pub(crate) fn rewrite_base<R>(&mut self, rewrite: impl FnOnce(&mut Type) -> R) -> R {
        let result = rewrite(&mut self.base);
        debug_assert!(self.validation.base_error(&self.base).is_none());
        result
    }

    /// The same validation over the base `rewrite` derives from this one.
    /// `rewrite` must keep the outer form of the base.
    pub(crate) fn map_base(&self, rewrite: impl FnOnce(&Type) -> Type) -> Self {
        let base = rewrite(&self.base);
        debug_assert!(self.validation.base_error(&base).is_none());
        Self {
            validation: self.validation,
            base,
        }
    }

    pub(super) fn annotation_source(&self) -> Option<String> {
        self.validation.annotation_source(&self.base)
    }
}

impl fmt::Display for ValidatedType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.validation.fmt_type(&self.base, f)
    }
}

impl Type {
    /// `NonEmpty[item]`.
    pub fn non_empty(item: Type) -> Self {
        Self::Validated(Box::new(ValidatedType {
            validation: Validation::NonEmpty,
            base: Self::List(Box::new(item)),
        }))
    }

    /// `RelPath`.
    pub fn rel_path() -> Self {
        Self::Validated(Box::new(ValidatedType {
            validation: Validation::RelPath,
            base: Self::Path,
        }))
    }

    /// The nominal type `name` over the record type `base`.
    pub fn nominal(name: Name, base: Type) -> Result<Self, String> {
        ValidatedType::new(Validation::Nominal(name), base)
            .map(|validated| Self::Validated(Box::new(validated)))
    }

    /// The declaration this type is the nominal identity of.
    pub fn nominal_name(&self) -> Option<Name> {
        match self.validated()?.validation() {
            Validation::Nominal(name) => Some(name),
            _ => None,
        }
    }

    /// `base range low..=high`, for `base` an `Int` or a `UInt` the range
    /// fits.
    pub fn bounded(range: IntRange, base: Type) -> Result<Self, String> {
        ValidatedType::new(Validation::Range(range), base)
            .map(|validated| Self::Validated(Box::new(validated)))
    }

    /// The bounds of a bounded integer type.
    pub fn int_range(&self) -> Option<IntRange> {
        match self.validated()?.validation() {
            Validation::Range(range) => Some(range),
            _ => None,
        }
    }

    pub fn validated(&self) -> Option<&ValidatedType> {
        match self {
            Self::Validated(validated) => Some(validated),
            _ => None,
        }
    }

    /// The type whose operations apply to a value of this type: the base of
    /// a validated type, and any other type itself. An operation that is not
    /// declared to preserve a validation reads its operand through this.
    pub fn unvalidated(&self) -> &Type {
        let mut ty = self;
        while let Self::Validated(validated) = ty {
            ty = validated.base();
        }
        ty
    }

    /// Whether a validated type is this type or sits inside it where a
    /// structure of base values may be expected instead. A value of such a
    /// type can have been given a wider type by the context it was written
    /// in.
    pub fn holds_validated(&self) -> bool {
        match self {
            Self::Validated(_) => true,
            Self::List(inner) | Self::Stream(inner) | Self::Optional(inner) => {
                inner.holds_validated()
            }
            Self::Map(key, value) | Self::Result(key, value) => {
                key.holds_validated() || value.holds_validated()
            }
            Self::Record(fields) => fields.values().any(Self::holds_validated),
            Self::Union(members) => members.iter().any(Self::holds_validated),
            _ => false,
        }
    }

    /// A nominal type that is this type or sits inside it. A runtime test of
    /// such a type cannot see the identity, so the checker allows the test
    /// only where the tested value's static type already carries it.
    pub fn held_nominal(&self) -> Option<&Type> {
        match self {
            Self::Validated(validated) => match validated.validation() {
                Validation::Nominal(_) => Some(self),
                _ => validated.base().held_nominal(),
            },
            Self::List(inner) | Self::Stream(inner) | Self::Optional(inner) => inner.held_nominal(),
            Self::Map(key, value) | Self::Result(key, value) => {
                key.held_nominal().or_else(|| value.held_nominal())
            }
            Self::Record(fields) => fields.values().find_map(Self::held_nominal),
            Self::Union(members) => members.iter().find_map(Self::held_nominal),
            _ => None,
        }
    }

    pub fn into_unvalidated(self) -> Type {
        let mut ty = self;
        while let Self::Validated(validated) = ty {
            ty = validated.into_base();
        }
        ty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_validated_type_fits_its_base_and_never_the_reverse() {
        let names = Type::non_empty(Type::Str);
        let list = Type::List(Box::new(Type::Str));
        assert!(names.matches_expected(&list));
        assert!(names.matches_expected(&names));
        assert!(names.matches_expected(&Type::Any));
        assert!(names.matches_expected(&Type::Optional(Box::new(list.clone()))));
        assert!(!list.matches_expected(&names));
        assert!(!Type::Any.matches_expected(&names));
        // The base is a list, so the element type is invariant.
        assert!(!names.matches_expected(&Type::List(Box::new(Type::Any))));
        assert!(!names.matches_expected(&Type::non_empty(Type::Path)));
        assert_eq!(names.unvalidated(), &list);
        assert_eq!(names.to_string(), "NonEmpty[Str]");
        assert_eq!(names.annotation_source().as_deref(), Some("NonEmpty[Str]"));
        assert_eq!(names.iteration_item_type(), Some(Type::Str));
    }

    #[test]
    fn a_structure_of_validated_values_fits_the_structure_of_base_values() {
        let list = |item: Type| Type::List(Box::new(item));
        let optional = |inner: Type| Type::Optional(Box::new(inner));
        let map = |value: Type| Type::Map(Box::new(Type::Str), Box::new(value));
        let rel = Type::rel_path;
        let wrappers: [&dyn Fn(Type) -> Type; 6] = [
            &list,
            &optional,
            &map,
            &|item| Type::Stream(Box::new(item)),
            &|item| list(list(item)),
            &|item| list(optional(item)),
        ];
        for wrap in wrappers {
            let (narrow, wide) = (wrap(rel()), wrap(Type::Path));
            assert!(narrow.matches_expected(&wide), "{narrow} -> {wide}");
            assert!(!wide.matches_expected(&narrow), "{wide} -> {narrow}");
        }
        // The base of a validated element is widened too, and the outer
        // validation is kept or dropped.
        let names = Type::non_empty(rel());
        assert!(names.matches_expected(&Type::non_empty(Type::Path)));
        assert!(names.matches_expected(&list(Type::Path)));
        assert!(list(names.clone()).matches_expected(&list(list(Type::Path))));
        assert!(!list(list(Type::Path)).matches_expected(&list(names)));
        // Only a validation is dropped: the element type is otherwise
        // invariant, and a scalar domain is not crossed.
        assert!(!list(rel()).matches_expected(&list(Type::Any)));
        assert!(!list(rel()).matches_expected(&list(Type::Str)));
        assert!(!list(Type::Int).matches_expected(&list(Type::UInt)));
        assert!(!optional(Type::Int).matches_expected(&optional(Type::UInt)));
        let union = |members: Vec<Type>| Type::Union(members);
        let narrow = list(union(vec![rel(), Type::Int]));
        let wide = list(union(vec![Type::Path, Type::Int]));
        assert!(narrow.matches_expected(&wide));
        assert!(!wide.matches_expected(&narrow));
        assert!(!list(rel()).matches_expected(&wide));
        // The two lists are one runtime shape, so a union cannot tell them
        // apart.
        assert!(
            crate::sema::types::union_member_error(&[list(rel()), list(Type::Path)])
                .is_some_and(|reason| reason.contains("already fits"))
        );
    }

    #[test]
    fn a_validation_names_one_base_form_and_one_code() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            assert!(ValidatedType::new(Validation::NonEmpty, Type::Str).is_err());
            assert!(
                ValidatedType::new(Validation::NonEmpty, Type::List(Box::new(Type::Int))).is_ok()
            );
            for validation in Validation::ALL {
                assert_eq!(
                    Validation::from_code(validation.code().unwrap()),
                    Some(validation)
                );
            }
            assert_eq!(Validation::from_code(0), None);
            assert_eq!(Validation::Nominal(Name::intern("Package")).code(), None);
            assert_eq!(Validation::Range(IntRange::new(1, 2).unwrap()).code(), None);
        });
    }

    #[test]
    fn a_nominal_type_fits_its_record_and_nothing_else_fits_it() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let record = |fields: &[&str]| {
                Type::Record(
                    fields
                        .iter()
                        .map(|field| (Name::intern(*field), Type::Str))
                        .collect(),
                )
            };
            let base = record(&["id", "version"]);
            let package = Type::nominal(Name::intern("Package"), base.clone()).unwrap();
            let release = Type::nominal(Name::intern("Release"), base.clone()).unwrap();
            assert!(Type::nominal(Name::intern("Package"), Type::Str).is_err());
            assert_eq!(package.to_string(), "Package");
            assert_eq!(package.nominal_name(), Some(Name::intern("Package")));
            assert_eq!(package.unvalidated(), &base);

            assert!(package.matches_expected(&package));
            assert!(package.matches_expected(&base));
            // The base is a record, so the nominal value fits a narrower schema
            // as any record with those fields does.
            assert!(package.matches_expected(&record(&["id"])));
            assert!(package.matches_expected(&Type::ErasedRecord));
            assert!(package.matches_expected(&Type::Any));
            assert!(!base.matches_expected(&package));
            assert!(!record(&["id", "version", "arch"]).matches_expected(&package));
            assert!(!Type::Any.matches_expected(&package));
            assert!(!Type::ErasedRecord.matches_expected(&package));
            assert!(!package.matches_expected(&release));
            assert!(!release.matches_expected(&package));

            let list = |item: Type| Type::List(Box::new(item));
            assert!(list(package.clone()).matches_expected(&list(base.clone())));
            assert!(!list(base.clone()).matches_expected(&list(package.clone())));
            assert!(!list(package.clone()).matches_expected(&list(release.clone())));
            assert_eq!(list(package.clone()).held_nominal(), Some(&package));
            assert_eq!(list(base.clone()).held_nominal(), None);

            // The runtime tells a nominal member from another by fields alone.
            for other in [base.clone(), release, record(&["id", "version", "arch"])] {
                assert!(
                    crate::sema::types::union_member_error(&[package.clone(), other.clone()])
                        .is_some(),
                    "{other}"
                );
                assert!(
                    crate::sema::types::union_member_error(&[other, package.clone()]).is_some()
                );
            }
            assert_eq!(
                crate::sema::types::union_member_error(&[package.clone(), Type::Int]),
                None
            );
            assert_eq!(
                crate::sema::types::union_member_error(&[package, record(&["name"])]),
                None
            );
        });
    }

    #[test]
    fn a_bounded_integer_fits_its_base_and_never_the_reverse() {
        let range = IntRange::new(1, 65535).unwrap();
        let port = Type::bounded(range, Type::Int).unwrap();
        assert!(IntRange::new(2, 1).is_none());
        assert!(range.contains(1) && range.contains(65535));
        assert!(!range.contains(0) && !range.contains(65536));
        assert_eq!(port.to_string(), "Int range 1..=65535");
        assert_eq!(port.int_range(), Some(range));
        assert_eq!(port.unvalidated(), &Type::Int);
        assert!(port.matches_expected(&Type::Int));
        assert!(port.matches_expected(&port));
        assert!(port.matches_expected(&Type::Any));
        assert!(!Type::Int.matches_expected(&port));
        assert!(!Type::UInt.matches_expected(&port));
        assert!(!Type::Any.matches_expected(&port));
        // The base's own rules apply to what a bounded value widens to: it
        // fits whatever an Int fits, and nothing else.
        assert_eq!(
            port.matches_expected(&Type::UInt),
            Type::Int.matches_expected(&Type::UInt)
        );
        assert!(!port.matches_expected(&Type::Str));
        // Other bounds are another type, in both directions.
        let low = Type::bounded(IntRange::new(1, 1023).unwrap(), Type::Int).unwrap();
        assert!(!low.matches_expected(&port));
        assert!(!port.matches_expected(&low));
        let list = |item: Type| Type::List(Box::new(item));
        assert!(list(port.clone()).matches_expected(&list(Type::Int)));
        assert!(!list(Type::Int).matches_expected(&list(port.clone())));

        let size = Type::bounded(IntRange::new(0, 255).unwrap(), Type::UInt).unwrap();
        assert_eq!(size.to_string(), "UInt range 0..=255");
        assert!(size.matches_expected(&Type::UInt));
        assert!(!size.matches_expected(&port));
        assert!(Type::bounded(IntRange::new(-1, 5).unwrap(), Type::UInt).is_err());
        assert!(Type::bounded(range, Type::Str).is_err());
        assert!(Type::bounded(range, Type::Float).is_err());
        assert!(
            crate::sema::types::union_member_error(&[port, Type::Int])
                .is_some_and(|reason| reason.contains("already fits"))
        );
    }

    #[test]
    fn a_rel_path_is_relative_and_never_climbs_above_its_start() {
        for accepted in [
            ".",
            "a",
            "a/b",
            "a/",
            "a//b",
            "./a",
            "a/./b",
            "a/..",
            "a/../b",
            "a/b/../..",
            "...",
            "..a",
            "a..",
            ".hidden",
        ] {
            assert!(is_rel_path(accepted.as_bytes()), "{accepted}");
            assert_eq!(rel_path_failure(accepted.as_bytes()), None);
        }
        for (rejected, why) in [
            ("", "is empty"),
            ("/", "is absolute"),
            ("/a", "is absolute"),
            ("//a", "is absolute"),
            ("..", "climbs above where it starts"),
            ("../a", "climbs above where it starts"),
            ("./..", "climbs above where it starts"),
            ("a/../..", "climbs above where it starts"),
            ("a/b/../../../c", "climbs above where it starts"),
            ("a//../..", "climbs above where it starts"),
        ] {
            assert!(!is_rel_path(rejected.as_bytes()), "{rejected}");
            assert_eq!(
                rel_path_failure(rejected.as_bytes()),
                Some(why),
                "{rejected}"
            );
        }
        // The rule reads bytes: a name that is not UTF-8 is a name.
        assert!(is_rel_path(b"a/\xff/b"));
        assert!(!is_rel_path(b"\xff/../.."));
    }

    #[test]
    fn a_rel_path_fits_a_path_and_never_the_reverse() {
        let rel = Type::rel_path();
        assert!(rel.matches_expected(&Type::Path));
        assert!(rel.matches_expected(&rel));
        assert!(rel.matches_expected(&Type::Optional(Box::new(Type::Path))));
        assert!(!Type::Path.matches_expected(&rel));
        assert!(!Type::Str.matches_expected(&rel));
        assert!(!Type::Any.matches_expected(&rel));
        assert!(!rel.matches_expected(&Type::Str));
        assert_eq!(rel.unvalidated(), &Type::Path);
        assert_eq!(rel.to_string(), "RelPath");
        assert_eq!(rel.annotation_source().as_deref(), Some("RelPath"));
        assert_eq!(Type::builtin_from_name("RelPath"), Some(rel.clone()));
        assert!(rel.can_display() && rel.can_be_argv_item());
        assert!(ValidatedType::new(Validation::RelPath, Type::Str).is_err());
        assert!(
            crate::sema::types::union_member_error(&[rel, Type::Path])
                .is_some_and(|reason| reason.contains("already fits"))
        );
    }

    #[test]
    fn a_union_cannot_list_a_validated_type_beside_its_base() {
        let reason = crate::sema::types::union_member_error(&[
            Type::non_empty(Type::Str),
            Type::List(Box::new(Type::Str)),
        ]);
        assert!(reason.is_some_and(|reason| reason.contains("already fits")));
    }
}
