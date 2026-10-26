//! The runtime side of `Set[T]`: construction, the methods, and the set
//! operators.
//!
//! A set stores its elements as map keys, so it has the order and the
//! equality a map's keys have. The checker gives every set one element type;
//! each operation here still refuses a value of another key domain, which is
//! the defended fallback for a set that reached it through `Any`.

use super::lowered_ops::{lowered_map_key_ref, lowered_map_key_value};
use super::{LoweredValue, RuntimeError};
use crate::map_key::{MapKey, MapKeyRef};
use crate::source::Span;
use crate::syntax::node::BinaryOp;
use std::collections::BTreeSet;
use std::sync::Arc;

fn require_element_domain(
    set: &BTreeSet<MapKey>,
    element: MapKeyRef<'_>,
    span: Span,
) -> Result<(), RuntimeError> {
    if set
        .first()
        .is_some_and(|existing| !existing.as_ref().same_domain(element))
    {
        return Err(RuntimeError::new(
            "type-error",
            "Set elements require one scalar domain without conversion",
        )
        .with_span(span));
    }
    Ok(())
}

fn element_ref(value: &LoweredValue, span: Span) -> Result<MapKeyRef<'_>, RuntimeError> {
    lowered_map_key_ref(value, span).map_err(|_| {
        RuntimeError::new(
            "type-error",
            format!(
                "Set elements require an ordered scalar value, found {}",
                value.type_name()
            ),
        )
        .with_span(span)
    })
}

/// The set of a list's elements. A repeated element is held once.
pub(super) fn lowered_set_from_items(
    items: &[LoweredValue],
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let mut set = BTreeSet::new();
    for item in items {
        let element = element_ref(item, span)?;
        require_element_domain(&set, element, span)?;
        set.insert(element.to_owned());
    }
    Ok(LoweredValue::Set(Arc::new(set)))
}

pub(super) fn lowered_set_items(set: &BTreeSet<MapKey>) -> Vec<LoweredValue> {
    set.iter().map(lowered_map_key_value).collect()
}

/// `needle in set`.
pub(super) fn lowered_set_contains(
    set: &BTreeSet<MapKey>,
    needle: &LoweredValue,
    span: Span,
) -> Result<bool, RuntimeError> {
    let element = element_ref(needle, span)?;
    require_element_domain(set, element, span)?;
    Ok(element.is_in(set))
}

pub(super) fn lowered_set_method_value(
    set: Arc<BTreeSet<MapKey>>,
    name: &str,
    args: Vec<LoweredValue>,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    match name {
        "len" if args.is_empty() => Ok(LoweredValue::Int(set.len() as i64)),
        "is_empty" if args.is_empty() => Ok(LoweredValue::Bool(set.is_empty())),
        "to_list" if args.is_empty() => Ok(LoweredValue::List(lowered_set_items(&set))),
        "add" if args.len() == 1 => {
            let element = element_ref(&args[0], span)?;
            require_element_domain(&set, element, span)?;
            if element.is_in(&set) {
                return Ok(LoweredValue::Set(set));
            }
            // The caller gave the receiver up, so a set held nowhere else is
            // updated in place.
            let mut set = super::lower::take_shared(set);
            set.insert(element.to_owned());
            Ok(LoweredValue::Set(Arc::new(set)))
        }
        "remove" if args.len() == 1 => {
            let element = element_ref(&args[0], span)?;
            require_element_domain(&set, element, span)?;
            if !element.is_in(&set) {
                return Ok(LoweredValue::Set(set));
            }
            let mut set = super::lower::take_shared(set);
            element.take_from(&mut set);
            Ok(LoweredValue::Set(Arc::new(set)))
        }
        _ => Err(RuntimeError::new("type-error", "unsupported lowered method").with_span(span)),
    }
}

/// `left | right`, `left & right`, and `left - right` on two sets.
pub(super) fn lowered_set_binary_value(
    op: BinaryOp,
    left: Arc<BTreeSet<MapKey>>,
    right: &LoweredValue,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let symbol = match op {
        BinaryOp::Union => "|",
        BinaryOp::Intersect => "&",
        BinaryOp::Sub => "-",
        _ => {
            return Err(
                RuntimeError::new("type-error", "invalid lowered binary operation").with_span(span),
            );
        }
    };
    let LoweredValue::Set(right) = right else {
        return Err(RuntimeError::new(
            "type-error",
            format!("`{symbol}` requires two sets, found {}", right.type_name()),
        )
        .with_span(span));
    };
    if let (Some(left), Some(right)) = (left.first(), right.first())
        && !left.as_ref().same_domain(right.as_ref())
    {
        return Err(RuntimeError::new(
            "type-error",
            "Set elements require one scalar domain without conversion",
        )
        .with_span(span));
    }
    let result = match op {
        BinaryOp::Union => {
            let mut result = super::lower::take_shared(left);
            result.extend(right.iter().cloned());
            result
        }
        BinaryOp::Intersect => left.intersection(right).cloned().collect(),
        _ => left.difference(right).cloned().collect(),
    };
    Ok(LoweredValue::Set(Arc::new(result)))
}
