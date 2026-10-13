use super::{
    Arc, Evaluator, FullExecution, LoweredValue, Name, RuntimeError, Span, Value,
    indexed_error, lowered_error_value_has_facet, lowered_record_field,
    lowered_value_from_runtime_any, ValueView, value_matches_static_type,
};
use crate::runtime::eval::indexed::full::schema::PatternView;

fn indexed_pattern_view<'e>(
    execution: &'e FullExecution<'_>,
    pattern: u32,
    span: Span,
) -> Result<PatternView<'e>, RuntimeError> {
    let (tag, payload) = execution
        .pattern(pattern)
        .map_err(|error| indexed_error(error, span))?;
    PatternView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))
}

impl Evaluator {
    // Structural validation precedes capture publication throughout the pattern tree.
    // Failed nested patterns leave every capture slot untouched and allocate no list rest.
    pub(in crate::runtime::eval) fn indexed_pattern_matches(
        execution: &FullExecution<'_>,
        pattern: u32,
        value: &LoweredValue,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<bool, RuntimeError> {
        if !Self::indexed_pattern_match_pass(execution, pattern, value, slots, span, false)? {
            return Ok(false);
        }
        Self::indexed_pattern_match_pass(execution, pattern, value, slots, span, true)
    }

    pub(super) fn indexed_pattern_match_pass(
        execution: &FullExecution<'_>,
        pattern: u32,
        value: &LoweredValue,
        slots: &mut [LoweredValue],
        span: Span,
        bind: bool,
    ) -> Result<bool, RuntimeError> {
        let view = indexed_pattern_view(execution, pattern, span)?;
        let matched = match view {
            PatternView::TagType {
                type_name,
                variants,
            } => {
                let Some(value) = (match value {
                    LoweredValue::Tag(value) if value.type_name == type_name => Some(value),
                    _ => None,
                }) else {
                    for variant in variants {
                        variant.map_err(|error| indexed_error(error, span))?;
                    }
                    return Ok(false);
                };
                let mut matched = false;
                for variant in variants {
                    let variant = variant.map_err(|error| indexed_error(error, span))?;
                    matched |= value.name.as_ref() == variant.as_str();
                }
                matched
            }
            PatternView::RecordTest { fields } => {
                let mut matched = matches!(value, LoweredValue::Record(_) | LoweredValue::RecordVec(_));
                for field in fields {
                    let (name, child) = field.map_err(|error| indexed_error(error, span))?;
                    if !matched {
                        continue;
                    }
                    let Some(field) = lowered_record_field(value, &name.as_str()) else {
                        matched = false;
                        continue;
                    };
                    if !Self::indexed_pattern_match_pass(
                        execution,
                        child.raw(),
                        field,
                        slots,
                        span,
                        bind,
                    )? {
                        matched = false;
                    }
                }
                matched
            }
            PatternView::ResultTest { ok, inner } => match value {
                LoweredValue::ResultOk(value) if ok => Self::indexed_pattern_match_pass(
                    execution,
                    inner.raw(),
                    value,
                    slots,
                    span,
                    bind,
                )?,
                LoweredValue::ResultErr(value) if !ok => {
                    if let Some(value) = lowered_value_from_runtime_any(value) {
                        Self::indexed_pattern_match_pass(
                            execution,
                            inner.raw(),
                            &value,
                            slots,
                            span,
                            bind,
                        )?
                    } else {
                        false
                    }
                }
                _ => false,
            },
            PatternView::TagTest {
                type_name,
                name,
                fields,
            } => {
                let tag = match value {
                    LoweredValue::Tag(value) => Some(value),
                    _ => None,
                };
                let mut matched = tag.is_some_and(|value| {
                    value.type_name == type_name && value.name.as_ref() == name.as_str()
                });
                let mut field_index = 0;
                for child in fields {
                    let child = child.map_err(|error| indexed_error(error, span))?;
                    if matched {
                        let Some(field) = tag.and_then(|tag| tag.fields.get(field_index)) else {
                            matched = false;
                            field_index += 1;
                            continue;
                        };
                        if !Self::indexed_pattern_match_pass(
                            execution,
                            child.raw(),
                            field,
                            slots,
                            span,
                            bind,
                        )? {
                            matched = false;
                        }
                    }
                    field_index += 1;
                }
                matched && tag.is_some_and(|tag| tag.fields.len() == field_index)
            }
            PatternView::ErrorTest {
                family,
                variant,
                fields,
            } => {
                let error_fields = if let LoweredValue::Error(value) = value {
                    match value.as_ref() {
                        Value::Error(error)
                            if crate::runtime::value::error_family_matches(
                                error.family_identity(),
                                family,
                            ) && error.variant_name() == variant =>
                        {
                            Some(error.payload.clone())
                        }
                        Value::RunError(error)
                            if family == Name::PROCESS_ERROR
                                && error.variant_name() == variant.as_str() =>
                        {
                            Some(error.payload())
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                let mut matched = error_fields.is_some();
                for field in fields {
                    let (name, child) = field.map_err(|error| indexed_error(error, span))?;
                    if !matched {
                        continue;
                    }
                    let Some(field) = error_fields
                        .as_ref()
                        .and_then(|values| values.get(&name.as_str()))
                        .and_then(lowered_value_from_runtime_any)
                    else {
                        matched = false;
                        continue;
                    };
                    if !Self::indexed_pattern_match_pass(
                        execution,
                        child.raw(),
                        &field,
                        slots,
                        span,
                        bind,
                    )? {
                        matched = false;
                    }
                }
                matched
            }
            PatternView::List { elements, rest } => {
                let count = elements.len();
                let items = match value {
                    LoweredValue::List(items) => Some(items.as_slice()),
                    LoweredValue::SharedList(items) => Some(items.as_slice()),
                    _ => None,
                };
                let mut matched = items.is_some_and(|items| {
                    items.len() >= count && (rest.is_some() || items.len() == count)
                });
                let mut index = 0;
                for child in elements {
                    let child = child.map_err(|error| indexed_error(error, span))?;
                    if matched {
                        let Some(item) = items.and_then(|items| items.get(index)) else {
                            matched = false;
                            index += 1;
                            continue;
                        };
                        if !Self::indexed_pattern_match_pass(
                            execution,
                            child.raw(),
                            item,
                            slots,
                            span,
                            bind,
                        )? {
                            matched = false;
                        }
                    }
                    index += 1;
                }
                matched = matched && index == count && if let Some(items) = items {
                    index <= items.len() && (rest.is_some() || index == items.len())
                } else {
                    false
                };
                if matched && bind && let Some(rest) = rest {
                    if let PatternView::Bind { slot } = indexed_pattern_view(
                        execution,
                        rest.raw(),
                        span,
                    )? {
                        let Some(items) = items else {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "indexed IR verification failed: matched list pattern lost its subject list",
                            )
                            .with_span(span));
                        };
                        slots[slot] = LoweredValue::List(items[index..].to_vec());
                    }
                }
                matched
            }
            PatternView::Alias { pattern, slot } => {
                let matched = Self::indexed_pattern_match_pass(
                    execution,
                    pattern.raw(),
                    value,
                    slots,
                    span,
                    bind,
                )?;
                if matched && bind {
                    slots[slot] = value.clone();
                }
                matched
            }
            PatternView::Alternation { patterns } => {
                let mut selected = None;
                for child in patterns {
                    let child = child.map_err(|error| indexed_error(error, span))?;
                    if selected.is_none()
                        && Self::indexed_pattern_match_pass(
                            execution,
                            child.raw(),
                            value,
                            slots,
                            span,
                            false,
                        )?
                    {
                        selected = Some(child.raw());
                    }
                }
                if let Some(selected) = selected {
                    if bind {
                        Self::indexed_pattern_match_pass(
                            execution, selected, value, slots, span, true,
                        )?
                    } else {
                        true
                    }
                } else {
                    false
                }
            }
            PatternView::Wildcard => true,
            PatternView::Text {
                mut holes,
                mut kinds,
                segments,
            } => {
                use crate::sema::check::{TextHoleValue, split_text};
                let segments = segments
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| indexed_error(error, span))?;
                let subject = match value {
                    LoweredValue::Str(text) => Some(text.as_ref()),
                    LoweredValue::StrView(view) => Some(view.as_str()),
                    _ => None,
                };
                let texts = subject.and_then(|subject| split_text(&segments, subject));
                let mut matched = texts.is_some();
                let mut index = 0;
                while let Some(hole) = holes.next() {
                    let hole = hole.map_err(|error| indexed_error(error, span))?;
                    let kind = kinds
                        .next()
                        .ok_or_else(|| {
                            RuntimeError::new(
                                "indexed-ir",
                                "indexed IR verification failed: text pattern hole and kind counts differ",
                            )
                            .with_span(span)
                        })?
                        .map_err(|error| indexed_error(error, span))?;
                    if matched {
                        let Some(text) = texts
                            .as_ref()
                            .and_then(|texts| texts.get(index))
                        else {
                            matched = false;
                            index += 1;
                            continue;
                        };
                        let converted = match kind.convert(text) {
                            Some(TextHoleValue::Text(_)) if !bind => None,
                            Some(TextHoleValue::Text(text)) => {
                                Some(LoweredValue::Str(Arc::from(text)))
                            }
                            Some(TextHoleValue::Int(number)) => Some(LoweredValue::Int(number)),
                            Some(TextHoleValue::Float(number)) => Some(LoweredValue::Float(
                                crate::runtime::value::FloatValue::new(number),
                            )),
                            None => {
                                matched = false;
                                None
                            }
                        };
                        if let Some(converted) = converted
                            && !Self::indexed_pattern_match_pass(
                                execution,
                                hole.raw(),
                                &converted,
                                slots,
                                span,
                                bind,
                            )?
                        {
                            matched = false;
                        }
                    }
                    index += 1;
                }
                matched
            }
            PatternView::Bind { slot } => {
                if bind {
                    slots[slot] = value.clone();
                }
                true
            }
            PatternView::Type { ty, slot } => {
                let ty = ty.materialize().map_err(|error| indexed_error(error, span))?;
                if !value_matches_static_type(ValueView::Lowered(value), &ty) {
                    false
                } else {
                    if let Some(slot) = slot
                        && bind
                    {
                        slots[slot] = value.clone();
                    }
                    true
                }
            }
            PatternView::Literal(value_ref) => {
                let pattern_value = value_ref
                    .materialize()
                    .map_err(|error| indexed_error(error, span))?;
                crate::runtime::eval::lowered_values_semantically_equal(&pattern_value, value)
            }
            PatternView::ResultOk { slot, unit_only } => {
                if let LoweredValue::ResultOk(inner) = value {
                    if unit_only && !matches!(inner.as_ref(), LoweredValue::Unit) {
                        false
                    } else {
                        if let Some(slot) = slot
                            && bind
                        {
                            slots[slot] = inner.as_ref().clone();
                        }
                        true
                    }
                } else {
                    false
                }
            }
            PatternView::ResultErr { slot, unit_only } => {
                if let LoweredValue::ResultErr(inner) = value {
                    if unit_only && !matches!(inner.as_ref(), Value::Unit) {
                        false
                    } else if let Some(slot) = slot {
                        let Some(inner) = lowered_value_from_runtime_any(inner.as_ref()) else {
                            return Ok(false);
                        };
                        if bind {
                            slots[slot] = inner;
                        }
                        true
                    } else {
                        true
                    }
                } else {
                    false
                }
            }
            PatternView::Facet {
                facet,
                result_wrapped,
            } => {
                let error = if result_wrapped {
                    let LoweredValue::ResultErr(error) = value else {
                        return Ok(false);
                    };
                    error.as_ref()
                } else {
                    let LoweredValue::Error(error) = value else {
                        return Ok(false);
                    };
                    error.as_ref()
                };
                lowered_error_value_has_facet(error, &facet.as_str())
            }
            PatternView::Tag {
                type_name,
                name,
                slots: field_slots,
            } => {
                let tag = match value {
                    LoweredValue::Tag(value) => Some(value),
                    _ => None,
                };
                let mut matched = tag.is_some_and(|value| {
                    value.type_name == type_name && value.name.as_ref() == name.as_str()
                });
                let mut field_count = 0;
                for slot in field_slots {
                    let slot = slot.map_err(|error| indexed_error(error, span))?;
                    let Some(field) = tag.and_then(|tag| tag.fields.get(field_count)) else {
                        matched = false;
                        field_count += 1;
                        continue;
                    };
                    if matched && bind && let Some(slot) = slot {
                        slots[slot] = field.clone();
                    }
                    field_count += 1;
                }
                matched && tag.is_some_and(|tag| tag.fields.len() == field_count)
            }
        };
        Ok(matched)
    }

}
