use super::{
    Arc, BLOCK_LIST, BinaryOp, FullExecution, FullPayload, FullTag, IndexedAssignStep,
    IndexedCompQualifier, IndexedFmt, IndexedFmtPart, IndexedModuleCall, IndexedOperands,
    IrVerifyError, Name, RuntimeError, RuntimeOp, Span,
};

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

pub(super) fn decode_record_updates<'a>(
    execution: &FullExecution<'a>,
    payload: &mut FullPayload<'a>,
    span: Span,
) -> Result<Vec<(Vec<Name>, u32, Span)>, RuntimeError> {
    let (_, mut entries) = execution
        .block(payload, BLOCK_LIST)
        .map_err(|error| indexed_error(error, span))?;
    let count = indexed_raw(&mut entries, span)? as usize;
    let mut updates = Vec::with_capacity(count);
    for _ in 0..count {
        let path = indexed_decode::<Vec<Name>>(&mut entries, execution, span)?;
        let value = indexed_raw(&mut entries, span)?;
        let field_span = indexed_decode::<Span>(&mut entries, execution, span)?;
        updates.push((path, value, field_span));
    }
    indexed_finish(entries, span)?;
    Ok(updates)
}

/// The first operand of a comparison chain and each later operator and operand.
pub(super) fn decode_comparison_chain<'a>(
    execution: &FullExecution<'a>,
    payload: &mut FullPayload<'a>,
    span: Span,
) -> Result<(u32, Vec<(BinaryOp, u32, Span)>), RuntimeError> {
    let (_, mut values) = execution
        .block(payload, BLOCK_LIST)
        .map_err(|error| indexed_error(error, span))?;
    let len = indexed_raw(&mut values, span)? as usize;
    indexed_decode::<bool>(payload, execution, span)?;
    let mut pairs = Vec::with_capacity(len);
    let mut first = None;
    for _ in 0..len {
        let pair = indexed_raw(&mut values, span)?;
        let (tag, mut pair_payload) = indexed_value(execution.instruction_id(pair), span)?;
        if tag != FullTag::ExprBinary {
            return Err(
                RuntimeError::new("indexed-ir", "comparison chain requires binary pairs")
                    .with_span(span),
            );
        }
        let op = indexed_decode(&mut pair_payload, execution, span)?;
        let left = indexed_raw(&mut pair_payload, span)?;
        let right = indexed_raw(&mut pair_payload, span)?;
        let pair_span = indexed_decode(&mut pair_payload, execution, span)?;
        indexed_finish(pair_payload, span)?;
        first.get_or_insert(left);
        pairs.push((op, right, pair_span));
    }
    indexed_finish(values, span)?;
    let first = first.ok_or_else(|| {
        RuntimeError::new("indexed-ir", "comparison chain is empty").with_span(span)
    })?;
    Ok((first, pairs))
}

/// A match expression's subject, its `(pattern, guard, value)` arms, and its span.
pub(super) fn decode_match_expr<'a>(
    execution: &FullExecution<'a>,
    payload: &mut FullPayload<'a>,
    span: Span,
) -> Result<(u32, Vec<(u32, Option<u32>, u32)>, Span), RuntimeError> {
    let value = indexed_raw(payload, span)?;
    let (_, mut arms) = execution
        .block(payload, BLOCK_LIST)
        .map_err(|error| indexed_error(error, span))?;
    let arm_count = indexed_raw(&mut arms, span)? as usize;
    let value_span = indexed_decode::<Span>(payload, execution, span)?;
    let mut decoded = Vec::with_capacity(arm_count);
    for _ in 0..arm_count {
        decoded.push((
            indexed_raw(&mut arms, value_span)?,
            indexed_optional_raw(&mut arms, value_span)?,
            indexed_raw(&mut arms, value_span)?,
        ));
    }
    indexed_finish(arms, value_span)?;
    Ok((value, decoded, value_span))
}

/// A module call's operation, CLI plan, optional argument operands, and span.
pub(super) fn decode_module_call<'a>(
    execution: &FullExecution<'a>,
    payload: &mut FullPayload<'a>,
    span: Span,
) -> Result<IndexedModuleCall, RuntimeError> {
    let op = indexed_decode::<RuntimeOp>(payload, execution, span)?;
    let cli_plan = indexed_decode::<Option<Arc<crate::modules::cli::CliDescriptorPlan>>>(
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
        cli_plan,
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

/// Format parts and the accumulator, whose path span follows the parts.
pub(super) fn fmt_operands<'e, 'a>(
    execution: &'e FullExecution<'a>,
    mut payload: FullPayload<'a>,
    path: bool,
    span: Span,
) -> Result<(IndexedOperands<'e, 'a, IndexedFmtPart>, IndexedFmt), RuntimeError> {
    let operands = IndexedOperands::new(execution, &mut payload, false, span)?;
    let path_span = if path {
        Some(indexed_decode::<Span>(&mut payload, execution, span)?)
    } else {
        None
    };
    indexed_finish(payload, span)?;
    Ok((
        operands,
        IndexedFmt {
            text: String::new(),
            native: Vec::new(),
            path_span,
        },
    ))
}
