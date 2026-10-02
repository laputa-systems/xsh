use super::*;

impl Evaluator {
    // Scalar expressions and calls recur together. Keeping their decoder
    // separate prevents each call from reserving the scratch space used by
    // unrelated host and container expressions.
    pub(super) fn eval_indexed_scalar_or_call_expr<'program>(
        &mut self,
        execution: &FullExecution<'program>,
        instruction: u32,
        slots: &mut [LoweredValue],
        call_span: Span,
        tag: FullTag,
        mut payload: FullPayload<'program>,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let result = match tag {
            FullTag::ExprNull => {
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Null)
            }
            FullTag::ExprUnit => {
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Unit)
            }
            FullTag::ExprInt => {
                let value = indexed_decode::<i64>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Int(value))
            }
            FullTag::ExprFloat => {
                let value = indexed_decode::<crate::runtime::value::FloatValue>(
                    &mut payload,
                    execution,
                    call_span,
                )?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Float(value))
            }
            FullTag::ExprDuration => {
                let value = indexed_decode::<DurationValue>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Duration(value))
            }
            FullTag::ExprBool => {
                let value = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Bool(value))
            }
            FullTag::ExprStr => {
                let value = indexed_decode::<Arc<str>>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Str(value))
            }
            FullTag::ExprPreparedConstant => {
                let value = indexed_decode::<crate::runtime::eval::PreparedConstantValue>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(value.0)
            }
            FullTag::ExprPreparedRegex => {
                let value = indexed_decode::<RegexValue>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Regex(Box::new(value)))
            }
            FullTag::ExprBytes => {
                let value = indexed_decode::<Arc<[u8]>>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Bytes(value))
            }
            FullTag::ExprPath => {
                let value = indexed_decode::<PathValue>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(LoweredValue::Path(value))
            }
            FullTag::ExprFunctionRef => {
                let function = indexed_decode::<FunctionName>(&mut payload, execution, call_span)?;
                let pure = indexed_decode::<bool>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                ControlFlow::Continue(match self.create_indexed_callable(execution, instruction, call_span)? {
                    Some(value) => value,
                    None if pure => LoweredValue::Pure(function),
                    None => LoweredValue::Proc(function),
                })
            }
            FullTag::ExprNativeCallableRef => {
                indexed_finish(payload, call_span)?;
                let id = execution.native_callable_value(instruction).map_err(|error| indexed_error(error, call_span))?;
                let program = Arc::clone(self.indexed_program.as_ref().ok_or_else(|| RuntimeError::new("indexed-ir", "native callable creation has no installed program").with_span(call_span))?);
                ControlFlow::Continue(crate::runtime::eval::RuntimeNativeCallableValue::new(program, id)
                    .map(LoweredValue::NativeCallable).map_err(|error| error.with_span(call_span))?)
            }
            FullTag::ExprParam => {
                let slot = indexed_decode::<usize>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                lowered_freeze_large_slot_list(&mut slots[slot]);
                ControlFlow::Continue(slots[slot].clone())
            }
            FullTag::ExprBinary => {
                let op = indexed_decode::<BinaryOp>(&mut payload, execution, call_span)?;
                let left = indexed_raw(&mut payload, call_span)?;
                let right = indexed_raw(&mut payload, call_span)?;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                if let Some(witness) = execution.requirement_witness(instruction)
                    .map_err(|error| indexed_error(error, span))?
                {
                    let RequirementWitness::Add { operation, .. } = witness else {
                        return Err(RuntimeError::new("indexed-ir", "binary instruction has projection evidence").with_span(span));
                    };
                    return self.eval_indexed_binary_stack(execution, slots, call_span, op, left, right, span, Some(operation));
                }
                if op == BinaryOp::And {
                    let left = match self.eval_indexed_bool(execution, left, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    if !left {
                        return Ok(ControlFlow::Continue(LoweredValue::Bool(false)));
                    }
                    return self
                        .eval_indexed_bool(execution, right, slots, span)
                        .map(|flow| flow.map_continue(LoweredValue::Bool));
                }
                if op == BinaryOp::Or {
                    let left = match self.eval_indexed_bool(execution, left, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    if left {
                        return Ok(ControlFlow::Continue(LoweredValue::Bool(true)));
                    }
                    return self
                        .eval_indexed_bool(execution, right, slots, span)
                        .map(|flow| flow.map_continue(LoweredValue::Bool));
                }
                return self
                    .eval_indexed_binary_stack(execution, slots, call_span, op, left, right, span, None);
            }
            FullTag::ExprCall => {
                let function =
                    indexed_decode::<LoweredFunctionKey>(&mut payload, execution, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = IndexedCallArguments::supplied(Vec::with_capacity(len));
                for _ in 0..len {
                    let kind = indexed_raw(&mut args, span)?;
                    let arg = indexed_raw(&mut args, span)?;
                    if kind == 2 {
                        values.omit(arg as usize, span)?;
                        continue;
                    }
                    let value = match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    match kind {
                        0 => values.values.push(value),
                        1 => values.values.extend(lowered_splice_arg_items(value, span)?),
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed call argument kind",
                            )
                            .with_span(span));
                        }
                    }
                }
                indexed_finish(args, span)?;
                return self
                    .eval_indexed_named_call_with_arguments(function, values, span,
                        execution.call_instantiation(instruction).map_err(|error| indexed_error(error, span))?)
                    .map(ControlFlow::Continue);
            }
            FullTag::ExprExternalCall => {
                let qualified =
                    indexed_decode::<QualifiedName>(&mut payload, execution, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = IndexedCallArguments::supplied(Vec::with_capacity(len));
                for _ in 0..len {
                    let kind = indexed_raw(&mut args, span)?;
                    let arg = indexed_raw(&mut args, span)?;
                    if kind == 2 {
                        values.omit(arg as usize, span)?; continue;
                    }
                    let value = match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    match kind {
                        0 => values.values.push(value),
                        1 => values.values.extend(lowered_splice_arg_items(value, span)?),
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed call argument kind",
                            )
                            .with_span(span));
                        }
                    }
                }
                indexed_finish(args, span)?;
                return self
                    .eval_indexed_external_call_with_arguments(qualified, values, span)
                    .map(ControlFlow::Continue);
            }
            FullTag::ExprDirectPureCall => {
                let function =
                    indexed_decode::<LoweredFunctionKey>(&mut payload, execution, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = IndexedCallArguments::supplied(Vec::with_capacity(len));
                for _ in 0..len {
                    let kind = indexed_raw(&mut args, span)?;
                    let arg = indexed_raw(&mut args, span)?;
                    if kind == 2 { values.omit(arg as usize, span)?; continue; }
                    let value = match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    match kind {
                        0 => values.values.push(value),
                        1 => values.values.extend(lowered_splice_arg_items(value, span)?),
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed call argument kind",
                            )
                            .with_span(span));
                        }
                    }
                }
                indexed_finish(args, span)?;
                let instantiation = execution.call_instantiation(instruction)
                    .map_err(|error| indexed_error(error, span))?;
                let result = if instantiation.is_some() || !values.omitted_parameters.is_empty() {
                    self.eval_indexed_named_call_with_arguments(function, values, span, instantiation)?
                } else if self.trace_enabled {
                    self.eval_indexed_named_call(function, &values.values, span)?
                } else {
                    self.eval_indexed_direct_pure_call(function, &values.values, span)?
                };
                return Ok(ControlFlow::Continue(result));
            }
            FullTag::ExprDynamicCall => {
                let callee = indexed_raw(&mut payload, call_span)?;
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let arg_count = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let callee = match self.eval_indexed_expr(execution, callee, slots, span)? {
                    ControlFlow::Continue(value) => value,
                    ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                };
                let mut values = IndexedCallArguments::supplied(Vec::with_capacity(arg_count));
                for _ in 0..arg_count {
                    let argument_kind = indexed_raw(&mut args, span)?;
                    if argument_kind == 2 {
                        let slot = indexed_raw(&mut args, span)? as usize;
                        values.omit(slot, span)?;
                        continue;
                    }
                    let splice = match argument_kind {
                        0 => false,
                        1 => true,
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed call argument tag",
                            )
                            .with_span(span));
                        }
                    };
                    let arg = indexed_raw(&mut args, span)?;
                    let value = match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => {
                            return Ok(ControlFlow::Break(value));
                        }
                    };
                    if splice {
                        values.values.extend(lowered_splice_arg_items(value, span)?);
                    } else {
                        values.values.push(value);
                    }
                }
                indexed_finish(args, span)?;
                if let Some(plan) = execution.native_invocation_plan(instruction).map_err(|error| indexed_error(error, span))? {
                    return self.eval_indexed_native_callable(execution, plan, &callee, values, span);
                }
                if let Some(plan) = execution.user_invocation_authority(instruction).map_err(|error| indexed_error(error, span))? {
                    let handle = self.checked_indexed_callable_authority(execution, plan, &callee, span)?;
                    let values = Self::prepared_indexed_callable_arguments(plan, &handle, values, span)?;
                    return self.eval_indexed_prepared_callable(handle, values, span).map(ControlFlow::Continue);
                }
                if matches!(callee, LoweredValue::Callable(_) | LoweredValue::NativeCallable(_)) {
                    return Err(RuntimeError::new("indexed-ir", "typed user invocation lacks its original prepared authority").with_span(span));
                }
                let (function, _) = indexed_callable_identity(&callee, span)?;
                let instantiation = execution.call_instantiation(instruction)
                    .map_err(|error| indexed_error(error, span))?;
                let result = if instantiation.is_some() {
                    self.eval_indexed_named_call_with_arguments(function, values, span, instantiation)?
                } else { match function {
                    LoweredFunctionKey::Name(_) => self.eval_indexed_named_call_with_arguments(function, values, span, None)?,
                    LoweredFunctionKey::Qualified(qualified) => self.eval_indexed_external_call_with_arguments(qualified, values, span)?,
                } };
                ControlFlow::Continue(result)
            }
            FullTag::ExprSelfCall => {
                let (_, mut args) = execution
                    .block(&mut payload, BLOCK_LIST)
                    .map_err(|error| indexed_error(error, call_span))?;
                let len = indexed_raw(&mut args, call_span)? as usize;
                let span = indexed_decode::<Span>(&mut payload, execution, call_span)?;
                indexed_finish(payload, call_span)?;
                let mut values = IndexedCallArguments::supplied(Vec::with_capacity(len));
                for _ in 0..len {
                    let kind = indexed_raw(&mut args, span)?;
                    let arg = indexed_raw(&mut args, span)?;
                    if kind == 2 { values.omit(arg as usize, span)?; continue; }
                    let value = match self.eval_indexed_expr(execution, arg, slots, span)? {
                        ControlFlow::Continue(value) => value,
                        ControlFlow::Break(value) => return Ok(ControlFlow::Break(value)),
                    };
                    match kind {
                        0 => values.values.push(value),
                        1 => values.values.extend(lowered_splice_arg_items(value, span)?),
                        _ => {
                            return Err(RuntimeError::new(
                                "indexed-ir",
                                "invalid indexed call argument kind",
                            )
                            .with_span(span));
                        }
                    }
                }
                indexed_finish(args, span)?;
                let (function, _) = execution
                    .function_identity()
                    .map_err(|error| indexed_error(error, span))?;
                let instantiation = execution.call_instantiation(instruction).map_err(|error| indexed_error(error, span))?;
                if instantiation.is_some() || !values.omitted_parameters.is_empty() {
                    return self.eval_indexed_named_call_with_arguments(function, values, span, instantiation).map(ControlFlow::Continue);
                }
                return self.eval_indexed_self_call(function, &values.values, span).map(ControlFlow::Continue);
            }
            _ => {
                return Err(RuntimeError::new(
                    "indexed-ir",
                    format!("direct indexed scalar and call evaluator does not support {tag:?}"),
                )
                .with_span(call_span));
            }
        };
        Ok(result)
    }
}
