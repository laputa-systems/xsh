use super::*;
use super::super::generic::{GroundNativeCallContract, NativeCallSource, OperationSourceOrigin, PreparedGroundNativeCall, PreparedInvocationArgument, PreparedNativeReceiver, PreparedNativeReceiverTransport, PreparedOperationAuthority, PreparedOperationBinding, PreparedOperationEffects, graph_ground_type};
use crate::sema::inference::{EffectSummary, OperationBinding, RequirementTemplate, TypeNode};
use crate::sema::registry_graph::RegistryOwner;
use crate::modules::signature::{ImplBinding, SemanticRule};
mod result_record;
mod path_methods;
mod record_arguments;
mod argument_lineages;
mod bytes_methods;
mod map_methods;
mod hash_policy;
mod list_text_methods;
mod record_get;
#[cfg(test)]
mod record_get_tests;
#[cfg(test)]
mod regex_methods_tests;
mod regex_methods;
mod text_methods;
mod list_methods;
mod fs_children;
#[cfg(test)]
mod record_items_tests;

fn native_problem(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

fn native_receiver_type(graph: &crate::sema::inference::InferenceContext, ty: crate::sema::inference::TypeId, record_keys: bool) -> Result<Type, IrVerifyError> {
    if !record_keys { return graph_ground_type(graph, ty); }
    let actual = graph.export_type(ty).map_err(|_| IrVerifyError::new("record keys receiver requires its original closed descriptor"))?;
    if !matches!(actual, Type::Record(_) | Type::ErasedRecord | Type::Module(_) | Type::DynamicModule) {
        return Err(IrVerifyError::new("record keys receiver has another original domain"));
    }
    Ok(actual)
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildSavedNativeReceiverOrigin {
    pub call: crate::sema::check::ExpressionIdentity,
    pub origin: crate::sema::check::ExpressionIdentity,
    pub source_type: crate::sema::inference::ScopedRoot,
    pub initializer: BuildExprId,
    pub slot: usize,
    pub wrapper: Option<(BuildExprId, BuildPatternId)>,
}

fn encoded_native_arguments(store: &FullStore, instruction: u32, parameter_count: usize) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprModuleCall) { return Err(IrVerifyError::new("native call proof is attached to another opcode")); }
    let words = store.payload(store.data[instruction as usize].range())?;
    if words.len() != 4 || words[1] != 0 { return Err(IrVerifyError::new("native call descriptor protocol is not prepared")); }
    let operation = *store.runtime_ops.get(words[0] as usize).ok_or_else(|| IrVerifyError::new("native call operation is invalid"))?;
    let block = IrBlockId::from_raw(words[2]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("native call argument block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("native call argument block has another kind")); }
    let mut cursor = FullCursor::new(store.payload(block.instructions)?);
    let count = cursor.raw()? as usize;
    if count > parameter_count || parameter_count > 65536 { return Err(IrVerifyError::new("native call argument count exceeds its original signature")); }
    let mut sources = vec![None; parameter_count];
    for source in &mut sources[..count] {
        *source = match cursor.raw()? { 0 => None, 1 => Some(cursor.raw()?), _ => return Err(IrVerifyError::new("native call optional argument is invalid")) };
    }
    cursor.finish()?;
    Ok((operation, sources, words[3]))
}

fn encoded_native_method_arguments(store: &FullStore, instruction: u32, parameter_count: usize, method_name: Name) -> Result<(Vec<Option<u32>>, u32), IrVerifyError> {
    if store.tags.get(instruction as usize) == Some(&FullTag::ExprStrByteAt) {
        let words = store.payload(store.data[instruction as usize].range())?;
        if words.len() != 3 || parameter_count != 2 || method_name != Name::intern("byte_at") { return Err(IrVerifyError::new("byte lookup changes its original method or operand protocol")); }
        return Ok((vec![Some(words[0]), Some(words[1])], words[2]));
    }
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprMethod) { return Err(IrVerifyError::new("native method proof is attached to another opcode")); }
    let words = store.payload(store.data[instruction as usize].range())?;
    if words.len() != 4 || parameter_count == 0 || parameter_count > 65536 || store.string(words[1])? != method_name.as_str().as_str() { return Err(IrVerifyError::new("native method changes its original spelling or parameter protocol")); }
    let block = IrBlockId::from_raw(words[2]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("native method argument block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("native method argument block has another kind")); }
    let mut cursor = FullCursor::new(store.payload(block.instructions)?);
    if cursor.raw()? as usize != parameter_count - 1 { return Err(IrVerifyError::new("native method omitted arguments require their own prepared protocol")); }
    let mut sources = Vec::with_capacity(parameter_count);
    sources.push(Some(words[0]));
    for _ in 1..parameter_count { sources.push(Some(cursor.raw()?)); }
    cursor.finish()?;
    Ok((sources, words[3]))
}

impl FullBuilder {
    pub(super) fn stage_saved_native_receiver(&mut self, expression: BuildExprId, instruction: u32, owner: InstructionOwner, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        if let Some(original) = scratch.native_receiver_origins.get(&expression) {
            if self.store.tags.get(instruction as usize) != Some(&FullTag::ExprParam)
                || self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| native_problem("native_saved_receiver_read_payload"))? != [u32::try_from(original.slot).map_err(|_| native_problem("native_saved_receiver_slot_overflow"))?] {
                return Err(native_problem("native_saved_receiver_read_changed"));
            }
            let (wrapper, _) = original.wrapper.ok_or_else(|| native_problem("native_saved_receiver_wrapper_missing"))?;
            self.active_native_receiver_wrappers.entry(wrapper).or_default().push(self.saved_native_receiver_rows.len());
            self.saved_native_receiver_rows.push((original.clone(), instruction, owner, None));
        }
        if let Some(rows) = self.active_native_receiver_wrappers.remove(&expression) {
            let (initializer, pattern, body) = super::argument_prepare::saved_argument_wrapper(&self.store, instruction).map_err(|_| native_problem("native_saved_receiver_wrapper_changed"))?;
            for row in rows {
                let original = &self.saved_native_receiver_rows[row].0;
                if self.active_encoded_expressions.get(&original.initializer) != Some(&initializer) {
                    return Err(native_problem("native_saved_receiver_initializer_changed"));
                }
                self.saved_native_receiver_rows[row].3 = Some((instruction, initializer, pattern, body));
            }
        }
        Ok(())
    }

    pub(super) fn prepare_native_calls(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let origins = self.generic_expression_rows.iter().map(|&(instruction, expression, owner)| (instruction, (expression, owner))).collect::<FxHashMap<_, _>>();
        let mut saved_receivers = FxHashMap::default();
        for (original, read, owner, resolved) in &self.saved_native_receiver_rows {
            if saved_receivers.insert(*read, (original.clone(), *owner, *resolved)).is_some() { return Err(native_problem("native_saved_receiver_read_ambiguous")); }
        }
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            let path_read = matches!(self.store.tags[instruction as usize], FullTag::ExprPathReadText | FullTag::ExprPathReadBytes);
            let method = path_read || matches!(self.store.tags[instruction as usize], FullTag::ExprMethod | FullTag::ExprStrByteAt);
            let process_factory = self.store.tags[instruction as usize] == FullTag::ExprProcessCommandArgv;
            let fs_list = self.store.tags[instruction as usize] == FullTag::ExprFsList;
            if !method && !process_factory && !fs_list && self.store.tags[instruction as usize] != FullTag::ExprModuleCall { continue; }
            let Some(operation) = solved.operations.get(&expression) else {
                if process_factory { return Err(native_problem("native_process_original_operation_missing")); }
                continue;
            };
            let graph = &solved.graph;
            let Some(selected) = graph.candidate_evidence(operation.requirement).map_err(|_| native_problem("native_candidate_owner"))? else {
                if process_factory { return Err(native_problem("native_process_original_candidate_missing")); }
                continue;
            };
            let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| native_problem("native_candidate_authority"))? else {
                if process_factory { return Err(native_problem("native_process_original_registry_missing")); }
                continue;
            };
            if fs_list && (metadata.owner != RegistryOwner::Module("fs") || metadata.operation != RuntimeOp::FsChildren) {
                return Err(native_problem("native_fs_children_original_registry_contract_changed"));
            }
            let process_command_argv = if process_factory {
                if metadata.operation != RuntimeOp::ProcessCommandArgv { return Err(native_problem("native_process_original_selected_operation_changed")); }
                let Some(descriptor) = self.prepare_process_command_argv_descriptor(instruction)? else { return Err(native_problem("native_process_original_operand_packet_not_prepared")); };
                Some(descriptor)
            } else { None };
            let fs_root = super::super::generic::is_fs_root_method_owner(metadata.owner);
            let path_method = metadata.owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Path);
            let text_optional_tail = metadata.owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Str) && matches!(metadata.operation, RuntimeOp::TextSplit | RuntimeOp::TextByteSlice);
            let bytes_optional = metadata.owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Bytes) && bytes_methods::bytes_method_operation_is_supported(metadata.operation);
            let list_text = list_text_methods::list_text_method_operation_is_supported(metadata.owner, metadata.operation);
            let record_keys = list_text && metadata.operation == RuntimeOp::RecordKeys;
            let record_get = metadata.owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Record)
                && metadata.operation == RuntimeOp::RecordGet && metadata.semantic_rule == SemanticRule::ConstantKeyProjection
                && solved.record_get_projection(expression).map_err(|_| native_problem("record_get_original_refinement_changed"))?.is_some();
            let dynamic_cli = super::cli_call_prepare::original_dynamic_cli_candidate(&solved, expression, &metadata);
            let list_join = metadata.owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::List) && metadata.operation == RuntimeOp::TextJoin;
            let method = method || fs_root || path_method;
            if !(matches!(metadata.owner, RegistryOwner::Module(_)) && !method
                || method && match metadata.owner {
                    RegistryOwner::Method(crate::modules::signature::MethodReceiver::Record) => record_get || record_keys,
                    RegistryOwner::Method(crate::modules::signature::MethodReceiver::Map) => matches!(metadata.operation, RuntimeOp::MapKeys | RuntimeOp::MapValues | RuntimeOp::MapGet | RuntimeOp::MapSet | RuntimeOp::MapPush | RuntimeOp::MapLen),
                    RegistryOwner::Method(crate::modules::signature::MethodReceiver::List) => matches!(metadata.operation, RuntimeOp::ListGet | RuntimeOp::ListPush | RuntimeOp::ListExtend | RuntimeOp::ListLen | RuntimeOp::StreamCollect | RuntimeOp::TextJoin),
                    RegistryOwner::Method(crate::modules::signature::MethodReceiver::Str) => text_optional_tail || super::super::native_methods::nondefault_method_spelling(crate::modules::signature::MethodReceiver::Str, metadata.operation).is_some(),
                    RegistryOwner::Method(crate::modules::signature::MethodReceiver::Bytes) => bytes_optional || list_text || super::super::native_methods::nondefault_method_spelling(crate::modules::signature::MethodReceiver::Bytes, metadata.operation).is_some(),
                    RegistryOwner::Method(crate::modules::signature::MethodReceiver::Stream) => metadata.operation == RuntimeOp::StreamCollect,
                    RegistryOwner::Method(crate::modules::signature::MethodReceiver::Digest) => matches!(metadata.operation, RuntimeOp::DigestHex | RuntimeOp::DigestBase64),
                    RegistryOwner::Method(crate::modules::signature::MethodReceiver::Regex) => regex_methods::regex_method_operation_is_supported(metadata.operation),
                    RegistryOwner::Method(crate::modules::signature::MethodReceiver::Path) => path_read || self.store.tags[instruction as usize] == FullTag::ExprModuleCall || path_methods::path_method_operation_is_supported(metadata.operation),
                    _ => fs_root,
                })
                || metadata.binding != ImplBinding::Native
                || !(metadata.semantic_rule == SemanticRule::Standard || record_get || dynamic_cli) {
                if process_factory { return Err(native_problem("native_process_original_registry_contract_not_prepared")); }
                continue;
            }
            // A Path method operand list has no representation for omitted slots.
            // Module packets retain omission markers for the selected defaults.
            if path_method && self.store.tags[instruction as usize] == FullTag::ExprMethod && !operation.binding.default_slots.is_empty() { continue; }
            let original_scope = solved.expression_scope(expression, operation.caller).map_err(|_| native_problem("native_original_requirement_scope"))?;
            graph.validate_requirement_scoped(crate::sema::inference::ScopedRequirementRoot { requirement: operation.requirement, scope: original_scope }).map_err(|_| native_problem("native_original_requirement_certificate"))?;
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| native_problem("native_requirement_owner"))? else { return Err(native_problem("native_requirement_kind")); };
            let call = graph.operation_call(call).map_err(|_| native_problem("native_call_owner"))?;
            if call.binding != OperationBinding::Slots || call.receiver.is_some() != method || operation.receiver.is_some() != method
                || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
                || !operation.argument_coercions.is_empty() || !selected.callback_invocations.is_empty() {
                if process_factory { return Err(native_problem("native_process_original_binding_not_prepared")); }
                continue;
            }
            let signature = graph.resolved(selected.signature).map_err(|_| native_problem("native_signature_owner"))?;
            let TypeNode::Arrow(arrow) = graph.node(signature).map_err(|_| native_problem("native_signature_owner"))? else { return Err(native_problem("native_signature_kind")); };
            let offset = usize::from(method);
            if arrow.kind != metadata.kind || arrow.params.len() != metadata.parameters.len() + offset
                || method && ((!fs_root && !path_method && !text_optional_tail && !list_join && !bytes_optional && !list_text && arrow.params.iter().any(|parameter| parameter.defaulted || parameter.rest))
                    || arrow.params[0].defaulted || arrow.params[0].rest || arrow.params[0].label != Name::intern("<receiver>"))
                || arrow.params[offset..].iter().zip(&metadata.parameters).any(|(formal, original)| formal.label != original.label || formal.defaulted != original.defaulted || formal.rest) {
                return Err(native_problem("native_original_parameter_contract"));
            }
            // A closed native result does not make unresolved operands ground.
            // Such a call needs its own scoped argument protocol.
            if arrow.params.iter().map(|parameter| parameter.ty).chain(std::iter::once(selected.result))
                .chain(operation.actual_arguments.iter().copied()).chain(operation.receiver).any(|ty| graph_ground_type(graph, ty).is_err()) { continue; }
            let closed = |summary| match graph.closed_effect_summary(summary).map_err(|_| native_problem("native_effect_owner"))? {
                EffectSummary::Closed(bits) => Ok(bits), _ => Err(native_problem("native_effect_scope_not_prepared")),
            };
            let effects = PreparedOperationEffects {
                creation: closed(selected.effects)?,
                inputs: call.effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
                outputs: call.output_effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
            };
            let (kind, signature) = self.intern_checked_callable_signature(graph, signature)?;
            let result_type = graph_ground_type(graph, selected.result).map_err(|_| native_problem("native_result_scope"))?;
            let result = self.intern_generic_ground_type(&result_type)?;
            if self.store.semantic.signature_return_type(signature).map_err(|_| native_problem("native_signature_result"))? != result {
                return Err(native_problem("native_original_result_contract"));
            }
            let original_result = solved.expressions.get(&expression).copied().ok_or_else(|| native_problem("native_original_result_missing"))?;
            let original_result_type = graph_ground_type(graph, original_result).map_err(|_| native_problem("native_original_result_scope"))?;
            let (encoded_operation, argument_sources, location) = if process_factory {
                super::process_prepare::encoded_process_command_argv_arguments(&self.store, instruction, arrow.params.len()).map_err(|_| native_problem("native_encoded_process_arguments"))?
            } else if fs_list {
                fs_children::encoded_fs_children_arguments(&self.store, instruction, arrow.params.len()).map_err(|cause| IrBuildError::verification("native_encoded_fs_children_arguments", cause))?
            } else if fs_root {
                super::fs_root_prepare::encoded_fs_root_method_arguments(&self.store, instruction, arrow.params.len()).map_err(|_| native_problem("native_encoded_fs_root_arguments"))?
            } else if path_method {
                path_methods::encoded_path_method_arguments(&self.store, instruction, arrow.params.len(), Name::intern(metadata.entry), metadata.operation).map_err(|cause| IrBuildError::verification("native_encoded_path_method_arguments", cause))?
            } else if list_text {
                list_text_methods::encoded_list_text_method_arguments(&self.store, instruction, arrow.params.len(), metadata.operation).map_err(|cause| IrBuildError::verification("native_encoded_list_text_arguments", cause))?
            } else if bytes_optional {
                bytes_methods::encoded_bytes_method_arguments(&self.store, instruction, arrow.params.len(), metadata.operation).map_err(|cause| IrBuildError::verification("native_encoded_bytes_method_arguments", cause))?
            } else if text_optional_tail {
                text_methods::encoded_text_method_arguments(&self.store, instruction, arrow.params.len(), metadata.operation).map_err(|cause| IrBuildError::verification("native_encoded_text_method_arguments", cause))?
            } else if list_join {
                list_methods::encoded_list_join_arguments(&self.store, instruction, arrow.params.len()).map_err(|cause| IrBuildError::verification("native_encoded_list_join_arguments", cause))?
            } else if method {
                let (arguments, location) = encoded_native_method_arguments(&self.store, instruction, arrow.params.len(), Name::intern(metadata.entry)).map_err(|_| native_problem("native_encoded_method_arguments"))?;
                (metadata.operation, arguments, location)
            } else { encoded_native_arguments(&self.store, instruction, arrow.params.len()).map_err(|_| native_problem("native_encoded_arguments"))? };
            if encoded_operation != metadata.operation || IrLocationId::from_raw(location).and_then(|location| self.store.location_sources.get(location.index())) != Some(&expression.source) { return Err(native_problem("native_original_opcode_or_source")); }
            let receiver = if method {
                let instruction = argument_sources[0].ok_or_else(|| native_problem("native_method_receiver_missing"))?;
                let (material_source, material_wrappers) = self.argument_initializer_lineage(instruction, owner)?;
                let (origin, receiver_owner, source_instruction, source_wrappers, saved, guarded, postfix) = if let Some(&(origin, receiver_owner)) = origins.get(&material_source) {
                    (origin, receiver_owner, material_source, material_wrappers, None, None, None)
                } else if let Some(guard) = self.optional_receiver_guard(instruction).cloned() {
                    let (source, wrappers) = self.argument_initializer_lineage(guard.carrier, owner)?;
                    if guard.call != expression || guard.owner != owner || origins.get(&source) != Some(&(guard.origin, owner)) { return Err(native_problem("native_guarded_receiver_original_source")); }
                    (guard.origin, owner, source, wrappers, None, Some(guard), None)
                } else if let Some(postfix) = self.prepare_result_receiver(instruction, owner,
                    &native_receiver_type(graph, operation.receiver.ok_or_else(|| native_problem("native_postfix_selected_receiver_missing"))?, record_keys).map_err(|_| native_problem("native_postfix_selected_receiver_scope"))?, expression)? {
                    (postfix.origin, postfix.owner, postfix.source_instruction, postfix.source_wrappers.clone(), None, None, Some(postfix))
                } else {
                    let (original, receiver_owner, resolved) = saved_receivers.get(&instruction).ok_or_else(|| {
                        let mut nearby = origins.iter().filter(|(source, _)| source.abs_diff(material_source) <= 16)
                            .map(|(&source, &(origin, owner))| (source, self.store.tags.get(source as usize), origin, owner)).collect::<Vec<_>>();
                        nearby.sort_unstable_by_key(|row| row.0);
                        let mut reads = saved_receivers.keys().copied().collect::<Vec<_>>();
                        reads.sort_unstable();
                        IrBuildError::verification("native_method_original_receiver_missing", IrVerifyError::new(format!(
                            "selected {:?} owner {:?} entry {} call {expression:?}; receiver {instruction} ({:?}) in {owner:?}, material source {material_source} ({:?}), wrappers {material_wrappers:?}; material payload {:?}; authentic origins within 16 instructions {nearby:?}; saved receiver reads {reads:?}",
                            metadata.operation, metadata.owner, metadata.entry, self.store.tags.get(instruction as usize),
                            self.store.tags.get(material_source as usize), self.store.payload(self.store.data[material_source as usize].range()),
                        )))
                    })?;
                    if original.call != expression { return Err(native_problem("native_saved_receiver_original_call_changed")); }
                    let (wrapper, initializer, pattern, body) = resolved.ok_or_else(|| native_problem("native_saved_receiver_wrapper_missing"))?;
                    graph.validate_scoped(original.source_type).map_err(|_| native_problem("native_saved_receiver_original_certificate"))?;
                    if solved.expressions.get(&original.origin) != Some(&original.source_type.ty)
                        || original.source_type.scope != solved.expression_scope(original.origin, operation.caller).map_err(|_| native_problem("native_saved_receiver_original_scope"))? {
                        return Err(native_problem("native_saved_receiver_original_type_changed"));
                    }
                    let guarded = self.optional_receiver_guard(initializer).cloned();
                    let (initializer_source_instruction, initializer_wrappers) = if let Some(guard) = &guarded {
                        if guard.call != expression || guard.origin != original.origin || guard.owner != *receiver_owner { return Err(native_problem("native_guarded_receiver_original_saved_source")); }
                        self.argument_initializer_lineage(guard.carrier, *receiver_owner)?
                    } else if fs_root {
                        let source_type = graph_ground_type(graph, original.source_type.ty).map_err(|_| native_problem("native_saved_receiver_original_source_type"))?;
                        self.fs_root_receiver_lineage(initializer, *receiver_owner, &source_type)?
                    } else { self.argument_initializer_lineage(initializer, *receiver_owner)? };
                    if origins.get(&initializer_source_instruction) != Some(&(original.origin, *receiver_owner)) { return Err(native_problem("native_saved_receiver_original_initializer_missing")); }
                    let slot = u32::try_from(original.slot).map_err(|_| native_problem("native_saved_receiver_slot_overflow"))?;
                    FullVerifier::verify_compiler_argument_wrapper(&self.store, wrapper, initializer, pattern, body, slot).map_err(|_| native_problem("native_saved_receiver_original_allocation"))?;
                    (original.origin, *receiver_owner, initializer_source_instruction, initializer_wrappers.clone(), Some(PreparedNativeReceiverTransport { initializer, guarded_read: guarded.as_ref().map(|guard| guard.read), initializer_source_instruction, initializer_wrappers, wrapper, pattern, body, slot }), guarded, None)
                };
                if receiver_owner != owner || solved.expression_owners.get(&origin).copied() != operation.caller { return Err(native_problem("native_method_original_receiver_owner")); }
                let original = *solved.expressions.get(&origin).ok_or_else(|| native_problem("native_method_original_receiver_type"))?;
                let source_type = native_receiver_type(graph, original, record_keys).map_err(|_| native_problem("native_method_original_receiver_scope"))?;
                let actual = if let Some(guard) = &guarded {
                    let TypeRef::Ground(source) = guard.source_type else { return Err(native_problem("native_guarded_receiver_source_requires_scope")); };
                    let TypeRef::Ground(success) = guard.success_type else { return Err(native_problem("native_guarded_receiver_success_requires_scope")); };
                    let actual = self.store.semantic.to_type(success).map_err(|_| native_problem("native_guarded_receiver_success_type"))?;
                    if self.store.semantic.to_type(source).map_err(|_| native_problem("native_guarded_receiver_source_type"))? != source_type
                        || !matches!(&source_type, Type::Optional(inner) if inner.as_ref() == &actual
                            && (metadata.owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Map) && metadata.operation == RuntimeOp::MapSet && matches!(actual, Type::Map(_, _))
                                || fs_root && actual == Type::FsRoot)) { return Err(native_problem("native_guarded_receiver_original_narrowing")); }
                    actual
                } else if let Some(postfix) = &postfix {
                    let (TypeRef::Ground(carrier), TypeRef::Ground(success)) = (postfix.source_type, postfix.success_type) else { return Err(native_problem("native_postfix_receiver_requires_closed_type")); };
                    if self.store.semantic.to_type(carrier).map_err(|_| native_problem("native_postfix_receiver_carrier_type"))? != source_type {
                        return Err(native_problem("native_postfix_receiver_original_carrier_changed"));
                    }
                    self.store.semantic.to_type(success).map_err(|_| native_problem("native_postfix_receiver_success_type"))?
                } else if fs_root {
                    super::fs_root_prepare::fs_root_receiver_type(source_type.clone(), saved.as_ref()).map_err(|_| native_problem("native_method_original_fs_root_receiver"))?
                } else { source_type.clone() };
                if path_method && actual != Type::Path { return Err(native_problem("native_method_original_path_receiver")); }
                if metadata.operation == RuntimeOp::MapKeys
                    && !matches!(&actual, Type::Map(key, _) if matches!(key.as_ref(), Type::Int | Type::UInt | Type::Str)) {
                    continue;
                }
                let formal = graph_ground_type(graph, arrow.params[0].ty).map_err(|_| native_problem("native_method_receiver_signature_scope"))?;
                let relation = graph.candidate(selected.candidate).map_err(|_| native_problem("native_method_receiver_relation"))?.argument_relations.first().copied().unwrap_or(crate::sema::inference::ArgumentRelation::Assignable);
                let erased_keys = record_keys && list_text_methods::record_keys_receiver_accepts(&formal, &actual, relation);
                let selected_receiver = if erased_keys && matches!(actual, Type::Module(_) | Type::DynamicModule) { &formal } else { &actual };
                if &native_receiver_type(graph, call.receiver.unwrap(), record_keys).map_err(|_| native_problem("native_method_receiver_scope"))? != selected_receiver
                    || &native_receiver_type(graph, operation.receiver.unwrap(), record_keys).map_err(|_| native_problem("native_method_receiver_scope"))? != selected_receiver
                    || !record_get && !erased_keys && formal != actual { return Err(native_problem("native_method_original_receiver_changed")); }
                let ty = TypeRef::Ground(self.intern_generic_ground_type(&actual)?);
                let source_type = TypeRef::Ground(self.intern_generic_ground_type(&source_type)?);
                Some(PreparedNativeReceiver { origin, instruction, source_instruction, source_wrappers, ty, source_type, method_name: Name::intern(metadata.entry), saved, postfix })
            } else { None };
            if original_result_type != result_type && !record_get {
                let guard = receiver.as_ref().and_then(|receiver| receiver.saved.as_ref().and_then(|saved| saved.guarded_read).or(Some(receiver.instruction)))
                    .and_then(|read| self.optional_receiver_guard(read)).ok_or_else(|| native_problem("native_original_result_changed"))?;
                let (TypeRef::Ground(source), TypeRef::Ground(result)) = (guard.call_source_type, guard.call_result_type) else { return Err(native_problem("native_guarded_call_result_requires_scope")); };
                if guard.call != expression || self.store.semantic.to_type(source).map_err(|_| native_problem("native_guarded_call_source_type"))? != original_result_type
                    || self.store.semantic.to_type(result).map_err(|_| native_problem("native_guarded_call_result_type"))? != result_type { return Err(native_problem("native_guarded_call_original_result_changed")); }
            }
            let recipes = solved.argument_sources.get(&expression).ok_or_else(|| native_problem("native_original_recipes_missing"))?;
            if recipes.len() != operation.binding.supplied_slots.len() || recipes.len() != operation.actual_arguments.len() || selected.actual_arguments.len() + offset != arrow.params.len() { return Err(native_problem("native_original_binding_count")); }
            let mut arguments = Vec::with_capacity(recipes.len());
            let mut record_arguments = Vec::new();
            let mut operands = Vec::with_capacity(recipes.len());
            for (ordinal, ((recipe, &slot), &checked)) in recipes.iter().zip(&operation.binding.supplied_slots).zip(&operation.actual_arguments).enumerate() {
                let argument = argument_sources.get(slot + offset).copied().flatten().ok_or_else(|| native_problem("native_supplied_operand_missing"))?;
                let original = match recipe.value {
                    crate::sema::arguments::ArgumentValueSource::Expression(value) => {
                        let actual = self.original_argument_expression(argument, expression, ordinal, recipe, owner)?;
                        if actual != (crate::sema::check::ExpressionIdentity { expression: value, ..expression }) { return Err(native_problem("native_operand_original_source_changed")); }
                        let original = solved.expressions.get(&actual).copied().ok_or_else(|| native_problem("native_operand_original_type_missing"))?;
                        graph_ground_type(graph, original).map_err(|_| native_problem("native_operand_original_type_scope"))?
                    }
                    crate::sema::arguments::ArgumentValueSource::RecordField { .. } => {
                        let (receipt, actual) = self.prepare_native_record_argument(expression, ordinal, slot + offset, recipe, argument, owner)?;
                        record_arguments.push(receipt);
                        actual
                    }
                    _ => return Err(native_problem("native_argument_recipe_not_prepared")),
                };
                let selected_argument = selected.actual_arguments.get(slot).copied().flatten().ok_or_else(|| native_problem("native_selected_operand_missing"))?;
                if graph_ground_type(graph, checked).map_err(|_| native_problem("native_operand_type_scope"))? != original
                    || graph_ground_type(graph, selected_argument).map_err(|_| native_problem("native_selected_operand_scope"))? != original { return Err(native_problem("native_original_operand_type_changed")); }
                let ty = TypeRef::Ground(self.intern_generic_ground_type(&original)?);
                arguments.push(PreparedInvocationArgument { original: recipe.clone(), instruction: argument, ty });
                operands.push(argument);
            }
            for (slot, argument) in argument_sources[offset..].iter().enumerate() {
                if argument.is_some() != operation.binding.supplied_slots.contains(&slot)
                    || argument.is_none() != operation.binding.default_slots.contains(&slot)
                    || argument.is_none() != selected.actual_arguments[slot].is_none() { return Err(native_problem("native_encoded_binding_changed")); }
            }
            let slots = |slots: &[usize]| slots.iter().map(|&slot| u32::try_from(slot + offset).map_err(|_| native_problem("native_slot_overflow"))).collect::<Result<Box<[_]>, _>>();
            let template = graph.candidate(selected.candidate).map_err(|_| native_problem("native_original_candidate_template"))?;
            let supplied_slots = slots(&operation.binding.supplied_slots)?;
            let argument_lineages = self.prepare_native_argument_lineages(&mut arguments, owner, signature, &supplied_slots)?;
            let result_refinement = self.prepare_record_get_refinement(&solved, expression, &receiver, &arguments, selected.result, original_result, owner)?;
            let contract = GroundNativeCallContract {
                authority: PreparedOperationAuthority::Registry { identity: metadata.identity, operation: metadata.operation, binding: metadata.binding,
                    argument_check: metadata.argument_check, semantic_rule: metadata.semantic_rule, lifecycle: metadata.lifecycle, producer_transfer: metadata.producer_transfer.clone() },
                registry_owner: metadata.owner, signature, kind, result: TypeRef::Ground(result), effects, argument_relations: template.argument_relations.clone().into_boxed_slice(), input_eligibility: template.actual_eligibility.clone().into_boxed_slice(), arguments: arguments.into_boxed_slice(),
                binding: PreparedOperationBinding { supplied_slots, default_slots: slots(&operation.binding.default_slots)?, rest_slot: None, dynamic: None, operands: operands.into_boxed_slice() },
                argument_sources: argument_sources.into_boxed_slice(), receiver, cli_descriptor: None, process_command_argv,
            };
            regex_methods::verify_regex_method_contract(&contract, &self.store.semantic).map_err(|error| IrBuildError::verification("native_regex_original_contract", error))?;
            list_text_methods::verify_list_text_method_contract(&contract, &self.store.semantic).map_err(|error| IrBuildError::verification("native_list_text_original_contract", error))?;
            let scope = operation.caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let result_record_layout = self.prepare_native_result_record(if result_refinement.is_some() { &original_result_type } else { &result_type })?;
            let source = self.generic_evidence_mut().add_native_call_source(NativeCallSource { origin: expression, instruction, owner, scope, expected: contract.clone(), result_record_layout, record_arguments: record_arguments.into_boxed_slice(), argument_lineages, result_refinement }).map_err(|_| native_problem("native_source_allocation"))?;
            self.generic_evidence_mut().add_ground_native_call(PreparedGroundNativeCall { source, contract }).map_err(|_| native_problem("native_proof_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(in crate::runtime::eval::indexed) fn record_keys_receiver_accepts(formal: &Type, actual: &Type, relation: crate::sema::inference::ArgumentRelation) -> bool {
        list_text_methods::record_keys_receiver_accepts(formal, actual, relation)
    }
    pub(super) fn verify_native_nominal_source(store: &FullStore, source: u32, expected: &Type) -> Result<bool, IrVerifyError> {
        let Type::Tag(expected) = expected else { return Ok(false); };
        let words = store.payload(store.data[source as usize].range())?;
        let (owner, variant, mapping) = match store.tags[source as usize] {
            FullTag::ExprTag => {
                if words.len() != 5 { return Err(IrVerifyError::new("native nominal constructor payload is invalid")); }
                let owner = Name::from_symbol(Symbol::from_raw(words[0]));
                let variant = Name::intern(store.string(words[1])?);
                let fields = IrBlockId::from_raw(words[2]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("native nominal constructor fields are invalid"))?;
                if fields.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || store.payload(fields.instructions)? != [0] {
                    return Err(IrVerifyError::new("native nominal constructor payload fields require their own prepared proof"));
                }
                if words[3] != 1 { return Err(IrVerifyError::new("native nominal constructor lacks its wire authority")); }
                let mapping = store.wire_enums.get(words[4] as usize).ok_or_else(|| IrVerifyError::new("native nominal constructor wire authority is invalid"))?;
                (owner, variant, mapping)
            }
            FullTag::ExprPreparedConstant => {
                if words.len() != 1 { return Err(IrVerifyError::new("native nominal constant payload is invalid")); }
                let value = store.prepared_constants.get(words[0] as usize).ok_or_else(|| IrVerifyError::new("native nominal constant is invalid"))?;
                let LoweredValue::Tag(tag) = &value.0 else { return Err(IrVerifyError::new("native nominal constant changes its value kind")); };
                if !tag.fields.is_empty() { return Err(IrVerifyError::new("native nominal constant payload fields require their own prepared proof")); }
                let mapping = tag.wire.as_ref().ok_or_else(|| IrVerifyError::new("native nominal constant lacks its wire authority"))?;
                (tag.type_name, Name::intern(tag.name.as_ref()), mapping)
            }
            _ => return Ok(false),
        };
        if owner != *expected || mapping.type_name != owner || !mapping.variants.contains_key(&variant)
            || mapping.variants.is_empty() || mapping.variants.values().collect::<std::collections::BTreeSet<_>>().len() != mapping.variants.len()
            || !wire_mapping_matches_pool(mapping, store) {
            return Err(IrVerifyError::new("native nominal operand changes its declaring owner or wire authority"));
        }
        Ok(true)
    }

    pub(super) fn native_call_result(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner) -> Result<Type, IrVerifyError> {
        let id = generic.ground_native_call_at(instruction)?.ok_or_else(|| IrVerifyError::new("original native call lacks its prepared proof"))?;
        let proof = generic.ground_native_call(id)?;
        let source = generic.native_call_source(proof.source)?;
        if source.instruction != instruction || source.owner != owner
            || source.expected != proof.contract
            || generic.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), owner)) { return Err(IrVerifyError::new("native call changes its original instruction or owner")); }
        let PreparedOperationAuthority::Registry { operation, .. } = proof.contract.authority else { return Err(IrVerifyError::new("native call lacks its selected registry authority")); };
        source.verify_result_record_layout(&store.semantic)?;
        regex_methods::verify_regex_method_contract(&proof.contract, &store.semantic)?;
        bytes_methods::verify_bytes_method_contract(&proof.contract, &store.semantic)?;
        map_methods::verify_map_push_contract(&proof.contract, &store.semantic)?;
        list_text_methods::verify_list_text_method_contract(&proof.contract, &store.semantic)?;
        Self::verify_native_record_argument_packets(store, generic, source)?;
        if proof.contract.cli_descriptor.is_none() { Self::verify_native_argument_lineages(store, generic, source)?; }
        Self::verify_record_get_key(store, generic, source)?;
        let count = store.semantic.signature_param_count(proof.contract.signature)?;
        let (encoded, arguments, location) = if let Some(process) = &proof.contract.process_command_argv {
            Self::verify_process_command_argv_descriptor(store, instruction, process)?;
            super::process_prepare::encoded_process_command_argv_arguments(store, instruction, count)?
        } else if store.tags.get(instruction as usize) == Some(&FullTag::ExprFsList) {
            if proof.contract.registry_owner != RegistryOwner::Module("fs") || operation != RuntimeOp::FsChildren { return Err(IrVerifyError::new("filesystem children changes its original registry owner or operation")); }
            fs_children::encoded_fs_children_arguments(store, instruction, count)?
        } else if let Some(cli) = &proof.contract.cli_descriptor {
            if source.expected != proof.contract || !proof.contract.verify_cli_descriptor(&store.semantic)? {
                return Err(IrVerifyError::new("CLI call changes its original prepared contract"));
            }
            Self::verify_cli_descriptor_rows(store, cli)?;
            super::cli_call_prepare::encoded_cli_arguments(store, instruction, count, &cli.plan)?
        } else if let Some(receiver) = &proof.contract.receiver {
            let TypeRef::Ground(source_type) = receiver.source_type else { return Err(IrVerifyError::new("native receiver source requires a scoped protocol")); };
            let TypeRef::Ground(receiver_type) = receiver.ty else { return Err(IrVerifyError::new("native receiver requires a scoped protocol")); };
            let original_type = store.semantic.to_type(source_type)?;
            let guarded_read = receiver.saved.as_ref().and_then(|saved| saved.guarded_read).unwrap_or(receiver.instruction);
            let guard = Self::verify_optional_receiver_guard(store, generic, guarded_read, owner)?;
            let actual_type = if let Some(guard) = guard {
                let (TypeRef::Ground(original), TypeRef::Ground(success)) = (guard.source_type, guard.success_type) else { return Err(IrVerifyError::new("guarded native receiver lacks closed source and success types")); };
                let actual = store.semantic.to_type(success)?;
                if guard.call != source.origin || guard.origin != receiver.origin || store.semantic.to_type(original)? != original_type
                    || !matches!(&original_type, Type::Optional(inner) if inner.as_ref() == &actual
                        && (proof.contract.registry_owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Map)
                                && matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::MapSet, .. }) && matches!(actual, Type::Map(_, _))
                            || super::super::generic::is_fs_root_method_owner(proof.contract.registry_owner) && actual == Type::FsRoot))
                    || guard.call_result_type != proof.contract.result { return Err(IrVerifyError::new("guarded native receiver changes its original optional narrowing or selected result")); }
                actual
            } else if let Some(postfix) = &receiver.postfix {
                if receiver.saved.is_some() || postfix.owner != owner || postfix.origin != receiver.origin
                    || postfix.instruction != receiver.instruction || postfix.source_instruction != receiver.source_instruction
                    || postfix.source_wrappers != receiver.source_wrappers || postfix.source_type != receiver.source_type
                    || postfix.success_type != receiver.ty {
                    return Err(IrVerifyError::new("native method changes its original Result postfix receiver boundary"));
                }
                Self::verify_result_receiver(store, generic, postfix, source.origin, &store.semantic.to_type(receiver_type)?, &mut vec![])?
            } else if proof.contract.verify_fs_root_method(&store.semantic)? {
                super::fs_root_prepare::fs_root_receiver_type(original_type, receiver.saved.as_ref())?
            } else { original_type };
            if proof.contract.registry_owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Path) && actual_type != Type::Path { return Err(IrVerifyError::new("Path method changes its original hidden receiver domain")); }
            if actual_type != store.semantic.to_type(receiver_type)? { return Err(IrVerifyError::new("native receiver changes its original type boundary")); }
            let original_instruction = receiver.source_instruction;
            if generic.registered_instruction_origin(original_instruction, false) != Some((OperationSourceOrigin::Expression(receiver.origin), owner)) {
                return Err(IrVerifyError::new("native method changes its original receiver source or owner"));
            }
            if let Some(saved) = &receiver.saved {
                if generic.registered_instruction_origin(receiver.instruction, false).is_some()
                    || store.tags.get(receiver.instruction as usize) != Some(&FullTag::ExprParam)
                    || store.payload(store.data[receiver.instruction as usize].range())? != [saved.slot]
                    || Self::original_compiler_argument_wrapper_body(store, generic, saved.wrapper, owner)? != Some(saved.body) {
                    return Err(IrVerifyError::new("native method changes its original saved receiver allocation"));
                }
                Self::verify_compiler_argument_wrapper(store, saved.wrapper, saved.initializer, saved.pattern, saved.body, saved.slot)?;
                if let Some(guard) = guard {
                    if saved.guarded_read != Some(guard.read) || saved.initializer != guard.read { return Err(IrVerifyError::new("saved native receiver changes its original guarded initializer")); }
                    Self::verify_argument_initializer_lineage(store, generic, guard.carrier, saved.initializer_source_instruction, &saved.initializer_wrappers, owner)?;
                } else if proof.contract.verify_fs_root_method(&store.semantic)? {
                    Self::verify_fs_root_receiver_lineage(store, generic, saved, owner)?;
                } else { Self::verify_argument_initializer_lineage(store, generic, saved.initializer, saved.initializer_source_instruction, &saved.initializer_wrappers, owner)?; }
                if receiver.source_instruction != saved.initializer_source_instruction || receiver.source_wrappers != saved.initializer_wrappers {
                    return Err(IrVerifyError::new("native method changes its original saved receiver lineage"));
                }
            } else if receiver.postfix.is_none() {
                let physical = guard.map_or(receiver.instruction, |guard| guard.carrier);
                Self::verify_argument_initializer_lineage(store, generic, physical, receiver.source_instruction, &receiver.source_wrappers, owner)?;
            }
            let (encoded, arguments, location) = if proof.contract.verify_fs_root_method(&store.semantic)? {
                super::fs_root_prepare::encoded_fs_root_method_arguments(store, instruction, count)?
            } else if proof.contract.registry_owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Path) {
                path_methods::encoded_path_method_arguments(store, instruction, count, receiver.method_name, operation)?
            } else if list_text_methods::list_text_method_operation_is_supported(proof.contract.registry_owner, operation) {
                list_text_methods::encoded_list_text_method_arguments(store, instruction, count, operation)?
            } else if proof.contract.registry_owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Bytes) && bytes_methods::bytes_method_operation_is_supported(operation) {
                bytes_methods::encoded_bytes_method_arguments(store, instruction, count, operation)?
            } else if proof.contract.registry_owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Str) && matches!(operation, RuntimeOp::TextSplit | RuntimeOp::TextByteSlice) {
                text_methods::encoded_text_method_arguments(store, instruction, count, operation)?
            } else if proof.contract.registry_owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::List) && operation == RuntimeOp::TextJoin {
                list_methods::encoded_list_join_arguments(store, instruction, count)?
            } else {
                let (arguments, location) = encoded_native_method_arguments(store, instruction, count, receiver.method_name)?;
                (operation, arguments, location)
            };
            if arguments.first() != Some(&Some(receiver.instruction)) { return Err(IrVerifyError::new("native method changes its original receiver instruction")); }
            (encoded, arguments, location)
        } else { encoded_native_arguments(store, instruction, count)? };
        if encoded != operation || arguments.as_slice() != proof.contract.argument_sources.as_ref()
            || IrLocationId::from_raw(location).and_then(|location| store.location_sources.get(location.index())) != Some(&source.origin.source) { return Err(IrVerifyError::new("native call changes its original opcode, operands, defaults, or source")); }
        let result = source.result_refinement.as_ref().map_or(proof.contract.result, |refinement| refinement.producer_result);
        let TypeRef::Ground(result) = result else { return Err(IrVerifyError::new("native call result is not closed")); };
        store.semantic.to_type(result)
    }

    pub(super) fn verify_native_call_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        if Self::native_call_result(store, generic, instruction, owner)? != *expected { return Err(IrVerifyError::new("native call result disagrees with its consumer")); }
        let id = generic.ground_native_call_at(instruction)?.unwrap();
        let proof = generic.ground_native_call(id)?;
        let already_active = active.last() == Some(&instruction);
        if active.len() >= 256 || (!already_active && active.contains(&instruction)) { return Err(IrVerifyError::new("native call operands are cyclic or too deep")); }
        if !already_active { active.push(instruction); }
        if let Some(receiver) = &proof.contract.receiver {
            let TypeRef::Ground(ty) = receiver.source_type else { return Err(IrVerifyError::new("native method receiver source requires a scoped protocol")); };
            let source = receiver.source_instruction;
            Self::verify_generic_source(store, generic, source, owner, &store.semantic.to_type(ty)?, None, active)?;
        }
        let source = generic.native_call_source(proof.source)?;
        for (ordinal, argument) in proof.contract.arguments.iter().enumerate() {
            let TypeRef::Ground(ty) = argument.ty else { return Err(IrVerifyError::new("native call operand requires a scoped protocol")); };
            if let Some(descriptor) = &proof.contract.cli_descriptor {
                if Self::verify_cli_constant_operand(store, descriptor, argument.instruction, &store.semantic.to_type(ty)?)? { continue; }
            }
            if matches!(argument.original.value, crate::sema::arguments::ArgumentValueSource::RecordField { .. }) {
                let field = source.record_arguments.iter().find(|field| field.ordinal as usize == ordinal).ok_or_else(|| IrVerifyError::new("native spread field loses its original source receipt"))?;
                Self::verify_generic_source(store, generic, field.record_source_instruction, owner, &store.semantic.to_type(field.record_type)?, None, active)?;
            } else {
                let material = if proof.contract.cli_descriptor.is_some() { argument.instruction } else {
                    source.argument_lineages.get(ordinal).ok_or_else(|| IrVerifyError::new("native call loses its original supplied argument lineage"))?.source_instruction
                };
                let source_ty = if proof.contract.cli_descriptor.is_some() { ty } else {
                    let TypeRef::Ground(source_ty) = source.argument_lineages[ordinal].source_type else {
                        return Err(IrVerifyError::new("native supplied argument source requires a ground type"));
                    };
                    source_ty
                };
                Self::verify_generic_source(store, generic, material, owner, &store.semantic.to_type(source_ty)?, None, active)?;
            }
        }
        if !already_active { active.pop(); }
        Ok(())
    }

    pub(super) fn verify_native_calls(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (_, proof) in generic.ground_native_calls() {
            let source = generic.native_call_source(proof.source)?;
            let result = Self::native_call_result(store, generic, source.instruction, source.owner)?;
            Self::verify_native_call_operand(store, generic, source.instruction, source.owner, &result, &mut Vec::new())?;
        }
        Ok(())
    }

    pub(super) fn verify_native_receiver_scopes(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        Self::verify_native_record_argument_scopes(store, tree)?;
        let Some(generic) = store.generic.as_deref() else { return Ok(()); };
        for (_, proof) in generic.ground_native_calls() {
            let source = generic.native_call_source(proof.source)?;
            let Some(receiver) = &proof.contract.receiver else { continue; };
            let Some(saved) = &receiver.saved else { continue; };
            if !tree.is_descendant(saved.body, receiver.instruction)?
                || source.instruction != saved.body && !tree.is_descendant(saved.body, source.instruction)? {
                return Err(IrVerifyError::new("native method saved receiver is outside its original compiler binding scope"));
            }
            Self::native_call_result(store, generic, source.instruction, source.owner)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "native_prepare/tests.rs"]
mod wire_operand_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::operation_prepare::tests::source_fixture;
    use crate::sema::operation_graph::PreparedLanguageOperation;

    #[test]
    fn canonical_native_module_result_carrier_requires_prepared_source_proof_after_frontend_drop() {
        let source = "test native_result_carrier [error] { |ctx| let result = test.run_script(ctx, \"let value = 1\\n\")?; result.stdout == \"\" }\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        let generic = program.generic_evidence().unwrap();
        assert_eq!(generic.ground_native_calls().count(), 1);
        let (_, proof) = generic.ground_native_calls().next().unwrap();
        assert!(matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TestRunScript, .. }));
        assert_eq!(proof.contract.arguments.len(), 2);
        assert_eq!(proof.contract.argument_sources.len(), 6);
        assert_eq!(proof.contract.binding.supplied_slots.as_ref(), &[0, 1]);
        assert_eq!(proof.contract.binding.default_slots.as_ref(), &[2, 3, 4, 5]);
        assert!(proof.contract.argument_sources[2..].iter().all(Option::is_none));
    }

    #[test]
    fn canonical_native_calls_preserve_closed_operands_inside_the_original_generic_scope() {
        let source = "proc compare(unused, ext: Str) -> Bool { let result = mime.lookup_ext(ext); ext == \"txt\" }\nlet result = compare(7, \"txt\")\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        let generic = program.generic_evidence().unwrap();
        let (_, proof) = generic.ground_native_calls().next().unwrap();
        let source = generic.native_call_source(proof.source).unwrap();
        let scope = source.scope.expect("an independently closed native call retains its original generic scope");
        assert_eq!(source.owner, InstructionOwner::Function(generic.scope(scope).unwrap().owner));
        assert_eq!(generic.scope(scope).unwrap().quantifiers.len(), 1);
        assert!(matches!(proof.contract.result, TypeRef::Ground(_)));
        assert!(proof.contract.arguments.iter().all(|argument| matches!(argument.ty, TypeRef::Ground(_))));
    }

    #[test]
    fn canonical_native_named_arguments_keep_original_recipes_before_formal_slot_order() {
        let source = "test native_named_result [error] { |ctx| let result = test.run_script(source: \"let value = 1\\n\", ctx: ctx)?; result.stdout == \"\" }\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        let (_, proof) = program.generic_evidence().unwrap().ground_native_calls().next().unwrap();
        assert_eq!(proof.contract.binding.supplied_slots.as_ref(), &[1, 0]);
        assert_eq!(proof.contract.arguments[0].original.name.unwrap().as_str().as_str(), "source");
        assert_eq!(proof.contract.arguments[1].original.name.unwrap().as_str().as_str(), "ctx");
    }

    fn native_pair() -> FullProgram {
        source_fixture("test native_result_carrier [error] { |ctx| let first = test.run_script(ctx, \"let value = 1\\n\")?; let second = test.run_script(ctx, \"let value = 2\\n\")?; first.stdout == second.stdout }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
    }

    #[test]
    fn canonical_native_calls_reject_missing_foreign_misplaced_and_rewritten_sources() {
        let program = native_pair();
        let foreign = native_pair();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().next().unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            let mut other_root = program.store.clone();
            other_root.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().source = foreign.generic_evidence().unwrap().ground_native_calls().next().unwrap().1.source;
            assert!(FullVerifier::verify_generic_evidence(&other_root).is_err());
            let mut misplaced = program.store.clone();
            misplaced.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().instruction = proof.contract.arguments[1].instruction;
            assert!(FullVerifier::verify_generic_evidence(&misplaced).is_err());
            let mut wrong_owner = program.store.clone();
            wrong_owner.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().owner = InstructionOwner::Driver(0);
            assert!(FullVerifier::verify_generic_evidence(&wrong_owner).is_err());
            let mut wrong_source = program.store.clone();
            wrong_source.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().origin.namespace = Some(Name::intern("changed"));
            assert!(FullVerifier::verify_generic_evidence(&wrong_source).is_err());
            let mut wrong_opcode = program.store.clone();
            wrong_opcode.tags[source.instruction as usize] = FullTag::ExprCall;
            assert!(FullVerifier::verify_generic_evidence(&wrong_opcode).is_err());
        });
    }

    #[test]
    fn canonical_native_calls_reject_opcode_argument_default_result_and_effect_rewrites() {
        let program = native_pair();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().next().unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let (_, other) = generic.ground_native_calls().nth(1).unwrap();
            let range = program.store.data[source.instruction as usize].range();
            let mut changed = program.store.clone();
            let replacement = changed.runtime_ops.len() as u32;
            changed.runtime_ops.push(RuntimeOp::TestRunXsh);
            changed.extra[range.start as usize] = replacement;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "an operation with the same Result shape cannot replace the original registry authority");
            let mut both = changed.clone();
            let PreparedOperationAuthority::Registry { operation, .. } = &mut both.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.authority else { unreachable!() };
            *operation = RuntimeOp::TestRunXsh;
            assert!(FullVerifier::verify_generic_evidence(&both).is_err());
            let mut operand = program.store.clone();
            let block = IrBlockId::from_raw(operand.payload(range).unwrap()[2]).unwrap();
            let argument_range = operand.blocks[block.index()].instructions;
            operand.extra[argument_range.start as usize + 4] = other.contract.arguments[1].instruction;
            assert!(FullVerifier::verify_generic_evidence(&operand).is_err(), "another same-typed literal is not the original authored operand");
            let mut wrong_value = program.store.clone();
            wrong_value.tags[proof.contract.arguments[1].instruction as usize] = FullTag::ExprBytes;
            assert!(FullVerifier::verify_generic_evidence(&wrong_value).is_err(), "the original source identity does not bypass the operand's producer proof");
            let mut default = program.store.clone();
            default.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.binding.default_slots[0] = 0;
            assert!(FullVerifier::verify_generic_evidence(&default).is_err());
            let mut result = program.store.clone();
            let boolean = SemanticPoolBuilder::default().intern_type(&mut result.semantic, &Type::Bool).unwrap();
            result.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.result = TypeRef::Ground(boolean);
            assert!(FullVerifier::verify_generic_evidence(&result).is_err());
            let mut effects = program.store.clone();
            effects.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.effects.creation = crate::sema::inference::EffectSet::IO;
            assert!(FullVerifier::verify_generic_evidence(&effects).is_err());
        });
    }

    #[test]
    fn canonical_native_calls_reject_coforged_opcode_proof_and_source_expectation() {
        let program = native_pair();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().next().unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let mut forged = program.store.clone();
            let replacement = forged.runtime_ops.len() as u32;
            forged.runtime_ops.push(RuntimeOp::TestRunXsh);
            let range = forged.data[source.instruction as usize].range();
            forged.extra[range.start as usize] = replacement;
            let evidence = forged.generic.as_deref_mut().unwrap();
            let PreparedOperationAuthority::Registry { operation, .. } = &mut evidence.test_ground_native_call_mut(id).unwrap().contract.authority else { unreachable!() };
            *operation = RuntimeOp::TestRunXsh;
            let PreparedOperationAuthority::Registry { operation, .. } = &mut evidence.test_native_call_source_mut(proof.source).unwrap().expected.authority else { unreachable!() };
            *operation = RuntimeOp::TestRunXsh;
            let failure = FullVerifier::verify_generic_evidence(&forged).unwrap_err();
            assert!(failure.message.contains("original receipt"), "the original selected registry operation must survive agreement among rewritten copies: {}", failure.message);
        });
    }

    #[test]
    fn canonical_native_call_lifetimes_keep_original_sources_caches_and_retained_payload_owned() {
        use super::super::super::generic::GenericEvidenceBuilder;
        let program = native_pair();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, proof) = generic.ground_native_calls().next().unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let mut builder = GenericEvidenceBuilder::default();
            assert!(builder.rewind(GenericEvidenceBuilder::default().checkpoint()).is_err());
            builder.register_instruction_origin(source.instruction, OperationSourceOrigin::Expression(source.origin), source.owner).unwrap();
            for argument in &proof.contract.arguments {
                let crate::sema::arguments::ArgumentValueSource::Expression(expression) = argument.original.value else { unreachable!() };
                builder.register_instruction_origin(argument.instruction, OperationSourceOrigin::Expression(crate::sema::check::ExpressionIdentity { expression, ..source.origin }), source.owner).unwrap();
            }
            let checkpoint = builder.checkpoint();
            let retired_source = builder.add_native_call_source(source.clone()).unwrap();
            let retired = builder.add_ground_native_call(PreparedGroundNativeCall { source: retired_source, contract: proof.contract.clone() }).unwrap();
            let stale = builder.checkpoint();
            builder.rewind(checkpoint).unwrap();
            let current_source = builder.add_native_call_source(source.clone()).unwrap();
            let current = builder.add_ground_native_call(PreparedGroundNativeCall { source: current_source, contract: proof.contract.clone() }).unwrap();
            assert!(builder.rewind(stale).is_err());
            let owners = program.store.generic_instruction_owners().unwrap();
            let mut store = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
            assert!(store.native_call_source(retired_source).is_err());
            assert!(store.ground_native_call(retired).is_err());
            assert!(store.native_call_source(proof.source).is_err());
            assert_eq!(store.ground_native_call_at(source.instruction).unwrap(), Some(current));
            let retained = store.retained_bytes();
            assert!(retained >= 2 * (proof.contract.arguments.len() * size_of::<PreparedInvocationArgument>() + proof.contract.argument_sources.len() * size_of::<Option<u32>>()));
            store.shrink_to_fit();
            assert!(store.retained_bytes() <= retained);
            assert_eq!(store.ground_native_call_at(source.instruction).unwrap(), Some(current));
        });
    }
}
