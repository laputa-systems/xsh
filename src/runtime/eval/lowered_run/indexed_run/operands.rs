use super::{
    Arc, BLOCK_LIST, BinaryOp, FullExecution, FullPayload, FullTag, IndexedAssignStep,
    IndexedCompQualifier, IndexedFmt, IndexedModuleCall, IrVerifyError, Name, RuntimeError,
    RuntimeOp, Span,
};
use crate::runtime::eval::LoweredFmtPart;
use crate::runtime::eval::indexed::full::decode::ListView;
use crate::runtime::eval::indexed::full::schema::ExprView;
use crate::runtime::eval::{BuildExprId, BuildPatternId};
use crate::runtime::eval::indexed::full::decode::{ExprInstructionId, ListPosition};

pub(super) type ComparisonChainPosition = ListPosition<BuildExprId>;
pub(super) type MatchExprPosition =
    ListPosition<(BuildPatternId, Option<BuildExprId>, BuildExprId)>;
pub(super) type RecordUpdatePosition = ListPosition<(Vec<Name>, BuildExprId, Span)>;

pub(super) fn indexed_error(error: IrVerifyError, span: Span) -> RuntimeError {
    RuntimeError::new(
        "indexed-ir",
        format!("indexed IR verification failed: {}", error.message),
    )
    .with_span(span)
}

#[inline(always)]
pub(super) fn indexed_value(
    value: Result<(FullTag, FullPayload<'_>), IrVerifyError>,
    span: Span,
) -> Result<(FullTag, FullPayload<'_>), RuntimeError> {
    value.map_err(|error| indexed_error(error, span))
}

#[inline(always)]
pub(super) fn indexed_decode<'a, T: crate::runtime::eval::indexed::full::FullCodec>(
    payload: &mut FullPayload<'a>,
    execution: &FullExecution<'a>,
    span: Span,
) -> Result<T, RuntimeError> {
    payload
        .decode(execution)
        .map_err(|error| indexed_error(error, span))
}

#[inline(always)]
pub(super) fn indexed_raw(payload: &mut FullPayload<'_>, span: Span) -> Result<u32, RuntimeError> {
    payload.raw().map_err(|error| indexed_error(error, span))
}

pub(super) fn indexed_string<'payload, 'program>(
    payload: &mut FullPayload<'payload>,
    execution: &'program FullExecution<'program>,
    span: Span,
) -> Result<&'program str, RuntimeError> {
    execution
        .string(indexed_raw(payload, span)?)
        .map_err(|error| indexed_error(error, span))
}

#[inline(always)]
pub(super) fn indexed_finish(payload: FullPayload<'_>, span: Span) -> Result<(), RuntimeError> {
    payload.finish().map_err(|error| indexed_error(error, span))
}

pub(super) fn indexed_optional_raw(
    payload: &mut FullPayload<'_>,
    span: Span,
) -> Result<Option<u32>, RuntimeError> {
    match indexed_raw(payload, span)? {
        0 => Ok(None),
        1 => indexed_raw(payload, span).map(Some),
        _ => Err(RuntimeError::new("indexed-ir", "invalid optional value tag").with_span(span)),
    }
}

pub(super) fn decode_record_updates<'e>(
    execution: &'e FullExecution<'_>,
    tag: FullTag,
    payload: FullPayload<'e>,
    span: Span,
) -> Result<(u32, ListView<'e, (Vec<Name>, BuildExprId, Span)>, Span), RuntimeError> {
    let ExprView::RecordUpdate {
        base,
        mut updates,
        span: update_span,
    } = ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("record-update tag has a matching borrowed expression view")
    };
    let position = updates.position();
    for update in updates.by_ref() {
        let (mut path, _, _) = update.map_err(|error| indexed_error(error, update_span))?;
        path.preflight()
            .map_err(|error| indexed_error(error, update_span))?;
    }
    updates
        .resume(position)
        .map_err(|error| indexed_error(error, update_span))?;
    Ok((base.raw(), updates, update_span))
}

/// The first operand of a comparison chain and each later operator and operand.
pub(super) fn decode_comparison_chain<'e>(
    execution: &'e FullExecution<'_>,
    tag: FullTag,
    payload: FullPayload<'e>,
    span: Span,
) -> Result<(u32, ListView<'e, BuildExprId>), RuntimeError> {
    let ExprView::ComparisonChain { mut pairs, .. } =
        ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("comparison-chain tag has a matching borrowed expression view")
    };
    let position = pairs.position();
    let mut first = None;
    for pair in pairs.by_ref() {
        let pair = pair.map_err(|error| indexed_error(error, span))?;
        let (_, left, _, _) = decode_comparison_pair(execution, pair, span)?;
        first.get_or_insert(left);
    }
    let first = first.ok_or_else(|| {
        RuntimeError::new("indexed-ir", "comparison chain is empty").with_span(span)
    })?;
    pairs
        .resume(position)
        .map_err(|error| indexed_error(error, span))?;
    Ok((first, pairs))
}

/// Reads one comparison pair while preserving the cursor for later operands.
pub(super) fn comparison_pair_at(
    execution: &FullExecution<'_>,
    instruction: u32,
    position: ComparisonChainPosition,
    span: Span,
) -> Result<Option<(BinaryOp, u32, Span, ComparisonChainPosition)>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    let ExprView::ComparisonChain { mut pairs, .. } =
        ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("comparison-chain instruction has a matching borrowed expression view")
    };
    pairs
        .resume(position)
        .map_err(|error| indexed_error(error, span))?;
    let Some(pair) = pairs.next().transpose().map_err(|error| indexed_error(error, span))? else {
        return Ok(None);
    };
    let (op, _, right, pair_span) = decode_comparison_pair(execution, pair, span)?;
    Ok(Some((op, right, pair_span, pairs.position())))
}

pub(super) fn decode_comparison_pair(
    execution: &FullExecution<'_>,
    pair: ExprInstructionId<'_>,
    span: Span,
) -> Result<(BinaryOp, u32, u32, Span), RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(pair.raw()), span)?;
    if tag != FullTag::ExprBinary {
        return Err(
            RuntimeError::new("indexed-ir", "comparison chain requires binary pairs")
                .with_span(span),
        );
    }
    let ExprView::Binary {
        op,
        left,
        right,
        span,
    } = ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("binary tag has a matching borrowed expression view")
    };
    Ok((op, left.raw(), right.raw(), span))
}

/// A match expression's subject, its borrowed arm cursor, and its span.
pub(super) fn decode_match_expr<'e>(
    execution: &'e FullExecution<'_>,
    tag: FullTag,
    payload: FullPayload<'e>,
    span: Span,
) -> Result<(u32, ListView<'e, (BuildPatternId, Option<BuildExprId>, BuildExprId)>, Span), RuntimeError> {
    let ExprView::MatchExpr {
        value,
        mut arms,
        span: value_span,
    } = ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("match-expression tag has a matching borrowed expression view")
    };
    arms.preflight()
        .map_err(|error| indexed_error(error, value_span))?;
    Ok((value.raw(), arms, value_span))
}

/// Reads one match arm and returns the checked position of the next arm.
pub(super) fn match_expr_arm_at(
    execution: &FullExecution<'_>,
    instruction: u32,
    position: MatchExprPosition,
    span: Span,
) -> Result<Option<((u32, Option<u32>, u32), MatchExprPosition)>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    let ExprView::MatchExpr { mut arms, .. } =
        ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("match-expression instruction has a matching borrowed expression view")
    };
    arms.resume(position)
        .map_err(|error| indexed_error(error, span))?;
    let Some((pattern, guard, body)) = arms.next().transpose().map_err(|error| indexed_error(error, span))? else {
        return Ok(None);
    };
    Ok(Some((
        (pattern.raw(), guard.map(|guard| guard.raw()), body.raw()),
        arms.position(),
    )))
}

/// Reads one record update and materializes only its already-consumed field path.
pub(super) fn record_update_at(
    execution: &FullExecution<'_>,
    instruction: u32,
    position: RecordUpdatePosition,
    span: Span,
) -> Result<Option<(Vec<Name>, u32, Span, RecordUpdatePosition)>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    let ExprView::RecordUpdate { mut updates, .. } =
        ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("record-update instruction has a matching borrowed expression view")
    };
    updates
        .resume(position)
        .map_err(|error| indexed_error(error, span))?;
    let Some((path, value, field_span)) = updates.next().transpose().map_err(|error| indexed_error(error, span))? else {
        return Ok(None);
    };
    let path = path
        .map(|name| name.map_err(|error| indexed_error(error, field_span)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some((path, value.raw(), field_span, updates.position())))
}

/// A module call's operation, prepared plan, optional argument operands, and span.
pub(super) fn decode_module_call<'a>(
    execution: &FullExecution<'a>,
    payload: &mut FullPayload<'a>,
    span: Span,
) -> Result<IndexedModuleCall, RuntimeError> {
    let op = indexed_decode::<RuntimeOp>(payload, execution, span)?;
    let call_plan = indexed_decode::<Option<Arc<crate::runtime::eval::call_plan::ModuleCallPlan>>>(
        payload, execution, span,
    )?;
    let (_, mut encoded) = execution
        .block(payload, BLOCK_LIST)
        .map_err(|error| indexed_error(error, span))?;
    let len = indexed_raw(&mut encoded, span)? as usize;
    let mut args = Vec::with_capacity(len);
    for _ in 0..len {
        args.push(indexed_optional_raw(&mut encoded, span)?);
    }
    indexed_finish(encoded, span)?;
    Ok((
        op,
        call_plan,
        args,
        indexed_decode::<Span>(payload, execution, span)?,
    ))
}

pub(super) fn decode_assign_path<'a>(
    execution: &FullExecution<'a>,
    payload: &mut FullPayload<'a>,
    span: Span,
) -> Result<Vec<IndexedAssignStep>, RuntimeError> {
    let (_, mut entries) = execution
        .block(payload, BLOCK_LIST)
        .map_err(|error| indexed_error(error, span))?;
    let count = indexed_raw(&mut entries, span)?;
    let mut path = Vec::with_capacity(count as usize);
    for _ in 0..count {
        path.push(match indexed_raw(&mut entries, span)? {
            0 => IndexedAssignStep::Field(indexed_decode(&mut entries, execution, span)?),
            1 => IndexedAssignStep::Index(indexed_raw(&mut entries, span)?),
            _ => {
                return Err(
                    RuntimeError::new("indexed-ir", "invalid assignment path step").with_span(span),
                );
            }
        });
    }
    indexed_finish(entries, span)?;
    Ok(path)
}

pub(super) fn decode_comp_qualifiers<'a>(
    execution: &FullExecution<'a>,
    payload: &mut FullPayload<'a>,
    span: Span,
) -> Result<Vec<IndexedCompQualifier>, RuntimeError> {
    let (_, mut entries) = execution
        .block(payload, BLOCK_LIST)
        .map_err(|error| indexed_error(error, span))?;
    let count = indexed_raw(&mut entries, span)?;
    let mut qualifiers = Vec::new();
    for _ in 0..count {
        qualifiers.push(match indexed_raw(&mut entries, span)? {
            0 => IndexedCompQualifier::For {
                target: indexed_decode(&mut entries, execution, span)?,
                iter: indexed_raw(&mut entries, span)?,
                span: indexed_decode(&mut entries, execution, span)?,
            },
            1 => IndexedCompQualifier::If {
                condition: indexed_raw(&mut entries, span)?,
                span: indexed_decode(&mut entries, execution, span)?,
            },
            _ => {
                return Err(
                    RuntimeError::new("indexed-ir", "invalid comprehension qualifier")
                        .with_span(span),
                );
            }
        });
    }
    indexed_finish(entries, span)?;
    if !matches!(qualifiers.first(), Some(IndexedCompQualifier::For { .. })) {
        return Err(RuntimeError::new(
            "indexed-ir",
            "comprehension qualifiers must start with for",
        )
        .with_span(span));
    }
    Ok(qualifiers)
}

/// Decode format parts through the expression row schema and keep their list borrowed.
pub(super) fn decode_fmt<'a>(
    execution: &FullExecution<'a>,
    tag: FullTag,
    payload: FullPayload<'a>,
    span: Span,
) -> Result<(ListView<'a, LoweredFmtPart>, IndexedFmt), RuntimeError> {
    let view = ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?;
    let (parts, path_span) = match view {
        ExprView::FmtString(parts) => (parts, None),
        ExprView::PathFmtString { parts, span } => (parts, Some(span)),
        _ => unreachable!("format string tag has a typed format view"),
    };
    Ok((
        parts,
        IndexedFmt {
            text: String::new(),
            native: Vec::new(),
            path_span,
        },
    ))
}
