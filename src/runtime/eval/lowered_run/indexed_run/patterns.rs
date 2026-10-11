use super::{
    Arc, BLOCK_LIST, Evaluator, FullExecution, FullPatternTag, FullPayload, LoweredValue, Name,
    RuntimeError, SmallVec, Span, StmtFlow, Type, Value, indexed_decode, indexed_error,
    indexed_finish, indexed_raw, indexed_string, lowered_error_value_has_facet,
    lowered_error_variant_matches, lowered_record_field, lowered_value_from_runtime_any,
    lowered_value_matches_static_type,
};

impl Evaluator {
    pub(super) fn decode_indexed_pattern_fields<'a>(
        payload: &mut FullPayload<'a>,
        execution: &FullExecution<'a>,
        span: Span,
    ) -> Result<Vec<(Name, u32)>, RuntimeError> {
        let (_, mut fields) = execution
            .block(payload, BLOCK_LIST)
            .map_err(|error| indexed_error(error, span))?;
        let count = indexed_raw(&mut fields, span)? as usize;
        let mut decoded = Vec::with_capacity(count);
        for _ in 0..count {
            decoded.push((
                indexed_decode::<Name>(&mut fields, execution, span)?,
                indexed_raw(&mut fields, span)?,
            ));
        }
        indexed_finish(fields, span)?;
        Ok(decoded)
    }

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
        let (tag, mut payload) = execution
            .pattern(pattern)
            .map_err(|error| indexed_error(error, span))?;
        let matched = match tag {
            FullPatternTag::TagType => {
                let type_name = indexed_decode::<Name>(&mut payload, execution, span)?;
                let variants = indexed_decode::<Vec<Name>>(&mut payload, execution, span)?;
                matches!(value, LoweredValue::Tag(value) if value.type_name == type_name && variants.iter().any(|name| value.name.as_ref() == name.as_str()))
            }
            FullPatternTag::RecordTest => {
                let fields = Self::decode_indexed_pattern_fields(&mut payload, execution, span)?;
                let mut matched =
                    matches!(value, LoweredValue::Record(_) | LoweredValue::RecordVec(_));
                for (name, pattern) in fields.iter() {
                    let Some(field) = lowered_record_field(value, &name.as_str()) else {
                        matched = false;
                        break;
                    };
                    if !Self::indexed_pattern_match_pass(
                        execution, *pattern, field, slots, span, bind,
                    )? {
                        matched = false;
                        break;
                    }
                }
                matched
            }
            FullPatternTag::ResultTest => {
                let ok = indexed_decode::<bool>(&mut payload, execution, span)?;
                let inner = indexed_raw(&mut payload, span)?;
                match value {
                    LoweredValue::ResultOk(value) if ok => Self::indexed_pattern_match_pass(
                        execution, inner, value, slots, span, bind,
                    )?,
                    LoweredValue::ResultErr(value) if !ok => {
                        if let Some(value) = lowered_value_from_runtime_any(value) {
                            Self::indexed_pattern_match_pass(
                                execution, inner, &value, slots, span, bind,
                            )?
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            }
            FullPatternTag::TagTest => {
                let type_name = indexed_decode::<Name>(&mut payload, execution, span)?;
                let name = indexed_decode::<Name>(&mut payload, execution, span)?;
                let (_, mut patterns) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let count = indexed_raw(&mut patterns, span)? as usize;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count {
                    fields.push(indexed_raw(&mut patterns, span)?);
                }
                indexed_finish(patterns, span)?;
                if let LoweredValue::Tag(value) = value {
                    let mut matched = value.type_name == type_name
                        && value.name.as_ref() == name.as_str()
                        && value.fields.len() == fields.len();
                    if matched {
                        for (pattern, value) in fields.iter().zip(&value.fields) {
                            if !Self::indexed_pattern_match_pass(
                                execution, *pattern, value, slots, span, bind,
                            )? {
                                matched = false;
                                break;
                            }
                        }
                    }
                    matched
                } else {
                    false
                }
            }
            FullPatternTag::ErrorTest => {
                let family = indexed_decode::<Name>(&mut payload, execution, span)?;
                let variant = indexed_decode::<Name>(&mut payload, execution, span)?;
                let fields = Self::decode_indexed_pattern_fields(&mut payload, execution, span)?;
                if let LoweredValue::Error(value) = value {
                    let error_fields = match value.as_ref() {
                        Value::Error(error)
                            if crate::runtime::value::error_family_matches(
                                error.family_name(),
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
                    };
                    if let Some(values) = error_fields {
                        let mut matched = true;
                        for (name, pattern) in fields.iter() {
                            let Some(value) = values
                                .get(&name.as_str())
                                .and_then(lowered_value_from_runtime_any)
                            else {
                                matched = false;
                                break;
                            };
                            if !Self::indexed_pattern_match_pass(
                                execution, *pattern, &value, slots, span, bind,
                            )? {
                                matched = false;
                                break;
                            }
                        }
                        matched
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            FullPatternTag::List => {
                let (_, mut patterns) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let count = indexed_raw(&mut patterns, span)? as usize;
                let mut elements = Vec::with_capacity(count);
                for _ in 0..count {
                    elements.push(indexed_raw(&mut patterns, span)?);
                }
                indexed_finish(patterns, span)?;
                let rest = if indexed_decode::<bool>(&mut payload, execution, span)? {
                    Some(indexed_raw(&mut payload, span)?)
                } else {
                    None
                };
                let items = match value {
                    LoweredValue::List(items) => Some(items.as_slice()),
                    LoweredValue::SharedList(items) => Some(items.as_slice()),
                    _ => None,
                };
                if let Some(items) = items {
                    let mut matched =
                        items.len() >= count && (rest.is_some() || items.len() == count);
                    if matched {
                        for (pattern, item) in elements.iter().zip(items) {
                            if !Self::indexed_pattern_match_pass(
                                execution, *pattern, item, slots, span, bind,
                            )? {
                                matched = false;
                                break;
                            }
                        }
                    }
                    if matched
                        && bind
                        && let Some(rest) = rest
                    {
                        let (tag, mut rest_payload) = execution
                            .pattern(rest)
                            .map_err(|error| indexed_error(error, span))?;
                        if tag == FullPatternTag::Bind {
                            let slot = indexed_decode::<usize>(&mut rest_payload, execution, span)?;
                            slots[slot] = LoweredValue::List(items[count..].to_vec());
                        }
                        indexed_finish(rest_payload, span)?;
                    }
                    matched
                } else {
                    false
                }
            }
            FullPatternTag::Alias => {
                let pattern = indexed_raw(&mut payload, span)?;
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                let matched =
                    Self::indexed_pattern_match_pass(execution, pattern, value, slots, span, bind)?;
                if matched && bind {
                    slots[slot] = value.clone();
                }
                matched
            }
            FullPatternTag::Alternation => {
                let (_, mut children) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let count = indexed_raw(&mut children, span)? as usize;
                let mut selected = None;
                for _ in 0..count {
                    let child = indexed_raw(&mut children, span)?;
                    if selected.is_none()
                        && Self::indexed_pattern_match_pass(
                            execution, child, value, slots, span, false,
                        )?
                    {
                        selected = Some(child);
                    }
                }
                indexed_finish(children, span)?;
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
            FullPatternTag::Wildcard => true,
            FullPatternTag::Text => {
                use crate::sema::check::{TextHoleKind, TextHoleValue, split_text};
                let (_, mut patterns) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, span))?;
                let count = indexed_raw(&mut patterns, span)? as usize;
                let mut holes = Vec::with_capacity(count);
                for _ in 0..count {
                    holes.push(indexed_raw(&mut patterns, span)?);
                }
                indexed_finish(patterns, span)?;
                let mut kinds = Vec::with_capacity(count);
                for _ in 0..count {
                    kinds.push(indexed_raw(&mut payload, span)? as usize);
                }
                let mut segments = Vec::with_capacity(count + 1);
                for _ in 0..=count {
                    segments.push(indexed_string(&mut payload, execution, span)?);
                }
                let subject = match value {
                    LoweredValue::Str(text) => Some(text.as_ref()),
                    LoweredValue::StrView(view) => Some(view.as_str()),
                    _ => None,
                };
                let texts = subject.and_then(|subject| split_text(&segments, subject));
                let mut matched = texts.is_some();
                // Every hole converts before any name is bound, and a hole
                // whose text is not a value of its kind fails the match.
                for ((hole, kind), text) in holes.iter().zip(&kinds).zip(texts.iter().flatten()) {
                    // The verifier checked each kind before the program ran.
                    let Some(kind) = TextHoleKind::from_index(*kind) else {
                        matched = false;
                        break;
                    };
                    let converted = match kind.convert(text) {
                        // Text needs no value unless a name takes it.
                        Some(TextHoleValue::Text(_)) if !bind => continue,
                        Some(TextHoleValue::Text(text)) => LoweredValue::Str(Arc::from(text)),
                        Some(TextHoleValue::Int(number)) => LoweredValue::Int(number),
                        Some(TextHoleValue::Float(number)) => {
                            LoweredValue::Float(crate::runtime::value::FloatValue::new(number))
                        }
                        None => {
                            matched = false;
                            break;
                        }
                    };
                    if !Self::indexed_pattern_match_pass(
                        execution, *hole, &converted, slots, span, bind,
                    )? {
                        matched = false;
                        break;
                    }
                }
                matched
            }
            FullPatternTag::Bind => {
                let slot = indexed_decode::<usize>(&mut payload, execution, span)?;
                if bind {
                    slots[slot] = value.clone();
                }
                true
            }
            FullPatternTag::Type => {
                let ty = indexed_decode::<Type>(&mut payload, execution, span)?;
                let slot = indexed_decode::<Option<usize>>(&mut payload, execution, span)?;
                if !lowered_value_matches_static_type(value, &ty) {
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
            FullPatternTag::Literal => {
                indexed_decode::<LoweredValue>(&mut payload, execution, span)? == *value
            }
            FullPatternTag::ResultOk => {
                let slot = indexed_decode::<Option<usize>>(&mut payload, execution, span)?;
                let unit_only = indexed_decode::<bool>(&mut payload, execution, span)?;
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
            FullPatternTag::ResultErr => {
                let slot = indexed_decode::<Option<usize>>(&mut payload, execution, span)?;
                let unit_only = indexed_decode::<bool>(&mut payload, execution, span)?;
                if let LoweredValue::ResultErr(inner) = value {
                    if unit_only && !matches!(inner.as_ref(), Value::Unit) {
                        false
                    } else if let Some(slot) = slot {
                        let Some(inner) = lowered_value_from_runtime_any(inner.as_ref()) else {
                            indexed_finish(payload, span)?;
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
            FullPatternTag::ErrorVariant => {
                let family = indexed_decode::<Name>(&mut payload, execution, span)?;
                let variant = indexed_decode::<Name>(&mut payload, execution, span)?;
                let fields = indexed_decode::<Box<SmallVec<[(Name, Option<usize>); 4]>>>(
                    &mut payload,
                    execution,
                    span,
                )?;
                let result_wrapped = indexed_decode::<bool>(&mut payload, execution, span)?;
                let error = if result_wrapped {
                    let LoweredValue::ResultErr(error) = value else {
                        indexed_finish(payload, span)?;
                        return Ok(false);
                    };
                    error.as_ref()
                } else {
                    let LoweredValue::Error(error) = value else {
                        indexed_finish(payload, span)?;
                        return Ok(false);
                    };
                    error.as_ref()
                };
                lowered_error_variant_matches(&family, &variant, &fields, error, slots, bind)
            }
            FullPatternTag::Facet => {
                let facet = indexed_decode::<Name>(&mut payload, execution, span)?;
                let result_wrapped = indexed_decode::<bool>(&mut payload, execution, span)?;
                let error = if result_wrapped {
                    let LoweredValue::ResultErr(error) = value else {
                        indexed_finish(payload, span)?;
                        return Ok(false);
                    };
                    error.as_ref()
                } else {
                    let LoweredValue::Error(error) = value else {
                        indexed_finish(payload, span)?;
                        return Ok(false);
                    };
                    error.as_ref()
                };
                lowered_error_value_has_facet(error, &facet.as_str())
            }
            FullPatternTag::Tag => {
                let type_name = indexed_decode::<Name>(&mut payload, execution, span)?;
                let name = indexed_decode::<Name>(&mut payload, execution, span)?;
                let field_count = indexed_raw(&mut payload, span)? as usize;
                let mut field_slots = SmallVec::<[Option<usize>; 2]>::with_capacity(field_count);
                for _ in 0..field_count {
                    field_slots.push(indexed_decode::<Option<usize>>(
                        &mut payload,
                        execution,
                        span,
                    )?);
                }
                let LoweredValue::Tag(value) = value else {
                    indexed_finish(payload, span)?;
                    return Ok(false);
                };
                if value.type_name != type_name
                    || value.name.as_ref() != name.as_str()
                    || value.fields.len() != field_slots.len()
                {
                    false
                } else {
                    for (slot, field) in field_slots.iter().zip(&value.fields) {
                        if let Some(slot) = slot
                            && bind
                        {
                            slots[*slot] = field.clone();
                        }
                    }
                    true
                }
            }
        };
        indexed_finish(payload, span)?;
        Ok(matched)
    }

    pub(super) fn finish_indexed_pattern_scope(
        &mut self,
        scope_id: u64,
        captures: &[usize],
        slots: &mut [LoweredValue],
        result: Result<StmtFlow, RuntimeError>,
    ) -> Result<StmtFlow, RuntimeError> {
        let parent_scope = self.parent_owned_host_scope();
        if let Ok(
            StmtFlow::Value(value)
            | StmtFlow::Return(value)
            | StmtFlow::Propagate(value)
            | StmtFlow::Break(Some(value)),
        ) = &result
        {
            self.transfer_owned_host_resources_in_value(
                &value.clone().into_value(),
                scope_id,
                parent_scope,
            );
        }
        if let Err(error) = &result
            && error.abort.is_none()
            && error.propagated
        {
            self.transfer_owned_host_resources_in_runtime_error(error, scope_id, parent_scope);
        }
        // Captures are iteration/branch locals. Retain escaping values before
        // releasing these references and the condition's temporary resources.
        for slot in captures {
            slots[*slot] = LoweredValue::Unit;
        }
        let cleanup = self.exit_owned_host_scope(scope_id);
        match (result, cleanup) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(flow), Ok(())) => Ok(flow),
        }
    }
}
