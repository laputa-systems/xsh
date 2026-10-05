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
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Validation {
    /// `NonEmpty[T]`: a `List[T]` that holds at least one element.
    NonEmpty,
    /// `RelPath`: a `Path` that is not absolute and whose `..` components
    /// never climb above where the path starts.
    RelPath,
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
    pub const ALL: [Self; 2] = [Self::NonEmpty, Self::RelPath];

    /// The identity a lowered program stores for the validation.
    pub const fn code(self) -> u32 {
        match self {
            Self::NonEmpty => 1,
            Self::RelPath => 2,
        }
    }

    pub fn from_code(code: u32) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|validation| validation.code() == code)
    }

    /// Why `base` cannot carry this validation, or `None` when it can.
    pub fn base_error(self, base: &Type) -> Option<String> {
        match self {
            Self::NonEmpty => (!matches!(base, Type::List(_)))
                .then(|| format!("`NonEmpty` validates a List, not {base}")),
            Self::RelPath => (!matches!(base, Type::Path))
                .then(|| format!("`RelPath` validates a Path, not {base}")),
        }
    }

    /// The registry receiver that lists the operations this validation
    /// guarantees or survives. A method found there is checked against the
    /// validated type; every other method is the base type's.
    pub fn method_receiver(self) -> Option<MethodReceiver> {
        match self {
            Self::NonEmpty => Some(MethodReceiver::NonEmpty),
            Self::RelPath => Some(MethodReceiver::RelPath),
        }
    }

    /// Whether `left + right` keeps the validation when either operand
    /// carries it.
    pub fn survives_concatenation(self) -> bool {
        match self {
            Self::NonEmpty => true,
            Self::RelPath => false,
        }
    }

    /// Whether a comprehension with one `for` clause and no filter keeps the
    /// validation of the list it iterates.
    pub fn survives_mapping(self) -> bool {
        match self {
            Self::NonEmpty => true,
            Self::RelPath => false,
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
        }
    }

    fn fmt_type(self, base: &Type, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self, base) {
            (Self::NonEmpty, Type::List(item)) => write!(f, "NonEmpty[{item}]"),
            (Self::NonEmpty, base) => write!(f, "NonEmpty<{base}>"),
            (Self::RelPath, _) => f.write_str("RelPath"),
        }
    }

    fn annotation_source(self, base: &Type) -> Option<String> {
        match (self, base) {
            (Self::NonEmpty, Type::List(item)) => {
                Some(format!("NonEmpty[{}]", item.annotation_source()?))
            }
            (Self::NonEmpty, _) => None,
            (Self::RelPath, _) => Some("RelPath".to_string()),
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
        // Inside a collection the validated type and its base are different
        // element types.
        assert!(
            !Type::List(Box::new(names.clone())).matches_expected(&Type::List(Box::new(list.clone())))
        );
        assert_eq!(names.unvalidated(), &list);
        assert_eq!(names.to_string(), "NonEmpty[Str]");
        assert_eq!(names.annotation_source().as_deref(), Some("NonEmpty[Str]"));
        assert_eq!(names.iteration_item_type(), Some(Type::Str));
    }

    #[test]
    fn a_validation_names_one_base_form_and_one_code() {
        assert!(ValidatedType::new(Validation::NonEmpty, Type::Str).is_err());
        assert!(
            ValidatedType::new(Validation::NonEmpty, Type::List(Box::new(Type::Int))).is_ok()
        );
        for validation in Validation::ALL {
            assert_eq!(Validation::from_code(validation.code()), Some(validation));
        }
        assert_eq!(Validation::from_code(0), None);
    }

    #[test]
    fn a_rel_path_is_relative_and_never_climbs_above_its_start() {
        for accepted in [
            ".", "a", "a/b", "a/", "a//b", "./a", "a/./b", "a/..", "a/../b", "a/b/../..",
            "...", "..a", "a..", ".hidden",
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
            assert_eq!(rel_path_failure(rejected.as_bytes()), Some(why), "{rejected}");
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
