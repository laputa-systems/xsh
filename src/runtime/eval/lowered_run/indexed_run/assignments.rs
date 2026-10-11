use super::{
    Arc, AssignOp, BLOCK_LIST, FullExecution, FullTag, LoweredTypeCheck, LoweredValue, MapKey,
    ResolvedAssignStep, RuntimeError, Span, indexed_error, indexed_finish, indexed_raw,
    indexed_value, lowered_assign_value, lowered_map_literal_key, lowered_record_field,
    lowered_record_field_value, lowered_value_matches_static_type,
};

// A singleton RHS carries its item directly through assignment execution,
// avoiding a temporary list while retaining ordinary RHS-before-update order.
pub(super) fn indexed_assignment_operand(
    execution: &FullExecution<'_>,
    value: u32,
    op: AssignOp,
    span: Span,
) -> Result<(u32, bool), RuntimeError> {
    if op == AssignOp::Add {
        let (tag, mut payload) = indexed_value(execution.instruction_id(value), span)?;
        if tag == FullTag::ExprList {
            let (_, mut items) = execution
                .block(&mut payload, BLOCK_LIST)
                .map_err(|error| indexed_error(error, span))?;
            if indexed_raw(&mut items, span)? == 1 {
                let item = indexed_raw(&mut items, span)?;
                indexed_finish(items, span)?;
                indexed_finish(payload, span)?;
                return Ok((item, true));
            }
        }
    }
    Ok((value, false))
}

// Taking a container is safe only after both operands prove a list update.
// Other operations may fail, and defers must still see the original target.
pub(super) fn apply_indexed_assignment(
    current: &mut LoweredValue,
    op: AssignOp,
    value: LoweredValue,
    singleton: bool,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    if op == AssignOp::Add && matches!(current, LoweredValue::List(_) | LoweredValue::SharedList(_))
    {
        if singleton {
            let owned = std::mem::replace(current, LoweredValue::Unit);
            return super::super::lowered_method_value(owned, "push", vec![value], span);
        }
        if matches!(value, LoweredValue::List(_) | LoweredValue::SharedList(_)) {
            let owned = std::mem::replace(current, LoweredValue::Unit);
            return lowered_assign_value(op, owned, value, span);
        }
    }
    let value = if singleton {
        LoweredValue::List(vec![value])
    } else {
        value
    };
    lowered_assign_value(op, current.clone(), value, span)
}

pub(super) fn resolve_assign_index(
    value: LoweredValue,
    span: Span,
) -> Result<ResolvedAssignStep, RuntimeError> {
    Ok(match value {
        LoweredValue::Int(index) => ResolvedAssignStep::List(index),
        value => ResolvedAssignStep::Map(lowered_map_literal_key(&value, span)?),
    })
}

pub(super) fn validate_indexed_assignment(
    value: &LoweredValue,
    check: &LoweredTypeCheck,
    span: Span,
) -> Result<(), RuntimeError> {
    if !lowered_value_matches_static_type(value, &check.ty) {
        return Err(RuntimeError::new(
            "type-error",
            format!("assignment violates UInt constraint in {}", check.name),
        )
        .with_span(span));
    }
    Ok(())
}

pub(super) fn checked_indexed_assignment(
    current: &LoweredValue,
    op: AssignOp,
    value: LoweredValue,
    singleton: bool,
    check: &LoweredTypeCheck,
    span: Span,
) -> Result<LoweredValue, RuntimeError> {
    let rhs = if singleton {
        LoweredValue::List(vec![value])
    } else {
        value
    };
    let replacement = if op == AssignOp::Set {
        rhs
    } else {
        lowered_assign_value(op, current.clone(), rhs, span)?
    };
    validate_indexed_assignment(&replacement, check, span)?;
    Ok(replacement)
}

// Validate the complete path before copying or rebuilding an ancestor. The root
// is observed after the RHS so unrelated changes made by either operand survive.
pub(super) fn apply_indexed_path_assignment(
    root: &mut LoweredValue,
    path: &[ResolvedAssignStep],
    op: AssignOp,
    value: LoweredValue,
    singleton: bool,
    check: Option<&LoweredTypeCheck>,
    span: Span,
) -> Result<(), RuntimeError> {
    let mut selected = &*root;
    let mut inline_field = None;
    for (position, step) in path.iter().enumerate() {
        if let ResolvedAssignStep::Field(name) = step
            && matches!(
                selected,
                LoweredValue::Stats { .. } | LoweredValue::StatsBlob(_)
            )
            && position + 1 == path.len()
        {
            inline_field = Some(
                lowered_record_field_value(selected, name.as_str().as_str()).ok_or_else(|| {
                    RuntimeError::new("missing-field", name.to_string()).with_span(span)
                })?,
            );
            break;
        }
        selected = match (step, selected) {
            (ResolvedAssignStep::Field(name), record) => {
                lowered_record_field(record, name.as_str().as_str()).ok_or_else(|| {
                    RuntimeError::new("missing-field", name.to_string()).with_span(span)
                })?
            }
            (ResolvedAssignStep::Map(key), LoweredValue::Map(map)) => {
                super::super::super::lowered_ops::require_lowered_map_key_domain(map, key.as_ref(), span)?;
                if position + 1 == path.len() && op == AssignOp::Set {
                    break;
                }
                map.get(key).ok_or_else(|| {
                    RuntimeError::new("missing-field", format!("{key:?}")).with_span(span)
                })?
            }
            (ResolvedAssignStep::List(index), LoweredValue::Map(map)) => {
                super::super::super::lowered_ops::require_lowered_map_key_domain(
                    map,
                    crate::map_key::MapKeyRef::Int(*index),
                    span,
                )?;
                if position + 1 == path.len() && op == AssignOp::Set {
                    break;
                }
                map.get(&MapKey::Int(*index)).ok_or_else(|| {
                    RuntimeError::new("missing-field", index.to_string()).with_span(span)
                })?
            }
            (ResolvedAssignStep::List(index), LoweredValue::List(list)) => {
                list.get(*index as usize).ok_or_else(|| {
                    RuntimeError::new("index-out-of-range", "list index").with_span(span)
                })?
            }
            (ResolvedAssignStep::List(index), LoweredValue::SharedList(list)) => {
                list.get(*index as usize).ok_or_else(|| {
                    RuntimeError::new("index-out-of-range", "list index").with_span(span)
                })?
            }
            _ => {
                return Err(RuntimeError::new(
                    "type-error",
                    "assignment path requires a compatible collection",
                )
                .with_span(span));
            }
        };
    }
    let selected = inline_field.as_ref().unwrap_or(selected);
    // Fallible arithmetic finishes before mutable descent. List concatenation is
    // safe to consume in place once both operand types have been established.
    let consume_list = check.is_none()
        && op == AssignOp::Add
        && matches!(
            selected,
            LoweredValue::List(_) | LoweredValue::SharedList(_)
        )
        && (singleton || matches!(value, LoweredValue::List(_) | LoweredValue::SharedList(_)));
    let mut operand = Some(value);
    let replacement = if consume_list {
        None
    } else {
        let value = operand.take().expect("assignment operand");
        let rhs = if singleton {
            LoweredValue::List(vec![value])
        } else {
            value
        };
        Some(if op == AssignOp::Set {
            rhs
        } else {
            lowered_assign_value(op, selected.clone(), rhs, span)?
        })
    };
    if let (Some(check), Some(replacement)) = (check, replacement.as_ref()) {
        validate_indexed_assignment(replacement, check, span)?;
    }
    let mut selected = root;
    for (position, step) in path.iter().enumerate() {
        selected = match step {
            ResolvedAssignStep::Field(name) => {
                super::super::super::lowered_ops::lowered_record_field_mut(selected, *name, span)?
            }
            ResolvedAssignStep::Map(key) => {
                let LoweredValue::Map(map) = selected else {
                    unreachable!("validated map path")
                };
                let map = Arc::make_mut(map);
                if position + 1 == path.len() && op == AssignOp::Set {
                    map.insert(key.clone(), replacement.expect("set replacement"));
                    return Ok(());
                }
                map.get_mut(key).expect("validated map key")
            }
            ResolvedAssignStep::List(index) => match selected {
                LoweredValue::Map(map) => {
                    let map = Arc::make_mut(map);
                    if position + 1 == path.len() && op == AssignOp::Set {
                        map.insert(MapKey::Int(*index), replacement.expect("set replacement"));
                        return Ok(());
                    }
                    map.get_mut(&MapKey::Int(*index))
                        .expect("validated map key")
                }
                LoweredValue::List(list) => &mut list[*index as usize],
                LoweredValue::SharedList(list) => &mut Arc::make_mut(list)[*index as usize],
                _ => unreachable!("validated list path"),
            },
        };
    }
    *selected = match replacement {
        Some(value) => value,
        None => apply_indexed_assignment(
            selected,
            op,
            operand.expect("consuming list operand"),
            singleton,
            span,
        )?,
    };
    Ok(())
}

/// Whether two values are the same container through shared backing.
///
/// A consuming call only takes the value out of its slot when the slot still
/// holds the very container the receiver was read from: an argument may have
/// replaced it, and that replacement must be what the slot ends up with.
pub(in crate::runtime::eval::lowered_run) fn lowered_shares_backing(left: &LoweredValue, right: &LoweredValue) -> bool {
    match (left, right) {
        (LoweredValue::Record(left), LoweredValue::Record(right))
        | (LoweredValue::Module(left), LoweredValue::Module(right)) => Arc::ptr_eq(left, right),
        (LoweredValue::RecordVec(left), LoweredValue::RecordVec(right)) => Arc::ptr_eq(left, right),
        (LoweredValue::Map(left), LoweredValue::Map(right)) => Arc::ptr_eq(left, right),
        (LoweredValue::SharedList(left), LoweredValue::SharedList(right)) => {
            Arc::ptr_eq(left, right)
        }
        _ => false,
    }
}
