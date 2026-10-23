use crate::runtime::eval::indexed::full::decode::ValueCheckView;
use super::{
    BTreeMap, BTreeSet, CheckedArgumentType, FullCallableContract, FullDecoder, FullStore, FullTag,
    IR_NONE, IrFunctionId, IrVerifyError, Name, SchemeInstantiationBudget, Type, TypeId, bounded,
    checked_argument_type, checked_expression_type_id, decoder_for_driver_step,
    decoder_for_function, driver_owner, ensure_closed_type, ensure_substitution_type, schema,
    merge_list_item_types, expected_list_item,
};
use crate::sema::types::{CallableKind, ConditionRole, ModuleExportType, TypedCallable};
use std::{cell::{OnceCell, RefCell}, rc::Rc, sync::Arc};
#[cfg(test)]
use std::cell::Cell;
use super::super::decode::BorrowCodec;

pub(in crate::runtime::eval::indexed::full) struct GroundProducerIndex {
    demand: crate::runtime::eval::ProducerDemand,
    scheme_scopes: OnceCell<super::VerifiedSchemeScopes>,
    checked_producers: Vec<usize>,
    capture_retry_roots: Vec<usize>,
    driver_outputs: Vec<usize>,
    effect_operations: Vec<usize>,
    conditions: Vec<(usize, ConditionRole)>,
    storage_reads: Vec<(u32, usize, TypeId)>,
    effect_demand_work: usize,
    owners: BTreeMap<u32, ProducerOwner>,
    scheme_owners: BTreeMap<u32, VerifiedSchemeOwner>,
    starts: BTreeMap<usize, u32>,
    binding_writers: Vec<Option<(usize, usize, u32)>>,
    storage_writers: BTreeMap<u32, Vec<StorageWriter>>,
    module_exports: BTreeMap<u32, Vec<(Name, u32)>>,
    function_signatures: Option<Arc<super::super::FunctionSignatureIndex>>,
    work: Option<Rc<RefCell<SchemeInstantiationBudget>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NativeParameterDomain {
    Invariant,
    ReadonlyArgv,
}

impl NativeParameterDomain {
    fn declared(ty: &Type) -> Self {
        if ty.is_readonly_argv_word_list() { Self::ReadonlyArgv } else { Self::Invariant }
    }

    fn accepts(self, parameter: &Type, actual: &Type) -> bool {
        self == Self::ReadonlyArgv
            && parameter.accepts_readonly_argv_argument(actual)
            && ensure_closed_type(actual).is_ok()
    }
}

struct VerifiedSchemeOwner {
    return_analysis: Rc<super::super::IndexedReturnAnalysis>,
    prepared_constant_work: usize,
}

#[derive(Clone, Copy)]
enum StorageWriter {
    Statement { owner: u32, instruction: usize, slot: usize, root_type: TypeId },
    DriverAssign { step: usize },
}

#[derive(Clone, Copy)]
struct StorageBinding {
    identity: u32,
    ty: TypeId,
    mutable: bool,
}

#[derive(Clone, Copy)]
enum LexicalSlotIdentity {
    Unique(ProducerBinding),
    Reused,
}

enum CompletedOptionalRead {
    WrapperPresence { source: usize, checked: TypeId },
    RecordValue { actual: Type, expected: Type },
}

struct ProducerOwner {
    prepared_constant_work: usize,
    work: Rc<RefCell<SchemeInstantiationBudget>>,
    range: std::ops::Range<usize>,
    children: Vec<u32>,
    siblings: Vec<u32>,
    writers: BTreeMap<ProducerBinding, Vec<usize>>,
    storage_bindings: BTreeMap<ProducerBinding, StorageBinding>,
    parents: Vec<Option<usize>>,
    statements: Vec<Option<usize>>,
    statement_positions: BTreeMap<usize, (usize, usize)>,
    block_declarations: BTreeMap<(usize, usize), Vec<(usize, ProducerBinding)>>,
    lexical_slots: BTreeMap<usize, LexicalSlotIdentity>,
    inherited_bindings: RefCell<BTreeMap<(usize, usize), Option<ProducerBinding>>>,
    #[cfg(test)]
    binding_work: Cell<(usize, usize, usize)>,
    #[cfg(test)]
    census_instruction_visits: usize,
    block_statements: BTreeMap<usize, Vec<usize>>,
    block_guards: BTreeMap<usize, Vec<(usize, usize)>>,
    block_can_complete: BTreeMap<usize, bool>,
    typed_calls: Vec<usize>,
    completed_optional_reads: RefCell<BTreeMap<usize, CompletedOptionalRead>>,
    completed_union_reads: RefCell<BTreeMap<(usize, TypeId), super::super::LoweredType>>,
    snapshot_predicates: RefCell<BTreeMap<(usize, usize, usize, usize, bool, bool, ProducerBinding), Vec<Vec<Name>>>>,
    declarations: BTreeMap<usize, Vec<usize>>,
    following_declarations: BTreeMap<usize, Vec<usize>>,
    region_bindings: BTreeMap<(usize, usize), ProducerBinding>,
    pattern_sources: BTreeMap<ProducerBinding, PatternProducer>,
    with_errors: BTreeMap<usize, Type>,
    comprehension_sources: BTreeMap<ProducerBinding, (usize, usize)>,
    pipeline_sources: BTreeMap<ProducerBinding, PipelineProducer>,
    callback_returns: BTreeMap<usize, Vec<usize>>,
    pipeline_groups: BTreeMap<(usize, usize, bool), usize>,
    pipeline_stages: BTreeMap<usize, Vec<u32>>,
    unreachable_roots: BTreeSet<usize>,
    returns: Vec<usize>,
    yield_sites: Vec<usize>,
    return_analysis: Rc<super::super::IndexedReturnAnalysis>,
}

pub(in crate::runtime::eval::indexed::full) struct ProducerPrefixContext<'a> {
    owner: u32,
    evidence: &'a ProducerOwner,
    index: &'a GroundProducerIndex,
}

impl ProducerPrefixContext<'_> {
    pub(in crate::runtime::eval::indexed::full) fn immutable_function_reference_source(&self, decoder: &FullDecoder<'_>, instruction: usize) -> Result<Option<super::super::ImmutableCallableSource>, IrVerifyError> {
        if decoder.owner != self.owner || decoder.instruction_range != self.evidence.range {
            return Err(IrVerifyError::new("callable source context disagrees with its owner range"));
        }
        let (mut owner, mut instruction) = (decoder.owner, instruction);
        let mut visited = BTreeSet::new();
        'source: loop {
            self.evidence.work.borrow_mut().charge(1)?;
            if !visited.insert((owner, instruction)) { return Err(IrVerifyError::new("immutable callable source contains a cycle")); }
            let evidence = self.index.owners.get(&owner).ok_or_else(|| IrVerifyError::new("callable source owner was not structurally verified"))?;
            let source = match super::super::driver_owner_index(owner) {
                Some(step) => decoder_for_driver_step(decoder.store, step)?,
                None => decoder_for_function(decoder.store, function_index(owner)?)?,
            };
            if source.owner != owner || source.instruction_range != evidence.range || !evidence.range.contains(&instruction) {
                return Err(IrVerifyError::new("callable source instruction belongs to another owner"));
            }
            let slot = match source.expression_view(instruction)? {
                schema::ExprView::FunctionRef { .. } => return Ok(Some(super::super::ImmutableCallableSource::Instruction(instruction))),
                schema::ExprView::Field { .. } | schema::ExprView::Method { .. } | schema::ExprView::Index { .. } => {
                    self.evidence.work.borrow_mut().charge(1)?;
                    let (base, field) = match source.expression_view(instruction)? {
                        schema::ExprView::Field { base, name, .. } => (base, Name::intern(name)),
                        schema::ExprView::Index { base, index, .. } => {
                            let Some(field) = source.checked_literal_string(index.raw() as usize)? else { return Ok(None); };
                            (base, Name::intern(&field))
                        }
                        schema::ExprView::Method { receiver, name: "get", mut args, .. } if args.len() == 1 => {
                            let key = args.next().transpose()?.ok_or_else(|| IrVerifyError::new("callable module get key is missing"))?;
                            let Some(field) = source.checked_literal_string(key.raw() as usize)? else { return Ok(None); };
                            (receiver, Name::intern(&field))
                        }
                        _ => return Ok(None),
                    };
                    let mut field_instruction = base.raw() as usize;
                    let schema::ExprView::Param(mut slot) = source.expression_view(field_instruction)? else { return Ok(None); };
                    let mut binding = evidence.binding(decoder.store, owner, field_instruction, slot)?;
                    loop {
                        self.evidence.work.borrow_mut().charge(1)?;
                        if let ProducerBinding::ScopedDeclaration(declaration, declared_slot) = binding {
                            let writers = evidence.writers.get(&binding).map(Vec::as_slice).unwrap_or(&[]);
                            self.evidence.work.borrow_mut().charge(writers.len())?;
                            if declared_slot != slot || writers != [declaration] { return Ok(None); }
                            let schema::StmtView::Let { slot: declared, value, .. } = source.statement_view(declaration)? else { return Ok(None); };
                            if declared != slot { return Ok(None); }
                            field_instruction = value.raw() as usize;
                            match source.expression_view(field_instruction)? {
                                schema::ExprView::Param(next_slot) => {
                                    slot = next_slot;
                                    binding = evidence.binding(decoder.store, owner, field_instruction, next_slot)?;
                                    continue;
                                }
                                schema::ExprView::Field { .. } => {
                                    instruction = field_instruction;
                                    continue 'source;
                                }
                                _ => return Ok(None),
                            }
                        }
                        break;
                    }
                    let ProducerBinding::DriverBinding(_) = binding else { return Ok(None); };
                    let Some(storage) = evidence.storage_bindings.get(&binding) else { return Ok(None); };
                    if storage.mutable || storage.identity == IR_NONE { return Ok(None); }
                    let identity = decoder.store.binding_identities.get(storage.identity as usize).ok_or_else(|| IrVerifyError::new("callable module binding identity is invalid"))?;
                    if identity.flags != 0 { return Ok(None); }
                    let Some((_, step, program_owner)) = self.index.binding_writers.get(storage.identity as usize).copied().flatten() else { return Ok(None); };
                    if program_owner != identity.owner { return Err(IrVerifyError::new("callable source binding owner disagrees with its writer")); }
                    let Some(current) = super::super::driver_owner_index(owner) else { return Ok(None); };
                    if step >= current { return Err(IrVerifyError::new("callable source binding is used before its import")); }
                    let mut same_program = false;
                    for program in &decoder.store.driver_programs {
                        self.evidence.work.borrow_mut().charge(1)?;
                        let steps = program.steps.bounds(decoder.store.driver_steps.len()).ok_or_else(|| IrVerifyError::new("callable source program range is invalid"))?;
                        if steps.contains(&current) {
                            same_program = program.owner == identity.owner && steps.contains(&step);
                            break;
                        }
                    }
                    if !same_program { return Err(IrVerifyError::new("callable source binding belongs to another driver program")); }
                    if decoder.store.driver_steps[step].tag != super::super::FullDriverTag::Use { return Ok(None); }
                    let writer_decoder = decoder_for_driver_step(decoder.store, step)?;
                    let mut payload = super::super::FullCursor::new(decoder.store.payload(decoder.store.driver_steps[step].data.range())?);
                    let _: Arc<str> = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
                    let alias: Option<Name> = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
                    let path: Vec<Name> = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
                    let _namespace: Name = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
                    let exports: Vec<super::super::LoweredModuleExport> = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
                    self.evidence.work.borrow_mut().charge(exports.len())?;
                    let child: u32 = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
                    let Some(local_name) = alias.or_else(|| path.last().copied()) else { return Ok(None); };
                    if local_name.as_str() != decoder.store.string(identity.name)? { return Ok(None); }
                    let Some((kind, function_namespace)) = exports.iter().find_map(|export| match export {
                        super::super::LoweredModuleExport::GenericCallable { name, kind, function_namespace, .. } if *name == field => Some((*kind, *function_namespace)),
                        _ => None,
                    }) else { return Ok(None); };
                    let type_id = TypeId::from_raw(decoder.store.driver_steps[step].registered_module_type)
                        .ok_or_else(|| IrVerifyError::new("callable module has no registered type"))?;
                    let Type::Module(module) = decoder.store.semantic.to_type(type_id)? else { return Ok(None); };
                    if !module.exact
                        || !matches!(module.exports.get(&field), Some(ModuleExportType::GenericCallable { kind: registered, optional: false }) if *registered == kind)
                    { return Ok(None); }
                    let Some(child_index) = child.checked_sub(1).map(|index| index as usize).filter(|index| *index < decoder.store.driver_programs.len()) else { return Ok(None); };
                    let child_owner = decoder.store.driver_programs[child_index].owner;
                    if child_owner == IR_NONE || decoder.store.string(child_owner)? != function_namespace.as_str() { return Ok(None); }
                    let function = super::super::decode::FunctionNameView::Qualified(crate::symbol::QualifiedName::new(function_namespace, field));
                    let (target, contract, target_kind) = decoder.function_reference_contract(function)?;
                    if target_kind != kind { return Ok(None); }
                    let metadata = decoder.store.function_metadata.get(target.index()).ok_or_else(|| IrVerifyError::new("generic callable target metadata is missing"))?;
                    if metadata.owner != child_owner || metadata.callable_kind != kind
                        || metadata.flags != (0b10 | u8::from(kind == crate::sema::types::CallableKind::Proc))
                        || decoder.store.string(decoder.store.functions[target.index()].name)? != field.as_str()
                    { return Ok(None); }
                    let super::super::FullCallableContract::Scheme(scheme_id) = contract else { return Ok(None); };
                    let scheme = decoder.store.scheme(scheme_id)?;
                    if scheme.callable_kind != kind || !scheme.is_quantified(&decoder.store)? { return Ok(None); }
                    return Ok(Some(super::super::ImmutableCallableSource::ImportedFunction(function)));
                }
                schema::ExprView::Param(slot) => slot,
                _ => return Ok(None),
            };
            let binding = evidence.binding(decoder.store, owner, instruction, slot)?;
            if let ProducerBinding::ScopedDeclaration(declaration, declared_slot) = binding {
                let writers = evidence.writers.get(&binding).map(Vec::as_slice).unwrap_or(&[]);
                self.evidence.work.borrow_mut().charge(writers.len())?;
                if declared_slot != slot || writers != [declaration] { return Ok(None); }
                match source.statement_view(declaration)? {
                    schema::StmtView::Let { slot: declared, value, .. } if declared == slot => {
                        instruction = value.raw() as usize;
                        continue;
                    }
                    _ => return Ok(None),
                }
            }
            let identity = match binding {
                ProducerBinding::HeaderCapture(identity) => identity,
                ProducerBinding::DriverBinding(_) => {
                    let Some(storage) = evidence.storage_bindings.get(&binding) else { return Ok(None) };
                    storage.identity
                }
                _ => return Ok(None),
            };
            if identity == IR_NONE || evidence.storage_bindings.get(&binding).is_none_or(|storage| storage.mutable) { return Ok(None); }
            let origin = decoder.store.binding_identities.get(identity as usize).ok_or_else(|| IrVerifyError::new("callable source binding identity is invalid"))?;
            if origin.flags != 0 { return Ok(None); }
            let Some((_, step, program_owner)) = self.index.binding_writers.get(identity as usize).copied().flatten() else { return Ok(None) };
            if program_owner != origin.owner { return Err(IrVerifyError::new("callable source binding owner disagrees with its writer")); }
            if let Some(current) = super::super::driver_owner_index(owner) {
                if step >= current { return Err(IrVerifyError::new("callable source binding is used before its declaration")); }
                let mut same_program = false;
                for program in &decoder.store.driver_programs {
                    self.evidence.work.borrow_mut().charge(1)?;
                    let steps = program.steps.bounds(decoder.store.driver_steps.len()).ok_or_else(|| IrVerifyError::new("callable source program range is invalid"))?;
                    if steps.contains(&current) {
                        same_program = program.owner == origin.owner && steps.contains(&step);
                        break;
                    }
                }
                if !same_program { return Err(IrVerifyError::new("callable source binding belongs to another driver program")); }
            }
            if decoder.store.driver_steps[step].tag != super::super::FullDriverTag::Let { return Ok(None); }
            let writer_decoder = decoder_for_driver_step(decoder.store, step)?;
            let mut payload = super::super::FullCursor::new(decoder.store.payload(decoder.store.driver_steps[step].data.range())?);
            let name: Name = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
            let _: Option<super::super::LoweredType> = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
            let _: Option<super::super::LoweredTypeCheck> = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
            let _: Option<Type> = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
            let mutable: bool = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
            if mutable { return Ok(None); }
            if name.as_str() != decoder.store.string(origin.name)? { return Err(IrVerifyError::new("callable source writer disagrees with its binding identity")); }
            owner = writer_decoder.owner;
            instruction = payload.raw()? as usize;
        }
    }

    pub(in crate::runtime::eval::indexed::full) fn instruction_forest(&self) -> super::super::InstructionParentContext<'_> {
        super::super::InstructionParentContext { owner: self.owner, range: self.evidence.range.clone(),
            parents: &self.evidence.parents, statements: &self.evidence.statements, children: &self.evidence.children, siblings: &self.evidence.siblings }
    }

    pub(in crate::runtime::eval::indexed::full) fn runtime_list_read_type(&self, decoder: &FullDecoder<'_>, instruction: usize) -> Result<Option<Type>, IrVerifyError> {
        if decoder.owner != self.owner || decoder.instruction_range != self.evidence.range || !self.evidence.range.contains(&instruction) {
            return Err(IrVerifyError::new("type-test list context disagrees with its owner range"));
        }
        let mut charge = |amount| self.evidence.work.borrow_mut().charge(amount);
        let Some(target) = decoder.checked_binding_path(instruction, &mut charge)? else { return Ok(None) };
        if decoder.checked_binding_path_type(instruction, &target, &mut charge)? != Some(Type::Any) { return Ok(None); }
        let binding = self.evidence.binding(decoder.store, decoder.owner, instruction, target.0)?;
        if !self.evidence.binding_path_is_snapshot(decoder, binding, target.0, &mut charge)? { return Ok(None); }
        let mut matched = None;
        let proven = self.evidence.guarded_binding_read_is_narrowed(decoder, instruction, target.0, binding,
            |condition, when_true, is_boolean, _, charge| {
                matched = decoder.checked_type_test_contract(condition, when_true, is_boolean, instruction, &target, charge)?
                    .and_then(|(ty, outcome)| (outcome == super::super::TypeTestOutcome::Accepted).then_some(ty));
                Ok(matched.is_some())
            }, None, &mut charge)?;
        if !proven { return Ok(None); }
        let Some(ty) = matched else { return Ok(None) };
        let mut work = self.evidence.work.borrow_mut();
        let ty = decoder.store.semantic.to_type_with_budget(ty, &mut work.semantic_types)?;
        ensure_closed_type(&ty)?;
        if matches!(ty, Type::List(_)) && super::runtime_complete_value_check_type(&ty, &mut work, 0)? { Ok(Some(ty)) } else { Ok(None) }
    }

    fn exact_runtime_kind(ty: &Type, work: &mut SchemeInstantiationBudget) -> Result<Option<super::super::LoweredType>, IrVerifyError> {
        if !super::runtime_complete_value_check_type(ty, work, 0)? { return Ok(None); }
        let Some(kind) = super::super::lowered_type_for_type(ty) else { return Ok(None) };
        Ok((super::super::lowered_type_to_type(kind).as_ref() == Some(ty)).then_some(kind))
    }

    fn union_member_read_type(&self, decoder: &FullDecoder<'_>, instruction: usize, declared: &Type) -> Result<Option<Type>, IrVerifyError> {
        if decoder.owner != self.owner || decoder.instruction_range != self.evidence.range || !self.evidence.range.contains(&instruction) {
            return Err(IrVerifyError::new("union type-test context disagrees with its owner range"));
        }
        let Type::Union(members) = declared else { return Ok(None) };
        let mut charge = |amount| self.evidence.work.borrow_mut().charge(amount);
        let Some(target) = decoder.checked_binding_path(instruction, &mut charge)? else { return Ok(None) };
        let Some(stored_id) = decoder.checked_binding_path_type_id(instruction, &target, &mut charge)? else { return Ok(None) };
        let stored = decoder.store.semantic.to_type_with_budget(stored_id, &mut self.evidence.work.borrow_mut().semantic_types)?;
        self.evidence.work.borrow_mut().charge_clone(declared)?;
        if &stored != declared { return Ok(None); }
        self.evidence.work.borrow_mut().charge(1)?;
        if let Some(&kind) = self.evidence.completed_union_reads.borrow().get(&(instruction, stored_id)) {
            let ty = super::super::lowered_type_to_type(kind).ok_or_else(|| IrVerifyError::new("runtime kind has no unique semantic type"))?;
            self.evidence.work.borrow_mut().charge_clone(&ty)?;
            return Ok(Some(ty));
        }
        let binding = self.evidence.binding(decoder.store, decoder.owner, instruction, target.0)?;
        if !self.evidence.binding_path_is_snapshot(decoder, binding, target.0, &mut charge)? { return Ok(None); }
        let mut kinds = Vec::new();
        for member in members {
            let mut work = self.evidence.work.borrow_mut();
            work.charge(1 + kinds.len())?;
            let Some(kind) = Self::exact_runtime_kind(member, &mut work)? else { return Ok(None) };
            if kinds.contains(&kind) { return Ok(None); }
            kinds.push(kind);
        }
        let mut narrowed = None;
        let proven = self.evidence.guarded_binding_read_is_narrowed(decoder, instruction, target.0, binding,
            |condition, when_true, is_boolean, _, charge| {
                let Some((tested, outcome)) = decoder.checked_type_test_contract(condition, when_true, is_boolean, instruction, &target, charge)? else { return Ok(false) };
                let mut work = self.evidence.work.borrow_mut();
                let tested = decoder.store.semantic.to_type_with_budget(tested, &mut work.semantic_types)?;
                let Some(tested_kind) = Self::exact_runtime_kind(&tested, &mut work)? else { return Ok(false) };
                let mut selected = None;
                for (member, kind) in members.iter().zip(&kinds) {
                    work.charge(1)?;
                    let accepted = member.matches_expected(&tested);
                    if accepted != (*kind == tested_kind) { return Ok(false); }
                    if accepted == (outcome == super::super::TypeTestOutcome::Accepted) {
                        if selected.replace((member, *kind)).is_some() { return Ok(false); }
                    }
                }
                let Some((selected, kind)) = selected else { return Ok(false) };
                work.charge_clone(selected)?;
                narrowed = Some((selected.clone(), kind));
                Ok(true)
            }, None, &mut charge)?;
        if proven && let Some((ty, kind)) = narrowed {
            self.evidence.work.borrow_mut().charge(1)?;
            self.evidence.completed_union_reads.borrow_mut().insert((instruction, stored_id), kind);
            Ok(Some(ty))
        } else { Ok(None) }
    }

    pub(in crate::runtime::eval::indexed::full) fn snapshot_condition_initializer(&self, decoder: &FullDecoder<'_>,
        condition: usize, slot: usize, use_site: usize, target: &(usize, Vec<Name>),
        charge: &mut dyn FnMut(usize) -> Result<(), IrVerifyError>,
    ) -> Result<Option<(usize, bool)>, IrVerifyError> {
        if decoder.owner != self.owner || decoder.instruction_range != self.evidence.range {
            return Err(IrVerifyError::new("condition snapshot context disagrees with its owner"));
        }
        let protected = self.evidence.binding(decoder.store, decoder.owner, use_site, target.0)?;
        if !self.evidence.binding_path_is_snapshot(decoder, protected, target.0, charge)? { return Ok(None); }
        let binding = self.evidence.binding(decoder.store, decoder.owner, condition, slot)?;
        let ProducerBinding::ScopedDeclaration(declaration, declared_slot) = binding else { return Ok(None) };
        charge(1)?;
        if declared_slot != slot || self.evidence.storage_bindings.contains_key(&binding)
            || super::super::driver_owner_index(decoder.owner).is_some()
            || decoder.is_parameter_or_capture_slot(slot, &mut |amount| charge(amount))?
            || self.evidence.writers.get(&binding).map(Vec::as_slice) != Some(&[declaration][..])
        { return Ok(None); }
        let initializer = match decoder.statement_view(declaration)? {
            schema::StmtView::Let { value, .. } => (value.raw() as usize, false),
            schema::StmtView::LetBool { value, .. } => (value.raw() as usize, true),
            _ => return Ok(None),
        };
        if self.evidence.visible_binding(decoder.store, decoder.owner, initializer.0, target.0)? != Some(protected) { return Ok(None); }
        Ok(Some(initializer))
    }

    pub(in crate::runtime::eval::indexed::full) fn charge_optional_path(&self, amount: usize) -> Result<(), IrVerifyError> {
        self.evidence.work.borrow_mut().charge(amount)
    }

    pub(in crate::runtime::eval::indexed::full) fn optional_path_is_narrowed(&self, decoder: &FullDecoder<'_>,
        instruction: usize, target: &(usize, Vec<Name>), payload: &Type,
        charge: &mut dyn FnMut(usize) -> Result<(), IrVerifyError>,
    ) -> Result<bool, IrVerifyError> {
        if decoder.owner != self.owner || decoder.instruction_range != self.evidence.range
            || !self.evidence.range.contains(&instruction) || target.1.len() > 128
        { return Err(IrVerifyError::new("optional path proof disagrees with its owner range")); }
        let binding = self.evidence.binding(decoder.store, decoder.owner, instruction, target.0)?;
        self.evidence.optional_path_read_is_narrowed(decoder, instruction, target, payload, binding, charge)
    }

    pub(in crate::runtime::eval::indexed::full) fn guarded_record_read_matches_type(&self, decoder: &FullDecoder<'_>,
        instruction: usize, actual: &Type, expected: &Type,
        charge: &mut dyn FnMut(usize) -> Result<(), IrVerifyError>,
    ) -> Result<bool, IrVerifyError> {
        if decoder.owner != self.owner || decoder.instruction_range != self.evidence.range
            || !self.evidence.range.contains(&instruction)
        { return Err(IrVerifyError::new("record read proof disagrees with its owner range")); }
        charge(1)?;
        if !matches!(decoder.expression_view(instruction)?, schema::ExprView::Param(_) | schema::ExprView::Field { .. }) {
            return Ok(false);
        }
        charge(1)?;
        if let Some(CompletedOptionalRead::RecordValue { actual: stored, expected: proven }) =
            self.evidence.completed_optional_reads.borrow().get(&instruction)
        {
            charge(stored.retained_bytes().div_ceil(size_of::<Type>()))?;
            if stored == actual {
                charge(proven.retained_bytes().div_ceil(size_of::<Type>()))?;
                if proven == expected { return Ok(true); }
            }
        }
        charge(1)?;
        if !crate::runtime::eval::lower::only_optional_read_narrowing(actual, expected) { return Ok(false); }
        let Some(target) = decoder.checked_binding_path(instruction, charge)? else { return Ok(false) };
        let binding = self.evidence.binding(decoder.store, decoder.owner, instruction, target.0)?;
        let proven = decoder.checked_declared_type_matches_narrowed_fact_with_optional_proof(
            instruction, target, actual, expected,
            &mut |target, payload, charge| {
                if target.1.len() > 128 { return Ok(false); }
                self.evidence.optional_path_read_is_narrowed(decoder, instruction, target, payload, binding, charge)
            }, charge)?;
        if !proven { return Ok(false); }
        charge(1)?;
        {
            let completed = self.evidence.completed_optional_reads.borrow();
            if completed.len() >= self.evidence.range.len() {
                charge(1)?;
                if !completed.contains_key(&instruction) {
                    return Err(IrVerifyError::new("record read proof exceeds its owner bound"));
                }
            }
        }
        SchemeInstantiationBudget::charge_clone_with(actual, charge)?;
        SchemeInstantiationBudget::charge_clone_with(expected, charge)?;
        let proof = CompletedOptionalRead::RecordValue { actual: actual.clone(), expected: expected.clone() };
        charge(1)?;
        self.evidence.completed_optional_reads.borrow_mut().insert(instruction, proof);
        Ok(true)
    }

    pub(in crate::runtime::eval::indexed::full) fn optional_source_is_narrowed(&self, decoder: &FullDecoder<'_>, instruction: usize, source: usize,
        checked_id: TypeId, checked: &Type, target: &(usize, Vec<Name>)) -> Result<bool, IrVerifyError> {
        if decoder.owner != self.owner || decoder.instruction_range != self.evidence.range
            || !self.evidence.range.contains(&instruction) || !self.evidence.range.contains(&source)
        {
            return Err(IrVerifyError::new("producer prefix context disagrees with its owner range"));
        }
        self.evidence.work.borrow_mut().charge(1)?;
        let schema::ExprView::CheckedValue { check: ValueCheckView::Ground(check), value, .. } = decoder.expression_view(instruction)? else {
            return Err(IrVerifyError::new("optional presence evidence requires a physical checked value"));
        };
        if check.schema.is_some() || check.ty.id != checked_id || value.raw() as usize != source {
            return Err(IrVerifyError::new("optional presence evidence disagrees with its physical source contract"));
        }
        self.evidence.work.borrow_mut().charge(3)?;
        if matches!(self.evidence.completed_optional_reads.borrow().get(&instruction),
            Some(CompletedOptionalRead::WrapperPresence { source: proven_source, checked: proven_checked })
                if *proven_source == source && *proven_checked == checked_id)
        { return Ok(true); }
        let narrowed = (|| -> Result<bool, IrVerifyError> {
            if let schema::ExprView::Param(slot) = decoder.expression_view(source)? {
                let binding = self.evidence.binding(decoder.store, decoder.owner, source, slot)?;
                if self.evidence.optional_binding_read_is_narrowed(decoder, source, slot, binding,
                    &mut |amount| self.evidence.work.borrow_mut().charge(amount))?
                { return Ok(true); }
            }
            let binding = self.evidence.binding(decoder.store, decoder.owner, source, target.0)?;
            self.evidence.optional_path_read_is_narrowed(decoder, source, target, checked, binding,
                &mut |amount| self.evidence.work.borrow_mut().charge(amount))
        })()?;
        if narrowed {
            self.evidence.work.borrow_mut().charge(1)?;
            self.evidence.completed_optional_reads.borrow_mut().insert(instruction,
                CompletedOptionalRead::WrapperPresence { source, checked: checked_id });
        }
        Ok(narrowed)
    }
}

struct PatternProducer {
    source: usize,
    pattern: u32,
    preceding: Vec<(u32, bool)>,
}

#[derive(Clone, Copy)]
struct PipelineProducer {
    pipeline: usize,
    stage: usize,
    role: super::super::PipelineStageSlotRole,
}

type FunctionProjection = super::super::CheckedValueProjection;

#[derive(Clone, Copy, Eq, PartialEq)]
enum ProducerTargetPurpose {
    LiteralPayload,
    Callable,
    Stream,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CapturedResultUse {
    Propagate,
    Discard,
    Iterate,
}

pub(super) enum StreamProducer {
    Value(usize),
    ResultSuccess(usize),
}

pub(super) struct StreamIteration {
    pub(super) owner: u32,
    pub(super) source: StreamProducer,
    pub(super) boundary: super::PropagationBoundary,
}

fn charged_capture_event_sort<T>(events: &mut [(usize, T)], budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let len = events.len();
    budget.charge(len.saturating_mul(2))?;
    let mut order = (0..len).collect::<Vec<_>>();
    let mut merged = vec![0; len];
    let mut width = 1usize;
    while width < len {
        for start in (0..len).step_by(width.saturating_mul(2)) {
            let middle = start.saturating_add(width).min(len);
            let end = middle.saturating_add(width).min(len);
            let (mut left, mut right) = (start, middle);
            for output in &mut merged[start..end] {
                budget.charge(1)?;
                let take_left = if left == middle { false }
                    else if right == end { true }
                    else {
                        budget.charge(1)?;
                        events[order[left]].0 <= events[order[right]].0
                    };
                *output = if take_left { let value = order[left]; left += 1; value }
                    else { let value = order[right]; right += 1; value };
            }
        }
        std::mem::swap(&mut order, &mut merged);
        width = width.saturating_mul(2);
    }
    budget.charge(len)?;
    for (target, source) in order.into_iter().enumerate() { merged[source] = target; }
    for index in 0..len {
        while merged[index] != index {
            budget.charge(1)?;
            let target = merged[index];
            events.swap(index, target);
            merged.swap(index, target);
        }
    }
    Ok(())
}

fn charged_capture_transfer_contains(transfers: &[usize], instruction: usize, budget: &mut SchemeInstantiationBudget) -> Result<bool, IrVerifyError> {
    let (mut start, mut end) = (0, transfers.len());
    while start < end {
        budget.charge(1)?;
        let middle = start + (end - start) / 2;
        match transfers[middle].cmp(&instruction) {
            std::cmp::Ordering::Less => start = middle + 1,
            std::cmp::Ordering::Greater => end = middle,
            std::cmp::Ordering::Equal => return Ok(true),
        }
    }
    Ok(false)
}

pub(super) struct CaptureErrorRequest {
    pub(super) owner: u32,
    pub(super) instruction: usize,
    pub(super) expected_error: crate::sema::schemes::SchemeType,
    pub(super) wrapper: Option<u32>,
    pub(super) transfers: Vec<(usize, crate::sema::wrapper_operation::WrapperErrorContribution<crate::sema::schemes::SchemeType>)>,
}

pub(super) struct GroundArgumentRequest {
    pub(super) owner: u32,
    pub(super) call: usize,
    pub(super) argument: usize,
    pub(super) expected: Type,
}

#[derive(Default)]
pub(super) struct SchemeProducerRequests {
    pub(super) streams: Vec<StreamIteration>,
    pub(super) ground_arguments: Vec<GroundArgumentRequest>,
    pub(super) captures: Vec<CaptureErrorRequest>,
    pub(super) applications: BTreeSet<(u32, usize)>,
    pub(super) methods: BTreeSet<(u32, usize)>,
}

impl SchemeProducerRequests {
    fn is_empty(&self) -> bool { self.streams.is_empty() && self.captures.is_empty() && self.applications.is_empty() && self.ground_arguments.is_empty() && self.methods.is_empty() }
}

pub(super) fn process_option_contract(name: &str, actual: Option<&Type>, budget: &mut SchemeInstantiationBudget) -> Result<Arc<Type>, IrVerifyError> {
    let mut signatures = crate::modules::signature::native_static_operation_signatures(super::super::RuntimeOp::ProcessCommandArgv);
    let first = signatures.next().ok_or_else(|| IrVerifyError::new("process command has no canonical option contracts"))?;
    let position = match first.params.iter().position(|parameter| parameter.name == name) {
        Some(position) => { budget.charge(position + 1)?; position }
        None => {
            budget.charge(first.params.len())?;
            return Err(IrVerifyError::new("process option has no canonical parameter"));
        }
    };
    let mut selected: Option<&Type> = None;
    for signature in std::iter::once(first).chain(signatures) {
        budget.charge(1)?;
        let parameter = signature.params.get(position).filter(|parameter| parameter.name == name)
            .ok_or_else(|| IrVerifyError::new("process option changes position across canonical contracts"))?;
        if let Some(actual) = actual {
            budget.charge_clone(actual)?;
            if !actual.matches_expected(&parameter.ty) { continue; }
        }
        if selected.is_some_and(|previous| previous != &parameter.ty) {
            return Err(IrVerifyError::new("process option has ambiguous canonical contracts"));
        }
        selected = Some(&parameter.ty);
    }
    let selected = selected.ok_or_else(|| IrVerifyError::new("process option disagrees with its canonical contract"))?;
    budget.charge_clone(selected)?;
    Ok(Arc::new(selected.clone()))
}

pub(super) fn process_word_type(ty: &Type, spliced: bool, optional_splice: bool, role: ProcessWordRole) -> Result<bool, IrVerifyError> {
    use crate::syntax::node::RedirectionKind as R;
    let bytes_input = matches!(role, ProcessWordRole::Redirection(R::StdinRead)) && !spliced && ty.unvalidated() == &Type::Bytes;
    let fits = match role {
        ProcessWordRole::Argument => match ty.unvalidated() {
        Type::List(item) if spliced || optional_splice => item.can_be_argv_item(),
        _ => !spliced && ty.can_be_argv_item(),
        },
        ProcessWordRole::Environment => !spliced && (ty.can_be_env_value()
        || (optional_splice && matches!(ty.unvalidated(), Type::List(item) if item.can_be_argv_item()))),
        ProcessWordRole::Redirection(kind) => !spliced && (bytes_input || match kind {
        R::StdinDup | R::StdoutDup => matches!(ty.unvalidated(), Type::Int | Type::Str),
        _ => matches!(ty.unvalidated(), Type::Path | Type::Str),
        }),
    };
    if !fits { return Err(IrVerifyError::new("process word disagrees with its physical conversion role")); }
    Ok(bytes_input)
}

pub(super) enum ProcessWordRole {
    Argument,
    Environment,
    Redirection(crate::syntax::node::RedirectionKind),
}

#[derive(Clone, Default, Eq, PartialEq)]
struct ProducerTargets {
    functions: BTreeSet<usize>,
    opaque: bool,
    parameters: BTreeSet<(usize, Vec<FunctionProjection>)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum CallableProducer {
    Binding(u32),
    Expression(usize),
    PipelinePrefix(usize, usize),
    Function(usize),
}

type StreamSummaryKey = (usize, Vec<FunctionProjection>);

#[derive(Default)]
struct StreamSummaries {
    values: BTreeMap<StreamSummaryKey, Arc<ProducerTargets>>,
    pending: BTreeSet<StreamSummaryKey>,
    dependents: BTreeMap<StreamSummaryKey, BTreeSet<StreamSummaryKey>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum ProducerBinding {
    HeaderParameter(usize),
    HeaderCapture(u32),
    ScopedDeclaration(usize, usize),
    DriverBinding(usize),
}

impl ProducerOwner {
    fn optional_binding_read_is_narrowed(&self, decoder: &FullDecoder<'_>, instruction: usize, slot: usize, binding: ProducerBinding,
        charge: &mut dyn FnMut(usize) -> Result<(), IrVerifyError>) -> Result<bool, IrVerifyError> {
        let local_declaration = if let ProducerBinding::ScopedDeclaration(declaration, declared_slot) = binding {
            charge(1)?;
            super::super::driver_owner_index(decoder.owner).is_none()
                && declared_slot == slot
                && self.writers.get(&binding).is_some_and(|writers| writers.as_slice() == [declaration])
                && matches!(decoder.statement_view(declaration)?, schema::StmtView::Let { slot: written, .. } if written == slot)
                && !decoder.is_parameter_or_capture_slot(slot, &mut |amount| charge(amount))?
        } else { false };
        if !local_declaration && ((!self.pattern_sources.contains_key(&binding) && !matches!(binding, ProducerBinding::HeaderParameter(_)))
            || self.writers.get(&binding).is_some_and(|writers| !writers.is_empty())) {
            return Ok(false);
        }
        self.guarded_binding_read_is_narrowed(decoder, instruction, slot, binding,
            |condition, when_true, is_boolean, _, charge| decoder.checked_slot_null_condition_proves_nonnull_with(condition, when_true, is_boolean, slot, |amount| charge(amount)), None, charge)
    }

    fn binding_path_is_snapshot(&self, decoder: &FullDecoder<'_>, binding: ProducerBinding, slot: usize,
        charge: &mut dyn FnMut(usize) -> Result<(), IrVerifyError>,
    ) -> Result<bool, IrVerifyError> {
        charge(1)?;
        if self.storage_bindings.contains_key(&binding) { return Ok(false); }
        match binding {
            ProducerBinding::HeaderParameter(_) => Ok(self.writers.get(&binding).is_none_or(Vec::is_empty)),
            ProducerBinding::ScopedDeclaration(declaration, declared_slot) => {
                if super::super::driver_owner_index(decoder.owner).is_some() || declared_slot != slot
                    || decoder.is_parameter_or_capture_slot(slot, &mut |amount| charge(amount))?
                { return Ok(false); }
                let writers = self.writers.get(&binding).map(Vec::as_slice).unwrap_or(&[]);
                charge(writers.len())?;
                if writers.is_empty() {
                    return Ok(self.pattern_sources.contains_key(&binding) || self.comprehension_sources.contains_key(&binding));
                }
                if writers != [declaration] { return Ok(false); }
                Ok(matches!(decoder.statement_view(declaration)?, schema::StmtView::Let { .. } | schema::StmtView::LetRecord { .. }
                    | schema::StmtView::For { .. } | schema::StmtView::ForRecord { .. } | schema::StmtView::ForStrLines { .. }))
            }
            ProducerBinding::HeaderCapture(_) | ProducerBinding::DriverBinding(_) => Ok(false),
        }
    }

    fn optional_path_read_is_narrowed(&self, decoder: &FullDecoder<'_>, instruction: usize,
        target: &(usize, Vec<Name>), payload: &Type, binding: ProducerBinding,
        charge: &mut dyn FnMut(usize) -> Result<(), IrVerifyError>,
    ) -> Result<bool, IrVerifyError> {
        let snapshot = self.binding_path_is_snapshot(decoder, binding, target.0, charge)?;
        let stability = if snapshot { super::super::GuardPathStability::ImmutableSnapshot } else { super::super::GuardPathStability::MayChange };
        let mut active = |parent, child, charge: &mut dyn FnMut(usize) -> Result<(), IrVerifyError>| {
            decoder.checked_binding_path_is_narrowed_at_parent_with_preservation(instruction, parent, child, target, payload, stability,
                &mut |parent, roots, charge| if snapshot { Ok(true) } else { decoder.checked_branch_preserves_binding_path(parent, roots, instruction, target, charge) }, charge)
        };
        self.guarded_binding_read_is_narrowed(decoder, instruction, target.0, binding,
            |condition, when_true, is_boolean, preceding, charge| {
                let Some((guard, branch_index, anchor, roots)) = preceding else { return Ok(false) };
                let key = snapshot.then(|| (self.statement_positions[&anchor].0, guard, branch_index, condition, when_true, is_boolean, binding));
                if let Some(key) = key {
                    charge(1)?;
                    if let Some(paths) = self.snapshot_predicates.borrow().get(&key) {
                        for path in paths {
                            charge(1 + target.1.len())?;
                            if path == &target.1 { return Ok(true); }
                        }
                    }
                }
                if !decoder.checked_condition_proves_nonnull(condition, when_true, is_boolean, instruction, target, payload, stability, charge)? { return Ok(false); }
                if let Some(key) = key {
                    if self.visible_binding(decoder.store, decoder.owner, condition, target.0)? != Some(binding) { return Ok(false); }
                    charge(1 + target.1.len())?;
                    self.snapshot_predicates.borrow_mut().entry(key).or_default().push(target.1.clone());
                    return Ok(true);
                }
                if !self.guard_fallthrough_preserves_binding_path(decoder, guard, branch_index, target, charge)?
                    || decoder.checked_child_is_in_repeatable_loop_body(anchor, instruction, charge)?
                    || decoder.checked_parent_path_crosses_repeatable_loop(instruction, anchor, charge)?
                { return Ok(false); }
                charge(roots.len())?;
                decoder.checked_anchored_prefix_preserves_binding_path(anchor, roots.to_vec(), instruction, target, charge)
            }, Some(&mut active), charge)
    }

    fn completing_block_preserves_binding_path(&self, decoder: &FullDecoder<'_>, block: usize, target: &(usize, Vec<Name>),
        charge: &mut dyn FnMut(usize) -> Result<(), IrVerifyError>,
    ) -> Result<bool, IrVerifyError> {
        charge(1)?;
        if !self.block_can_complete[&block] { return Ok(true); }
        decoder.checked_prefix_roots_preserve_binding_path(&self.block_statements[&block], target, charge)
    }

    fn guard_fallthrough_preserves_binding_path(&self, decoder: &FullDecoder<'_>, guard: usize, branch_index: usize, target: &(usize, Vec<Name>),
        charge: &mut dyn FnMut(usize) -> Result<(), IrVerifyError>,
    ) -> Result<bool, IrVerifyError> {
        charge(1)?;
        match decoder.statement_view(guard)? {
            schema::StmtView::If { mut branches, else_body } => {
                for (index, branch) in (&mut branches).enumerate() {
                    charge(1)?;
                    let (condition, body) = branch?;
                    if index < branch_index { continue; }
                    if !decoder.checked_expression_preserves_binding_path(condition.raw() as usize, target, charge)?
                        || !self.completing_block_preserves_binding_path(decoder, body.block_identity().index(), target, charge)?
                    { return Ok(false); }
                }
                if let Some(body) = else_body {
                    return self.completing_block_preserves_binding_path(decoder, body.block_identity().index(), target, charge);
                }
            }
            schema::StmtView::IfBool { mut branches, else_body } => {
                for (index, branch) in (&mut branches).enumerate() {
                    charge(1)?;
                    let (condition, body) = branch?;
                    if index < branch_index { continue; }
                    if !decoder.checked_expression_preserves_binding_path(condition.raw() as usize, target, charge)?
                        || !self.completing_block_preserves_binding_path(decoder, body.block_identity().index(), target, charge)?
                    { return Ok(false); }
                }
                if let Some(body) = else_body {
                    return self.completing_block_preserves_binding_path(decoder, body.block_identity().index(), target, charge);
                }
            }
            schema::StmtView::Assert { value, .. } if branch_index == 0 => {
                return decoder.checked_expression_preserves_binding_path(value.raw() as usize, target, charge);
            }
            _ => return Err(IrVerifyError::new("optional prefix guard is not conditional")),
        }
        Ok(true)
    }

    fn guarded_binding_read_is_narrowed(&self, decoder: &FullDecoder<'_>, instruction: usize, slot: usize, binding: ProducerBinding,
        mut proves: impl FnMut(usize, bool, bool, Option<(usize, usize, usize, &[usize])>, &mut dyn FnMut(usize) -> Result<(), IrVerifyError>) -> Result<bool, IrVerifyError>,
        mut active: Option<&mut dyn FnMut(usize, usize, &mut dyn FnMut(usize) -> Result<(), IrVerifyError>) -> Result<bool, IrVerifyError>>,
        charge: &mut dyn FnMut(usize) -> Result<(), IrVerifyError>,
    ) -> Result<bool, IrVerifyError> {
        let mut at = instruction;
        loop {
            charge(1)?;
            if let Some(&(block, position)) = self.statement_positions.get(&at) {
                charge(1)?;
                let guards = self.block_guards.get(&block).map(Vec::as_slice).unwrap_or(&[]);
                let (mut end, mut upper) = (0, guards.len());
                while end < upper {
                    charge(1)?;
                    let middle = end + (upper - end) / 2;
                    if guards[middle].0 < position { end = middle + 1; } else { upper = middle; }
                }
                for &(index, preceding) in guards[..end].iter().rev() {
                    charge(1)?;
                    let mut conditions = Vec::new();
                    match decoder.statement_view(preceding)? {
                        schema::StmtView::If { mut branches, else_body } => {
                            let single = branches.len() == 1;
                            for (branch_index, branch) in (&mut branches).enumerate() {
                                charge(1)?;
                                let (condition, body) = branch?;
                                if self.block_can_complete[&body.block_identity().index()] {
                                    if single && else_body.as_ref().is_some_and(|body| !self.block_can_complete[&body.block_identity().index()]) {
                                        conditions.push((condition.raw() as usize, true, false, branch_index));
                                    }
                                    break;
                                }
                                conditions.push((condition.raw() as usize, false, false, branch_index));
                            }
                        }
                        schema::StmtView::IfBool { mut branches, else_body } => {
                            let single = branches.len() == 1;
                            for (branch_index, branch) in (&mut branches).enumerate() {
                                charge(1)?;
                                let (condition, body) = branch?;
                                if self.block_can_complete[&body.block_identity().index()] {
                                    if single && else_body.as_ref().is_some_and(|body| !self.block_can_complete[&body.block_identity().index()]) {
                                        conditions.push((condition.raw() as usize, true, true, branch_index));
                                    }
                                    break;
                                }
                                conditions.push((condition.raw() as usize, false, true, branch_index));
                            }
                        }
                        schema::StmtView::Assert { value, .. } => {
                            charge(1)?;
                            conditions.push((value.raw() as usize, true, false, 0));
                        }
                        _ => {}
                    }
                    for (condition, when_true, is_boolean, branch_index) in conditions {
                        charge(1)?;
                        if proves(condition, when_true, is_boolean, Some((preceding, branch_index, at, &self.block_statements[&block][index + 1..position])), charge)?
                            && self.visible_binding(decoder.store, decoder.owner, condition, slot)? == Some(binding)
                        { return Ok(true); }
                    }
                }
            }
            let Some(parent) = self.parents[at - self.range.start] else { return Ok(false) };
            if let Some(active) = active.as_mut() {
                if active(parent, at, charge)? { return Ok(true); }
            } else if let Some((conditions, _)) = decoder.checked_branch_conditions_for_child_with(parent, at,
                |child, roots| Ok(roots.contains(&child)), |amount| charge(amount))?
            {
                for (condition, when_true, is_boolean) in conditions {
                    charge(1)?;
                    if proves(condition, when_true, is_boolean, None, charge)?
                        && self.visible_binding(decoder.store, decoder.owner, condition, slot)? == Some(binding)
                    { return Ok(true); }
                }
            }
            at = parent;
        }
    }

    fn header_returns(&self) -> impl Iterator<Item = &usize> {
        self.returns.iter().chain(self.callback_returns.values().flatten())
    }

    fn binding(&self, store: &FullStore, owner: u32, at: usize, slot: usize) -> Result<ProducerBinding, IrVerifyError> {
        self.visible_binding(store, owner, at, slot)?.ok_or_else(|| IrVerifyError::new("ground producer use has no visible lexical declaration"))
    }

    fn writer_binding(&self, store: &FullStore, owner: u32, at: usize, slot: usize) -> Result<ProducerBinding, IrVerifyError> {
        self.resolve_binding(store, owner, at, slot, Some(&self.inherited_bindings))?
            .ok_or_else(|| IrVerifyError::new("ground producer use has no visible lexical declaration"))
    }

    fn visible_binding(&self, store: &FullStore, owner: u32, at: usize, slot: usize) -> Result<Option<ProducerBinding>, IrVerifyError> {
        self.resolve_binding(store, owner, at, slot, None)
    }

    fn resolve_binding(&self, store: &FullStore, owner: u32, mut at: usize, slot: usize,
        inherited_bindings: Option<&RefCell<BTreeMap<(usize, usize), Option<ProducerBinding>>>>,
    ) -> Result<Option<ProducerBinding>, IrVerifyError> {
        self.work.borrow_mut().charge(1)?;
        #[cfg(test)] { let (queries, parents, comparisons) = self.binding_work.get(); self.binding_work.set((queries + 1, parents, comparisons)); }
        if !self.lexical_slots.contains_key(&slot) { return self.header_binding(store, owner, slot); }
        let mut inherited = Vec::new();
        let binding = 'resolve: loop {
            let mut root = at;
            loop {
                self.work.borrow_mut().charge(1)?;
                if self.region_bindings.is_empty() {
                    let local = root.checked_sub(self.range.start).filter(|&local| local < self.range.len())
                        .ok_or_else(|| IrVerifyError::new("producer lexical query belongs to another owner"))?;
                    let Some(statement) = self.statements[local] else { break 'resolve self.header_binding(store, owner, slot)? };
                    if !self.statement_positions.contains_key(&statement) { return Err(IrVerifyError::new("producer lexical anchor is not an owned statement")); }
                    root = statement;
                }
                #[cfg(test)] { let (queries, parents, comparisons) = self.binding_work.get(); self.binding_work.set((queries, parents + 1, comparisons)); }
                if let Some(binding) = self.region_bindings.get(&(root, slot)) { break 'resolve Some(*binding); }
                if let Some((block, position)) = self.statement_positions.get(&root) {
                    if let Some(declarations) = self.block_declarations.get(&(*block, slot)) {
                        let (mut end, mut upper) = (0, declarations.len());
                        while end < upper {
                            self.work.borrow_mut().charge(1)?;
                            #[cfg(test)] { let (queries, parents, comparisons) = self.binding_work.get(); self.binding_work.set((queries, parents, comparisons + 1)); }
                            let middle = end + (upper - end) / 2;
                            if declarations[middle].0 < *position { end = middle + 1; } else { upper = middle; }
                        }
                        if let Some(previous) = end.checked_sub(1) { break 'resolve Some(declarations[previous].1); }
                    }
                    let Some(parent) = self.parents[root - self.range.start] else { break 'resolve self.header_binding(store, owner, slot)? };
                    if !matches!(store.tags[parent], FullTag::StmtWith | FullTag::StmtGuard)
                        && self.declarations.get(&parent).is_some_and(|slots| slots.contains(&slot)) {
                        break 'resolve Some(ProducerBinding::ScopedDeclaration(parent, slot));
                    }
                    if let Some(inherited_bindings) = inherited_bindings {
                        self.work.borrow_mut().charge(1)?;
                        if let Some(binding) = inherited_bindings.borrow().get(&(*block, slot)) {
                            break 'resolve *binding;
                        }
                        self.work.borrow_mut().charge(1)?;
                        inherited.push(*block);
                    }
                    break;
                }
                match self.parents[root - self.range.start] {
                    Some(parent) => root = parent,
                    None => break,
                }
            }
            let Some(parent) = self.parents[root - self.range.start] else { break 'resolve self.header_binding(store, owner, slot)? };
            if self.statement_positions.contains_key(&root)
                && !matches!(store.tags[parent], FullTag::StmtWith | FullTag::StmtGuard)
                && self.declarations.get(&parent).is_some_and(|slots| slots.contains(&slot))
            {
                break 'resolve Some(ProducerBinding::ScopedDeclaration(parent, slot));
            }
            at = parent;
        };
        for block in inherited {
            self.work.borrow_mut().charge(1)?;
            inherited_bindings.unwrap().borrow_mut().insert((block, slot), binding);
        }
        Ok(binding)
    }

    fn header_binding(&self, store: &FullStore, owner: u32, slot: usize) -> Result<Option<ProducerBinding>, IrVerifyError> {
        if super::super::driver_owner_index(owner).is_some() { return Ok(Some(ProducerBinding::DriverBinding(slot))); }
        let function = store.functions[function_index(owner)?];
        if slot < function.params.len as usize { return Ok(Some(ProducerBinding::HeaderParameter(slot))); }
        for capture in bounded(function.captures, &store.captures, "producer capture range is invalid")? {
            self.work.borrow_mut().charge(1)?;
            if (capture.slot_and_flags & !(1 << 31)) as usize == slot {
                return Ok(Some(ProducerBinding::HeaderCapture(capture.binding_identity)));
            }
        }
        Ok(None)
    }
}

fn has_condition_inputs(tag: FullTag) -> bool {
    matches!(tag, FullTag::StmtIf | FullTag::StmtIfBool | FullTag::StmtWhile | FullTag::StmtWhileBool
        | FullTag::StmtPatternIf | FullTag::StmtPatternWhile | FullTag::StmtMatch | FullTag::StmtAssert
        | FullTag::ExprIf | FullTag::ExprPatternIf | FullTag::ExprMatch
        | FullTag::ExprListComp | FullTag::ExprMapComp | FullTag::ExprPipeline)
}

fn record_condition_inputs(decoder: &FullDecoder<'_>, instruction: usize, output: &mut Vec<(usize, ConditionRole)>, mut charge: impl FnMut(usize) -> Result<(), IrVerifyError>) -> Result<(), IrVerifyError> {
    match decoder.store.tags[instruction].kind() {
        schema::InstructionKind::Stmt => match decoder.statement_view(instruction)? {
            schema::StmtView::If { branches, .. } => for branch in branches { charge(1)?; output.push((branch?.0.raw() as usize, ConditionRole::Control)); },
            schema::StmtView::IfBool { branches, .. } => for branch in branches { charge(1)?; output.push((branch?.0.raw() as usize, ConditionRole::Control)); },
            schema::StmtView::PatternIf { branches, .. } => for branch in branches { charge(1)?; output.push((branch?.0.raw() as usize, ConditionRole::Control)); },
            schema::StmtView::While { condition, .. } | schema::StmtView::PatternWhile { condition, .. } => output.push((condition.raw() as usize, ConditionRole::Control)),
            schema::StmtView::WhileBool { condition, .. } => output.push((condition.raw() as usize, ConditionRole::Control)),
            schema::StmtView::Assert { value, .. } => output.push((value.raw() as usize, ConditionRole::Boolean)),
            schema::StmtView::Match { arms, .. } => for arm in arms { charge(1)?; if let Some(guard) = arm?.1 { output.push((guard.raw() as usize, ConditionRole::Boolean)); } },
            _ => {}
        },
        schema::InstructionKind::Expr => match decoder.expression_view(instruction)? {
            schema::ExprView::IfExpr { branches, .. } => for branch in branches { charge(1)?; output.push((branch?.0.raw() as usize, ConditionRole::Control)); },
            schema::ExprView::PatternIf { branches, .. } => for branch in branches { charge(1)?; output.push((branch?.0.raw() as usize, ConditionRole::Control)); },
            schema::ExprView::MatchExpr { arms, .. } => for arm in arms { charge(1)?; if let Some(guard) = arm?.1 { output.push((guard.raw() as usize, ConditionRole::Boolean)); } },
            schema::ExprView::ListComp { qualifiers, .. } | schema::ExprView::MapComp { qualifiers, .. } => for qualifier in qualifiers {
                charge(1)?;
                if let super::super::decode::CompQualifierView::If { condition, .. } = qualifier? { output.push((condition.raw() as usize, ConditionRole::Control)); }
            },
            _ => {}
        },
        _ => {}
    }
    Ok(())
}

impl GroundProducerIndex {
    pub(in crate::runtime::eval::indexed::full) fn new(store: &FullStore) -> Result<Self, IrVerifyError> {
        let mut checked_producers = Vec::new();
        for (index, fact) in store.checked_expr_types.iter().enumerate() {
            if !fact.is_contract() { continue; }
            let Some(&tag) = store.tags.get(fact.instruction as usize) else { continue };
            if store.semantic.to_type(fact.type_id)?.contains_scheme_callable() { checked_producers.push(index); continue; }
            if !matches!(tag, FullTag::ExprOk | FullTag::ExprErr | FullTag::ExprCheckedValue | FullTag::ExprRange | FullTag::ExprCapture | FullTag::ExprErrorContext) { continue; }
            let ty = store.semantic.to_type(fact.type_id)?;
            if (tag == FullTag::ExprCheckedValue && ty.contains_typed_callable())
                || (tag == FullTag::ExprRange && matches!(&ty, Type::Stream(_)))
                || (tag == FullTag::ExprCapture && matches!(&ty, Type::Result(ok, _) if crate::runtime::eval::stream_return_item(ok).is_some()))
                || (tag == FullTag::ExprErrorContext && crate::runtime::eval::stream_return_item(&ty).is_some())
                || fact.authenticates_result_constructor(tag, &ty)?
            { checked_producers.push(index); }
        }
        let mut effect_demand_work = 0usize;
        let mut selected_demand = !store.registered_calls.is_empty() || !checked_producers.is_empty();
        if !selected_demand {
            selected_demand = store.call_instantiations.iter().any(|call| {
                effect_demand_work += 1;
                matches!(store.tags.get(call.instruction as usize), Some(FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall))
            });
        }
        if !selected_demand {
            for instruction in 0..store.tags.len() {
                effect_demand_work += 1;
                if has_condition_inputs(store.tags[instruction])
                    || matches!(store.tags[instruction], FullTag::ExprCapture | FullTag::ExprRetry)
                    || super::super::verify_effects::needs_operand_proof(store, instruction)?
                {
                    selected_demand = true;
                    break;
                }
            }
        }
        if !selected_demand {
            selected_demand = store.captures.iter().any(|capture| { effect_demand_work += 1; capture.slot_and_flags & (1 << 31) != 0 })
                || store.driver_slots.iter().any(|slot| { effect_demand_work += 1; slot.flags & super::super::DRIVER_SLOT_MUTABLE != 0 && slot.flags & (super::super::DRIVER_SLOT_TEMPORARY | super::super::DRIVER_SLOT_DEFINITION) == 0 && slot.binding_identity != IR_NONE });
        }
        if !selected_demand {
            for function in &store.functions {
                effect_demand_work += 1;
                if !matches!(function.contract, FullCallableContract::Scheme(_)) { continue; }
                for capture in bounded(function.captures, &store.captures, "generic capture range is invalid")? {
                    effect_demand_work += 1;
                    if capture.slot_and_flags & (1 << 31) == 0 && store.semantic.type_tag(capture.type_id)? == super::super::super::semantic::TypeTag::Record {
                        selected_demand = true;
                        break;
                    }
                }
                if selected_demand { break; }
            }
        }
        if !selected_demand {
            selected_demand = store.functions.iter().enumerate().any(|(index, _)| {
                effect_demand_work += 1;
                store.function_metadata.get(index).is_some_and(|metadata| metadata.callable_kind == CallableKind::Stream)
            });
        }
        if !selected_demand {
            for (index, function) in store.functions.iter().enumerate() {
                effect_demand_work += 1;
                if !matches!(function.contract, FullCallableContract::Scheme(_)) { continue; }
                let range = store.function_instruction_range(index)?;
                if store.tags[range].iter().any(|tag| { effect_demand_work += 1; *tag == FullTag::StmtForRecord }) {
                    selected_demand = true;
                    break;
                }
            }
        }
        let driver_output_demand = !selected_demand && store.driver_steps.iter().any(|step| {
            effect_demand_work += 1;
            step.tag == super::super::FullDriverTag::LetRecord
        });
        let demand = if driver_output_demand || selected_demand {
            crate::runtime::eval::ProducerDemand::Needed
        } else { crate::runtime::eval::ProducerDemand::Absent };
        Ok(Self { demand, scheme_scopes: OnceCell::new(), checked_producers, capture_retry_roots: Vec::new(), driver_outputs: Vec::new(), effect_operations: Vec::new(), conditions: Vec::new(), storage_reads: Vec::new(), effect_demand_work, owners: BTreeMap::new(), scheme_owners: BTreeMap::new(), starts: BTreeMap::new(),
            binding_writers: Vec::new(), storage_writers: BTreeMap::new(), module_exports: BTreeMap::new(), function_signatures: None, work: None })
    }

    pub(in crate::runtime::eval::indexed::full) fn scheme_scopes(&self, store: &FullStore) -> Result<&super::VerifiedSchemeScopes, IrVerifyError> {
        if self.scheme_scopes.get().is_none() {
            self.scheme_scopes.set(super::prepare_tables(store)?)
                .map_err(|_| IrVerifyError::new("scheme scopes were initialized more than once"))?;
        }
        self.scheme_scopes.get().ok_or_else(|| IrVerifyError::new("scheme scopes were not initialized"))
    }

    pub(in crate::runtime::eval::indexed::full) fn prefix_context(&self, owner: u32, range: &std::ops::Range<usize>) -> Result<Option<ProducerPrefixContext<'_>>, IrVerifyError> {
        if self.demand == crate::runtime::eval::ProducerDemand::Absent { return Ok(None); }
        let evidence = self.owners.get(&owner).ok_or_else(|| IrVerifyError::new("producer prefix owner was not structurally verified"))?;
        if evidence.range != *range { return Err(IrVerifyError::new("producer prefix owner range differs from its structural proof")); }
        Ok(Some(ProducerPrefixContext { owner, evidence, index: self }))
    }

    pub(in crate::runtime::eval::indexed::full) fn demand(&self) -> crate::runtime::eval::ProducerDemand {
        self.demand
    }

    pub(in crate::runtime::eval::indexed::full) fn verify_callable_facts(&self, store: &FullStore) -> Result<(), IrVerifyError> {
        if self.demand == crate::runtime::eval::ProducerDemand::Absent { return Ok(()); }
        for (&owner, evidence) in &self.owners {
            let driver = super::super::driver_owner_index(owner);
            let mut decoder = match driver {
                Some(step) => decoder_for_driver_step(store, step)?,
                None => decoder_for_function(store, function_index(owner)?)?,
            };
            if decoder.owner != owner || decoder.instruction_range != evidence.range {
                return Err(IrVerifyError::new("callable fact owner disagrees with its structural proof"));
            }
            decoder.borrowed_instruction_parents = Some(super::super::InstructionParentContext { owner, range: evidence.range.clone(), parents: &evidence.parents, statements: &evidence.statements, children: &evidence.children, siblings: &evidence.siblings });
            decoder.producer_prefix = self.prefix_context(owner, &decoder.instruction_range)?;
            decoder.producer_demand = self.demand;
            decoder.function_signature_index.clone_from(&self.function_signatures);
            *decoder.return_analysis.borrow_mut() = Some(super::super::ReturnAnalysisSource::Shared(Rc::clone(&evidence.return_analysis)));
            decoder.prepared_constant_work.set(evidence.prepared_constant_work);
            decoder.verify_checked_expression_facts()?;
            for &instruction in &evidence.typed_calls {
                evidence.work.borrow_mut().charge(1)?;
                let schema::ExprView::TypedCall { callee, signature, .. } = decoder.expression_view(instruction)? else {
                    return Err(IrVerifyError::new("typed callable proof has another physical expression"));
                };
                if self.scheme_owners.contains_key(&owner) { continue; }
                decoder.verify_typed_callee_signature(callee.raw() as usize, signature.id)?;
            }
            decoder.verify_callable_slot_initializers()?;
            if driver.is_some() { decoder.verify_temporary_driver_slot_types()?; }
        }
        Ok(())
    }

    pub(super) fn scheme_return_analysis(&self, function: IrFunctionId) -> Result<Rc<super::super::IndexedReturnAnalysis>, IrVerifyError> {
        self.scheme_owners.get(&function.raw()).map(|owner| Rc::clone(&owner.return_analysis))
            .ok_or_else(|| IrVerifyError::new("generic body has no authenticated return analysis"))
    }

    pub(super) fn scheme_prepared_constant_work(&self, function: IrFunctionId) -> Result<usize, IrVerifyError> {
        self.scheme_owners.get(&function.raw()).map(|owner| owner.prepared_constant_work)
            .ok_or_else(|| IrVerifyError::new("generic body has no authenticated constant data work"))
    }

    pub(in crate::runtime::eval::indexed::full) fn record(&mut self, decoder: &FullDecoder<'_>) -> Result<(), IrVerifyError> {
        if IrFunctionId::from_raw(decoder.owner).and_then(|owner| decoder.store.functions.get(owner.index())).is_some_and(|function| matches!(function.contract, FullCallableContract::Scheme(_))) {
            let analysis = decoder.share_return_analysis()?;
            if self.demand == crate::runtime::eval::ProducerDemand::Absent {
                let work = self.work.get_or_insert_with(|| Rc::new(RefCell::new(SchemeInstantiationBudget::new(decoder.store))));
                work.borrow_mut().charge(decoder.instruction_range.len() + analysis.loop_values.as_ref().map_or(0, |values| values.len() + values.values().map(Vec::len).sum::<usize>()) + analysis.loop_unit_exits.as_ref().map_or(0, |exits| exits.len()))?;
            }
            if self.scheme_owners.insert(decoder.owner, VerifiedSchemeOwner { return_analysis: analysis, prepared_constant_work: decoder.prepared_constant_work.get() }).is_some() { return Err(IrVerifyError::new("generic return analysis has multiple owners")); }
        }
        if self.demand == crate::runtime::eval::ProducerDemand::Absent { return Ok(()); }
        if let Some(step) = super::super::driver_owner_index(decoder.owner)
            && decoder.store.driver_steps[step].tag == super::super::FullDriverTag::LetRecord
        { self.driver_outputs.push(step); }
        if self.function_signatures.is_none() { self.function_signatures.clone_from(&decoder.function_signature_index); }
        let work = Rc::clone(self.work.get_or_insert_with(|| Rc::new(RefCell::new(SchemeInstantiationBudget::new(decoder.store)))));
        work.borrow_mut().charge(std::mem::take(&mut self.effect_demand_work))?;
        let range = decoder.instruction_range.clone();
        let return_analysis = decoder.share_return_analysis()?;
        work.borrow_mut().charge(range.len() + return_analysis.uncaught_try_roots.len() + return_analysis.loop_transfers.len() + return_analysis.loop_values.as_ref().map_or(0, |values| values.len() + values.values().map(Vec::len).sum::<usize>()) + return_analysis.loop_unit_exits.as_ref().map_or(0, |exits| exits.len()))?;
        let mut owner = ProducerOwner { prepared_constant_work: decoder.prepared_constant_work.get(), work: Rc::clone(&work), children: vec![IR_NONE; range.len()], siblings: vec![IR_NONE; range.len()], writers: BTreeMap::new(), storage_bindings: BTreeMap::new(), parents: vec![None; range.len()], statements: vec![None; range.len()], statement_positions: BTreeMap::new(), block_declarations: BTreeMap::new(), lexical_slots: BTreeMap::new(), inherited_bindings: RefCell::default(), #[cfg(test)] binding_work: Cell::new((0, 0, 0)), #[cfg(test)] census_instruction_visits: 0, block_statements: BTreeMap::new(), block_guards: BTreeMap::new(), block_can_complete: BTreeMap::new(), typed_calls: Vec::new(), completed_optional_reads: RefCell::default(), completed_union_reads: RefCell::default(), snapshot_predicates: RefCell::default(), declarations: BTreeMap::new(), following_declarations: BTreeMap::new(), region_bindings: BTreeMap::new(), pattern_sources: BTreeMap::new(), with_errors: decoder.checked_with_initializer_error_types()?, comprehension_sources: BTreeMap::new(), pipeline_sources: BTreeMap::new(), callback_returns: BTreeMap::new(), pipeline_groups: BTreeMap::new(), pipeline_stages: BTreeMap::new(), unreachable_roots: BTreeSet::new(), returns: Vec::new(), yield_sites: Vec::new(), return_analysis, range: range.clone() };
        let mut roots = Vec::new();
        let mut slot_reads = Vec::new();
        let mut pattern_instructions = Vec::new();
        let mut physical_statements = 0;
        let mut complete_statement_positions = decoder.block_states.as_ref().is_some_and(|blocks| blocks.finished_owned.get() == blocks.owned);
        for instruction in range.clone() {
            #[cfg(test)] { owner.census_instruction_visits += 1; }
            work.borrow_mut().charge(1)?;
            if matches!(decoder.store.tags[instruction], FullTag::ExprCapture | FullTag::ExprRetry) {
                self.capture_retry_roots.push(instruction);
            }
            if decoder.store.tags[instruction] == FullTag::ExprTypedCall { owner.typed_calls.push(instruction); }
            if decoder.store.tags[instruction].kind() == schema::InstructionKind::Stmt { physical_statements += 1; }
            if matches!(decoder.store.tags[instruction], FullTag::StmtMatch | FullTag::ExprMatch
                | FullTag::ExprPatternIf | FullTag::StmtPatternIf | FullTag::StmtPatternWhile
                | FullTag::ExprListComp | FullTag::ExprMapComp | FullTag::ExprPipeline
                | FullTag::StmtWith | FullTag::StmtGuard)
            {
                work.borrow_mut().charge(1)?;
                pattern_instructions.push(instruction);
            }
            if super::super::verify_effects::needs_operand_proof(decoder.store, instruction)? { self.effect_operations.push(instruction); }
            if has_condition_inputs(decoder.store.tags[instruction]) {
                record_condition_inputs(decoder, instruction, &mut self.conditions, |amount| work.borrow_mut().charge(amount))?;
            }
            if decoder.store.tags[instruction] == FullTag::ExprParam {
                let schema::ExprView::Param(slot) = decoder.expression_view(instruction)? else { unreachable!() };
                slot_reads.push((instruction, slot));
            }
            owner.parents[instruction - range.start] = decoder.instruction_parent_of(instruction)?;
            owner.statements[instruction - range.start] = decoder.instruction_statement_of(instruction)?;
            if let Some(parent) = owner.parents[instruction - range.start] {
                let parent = parent.checked_sub(range.start).filter(|index| *index < range.len()).ok_or_else(|| IrVerifyError::new("ground producer parent belongs to another owner"))?;
                owner.siblings[instruction - range.start] = owner.children[parent];
                owner.children[parent] = instruction as u32;
            } else { roots.push(instruction); }
            if decoder.store.tags[instruction].kind() == schema::InstructionKind::Stmt
                && !matches!(decoder.store.tags[instruction], FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath | FullTag::StmtDefaultParameter | FullTag::StmtScanBytes)
            {
                let declared = match decoder.statement_view(instruction)? {
                    schema::StmtView::ScanLines { line_slot, .. } => vec![line_slot],
                    _ => decoder.statement_written_slots(instruction)?.0,
                };
                owner.declarations.insert(instruction, declared);
                let following = match decoder.statement_view(instruction)? {
                    schema::StmtView::Let { slot, .. } | schema::StmtView::LetInt { slot, .. } | schema::StmtView::LetBool { slot, .. } => vec![slot],
                    schema::StmtView::LetRecord { target, .. } | schema::StmtView::Guard { target, .. } => {
                        let mut slots = Vec::new();
                        decoder.comp_target_written_slots(target, &mut slots)?;
                        slots
                    }
                    _ => Vec::new(),
                };
                owner.following_declarations.insert(instruction, following);
            }
        }
        if let Some(blocks) = &decoder.block_states {
            for (&block, &state) in blocks.states.borrow().iter() {
                complete_statement_positions &= state == 2;
                work.borrow_mut().charge(1)?;
                if decoder.store.blocks[block].flags & super::super::BLOCK_STATEMENTS == 0 { continue; }
                let statements = decoder.store.payload(decoder.store.blocks[block].instructions)?.iter().skip(1).map(|word| *word as usize).collect::<Vec<_>>();
                let mut reachable = true;
                let mut guards = Vec::new();
                for (position, &statement) in statements.iter().enumerate() {
                    work.borrow_mut().charge(1)?;
                    complete_statement_positions &= range.contains(&statement)
                        && decoder.store.tags.get(statement).is_some_and(|tag| tag.kind() == schema::InstructionKind::Stmt);
                    complete_statement_positions &= owner.statement_positions.insert(statement, (block, position)).is_none();
                    if matches!(decoder.store.tags[statement], FullTag::StmtIf | FullTag::StmtIfBool | FullTag::StmtAssert) {
                        work.borrow_mut().charge(1)?;
                        guards.push((position, statement));
                    }
                    for &slot in owner.following_declarations.get(&statement).into_iter().flatten() {
                        owner.block_declarations.entry((block, slot)).or_default().push((position, ProducerBinding::ScopedDeclaration(statement, slot)));
                    }
                    if reachable { reachable = !super::super::indexed_stmt_can_return_with_decoder(decoder.store, statement, Some(decoder))?; }
                    else { owner.unreachable_roots.insert(statement); }
                }
                if !guards.is_empty() {
                    work.borrow_mut().charge(1)?;
                    owner.block_guards.insert(block, guards);
                }
                owner.block_can_complete.insert(block, reachable);
                owner.block_statements.insert(block, statements);
            }
        }
        let mut callback_anchors = BTreeMap::new();
        for instruction in pattern_instructions {
            #[cfg(test)] { owner.census_instruction_visits += 1; }
            work.borrow_mut().charge(1)?;
            if decoder.store.tags[instruction] == FullTag::StmtMatch {
                let schema::StmtView::Match { value, mut arms, .. } = decoder.statement_view(instruction)? else { unreachable!("match statement has another view") };
                let mut preceding = Vec::new();
                for arm in &mut arms {
                    work.borrow_mut().charge(1 + preceding.len())?;
                    let (pattern, guard, body) = arm?;
                    let block = body.block_identity().index();
                    let roots = owner.block_statements.get(&block).cloned().unwrap_or_default();
                    if let Some(&root) = roots.first() {
                        for slot in decoder.pattern_capture_slots(pattern.raw() as usize)? {
                            work.borrow_mut().charge(1 + preceding.len() + roots.len())?;
                            let binding = ProducerBinding::ScopedDeclaration(root, slot);
                            for &region in &roots { owner.region_bindings.insert((region, slot), binding); }
                            if let Some(guard) = guard { owner.region_bindings.insert((guard.raw() as usize, slot), binding); }
                            owner.pattern_sources.insert(binding, PatternProducer { source: value.raw() as usize, pattern: pattern.raw(), preceding: preceding.clone() });
                        }
                    }
                    preceding.push((pattern.raw(), guard.is_some()));
                }
            }
            if decoder.store.tags[instruction] == FullTag::ExprMatch {
                let schema::ExprView::MatchExpr { value, mut arms, .. } = decoder.expression_view(instruction)? else { unreachable!("match instruction has another view") };
                let mut preceding = Vec::new();
                for arm in &mut arms {
                    work.borrow_mut().charge(1 + preceding.len())?;
                    let (pattern, guard, result) = arm?;
                    let root = result.raw() as usize;
                    for slot in decoder.pattern_capture_slots(pattern.raw() as usize)? {
                        work.borrow_mut().charge(1 + preceding.len())?;
                        let binding = ProducerBinding::ScopedDeclaration(root, slot);
                        owner.region_bindings.insert((root, slot), binding);
                        if let Some(guard) = guard { owner.region_bindings.insert((guard.raw() as usize, slot), binding); }
                        owner.pattern_sources.insert(binding, PatternProducer { source: value.raw() as usize, pattern: pattern.raw(), preceding: preceding.clone() });
                    }
                    preceding.push((pattern.raw(), guard.is_some()));
                }
            }
            match decoder.store.tags[instruction] {
                FullTag::StmtWith => {
                    let schema::StmtView::With { mut bindings, body, else_body, else_param_slot, .. } = decoder.statement_view(instruction)? else { unreachable!() };
                    let mut preceding = Vec::new();
                    for binding in &mut bindings {
                        let (slot, value) = binding?;
                        work.borrow_mut().charge(1 + preceding.len())?;
                        for &(previous, binding) in &preceding { owner.region_bindings.insert((value.raw() as usize, previous), binding); }
                        preceding.push((slot, ProducerBinding::ScopedDeclaration(instruction, slot)));
                    }
                    let roots = owner.block_statements.get(&body.block_identity().index()).cloned().unwrap_or_default();
                    work.borrow_mut().charge(roots.len().saturating_mul(preceding.len()))?;
                    for root in roots { for &(slot, binding) in &preceding { owner.region_bindings.insert((root, slot), binding); } }
                    if let Some(slot) = else_param_slot {
                        let roots = owner.block_statements.get(&else_body.block_identity().index()).cloned().unwrap_or_default();
                        work.borrow_mut().charge(roots.len())?;
                        for root in roots { owner.region_bindings.insert((root, slot), ProducerBinding::ScopedDeclaration(instruction, slot)); }
                    }
                }
                FullTag::StmtGuard => {
                    let schema::StmtView::Guard { else_body, else_param_slot, .. } = decoder.statement_view(instruction)? else { unreachable!() };
                    if let Some(slot) = else_param_slot {
                        let roots = owner.block_statements.get(&else_body.block_identity().index()).cloned().unwrap_or_default();
                        work.borrow_mut().charge(roots.len())?;
                        for root in roots { owner.region_bindings.insert((root, slot), ProducerBinding::ScopedDeclaration(instruction, slot)); }
                    }
                }
                FullTag::ExprPatternIf => {
                    let schema::ExprView::PatternIf { mut branches, .. } = decoder.expression_view(instruction)? else { unreachable!("pattern conditional has another view") };
                    for branch in &mut branches {
                        let (condition, value, captures) = branch?;
                        install_condition_regions(decoder, &mut owner, condition.raw() as usize, captures, &[value.raw() as usize])?;
                    }
                }
                FullTag::StmtPatternIf => {
                    let schema::StmtView::PatternIf { mut branches, .. } = decoder.statement_view(instruction)? else { unreachable!("pattern conditional has another view") };
                    for branch in &mut branches {
                        let (condition, body, captures) = branch?;
                        let roots = owner.block_statements.get(&body.block_identity().index()).cloned().unwrap_or_default();
                        install_condition_regions(decoder, &mut owner, condition.raw() as usize, captures, &roots)?;
                    }
                }
                FullTag::StmtPatternWhile => {
                    let schema::StmtView::PatternWhile { condition, body, captures, .. } = decoder.statement_view(instruction)? else { unreachable!("pattern loop has another view") };
                    let roots = owner.block_statements.get(&body.block_identity().index()).cloned().unwrap_or_default();
                    install_condition_regions(decoder, &mut owner, condition.raw() as usize, captures, &roots)?;
                }
                _ => {}
            }
            if matches!(decoder.store.tags[instruction], FullTag::ExprListComp | FullTag::ExprMapComp) {
                let (mut qualifiers, outputs) = match decoder.expression_view(instruction)? {
                    schema::ExprView::ListComp { qualifiers, value, .. } => (qualifiers, vec![value.raw() as usize]),
                    schema::ExprView::MapComp { qualifiers, key, value, .. } => (qualifiers, vec![key.raw() as usize, value.raw() as usize]),
                    _ => unreachable!("comprehension has another expression view"),
                };
                let mut active = Vec::new();
                for qualifier in &mut qualifiers {
                    work.borrow_mut().charge(1 + active.len())?;
                    match qualifier? {
                        super::super::decode::CompQualifierView::For { target, iter, .. } => {
                            let iter = iter.raw() as usize;
                            for &(slot, binding) in &active { owner.region_bindings.insert((iter, slot), binding); }
                            let mut slots = Vec::new();
                            decoder.comp_target_written_slots(target, &mut slots)?;
                            for slot in slots {
                                work.borrow_mut().charge(1)?;
                                let binding = ProducerBinding::ScopedDeclaration(iter, slot);
                                owner.comprehension_sources.insert(binding, (instruction, iter));
                                active.push((slot, binding));
                            }
                        }
                        super::super::decode::CompQualifierView::If { condition, .. } => {
                            for &(slot, binding) in &active { owner.region_bindings.insert((condition.raw() as usize, slot), binding); }
                        }
                    }
                }
                work.borrow_mut().charge(outputs.len().saturating_mul(active.len()))?;
                for output in outputs { for &(slot, binding) in &active { owner.region_bindings.insert((output, slot), binding); } }
            }
            if decoder.store.tags[instruction] != FullTag::ExprPipeline { continue; }
            let schema::ExprView::ListPipeline { mut stages, .. } = decoder.expression_view(instruction)? else { unreachable!("pipeline has another view") };
            let mut position = 0;
            while let Some(stage) = stages.next_with_id() {
                work.borrow_mut().charge(1)?;
                let stage = stage?;
                owner.pipeline_stages.entry(instruction).or_default().push(stage.id.raw());
                let stage = stage.view;
                use super::super::StageView as S;
                if let S::Range { start, end } = &stage
                    && let Some(bounds) = super::super::indexed_configured_range_bounds_with(
                        decoder, start.raw(), end.raw(), &mut |amount| work.borrow_mut().charge(amount))?
                {
                    work.borrow_mut().charge(2)?;
                    let binding = ProducerBinding::ScopedDeclaration(bounds.end_root, bounds.end_slot);
                    let source = owner.pattern_sources.get(&binding).ok_or_else(|| IrVerifyError::new("configured range binding has no pattern producer"))?;
                    if source.source != bounds.end_subject as usize || source.pattern != bounds.end_pattern || !source.preceding.is_empty() {
                        return Err(IrVerifyError::new("configured range binding disagrees with its pattern producer"));
                    }
                    if owner.region_bindings.insert((end.raw() as usize, bounds.end_slot), binding).is_some() {
                        return Err(IrVerifyError::new("configured range end has another binding region"));
                    }
                }
                match &stage {
                    S::Where { predicate, .. } | S::Any { predicate, .. } | S::All { predicate, .. } => self.conditions.push((predicate.raw() as usize, ConditionRole::Boolean)),
                    S::WhereBlock { value, .. } | S::AnyBlock { value, .. } | S::AllBlock { value, .. } => self.conditions.push((value.raw() as usize, ConditionRole::Boolean)),
                    _ => {}
                }
                let mut all = rustc_hash::FxHashSet::default();
                super::super::indexed_stage_callback_roots(decoder, &stage, &mut all)?;
                let mut reduced = rustc_hash::FxHashSet::default();
                if let super::super::StageView::ParMapFlatMapReduceBy { ref reduce_body, reduce_value, .. } = stage {
                    reduced.insert(reduce_value.raw() as usize);
                    reduced.extend(decoder.store.payload(decoder.store.blocks[reduce_body.block_identity().index()].instructions)?.iter().skip(1).map(|word| *word as usize));
                }
                work.borrow_mut().charge(all.len() + reduced.len())?;
                for is_reduced in [false, true] {
                    let roots = all.iter().copied().filter(|root| reduced.contains(root) == is_reduced).collect::<Vec<_>>();
                    let Some(&anchor) = roots.iter().min() else { continue };
                    owner.pipeline_groups.insert((instruction, position, is_reduced), anchor);
                    for &root in &roots { callback_anchors.insert(root, anchor); }
                    for (slot, role) in FullDecoder::pipeline_stage_slots(&stage) {
                        if matches!(role, super::super::PipelineStageSlotRole::ReducedItem { .. }) != is_reduced { continue; }
                        let binding = ProducerBinding::ScopedDeclaration(anchor, slot);
                        owner.pipeline_sources.insert(binding, PipelineProducer { pipeline: instruction, stage: position, role });
                        for &root in &roots { owner.region_bindings.insert((root, slot), binding); }
                    }
                }
                position += 1;
            }
        }
        for (slot, binding) in owner.block_declarations.values().flatten().map(|(_, binding)| match *binding {
                ProducerBinding::ScopedDeclaration(_, slot) => (slot, *binding),
                _ => unreachable!(),
            })
            .chain(owner.region_bindings.iter().map(|((_, slot), binding)| (*slot, *binding)))
            .chain(owner.declarations.iter().flat_map(|(declaration, slots)| slots.iter().map(move |slot| (*slot, ProducerBinding::ScopedDeclaration(*declaration, *slot)))))
            .chain(owner.pattern_sources.keys().chain(owner.comprehension_sources.keys()).chain(owner.pipeline_sources.keys()).map(|binding| match *binding {
                ProducerBinding::ScopedDeclaration(_, slot) => (slot, *binding),
                _ => unreachable!(),
            }))
        {
            work.borrow_mut().charge(1)?;
            owner.lexical_slots.entry(slot).and_modify(|identity| {
                if !matches!(*identity, LexicalSlotIdentity::Unique(previous) if previous == binding) { *identity = LexicalSlotIdentity::Reused; }
            }).or_insert(LexicalSlotIdentity::Unique(binding));
        }
        if super::super::driver_owner_index(decoder.owner).is_none() {
            let function = decoder.store.functions[function_index(decoder.owner)?];
            let mut header_end = function.params.len as usize;
            for capture in bounded(function.captures, &decoder.store.captures, "lexical preflight capture range is invalid")? {
                work.borrow_mut().charge(1)?;
                header_end = header_end.max(((capture.slot_and_flags & !(1 << 31)) as usize).saturating_add(1));
            }
            let negative_preflight = |binding, slot| -> Result<bool, IrVerifyError> {
                work.borrow_mut().charge(1)?;
                Ok(slot >= header_end && matches!(owner.lexical_slots.get(&slot), Some(LexicalSlotIdentity::Unique(unique)) if *unique == binding))
            };
            for (&binding, source) in &owner.pipeline_sources {
                let ProducerBinding::ScopedDeclaration(_, slot) = binding else { unreachable!("pipeline producer has a header binding") };
                if negative_preflight(binding, slot)? { continue; }
                if owner.visible_binding(decoder.store, decoder.owner, source.pipeline, slot)?.is_some() {
                    return Err(IrVerifyError::new("pipeline producer declaration overwrites an active lexical slot"));
                }
            }
            for (&binding, source) in &owner.pattern_sources {
                let ProducerBinding::ScopedDeclaration(_, slot) = binding else { unreachable!("pattern producer has a header binding") };
                if negative_preflight(binding, slot)? { continue; }
                if let Some(active) = owner.visible_binding(decoder.store, decoder.owner, source.source, slot)? {
                    return Err(IrVerifyError::new(format!("pattern producer {binding:?} overwrites active lexical slot {active:?}")));
                }
            }
            for (&binding, &(_, iter)) in &owner.comprehension_sources {
                let ProducerBinding::ScopedDeclaration(_, slot) = binding else { unreachable!("comprehension producer has a header binding") };
                if negative_preflight(binding, slot)? { continue; }
                if owner.visible_binding(decoder.store, decoder.owner, iter, slot)?.is_some() {
                    return Err(IrVerifyError::new("comprehension producer declaration overwrites an active lexical slot"));
                }
            }
            for (&declaration, slots) in &owner.declarations {
                for &slot in slots {
                    if negative_preflight(ProducerBinding::ScopedDeclaration(declaration, slot), slot)? { continue; }
                    if owner.visible_binding(decoder.store, decoder.owner, declaration, slot)?.is_some() {
                        return Err(IrVerifyError::new("scoped producer declaration overwrites an active lexical slot"));
                    }
                }
            }
        }
        let trivial_flow = callback_anchors.is_empty() && owner.unreachable_roots.is_empty()
            && complete_statement_positions && owner.statement_positions.len() == physical_statements;
        let mut statements = owner.statement_positions.keys().copied();
        let mut pending = if trivial_flow { Vec::new() } else {
            roots.into_iter().map(|instruction| (instruction, None, true)).collect::<Vec<_>>()
        };
        loop {
            let next = if trivial_flow { statements.next().map(|instruction| (instruction, None, true)) } else { pending.pop() };
            let Some((instruction, callback, reachable)) = next else { break };
            #[cfg(test)] { owner.census_instruction_visits += 1; }
            work.borrow_mut().charge(1)?;
            if decoder.store.tags[instruction].kind() == schema::InstructionKind::Stmt {
                for slot in decoder.statement_written_slots(instruction)?.0 {
                    let binding = if owner.declarations.get(&instruction).is_some_and(|slots| slots.contains(&slot)) {
                        ProducerBinding::ScopedDeclaration(instruction, slot)
                    } else {
                        owner.writer_binding(decoder.store, decoder.owner, instruction, slot)?
                    };
                    owner.writers.entry(binding).or_default().push(instruction);
                }
            }
            if matches!(decoder.store.tags[instruction], FullTag::StmtYield | FullTag::StmtYieldDelegate) {
                work.borrow_mut().charge(1)?;
                owner.yield_sites.push(instruction);
            }
            let reachable = reachable && !owner.unreachable_roots.contains(&instruction);
            let callback = callback_anchors.get(&instruction).copied().or(callback);
            if reachable && decoder.store.tags[instruction] == FullTag::StmtReturn {
                match callback {
                    Some(anchor) => owner.callback_returns.entry(anchor).or_default().push(instruction),
                    None => owner.returns.push(instruction),
                }
            }
            if !trivial_flow {
                let mut child = owner.children[instruction - range.start];
                while child != IR_NONE {
                    pending.push((child as usize, callback, reachable));
                    child = owner.siblings[child as usize - range.start];
                }
            }
        }
        let mut storage_roots = BTreeMap::new();
        let mut has_mutable_storage = false;
        if let Some(step) = super::super::driver_owner_index(decoder.owner) {
            let driver = decoder.store.driver_steps[step];
            for slot in bounded(driver.slots, &decoder.store.driver_slots, "storage writer slot range is invalid")? {
                work.borrow_mut().charge(1)?;
                let binding = ProducerBinding::DriverBinding(slot.slot as usize);
                storage_roots.insert(binding, (slot.binding_identity, slot.slot as usize, slot.type_id));
                let mutable = slot.flags & super::super::DRIVER_SLOT_MUTABLE != 0 && slot.flags & (super::super::DRIVER_SLOT_TEMPORARY | super::super::DRIVER_SLOT_DEFINITION) == 0 && slot.binding_identity != IR_NONE;
                owner.storage_bindings.insert(binding, StorageBinding { identity: slot.binding_identity, ty: slot.type_id, mutable });
                has_mutable_storage |= mutable;
            }
            if driver.tag == super::super::FullDriverTag::Assign {
                let mut payload = super::super::FullCursor::new(decoder.store.payload(driver.data.range())?);
                let _: Name = super::super::FullCodec::decode(decoder, &mut payload)?;
                let identity = payload.raw()?;
                work.borrow_mut().charge(1)?;
                self.storage_writers.entry(identity).or_default().push(StorageWriter::DriverAssign { step });
            }
        } else {
            let function = decoder.store.functions[function_index(decoder.owner)?];
            for capture in bounded(function.captures, &decoder.store.captures, "storage writer capture range is invalid")? {
                work.borrow_mut().charge(1)?;
                let slot = (capture.slot_and_flags & !(1 << 31)) as usize;
                storage_roots.insert(ProducerBinding::HeaderCapture(capture.binding_identity), (capture.binding_identity, slot, capture.type_id));
                let mutable = capture.slot_and_flags & (1 << 31) != 0;
                owner.storage_bindings.insert(ProducerBinding::HeaderCapture(capture.binding_identity), StorageBinding { identity: capture.binding_identity, ty: capture.type_id, mutable });
                has_mutable_storage |= mutable;
            }
        }
        if has_mutable_storage {
            for (instruction, slot) in slot_reads {
                work.borrow_mut().charge(1)?;
                let binding = owner.binding(decoder.store, decoder.owner, instruction, slot)?;
                if let Some(source) = owner.storage_bindings.get(&binding) && source.mutable {
                    self.storage_reads.push((decoder.owner, instruction, source.ty));
                }
            }
        }
        for (binding, writers) in &owner.writers {
            let Some(&(identity, slot, root_type)) = storage_roots.get(binding) else { continue };
            for &instruction in writers {
                work.borrow_mut().charge(1)?;
                self.storage_writers.entry(identity).or_default().push(StorageWriter::Statement { owner: decoder.owner, instruction, slot, root_type });
            }
        }
        if !range.is_empty() && self.starts.insert(range.start, decoder.owner).is_some() { return Err(IrVerifyError::new("ground producer owner ranges overlap")); }
        if self.owners.insert(decoder.owner, owner).is_some() { return Err(IrVerifyError::new("ground producer owner is duplicated")); }
        Ok(())
    }

    pub(in crate::runtime::eval::indexed::full) fn set_binding_writers(&mut self, writers: &[Option<(usize, usize, u32)>]) -> Result<(), IrVerifyError> {
        if !self.owners.is_empty() {
            self.work.as_ref().ok_or_else(|| IrVerifyError::new("producer writers have no work budget"))?.borrow_mut().charge(writers.len())?;
            self.binding_writers = writers.to_vec();
        }
        Ok(())
    }

    pub(in crate::runtime::eval::indexed::full) fn record_module_exports(&mut self, store: &FullStore, owner: u32, exports: &[super::super::LoweredModuleExport]) -> Result<(), IrVerifyError> {
        if self.demand == crate::runtime::eval::ProducerDemand::Absent { return Ok(()); }
        self.work.get_or_insert_with(|| Rc::new(RefCell::new(SchemeInstantiationBudget::new(store)))).borrow_mut().charge(exports.len())?;
        let values = exports.iter().filter_map(|export| match export {
            super::super::LoweredModuleExport::Value { name, binding: super::super::LoweredModuleExportBinding::Encoded(binding) } => Some((*name, *binding)),
            _ => None,
        }).collect();
        self.module_exports.insert(owner, values);
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::runtime::eval::indexed::full) fn census_instruction_work(&self) -> (usize, usize) {
        self.owners.values().fold((0, 0), |(visits, instructions), owner|
            (visits + owner.census_instruction_visits, instructions + owner.range.len()))
    }

    #[cfg(test)]
    pub(in crate::runtime::eval::indexed::full) fn census_matches_depth_first(&self, decoder: &FullDecoder<'_>) -> Result<bool, IrVerifyError> {
        let owner = &self.owners[&decoder.owner];
        assert!(owner.unreachable_roots.is_empty() && owner.pipeline_sources.is_empty());
        let mut pending = owner.range.clone().filter(|&instruction| owner.parents[instruction - owner.range.start].is_none()).collect::<Vec<_>>();
        let mut statements = BTreeSet::new();
        let mut writers = BTreeMap::<ProducerBinding, BTreeSet<usize>>::new();
        let mut returns = BTreeSet::new();
        while let Some(instruction) = pending.pop() {
            if decoder.store.tags[instruction].kind() == schema::InstructionKind::Stmt {
                assert!(statements.insert(instruction));
                for slot in decoder.statement_written_slots(instruction)?.0 {
                    let binding = if owner.declarations.get(&instruction).is_some_and(|slots| slots.contains(&slot)) {
                        ProducerBinding::ScopedDeclaration(instruction, slot)
                    } else { owner.writer_binding(decoder.store, decoder.owner, instruction, slot)? };
                    writers.entry(binding).or_default().insert(instruction);
                }
                if decoder.store.tags[instruction] == FullTag::StmtReturn { returns.insert(instruction); }
            }
            let mut child = owner.children[instruction - owner.range.start];
            while child != IR_NONE {
                pending.push(child as usize);
                child = owner.siblings[child as usize - owner.range.start];
            }
        }
        Ok(statements == owner.statement_positions.keys().copied().collect()
            && returns == owner.returns.iter().copied().collect()
            && owner.callback_returns.is_empty()
            && writers.len() == owner.writers.len()
            && writers.iter().all(|(binding, expected)| owner.writers.get(binding).is_some_and(|actual| actual.len() == expected.len() && actual.iter().copied().collect::<BTreeSet<_>>() == *expected)))
    }

    #[cfg(test)]
    pub(in crate::runtime::eval::indexed::full) fn binding_work(&self) -> (usize, usize, usize) {
        self.owners.values().fold((0, 0, 0), |total, owner| {
            let work = owner.binding_work.get();
            (total.0 + work.0, total.1 + work.1, total.2 + work.2)
        })
    }

    #[cfg(test)]
    pub(in crate::runtime::eval::indexed::full) fn temporary_payload_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            + self.scheme_scopes.get().map_or(0, super::VerifiedSchemeScopes::temporary_payload_bytes)
            + self.checked_producers.capacity() * size_of::<usize>()
            + self.capture_retry_roots.capacity() * size_of::<usize>()
            + self.conditions.capacity() * size_of::<(usize, ConditionRole)>() + self.effect_operations.capacity() * size_of::<usize>() + self.storage_reads.capacity() * size_of::<(u32, usize, TypeId)>()
            + self.work.as_ref().map_or(0, |_| size_of::<RefCell<SchemeInstantiationBudget>>() + 2 * size_of::<usize>())
            + self.starts.len() * size_of::<(usize, u32)>()
            + self.binding_writers.capacity() * size_of::<Option<(usize, usize, u32)>>()
            + self.storage_writers.len() * size_of::<(u32, Vec<StorageWriter>)>()
            + self.storage_writers.values().map(|writers| writers.capacity() * size_of::<StorageWriter>()).sum::<usize>()
            + self.module_exports.len() * size_of::<(u32, Vec<(Name, u32)>)>()
            + self.module_exports.values().map(|values| values.capacity() * size_of::<(Name, u32)>()).sum::<usize>()
            + self.scheme_owners.len() * size_of::<(u32, VerifiedSchemeOwner)>()
            + self.scheme_owners.iter().filter(|(owner, _)| !self.owners.contains_key(owner)).map(|(_, owner)| owner.return_analysis.temporary_payload_bytes()).sum::<usize>()
            + self.owners.values().map(|owner| {
                size_of::<(u32, ProducerOwner)>()
                    + owner.storage_bindings.len() * size_of::<(ProducerBinding, StorageBinding)>()
                    + owner.children.capacity() * size_of::<u32>() + owner.siblings.capacity() * size_of::<u32>()
                    + owner.parents.capacity() * size_of::<Option<usize>>() + owner.statements.capacity() * size_of::<Option<usize>>() + owner.returns.capacity() * size_of::<usize>()
                    + owner.statement_positions.len() * size_of::<(usize, (usize, usize))>()
                    + owner.snapshot_predicates.borrow().iter().map(|(_, paths)| size_of::<((usize, usize, usize, usize, bool, bool, ProducerBinding), Vec<Vec<Name>>)>() + paths.capacity() * size_of::<Vec<Name>>() + paths.iter().map(|path| path.capacity() * size_of::<Name>()).sum::<usize>()).sum::<usize>()
                    + owner.block_statements.len() * size_of::<(usize, Vec<usize>)>()
                    + owner.block_can_complete.len() * size_of::<(usize, bool)>()
                    + owner.completed_optional_reads.borrow().values().map(|proof| size_of::<(usize, CompletedOptionalRead)>() + match proof {
                        CompletedOptionalRead::WrapperPresence { .. } => 0,
                        CompletedOptionalRead::RecordValue { actual, expected } => actual.retained_bytes().saturating_add(expected.retained_bytes()),
                    }).sum::<usize>()
                    + owner.completed_union_reads.borrow().len() * size_of::<((usize, TypeId), super::super::LoweredType)>()
                    + owner.block_statements.values().map(|values| values.capacity() * size_of::<usize>()).sum::<usize>()
                    + owner.block_declarations.len() * size_of::<((usize, usize), Vec<(usize, ProducerBinding)>)>()
                    + owner.block_declarations.values().map(|values| values.capacity() * size_of::<(usize, ProducerBinding)>()).sum::<usize>()
                    + owner.writers.len() * size_of::<(ProducerBinding, Vec<usize>)>()
                    + owner.writers.values().map(|values| values.capacity() * size_of::<usize>()).sum::<usize>()
                    + owner.declarations.len() * size_of::<(usize, Vec<usize>)>()
                    + owner.declarations.values().map(|values| values.capacity() * size_of::<usize>()).sum::<usize>()
                    + owner.following_declarations.len() * size_of::<(usize, Vec<usize>)>()
                    + owner.following_declarations.values().map(|values| values.capacity() * size_of::<usize>()).sum::<usize>()
                    + owner.region_bindings.len() * size_of::<((usize, usize), ProducerBinding)>()
                    + owner.lexical_slots.len() * size_of::<(usize, LexicalSlotIdentity)>()
                    + owner.block_guards.len() * size_of::<(usize, Vec<(usize, usize)>)>()
                    + owner.block_guards.values().map(|guards| guards.capacity() * size_of::<(usize, usize)>()).sum::<usize>()
                    + owner.inherited_bindings.borrow().len() * size_of::<((usize, usize), Option<ProducerBinding>)>()
                    + owner.pattern_sources.len() * size_of::<(ProducerBinding, PatternProducer)>()
                    + owner.pattern_sources.values().map(|source| source.preceding.capacity() * size_of::<(u32, bool)>()).sum::<usize>()
                    + owner.comprehension_sources.len() * size_of::<(ProducerBinding, (usize, usize))>()
                    + owner.with_errors.len() * size_of::<(usize, Type)>()
                    + owner.with_errors.values().map(Type::retained_bytes).sum::<usize>()
                    + owner.pipeline_sources.len() * size_of::<(ProducerBinding, PipelineProducer)>()
                    + owner.yield_sites.capacity() * size_of::<usize>()
                    + owner.callback_returns.len() * size_of::<(usize, Vec<usize>)>()
                    + owner.callback_returns.values().map(|values| values.capacity() * size_of::<usize>()).sum::<usize>()
                    + owner.pipeline_groups.len() * size_of::<((usize, usize, bool), usize)>()
                    + owner.return_analysis.temporary_payload_bytes()
                    + owner.pipeline_stages.len() * size_of::<(usize, Vec<u32>)>()
                    + owner.pipeline_stages.values().map(|values| values.capacity() * size_of::<u32>()).sum::<usize>()
            }).sum::<usize>()
    }

    fn instruction_owner(&self, instruction: usize) -> Result<u32, IrVerifyError> {
        let (_, owner) = self.starts.range(..=instruction).next_back().ok_or_else(|| IrVerifyError::new("ground producer instruction has no owner"))?;
        if !self.owners[owner].range.contains(&instruction) { return Err(IrVerifyError::new("ground producer instruction is outside its owner")); }
        Ok(*owner)
    }
}

fn install_condition_regions(decoder: &FullDecoder<'_>, owner: &mut ProducerOwner, condition: usize, captures: super::super::decode::ListView<'_, usize>, roots: &[usize]) -> Result<(), IrVerifyError> {
    for slot in captures {
        owner.work.borrow_mut().charge(1 + roots.len())?;
        let slot = slot?;
        let schema::ExprView::MatchExpr { mut arms, .. } = decoder.expression_view(condition)? else { return Err(IrVerifyError::new("pattern condition capture has no match producer")) };
        let mut found = None;
        for arm in &mut arms {
            owner.work.borrow_mut().charge(1)?;
            let (_, _, value) = arm?;
            if let Some(binding) = owner.region_bindings.get(&(value.raw() as usize, slot))
                && found.replace(*binding).is_some()
            {
                return Err(IrVerifyError::new("pattern condition capture has multiple lexical producers"));
            }
        }
        let binding = found.ok_or_else(|| IrVerifyError::new("pattern condition capture has no lexical producer"))?;
        for &root in roots { owner.region_bindings.insert((root, slot), binding); }
    }
    Ok(())
}

struct DeclaredProducerFact {
    owner: u32,
    instruction: usize,
    ty: Arc<Type>,
    binding: Option<DeclaredParamBinding>,
}

#[derive(Clone, Copy, Debug)]
struct DeclaredParamBinding {
    slot: usize,
    binding: ProducerBinding,
}

impl DeclaredProducerFact {
    fn into_evidence(self, owner: u32, instruction: usize) -> Result<(Arc<Type>, Option<DeclaredParamBinding>), IrVerifyError> {
        if self.owner != owner || self.instruction != instruction { return Err(IrVerifyError::new("declared producer fact belongs to another expression")); }
        Ok((self.ty, self.binding))
    }
}

enum ProducerOperandContext {
    Declared(DeclaredProducerFact),
    Contextual(Option<Arc<Type>>),
}

impl ProducerOperandContext {
    fn ty(&self) -> Option<&Type> {
        match self {
            Self::Declared(fact) => Some(&fact.ty),
            Self::Contextual(ty) => ty.as_deref(),
        }
    }

    fn into_parts(self) -> (Option<Arc<Type>>, Option<DeclaredProducerFact>) {
        match self {
            Self::Declared(fact) => (Some(Arc::clone(&fact.ty)), Some(fact)),
            Self::Contextual(ty) => (ty, None),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum StorageContractRelation { ReadCompatible, ExactDeclaration }

enum Obligation {
    GroundFunction(usize),
    LiteralPayload(u32, usize),
    Expression(u32, usize, Arc<Type>, bool, Option<DeclaredProducerFact>),
    Slot(u32, ProducerBinding, usize, Arc<Type>),
    PipelinePrefix(u32, usize, usize, Arc<Type>),
    Storage(u32, Arc<Type>, StorageContractRelation),
}

pub(in crate::runtime::eval::indexed::full) enum ProducerContexts<'a> {
    Pending(&'a [(u32, super::super::ExecutableOwner, Arc<Type>)]),
    Published,
}

struct RecordProducerWriters {
    writers: BTreeMap<Name, super::super::CheckedRecordFieldWriter>,
    unresolved: BTreeSet<usize>,
    types: BTreeMap<usize, Arc<Type>>,
}

struct ProducerProof<'a> {
    store: &'a FullStore,
    contexts: ProducerContexts<'a>,
    used_contexts: BTreeMap<u32, (super::super::ExecutableOwner, Arc<Type>)>,
    index: &'a GroundProducerIndex,
    pending: Vec<Obligation>,
    functions: BTreeSet<usize>,
    contract_owners: BTreeSet<u32>,
    expressions: BTreeMap<(u32, usize), Vec<(Arc<Type>, bool)>>,
    conditions: BTreeSet<(u32, usize, ConditionRole)>,
    operand_expansions: BTreeSet<(u32, usize)>,
    bindings: BTreeMap<(u32, usize, usize), ProducerBinding>,
    pipeline_outputs: BTreeMap<(u32, usize, usize), ProducerTypeDerivation>,
    slots: BTreeMap<(u32, ProducerBinding), Vec<Arc<Type>>>,
    prefixes: BTreeMap<(u32, usize, usize), Vec<Arc<Type>>>,
    storage: BTreeMap<u32, Vec<(Arc<Type>, StorageContractRelation)>>,
    decoders: BTreeMap<u32, Rc<FullDecoder<'a>>>,
    types: BTreeMap<TypeId, Arc<Type>>,
    declared_types: BTreeMap<(u32, usize), ProducerTypeDerivation>,
    driver_output_types: BTreeMap<usize, Rc<BTreeMap<u32, Arc<Type>>>>,
    pattern_types: BTreeMap<(u32, ProducerBinding), Arc<Type>>,
    fresh_lists: BTreeMap<(u32, usize), Option<usize>>,
    contextual_sources: BTreeMap<(u32, ProducerBinding), (usize, Arc<Type>)>,
    deferred_sources: BTreeMap<(u32, ProducerBinding), usize>,
    contextual_reads: BTreeMap<(u32, ProducerBinding), Vec<Arc<Type>>>,
    captured_errors: BTreeMap<(u32, usize), CapturedErrorDerivation>,
    runtime_checked_inputs: BTreeSet<(u32, usize)>,
    scheme_applications: BTreeSet<(u32, usize)>,
    scheme_captures: BTreeSet<(u32, usize)>,
    incoming: BTreeMap<usize, Vec<usize>>,
    budget: SchemeInstantiationBudget,
}

pub(super) fn verify<'a>(store: &'a FullStore, index: &'a GroundProducerIndex, contexts: ProducerContexts<'a>, requests: SchemeProducerRequests) -> Result<Vec<(u32, super::super::ExecutableOwner, Arc<Type>)>, IrVerifyError> {
    if index.demand == crate::runtime::eval::ProducerDemand::Absent
        && store.checked_expr_types.iter().any(|fact| !fact.is_contract())
    {
        return Err(IrVerifyError::new("producer context has no selected operation"));
    }
    if index.demand == crate::runtime::eval::ProducerDemand::Absent {
        if !requests.is_empty() { return Err(IrVerifyError::new("generic body requests have no mandatory producer demand")); }
        return Ok(Vec::new());
    }
    let mut scheme_methods = requests.methods;
    let mut proof = ProducerProof { store, index, contexts, used_contexts: BTreeMap::new(), pending: Vec::new(), functions: BTreeSet::new(), contract_owners: BTreeSet::new(), expressions: BTreeMap::new(), conditions: BTreeSet::new(), operand_expansions: BTreeSet::new(), bindings: BTreeMap::new(), pipeline_outputs: BTreeMap::new(), slots: BTreeMap::new(), prefixes: BTreeMap::new(), storage: BTreeMap::new(), decoders: BTreeMap::new(), types: BTreeMap::new(), declared_types: BTreeMap::new(), driver_output_types: BTreeMap::new(), pattern_types: BTreeMap::new(), fresh_lists: BTreeMap::new(), contextual_sources: BTreeMap::new(), deferred_sources: BTreeMap::new(), contextual_reads: BTreeMap::new(), captured_errors: BTreeMap::new(), runtime_checked_inputs: BTreeSet::new(), scheme_applications: requests.applications, scheme_captures: BTreeSet::new(), incoming: BTreeMap::new(), budget: SchemeInstantiationBudget::for_prepared_constants(store, index.owners.values().fold(0usize, |work, owner| work.saturating_add(owner.prepared_constant_work))) };
    proof.budget.charge(requests.captures.len())?;
    for request in &requests.captures {
        if !proof.scheme_captures.insert((request.owner, request.instruction)) {
            return Err(IrVerifyError::new("generic captured-error request is duplicated"));
        }
    }
    for (index, function) in store.functions.iter().enumerate() {
        proof.budget.charge(1)?;
        if matches!(function.contract, FullCallableContract::Ground(_))
            && store.function_metadata.get(index).is_some_and(|metadata| metadata.callable_kind == CallableKind::Stream)
        {
            proof.budget.charge(2)?;
            proof.ground_function(index)?;
        }
    }
    for call in &store.checked_call_targets {
        if matches!(store.tags[call.instruction as usize], FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall) {
            proof.incoming.entry(call.function.index()).or_default().push(call.instruction as usize);
        }
    }
    for &step in &index.driver_outputs {
        proof.budget.charge(1)?;
        let driver = &store.driver_steps[step];
        proof.driver_output_types(step)?;
        for slot in bounded(driver.slots, &store.driver_slots, "driver output slot range is invalid")? {
            proof.budget.charge(1)?;
            if slot.flags & super::super::DRIVER_SLOT_DEFINITION == 0 { continue; }
            let ty = proof.contract_type(slot.type_id)?;
            proof.binding_writer(slot.binding_identity, ty)?;
        }
    }
    for &(owner, instruction, ty) in &index.storage_reads {
        let ty = proof.contract_type(ty)?;
        proof.budget.charge_clone(&ty)?;
        if ty.contains_typed_callable() { proof.expression(owner, instruction, ty, false); }
    }
    for &(instruction, role) in &index.conditions {
        let owner = index.instruction_owner(instruction)?;
        proof.verify_condition(owner, instruction, role)?;
    }
    for &instruction in &index.capture_retry_roots {
        proof.budget.charge(1)?;
        let owner = index.instruction_owner(instruction)?;
        let Some(type_id) = checked_expression_type_id(store, instruction as u32) else {
            if super::super::driver_owner_index(owner).is_none()
                && matches!(store.functions[function_index(owner)?].contract, FullCallableContract::Scheme(_))
            { continue; }
            return Err(IrVerifyError::new("capture or retry has no checked Result contract"));
        };
        let expected = proof.contract_type(type_id)?;
        proof.expression(owner, instruction, Arc::clone(&expected), false);
        if !matches!(expected.unvalidated(), Type::Result(_, _)) {
            return Err(IrVerifyError::new("capture or retry checked contract is not a Result"));
        }
    }
    let mut deferred_method_roots = Vec::new();
    for &instruction in &index.effect_operations {
        let owner = index.instruction_owner(instruction)?;
        if proof.scheme_applications.contains(&(owner, instruction)) || proof.scheme_captures.contains(&(owner, instruction)) { continue; }
        if store.tags[instruction] == FullTag::ExprMethod && store.registered_calls.contains_key(&(instruction as u32)) { continue; }
        if store.tags[instruction] == FullTag::ExprMethod {
            proof.budget.charge(1)?;
            deferred_method_roots.push((owner, instruction));
            continue;
        }
        if matches!(store.tags[instruction], FullTag::ExprCapture | FullTag::ExprSpawnRun | FullTag::ExprSpawnCommand
            | FullTag::ExprWait | FullTag::ExprProcessCommandArgv | FullTag::ExprProcessCommandBuilder)
        { proof.producer_owner_contract(owner)?; }
        proof.expression(owner, instruction, Arc::new(Type::Any), false);
    }
    for &fact_index in &index.checked_producers {
        let fact = store.checked_expr_types[fact_index];
        let instruction = fact.instruction as usize;
        let owner = index.instruction_owner(instruction)?;
        let ty = proof.contract_type(fact.type_id)?;
        proof.expression(owner, instruction, ty, false);
    }
    for (function_index, function) in store.functions.iter().enumerate() {
        let FullCallableContract::Scheme(_) = function.contract else { continue };
        let decoder = proof.owner(function_owner(function_index)?)?;
        for capture in bounded(function.captures, &store.captures, "generic capture range is invalid")? {
            let ty = proof.ground_type(capture.type_id)?;
            if capture.slot_and_flags & (1 << 31) != 0 {
                proof.storage_contract(capture.binding_identity, Arc::clone(&ty), StorageContractRelation::ExactDeclaration)?;
            }
            if capture.slot_and_flags & (1 << 31) == 0 && matches!(ty.as_ref(), Type::Record(_)) {
                let (owner, producer) = proof.binding_producer(capture.binding_identity, ProducerTargetPurpose::LiteralPayload, &mut Vec::new())?;
                let CallableProducer::Expression(instruction) = producer else { return Err(IrVerifyError::new("generic record capture has no literal lexical initializer")); };
                proof.pending.push(Obligation::LiteralPayload(owner, instruction));
            }
            proof.slot(decoder.owner, decoder.instruction_range.start, (capture.slot_and_flags & !(1 << 31)) as usize, ty)?;
        }
        for instruction in decoder.instruction_range.clone() {
            match store.tags[instruction] {
                FullTag::ExprCall | FullTag::ExprDirectPureCall => {
                    let (function, arguments) = call_parts(&decoder, instruction)?;
                    if matches!(store.functions[function].contract, FullCallableContract::Ground(_)) {
                        proof.ground_function(function)?;
                        proof.call_arguments(&decoder, function, &arguments)?;
                    }
                }
                _ => {}
            }
        }
    }
    let mut ground_arguments = BTreeMap::new();
    for request in requests.ground_arguments {
        proof.budget.charge(1)?;
        if ground_arguments.insert((request.owner, request.call, request.argument), request.expected).is_some() {
            return Err(IrVerifyError::new("generic argument request is duplicated"));
        }
    }
    for instantiation in &store.call_instantiations {
        let owner = index.instruction_owner(instantiation.instruction as usize)?;
        if super::super::driver_owner_index(owner).is_none() && matches!(store.functions[function_index(owner)?].contract, FullCallableContract::Scheme(_)) { continue; }
        let decoder = proof.owner(owner)?;
        if matches!(store.tags[instantiation.instruction as usize], FullTag::ExprFunctionRef | FullTag::ExprParam | FullTag::ExprCheckedValue) {
            let type_id = super::checked_expression_type_id(store, instantiation.instruction)
                .ok_or_else(|| IrVerifyError::new("materialized callable has no checked producer type"))?;
            let ty = proof.contract_type(type_id)?;
            proof.expression(owner, instantiation.instruction as usize, ty, false);
            continue;
        }
        let arguments = call_values(&decoder, instantiation.instruction as usize)?;
        for argument in arguments {
            let (ProducerArgument::Single(argument) | ProducerArgument::Splice(argument)) = argument else { continue };
            let ty = if let Some(expected) = ground_arguments.remove(&(owner, instantiation.instruction as usize, argument)) {
                proof.budget.charge(1)?;
                Arc::new(expected)
            } else {
                proof.need_context(&decoder, argument)?.ok_or_else(|| IrVerifyError::new("generic call argument has no ground producer contract"))?
            };
            proof.expression(owner, argument, ty, false);
        }
    }
    if !ground_arguments.is_empty() { return Err(IrVerifyError::new("generic argument request has no actual call operand")); }
    for request in requests.captures {
        proof.budget.charge(1)?;
        let decoder = proof.owner(request.owner)?;
        if !matches!(store.tags.get(request.instruction), Some(FullTag::ExprCapture | FullTag::ExprRetry)) {
            return Err(IrVerifyError::new("generic captured-error request has no capture instruction"));
        }
        if let Some(ordinal) = request.wrapper {
            let function = store.functions.get(function_index(request.owner)?).ok_or_else(|| IrVerifyError::new("capture relationship has no function owner"))?;
            let FullCallableContract::Scheme(id) = function.contract else { return Err(IrVerifyError::new("capture relationship has no scheme owner")); };
            let scope = proof.index.scheme_scopes(store)?.get(id)?;
            let encoded = *super::bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("capture relationship ordinal is out of bounds"))?;
            let expected = super::wrappers::decode(store, scope, encoded, &super::applications::ApplicationSubstitution::Identity, &mut proof.budget)?;
            let crate::sema::wrapper_operation::WrapperRequirementKind::Capture { contributions: expected, result, .. } = expected.kind else { return Err(IrVerifyError::new("capture has another wrapper relationship")); };
            if !super::body_types_equal(&result, &request.expected_error) { return Err(IrVerifyError::new("capture error result disagrees with its actual body")); }
            proof.budget.charge(request.transfers.len())?;
            let mut actual = request.transfers;
            charged_capture_event_sort(&mut actual, &mut proof.budget)?;
            let transfers = actual.iter().map(|(instruction, _)| *instruction).collect::<Vec<_>>();
            proof.budget.charge(transfers.len())?;
            if transfers.windows(2).any(|pair| pair[0] == pair[1]) { return Err(IrVerifyError::new("capture transfer is authenticated more than once")); }
            for (instruction, error) in proof.derive_captured_error_events(&decoder, request.instruction, &transfers)? {
                actual.push((instruction, crate::sema::wrapper_operation::WrapperErrorContribution::Explicit(crate::sema::schemes::SchemeType::Ground(error))));
            }
            charged_capture_event_sort(&mut actual, &mut proof.budget)?;
            proof.budget.charge(actual.len() + expected.len())?;
            if actual.len() != expected.len() || !actual.iter().zip(expected.iter()).all(|((_, actual), expected)| match (actual, expected) {
                (crate::sema::wrapper_operation::WrapperErrorContribution::Source(actual), crate::sema::wrapper_operation::WrapperErrorContribution::Source(expected))
                | (crate::sema::wrapper_operation::WrapperErrorContribution::Explicit(actual), crate::sema::wrapper_operation::WrapperErrorContribution::Explicit(expected)) => super::body_types_equal(actual, expected),
                _ => false,
            }) { return Err(IrVerifyError::new("capture contributions disagree with its actual body")); }
            continue;
        }
        let errors = proof.captured_error_types(&decoder, request.instruction)?;
        if !errors.is_empty() {
            let actual = proof.join_captured_errors(errors.as_ref())?;
            if !super::body_type_matches(&crate::sema::schemes::SchemeType::Ground(actual), &request.expected_error, &mut proof.budget)? {
                return Err(IrVerifyError::new("generic captured error disagrees with its actual producer"));
            }
        }
    }
    for iteration in requests.streams {
        proof.budget.charge(1)?;
        let decoder = proof.owner(iteration.owner)?;
        if proof.stream_may_propagate_error(&decoder, iteration.source)? {
            super::body_propagated_error_matches_owner(store, &crate::sema::schemes::SchemeType::Ground(Type::Error), &iteration.boundary, &mut proof.budget)?;
        }
    }
    proof.finish()?;
    let mut seeded_method_roots = Vec::new();
    for (owner, instruction) in deferred_method_roots {
        proof.budget.charge(1)?;
        if scheme_methods.remove(&(owner, instruction)) { continue; }
        let contexts = proof.expressions.get(&(owner, instruction));
        proof.budget.charge(contexts.map_or(0, Vec::len))?;
        if contexts.is_some_and(|contexts| contexts.iter().any(|(expected, _)| !matches!(expected.unvalidated(), Type::Any))) {
            continue;
        }
        let decoder = proof.owner(owner)?;
        let schema::ExprView::Method { receiver, name, args, .. } = decoder.expression_view(instruction)? else {
            return Err(IrVerifyError::new("deferred method root changed instruction kind"));
        };
        let receiver = receiver.raw() as usize;
        let arguments = args.map(|argument| argument.map(|argument| argument.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
        proof.budget.charge(arguments.len())?;
        if proof.operand_context(&decoder, receiver)?.ty().is_none() {
            if let Some((call, expected)) = proof.ground_call_argument_ancestor(owner, instruction)? {
                proof.budget.charge(1)?;
                proof.expression(owner, call, expected, false);
                seeded_method_roots.push((owner, instruction));
                continue;
            }
        }
        proof.budget.charge(1)?;
        proof.verify_registered_method(&decoder, instruction, receiver, name, &arguments, &Type::Any)?;
    }
    if !seeded_method_roots.is_empty() {
        proof.finish()?;
        for (owner, instruction) in seeded_method_roots {
            proof.budget.charge(1)?;
            let contexts = proof.expressions.get(&(owner, instruction));
            proof.budget.charge(contexts.map_or(0, Vec::len))?;
            if contexts.is_some_and(|contexts| contexts.iter().any(|(expected, _)| !matches!(expected.unvalidated(), Type::Any))) {
                continue;
            }
            let decoder = proof.owner(owner)?;
            let schema::ExprView::Method { receiver, name, args, .. } = decoder.expression_view(instruction)? else {
                return Err(IrVerifyError::new("deferred method root changed instruction kind"));
            };
            let receiver = receiver.raw() as usize;
            let arguments = args.map(|argument| argument.map(|argument| argument.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
            proof.budget.charge(arguments.len())?;
            proof.budget.charge(1)?;
            proof.verify_registered_method(&decoder, instruction, receiver, name, &arguments, &Type::Any)?;
        }
    }
    if !scheme_methods.is_empty() { return Err(IrVerifyError::new("generic method site has no producer method root")); }
    proof.finish()?;
    if let Some(work) = &index.work {
        let census_work = SchemeInstantiationBudget::new(store).remaining - work.borrow().remaining;
        proof.budget.charge(census_work)?;
    }
    if matches!(proof.contexts, ProducerContexts::Published) {
        for fact in &store.checked_expr_types {
            if let super::super::FullCheckedExprPurpose::ProducerContext(owner) = fact.purpose
                && !matches!(proof.used_contexts.get(&fact.instruction), Some((used_owner, _)) if *used_owner == owner)
            {
                return Err(IrVerifyError::new("published producer context is unused or has a wrong owner"));
            }
        }
    }
    Ok(proof.used_contexts.into_iter().map(|(instruction, (owner, ty))| (instruction, owner, ty)).collect())
}

enum CapturedErrorDerivation {
    Pending,
    Resolved(Arc<[Type]>),
}

enum ProducerTypeDerivation {
    Pending,
    Resolved(Option<Arc<Type>>),
    ResolvedBinding(Arc<Type>, DeclaredParamBinding),
}

#[derive(Clone, Copy)]
enum ProducerArgument {
    Single(usize),
    Splice(usize),
    Default(usize),
}

fn call_parts(decoder: &FullDecoder<'_>, instruction: usize) -> Result<(usize, Vec<ProducerArgument>), IrVerifyError> {
    let function = match decoder.expression_view(instruction)? {
        schema::ExprView::Call { function, .. } | schema::ExprView::DirectPureCall { function, .. } => function.id.index(),
        schema::ExprView::SelfCall { .. } if super::super::driver_owner_index(decoder.owner).is_none() => function_index(decoder.owner)?,
        _ => return Err(IrVerifyError::new("ground producer call has no static function target")),
    };
    Ok((function, call_values(decoder, instruction)?))
}

fn call_values(decoder: &FullDecoder<'_>, instruction: usize) -> Result<Vec<ProducerArgument>, IrVerifyError> {
    let arguments = match decoder.expression_view(instruction)? {
        schema::ExprView::Call { args, .. } | schema::ExprView::DirectPureCall { args, .. } | schema::ExprView::SelfCall { args, .. } | schema::ExprView::TypedCall { args, .. } | schema::ExprView::DynamicCall { args, .. } => args,
        _ => return Err(IrVerifyError::new("ground producer has no callable argument row")),
    };
    let mut result = Vec::new();
    for argument in arguments {
        result.push(match argument? {
            super::super::decode::CallArgView::Single(value) => ProducerArgument::Single(value.raw() as usize),
            super::super::decode::CallArgView::Splice(value) => ProducerArgument::Splice(value.raw() as usize),
            super::super::decode::CallArgView::Default(slot) => ProducerArgument::Default(slot),
        });
    }
    Ok(result)
}


pub(super) struct ConditionalProducerType {
    candidate: Option<Arc<Type>>,
    null: Option<Arc<Type>>,
    unknown: bool,
    records: bool,
    incompatible: bool,
}

impl ConditionalProducerType {
    pub(super) fn new() -> Self {
        Self { candidate: None, null: None, unknown: false, records: true, incompatible: false }
    }

    pub(super) fn observe(&mut self, actual: Option<Arc<Type>>) {
        match actual {
            Some(ty) if ty.as_ref() == &Type::Null => self.null = Some(ty),
            Some(ty) => {
                self.records &= matches!(ty.as_ref(), Type::Record(_) | Type::ErasedRecord);
                if !self.incompatible {
                    match &self.candidate {
                        Some(previous) if !ty.matches_expected(previous) => {
                            if previous.matches_expected(&ty) { self.candidate = Some(ty); }
                            else { self.incompatible = true; }
                        }
                        None => self.candidate = Some(ty),
                        _ => {}
                    }
                }
            }
            None => self.unknown = true,
        }
    }

    pub(super) fn finish(mut self, budget: &mut SchemeInstantiationBudget) -> Result<(Option<Arc<Type>>, bool), IrVerifyError> {
        if self.incompatible { self.candidate = self.records.then(|| Arc::new(Type::ErasedRecord)); }
        let candidate = match self.candidate {
            Some(candidate) if self.null.is_some() && !Type::Null.matches_expected(&candidate) => {
                ensure_substitution_type(&candidate)?;
                budget.charge(1)?;
                budget.charge_clone(&candidate)?;
                Some(Arc::new(Type::Optional(Box::new((*candidate).clone()))))
            }
            Some(candidate) => Some(candidate),
            None if !self.unknown && !self.incompatible => self.null,
            None => None,
        };
        Ok((candidate, self.unknown))
    }
}

impl<'a> ProducerProof<'a> {
    fn require_callable_effects(&mut self, decoder: &FullDecoder<'_>, instruction: usize, effects: Option<&[crate::syntax::node::Effect]>, pure: bool) -> Result<(), IrVerifyError> {
        let index = self.index;
        super::super::verify_effects::require_callable(decoder, index.scheme_scopes(self.store)?, &mut self.budget, instruction, effects, pure, |at| {
            let owner = index.owners.get(&decoder.owner).ok_or_else(|| IrVerifyError::new("callable effect proof has no executable owner"))?;
            let offset = at.checked_sub(owner.range.start).ok_or_else(|| IrVerifyError::new("callable effect proof leaves its owner"))?;
            owner.parents.get(offset).copied().ok_or_else(|| IrVerifyError::new("callable effect proof parent is out of bounds"))
        })
    }

    fn module_callable(&mut self, decoder: &FullDecoder<'a>, callee: usize) -> Result<Option<(usize, Arc<Type>, TypedCallable)>, IrVerifyError> {
        let schema::ExprView::Field { base, name, .. } = decoder.expression_view(callee)? else { return Ok(None) };
        let base = base.raw() as usize;
        let Some(receiver) = self.need_context(decoder, base)? else { return Ok(None) };
        let Type::Module(module) = receiver.unvalidated() else { return Ok(None) };
        let Some(ModuleExportType::Callable { kind, sig: signature, optional: false }) = module.exports.get(&Name::intern(name)) else { return Ok(None) };
        let kind = *kind;
        ensure_closed_type(&receiver)?;
        let effects = signature.effects.concrete()
            .map_err(|_| IrVerifyError::new("module callable effect contract is unresolved"))?;
        self.budget.charge(signature.params.len() + effects.as_ref().map_or(0, Vec::len) + 1)?;
        self.budget.charge_clone(&signature.return_ty)?;
        for parameter in &signature.params { self.budget.charge_clone(&parameter.ty)?; }
        let callable = TypedCallable { kind, sig: signature.clone() };
        Ok(Some((base, receiver, callable)))
    }

    fn ground_type(&mut self, id: TypeId) -> Result<Arc<Type>, IrVerifyError> {
        if let Some(ty) = self.types.get(&id) { ensure_substitution_type(ty)?; return Ok(Arc::clone(ty)); }
        let ty = Arc::new(self.store.semantic.to_type_with_budget(id, &mut self.budget.semantic_types)?);
        ensure_substitution_type(&ty).map_err(|error| IrVerifyError::new(format!("{}: ground producer type {ty}", error.message)))?;
        self.types.insert(id, Arc::clone(&ty));
        Ok(ty)
    }

    fn contract_type(&mut self, id: TypeId) -> Result<Arc<Type>, IrVerifyError> {
        if let Some(ty) = self.types.get(&id) { return Ok(Arc::clone(ty)); }
        let ty = Arc::new(self.store.semantic.to_type_with_budget(id, &mut self.budget.semantic_types)?);
        ensure_closed_type(&ty)?;
        self.types.insert(id, Arc::clone(&ty));
        Ok(ty)
    }

    fn owner(&mut self, owner: u32) -> Result<Rc<FullDecoder<'a>>, IrVerifyError> {
        if let Some(decoder) = self.decoders.get(&owner) { return Ok(Rc::clone(decoder)); }
        if !self.index.owners.contains_key(&owner) { return Err(IrVerifyError::new("ground producer owner was not verified")); }
        let mut decoder = match super::super::driver_owner_index(owner) {
            Some(index) => decoder_for_driver_step(self.store, index)?,
            None => decoder_for_function(self.store, function_index(owner)?)?,
        };
        let parent_owner = &self.index.owners[&owner];
        if decoder.owner != owner || decoder.instruction_range != parent_owner.range || parent_owner.parents.len() != parent_owner.range.len() {
            return Err(IrVerifyError::new("producer parent context disagrees with its owner census"));
        }
        decoder.borrowed_instruction_parents = Some(super::super::InstructionParentContext { owner, range: parent_owner.range.clone(), parents: &parent_owner.parents, statements: &parent_owner.statements, children: &parent_owner.children, siblings: &parent_owner.siblings });
        decoder.producer_prefix = self.index.prefix_context(owner, &decoder.instruction_range)?;
        decoder.producer_demand = self.index.demand;
        decoder.function_signature_index.clone_from(&self.index.function_signatures);
        *decoder.return_analysis.borrow_mut() = Some(super::super::ReturnAnalysisSource::Shared(Rc::clone(&self.index.owners[&owner].return_analysis)));
        let decoder = Rc::new(decoder);
        self.decoders.insert(owner, Rc::clone(&decoder));
        Ok(decoder)
    }

    fn ground_function(&mut self, function: usize) -> Result<(), IrVerifyError> {
        let contract = self.store.functions.get(function).ok_or_else(|| IrVerifyError::new("producer function target is out of bounds"))?.contract;
        if matches!(contract, FullCallableContract::Ground(_)) && self.functions.insert(function) {
            self.pending.push(Obligation::GroundFunction(function));
        }
        Ok(())
    }

    fn expression(&mut self, owner: u32, instruction: usize, expected: Arc<Type>, returned: bool) {
        self.expression_with_fact(owner, instruction, expected, returned, None);
    }

    fn expression_with_fact(&mut self, owner: u32, instruction: usize, expected: Arc<Type>, returned: bool, fact: Option<DeclaredProducerFact>) {
        let previous = self.expressions.entry((owner, instruction)).or_default();
        if previous.iter().any(|(ty, transport)| **ty == *expected && *transport == returned) { return; }
        previous.push((Arc::clone(&expected), returned));
        self.pending.push(Obligation::Expression(owner, instruction, expected, returned, fact));
    }

    fn binding(&mut self, owner: u32, at: usize, slot: usize) -> Result<ProducerBinding, IrVerifyError> {
        self.budget.charge(1)?;
        if let Some(binding) = self.bindings.get(&(owner, at, slot)) { return Ok(*binding); }
        let binding = self.index.owners[&owner].binding(self.store, owner, at, slot)?;
        self.bindings.insert((owner, at, slot), binding);
        Ok(binding)
    }

    fn scheme_parameter(&mut self, owner: u32, slot: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        let function = function_index(owner)?;
        let FullCallableContract::Scheme(id) = self.store.functions[function].contract else { return Ok(None) };
        let scheme = self.store.scheme(id)?;
        let parameter = bounded(scheme.params, &self.store.scheme_params, "scheme parameter range is invalid")?.get(slot).ok_or_else(|| IrVerifyError::new("scheme parameter is out of bounds"))?;
        let ty = super::decode_scheme_type(self.store, self.index.scheme_scopes(self.store)?.get(id)?, parameter.ty, &mut self.budget, 0)?;
        Ok(ty.closed_ground_type().map(Arc::new))
    }

    fn scoped_field_type(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<Option<crate::sema::schemes::SchemeType>, IrVerifyError> {
        if !self.index.scheme_owners.contains_key(&decoder.owner) { return Ok(None); }
        if !matches!(decoder.expression_view(instruction)?, schema::ExprView::Field { .. }) {
            return Err(IrVerifyError::new("scoped field proof has another physical expression"));
        }
        let function = function_index(decoder.owner)?;
        let FullCallableContract::Scheme(id) = self.store.functions[function].contract else {
            return Err(IrVerifyError::new("scoped field proof has no scheme owner"));
        };
        let scheme = self.store.scheme(id)?;
        let facts = bounded(scheme.body_facts, &self.store.scheme_body_facts, "scoped field fact range is invalid")?;
        self.budget.charge(2 + 2 * facts.len().max(1).ilog2() as usize)?;
        let instruction = u32::try_from(instruction).map_err(|_| IrVerifyError::new("scoped field instruction is invalid"))?;
        let key = (instruction, super::FullSchemeBodyFactKind::Expression as u8);
        let position = facts.binary_search_by_key(&key, |fact| (fact.instruction, fact.kind() as u8))
            .map_err(|_| IrVerifyError::new("scoped field has no mandatory expression fact"))?;
        let fact = facts[position];
        if fact.function.raw() != decoder.owner { return Err(IrVerifyError::new("scoped field fact belongs to another owner")); }
        let ty = fact.ty().ok_or_else(|| IrVerifyError::new("scoped field fact has no semantic type"))?;
        let scope = self.index.scheme_scopes(self.store)?.get(id)?;
        super::decode_scheme_type(self.store, scope, ty, &mut self.budget, 0).map(Some)
    }

    fn slot(&mut self, owner: u32, at: usize, slot: usize, expected: Arc<Type>) -> Result<(), IrVerifyError> {
        let binding = self.binding(owner, at, slot)?;
        self.binding_slot(owner, binding, slot, expected)
    }

    fn binding_slot(&mut self, owner: u32, binding: ProducerBinding, slot: usize, expected: Arc<Type>) -> Result<(), IrVerifyError> {
        let previous = self.slots.entry((owner, binding)).or_default();
        if previous.iter().any(|ty| **ty == *expected) { return Ok(()); }
        previous.push(Arc::clone(&expected));
        self.pending.push(Obligation::Slot(owner, binding, slot, expected));
        Ok(())
    }

    fn declared_type(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        self.declared_type_evidence(decoder, instruction).map(|(ty, _)| ty)
    }

    fn declared_type_evidence(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<(Option<Arc<Type>>, Option<DeclaredParamBinding>), IrVerifyError> {
        self.budget.charge(1)?;
        let key = (decoder.owner, instruction);
        if let Some(derivation) = self.declared_types.get(&key) {
            return match derivation {
                ProducerTypeDerivation::Pending => Err(IrVerifyError::new("ground producer type derivation is cyclic")),
                ProducerTypeDerivation::Resolved(ty) => Ok((ty.clone(), None)),
                ProducerTypeDerivation::ResolvedBinding(ty, binding) => Ok((Some(Arc::clone(ty)), Some(*binding))),
            };
        }
        self.declared_types.insert(key, ProducerTypeDerivation::Pending);
        let (ty, binding) = if let Some(id) = checked_expression_type_id(self.store, instruction as u32) {
            (Some(self.contract_type(id)?), None)
        } else if self.store.tags.get(instruction) == Some(&FullTag::ExprParam) {
            let schema::ExprView::Param(slot) = decoder.expression_view(instruction)? else { unreachable!("parameter tag has another expression view") };
            let binding = self.binding(decoder.owner, instruction, slot)?;
            let ty = self.slot_context_type_for_binding(decoder, instruction, slot, binding)?;
            let witness = ty.as_ref().map(|_| DeclaredParamBinding { slot, binding });
            (ty, witness)
        } else { (self.derive_unannotated_type(decoder, instruction)?, None) };
        self.declared_types.insert(key, match (&ty, binding) {
            (Some(ty), Some(binding)) => ProducerTypeDerivation::ResolvedBinding(Arc::clone(ty), binding),
            _ => ProducerTypeDerivation::Resolved(ty.clone()),
        });
        Ok((ty, binding))
    }

    fn inductive_storage_type(&mut self, decoder: &FullDecoder<'a>, instruction: usize,
        declaration: usize, slot: usize, initial: Arc<Type>,
    ) -> Result<Option<Arc<Type>>, IrVerifyError> {
        if !matches!(initial.as_ref(), Type::Bytes | Type::Str | Type::Int | Type::Bool) { return Ok(None); }
        let binding = ProducerBinding::ScopedDeclaration(declaration, slot);
        if decoder.is_parameter_or_capture_slot(slot, &mut |amount| self.budget.charge(amount))?
            || self.index.owners[&decoder.owner].storage_bindings.contains_key(&binding)
        { return Ok(None); }
        let Some(writers) = self.index.owners[&decoder.owner].writers.get(&binding) else { return Ok(None) };
        self.budget.charge(writers.len())?;
        let writers = writers.clone();
        let mut initialized = false;
        for writer in writers {
            self.budget.charge(1)?;
            if writer == declaration { initialized = true; continue; }
            let statement = decoder.statement_view(writer)?;
            match (&*initial, &statement) {
                (Type::Int, schema::StmtView::AssignInt { slot: written, .. })
                | (Type::Bool, schema::StmtView::AssignBool { slot: written, .. }) if *written == slot => continue,
                (Type::Int, schema::StmtView::Assign { slot: written, value, check: None, .. }) if *written == slot => {
                    if matches!(decoder.expression_view(value.raw() as usize)?, schema::ExprView::Param(_)
                        | schema::ExprView::Binary { op: super::super::BinaryOp::Add | super::super::BinaryOp::Sub | super::super::BinaryOp::Mul | super::super::BinaryOp::Div | super::super::BinaryOp::Rem, .. })
                    { continue; }
                    return Ok(None);
                }
                (Type::Int | Type::Bool, _) => return Ok(None),
                _ => {}
            }
            let schema::StmtView::Assign { slot: written, value, op: super::super::AssignOp::Set, check: None, .. } = statement else { return Ok(None) };
            if written != slot { return Ok(None); }
            let schema::ExprView::Slice { base, .. } = decoder.expression_view(value.raw() as usize)? else { return Ok(None) };
            let schema::ExprView::Param(read) = decoder.expression_view(base.raw() as usize)? else { return Ok(None) };
            if read != slot || self.binding(decoder.owner, base.raw() as usize, read)? != binding { return Ok(None); }
        }
        if !initialized { return Ok(None); }
        self.slot(decoder.owner, instruction, slot, Arc::clone(&initial))?;
        Ok(Some(initial))
    }

    fn slot_context_type(&mut self, decoder: &FullDecoder<'a>, instruction: usize, slot: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        let binding = self.binding(decoder.owner, instruction, slot)?;
        self.slot_context_type_for_binding(decoder, instruction, slot, binding)
    }

    fn slot_context_type_for_binding(&mut self, decoder: &FullDecoder<'a>, instruction: usize, slot: usize, binding: ProducerBinding) -> Result<Option<Arc<Type>>, IrVerifyError> {
        if self.index.owners[&decoder.owner].pattern_sources.contains_key(&binding) {
            return self.pattern_binding_type(decoder, binding, slot);
        }
        if let Some(&(comprehension, iter)) = self.index.owners[&decoder.owner].comprehension_sources.get(&binding) {
            return self.comprehension_binding_type(decoder, comprehension, iter, slot);
        }
        if let Some(source) = self.index.owners[&decoder.owner].pipeline_sources.get(&binding).copied() {
            return self.pipeline_binding_type(decoder, source);
        }
        if let ProducerBinding::ScopedDeclaration(declaration, _) = binding {
            if let Some(ty) = self.statement_binding_type(decoder, declaration, slot)? { return Ok(Some(ty)); }
            return match decoder.statement_view(declaration)? {
                schema::StmtView::Let { annotation: Some(annotation), .. } => self.contract_type(annotation.id).map(Some),
                schema::StmtView::Let { value, .. } => {
                    let initial = self.declared_type(decoder, value.raw() as usize)?;
                    if initial.is_some() && self.index.owners[&decoder.owner].writers.get(&binding).is_none_or(|writers| writers.len() <= 1) { return Ok(initial); }
                    if let Some(ty) = initial.as_ref()
                        && ty.contains_any()
                    {
                        ensure_closed_type(ty)?;
                        self.slot(decoder.owner, instruction, slot, Arc::clone(ty))?;
                        return Ok(Some(Arc::clone(ty)));
                    }
                    let Some(id) = decoder.checked_slot_type(slot)? else {
                        return match initial {
                            Some(initial) => self.inductive_storage_type(decoder, instruction, declaration, slot, initial),
                            None => Ok(None),
                        };
                    };
                    let ty = self.contract_type(id)?;
                    if ty.contains_any() { return Ok(None); }
                    self.slot(decoder.owner, instruction, slot, Arc::clone(&ty))?;
                    Ok(Some(ty))
                }
                schema::StmtView::LetInt { .. } => Ok(Some(Arc::new(Type::Int))),
                schema::StmtView::LetBool { .. } => Ok(Some(Arc::new(Type::Bool))),
                schema::StmtView::With { mut bindings, else_param_slot, .. } => {
                    if else_param_slot == Some(slot) {
                        return Ok(self.index.owners[&decoder.owner].with_errors.get(&declaration).cloned().map(Arc::new));
                    }
                    for binding in &mut bindings {
                        self.budget.charge(1)?;
                        let (bound, value) = binding?;
                        if bound != slot { continue; }
                        return Ok(self.need_context(decoder, value.raw() as usize)?.map(|source| match source.unvalidated() {
                            Type::Result(ok, _) => Arc::new((**ok).clone()),
                            _ => source,
                        }));
                    }
                    Ok(None)
                }
                _ => Ok(None),
            };
        }
        if let ProducerBinding::HeaderParameter(parameter) = binding {
            return match self.store.functions[function_index(decoder.owner)?].contract {
                FullCallableContract::Scheme(_) => self.scheme_parameter(decoder.owner, parameter),
                FullCallableContract::Ground(signature) => {
                    let (_, ty, _) = self.store.semantic.signature_param(signature, parameter)?;
                    self.contract_type(ty).map(Some)
                }
            };
        }
        if let ProducerBinding::HeaderCapture(identity) = binding {
            let source = self.index.owners[&decoder.owner].storage_bindings.get(&ProducerBinding::HeaderCapture(identity)).ok_or_else(|| IrVerifyError::new("capture contract is absent from its owner census"))?;
            return self.contract_type(source.ty).map(Some);
        }
        if let ProducerBinding::DriverBinding(_) = binding {
            if let Some(id) = decoder.checked_slot_type(slot)? {
                let ty = self.contract_type(id)?;
                self.slot(decoder.owner, instruction, slot, Arc::clone(&ty))?;
                return Ok(Some(ty));
            }
            let identity = self.index.owners[&decoder.owner].storage_bindings.get(&binding).map(|source| source.identity);
            let ty = match identity.filter(|identity| *identity != IR_NONE) {
                Some(identity) => {
                    self.driver_binding_type(identity)?
                }
                None => None,
            };
            if let Some(ty) = ty {
                ensure_closed_type(&ty)?;
                self.slot(decoder.owner, instruction, slot, Arc::clone(&ty))?;
                return Ok(Some(ty));
            }
        }
        if let Some(id) = decoder.checked_slot_type(slot)? { return self.contract_type(id).map(Some); }
        Ok(None)
    }

    fn pattern_binding_type(&mut self, decoder: &FullDecoder<'a>, binding: ProducerBinding, slot: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        if let Some(ty) = self.pattern_types.get(&(decoder.owner, binding)) { return Ok(Some(Arc::clone(ty))); }
        let source = &self.index.owners[&decoder.owner].pattern_sources[&binding];
        let (instruction, pattern, preceding) = (source.source, source.pattern, source.preceding.clone());
        let Some(subject) = self.need_context(decoder, instruction)? else { return Ok(None) };
        let mut narrowed = Some((*subject).clone());
        for (pattern, guarded) in preceding { narrowed = decoder.subject_after_arm(pattern, guarded, narrowed)?; }
        let Some(ty) = decoder.checked_pattern_bound_type_with_budget(pattern, slot, narrowed.as_ref(), |amount| self.budget.charge(amount))? else { return Ok(None) };
        let ty = Arc::new(ty);
        self.expression(decoder.owner, instruction, subject, false);
        self.pattern_types.insert((decoder.owner, binding), Arc::clone(&ty));
        Ok(Some(ty))
    }

    fn structured_error_payload(
        &mut self,
        owner: u32,
        family: Name,
        variant: Name,
        fields: impl Iterator<Item = Result<(Name, usize), IrVerifyError>>,
        selected: Option<Name>,
    ) -> Result<Option<usize>, IrVerifyError> {
        let key = super::super::CheckedPatternVariantKey { kind: super::super::CheckedPatternVariantKind::Error, owner: family, variant };
        let layout = self.store.declared_pattern_variants.get(&key).ok_or_else(|| IrVerifyError::new("ground error producer has no declared payload layout"))?;
        self.budget.charge(layout.fields.len())?;
        let mut expected_fields = layout.fields.iter().map(|field| field.name.map(|name| (name, field.type_id)).ok_or_else(|| IrVerifyError::new("ground error payload declaration has an unnamed field"))).collect::<Result<BTreeMap<_, _>, _>>()?;
        let mut projection = None;
        for field in fields {
            self.budget.charge(1)?;
            let (name, value) = field?;
            let ty = expected_fields.remove(&name).ok_or_else(|| IrVerifyError::new("ground error producer repeats or adds a payload field"))?;
            let ty = self.contract_type(ty)?;
            self.expression(owner, value, ty, false);
            if selected == Some(name) { projection = Some(value); }
        }
        if !expected_fields.is_empty() { return Err(IrVerifyError::new("ground error producer omits a declared payload field")); }
        if selected.is_some() && projection.is_none() { return Err(IrVerifyError::new("callable error producer omits its projected field")); }
        Ok(projection)
    }

    fn driver_record_producer(&mut self, decoder: &FullDecoder<'a>, value: usize, guard: bool) -> Result<Arc<Type>, IrVerifyError> {
        let (producer, ty, success) = if guard {
            let (_, operand, optional) = decoder.driver_guard_source(value)?;
            match (optional, decoder.expression_view(operand)?) {
                (false, schema::ExprView::Ok(inner)) => {
                    let producer = inner.raw() as usize;
                    let ty = self.need_context(decoder, producer)?.ok_or_else(|| IrVerifyError::new("driver guard has no success producer contract"))?;
                    (producer, Arc::clone(&ty), ty)
                }
                _ => {
                    let ty = self.need_context(decoder, operand)?.ok_or_else(|| IrVerifyError::new("driver guard has no subject producer contract"))?;
                    let payload = match ty.unvalidated() {
                        Type::Result(ok, _) if !optional => ok.as_ref(),
                        Type::Optional(ok) if optional => ok.as_ref(),
                        _ => return Err(IrVerifyError::new("driver guard writer has no success payload")),
                    };
                    self.budget.charge_clone(payload)?;
                    let success = Arc::new(payload.clone());
                    (operand, ty, success)
                }
            }
        } else {
            let ty = self.need_context(decoder, value)?.ok_or_else(|| IrVerifyError::new("driver destructuring has no source producer contract"))?;
            if !matches!(ty.unvalidated(), Type::Record(_)) {
                return Err(IrVerifyError::new("driver destructuring source is not a record"));
            }
            (value, Arc::clone(&ty), ty)
        };
        ensure_closed_type(&success)?;
        self.expression(decoder.owner, producer, ty, false);
        Ok(success)
    }

    fn driver_output_types(&mut self, step: usize) -> Result<Rc<BTreeMap<u32, Arc<Type>>>, IrVerifyError> {
        if let Some(types) = self.driver_output_types.get(&step) { return Ok(Rc::clone(types)); }
        let driver = self.store.driver_steps[step];
        let decoder = self.owner(driver_owner(step).map_err(|_| IrVerifyError::new("driver projection owner is invalid"))?)?;
        let mut payload = super::super::FullCursor::new(self.store.payload(driver.data.range())?);
        let value = payload.raw()? as usize;
        let context = super::super::DecodeContext::from_decoder(&decoder);
        Vec::<(Name, usize)>::borrow(context, &mut payload)?;
        let target = super::super::LoweredCompTarget::borrow(context, &mut payload)?;
        let guard = matches!(target, super::super::decode::CompTargetView::Discard);
        let source = self.driver_record_producer(&decoder, value, guard)?;
        let target = if guard { decoder.driver_guard_source(value)?.0 } else { target };
        let projected = FullDecoder::checked_target_slot_types_with_charge(target, &source, |amount, cloned_type| {
            self.budget.charge(amount)?;
            if let Some(ty) = cloned_type { self.budget.charge_clone(ty)?; }
            Ok(())
        })?;
        let slots = projected.into_iter().map(|(slot, ty)| (slot, Arc::new(ty))).collect::<BTreeMap<_, _>>();
        let writers = bounded(driver.writers, &self.store.driver_writers, "driver value writer range is invalid")?;
        self.budget.charge(writers.len())?;
        let mut types = BTreeMap::new();
        for writer in writers {
            let ty = slots.get(&(writer.slot as usize)).ok_or_else(|| IrVerifyError::new("driver value binding has no canonical output slot"))?;
            types.insert(writer.binding_identity, Arc::clone(ty));
        }
        let types = Rc::new(types);
        self.driver_output_types.insert(step, Rc::clone(&types));
        Ok(types)
    }

    fn driver_binding_type(&mut self, binding: u32) -> Result<Option<Arc<Type>>, IrVerifyError> {
        self.budget.charge(1)?;
        let identity = self.store.binding_identities.get(binding as usize).ok_or_else(|| IrVerifyError::new("driver value binding identity is invalid"))?;
        if identity.flags & 1 != 0 { return Ok(None); }
        let (_, step, _) = self.index.binding_writers.get(binding as usize).and_then(|writer| *writer).ok_or_else(|| IrVerifyError::new("driver value binding has no verified defining writer"))?;
        let owner = driver_owner(step).map_err(|_| IrVerifyError::new("driver value binding owner is invalid"))?;
        let decoder = self.owner(owner)?;
        let driver = self.store.driver_steps[step];
        let mut payload = super::super::FullCursor::new(self.store.payload(driver.data.range())?);
        match driver.tag {
            super::super::FullDriverTag::Use => match TypeId::from_raw(driver.registered_module_type) {
                Some(ty) => self.contract_type(ty).map(Some),
                None => Ok(Some(Arc::new(Type::DynamicModule))),
            },
            super::super::FullDriverTag::Let => {
                let _: Name = super::super::FullCodec::decode(&decoder, &mut payload)?;
                let context = super::super::DecodeContext::from_decoder(&decoder);
                Option::<super::super::LoweredType>::borrow(context, &mut payload)?;
                Option::<super::super::LoweredTypeCheck>::borrow(context, &mut payload)?;
                let annotation = Option::<Type>::borrow(context, &mut payload)?;
                bool::borrow(context, &mut payload)?;
                let value = payload.raw()? as usize;
                match annotation {
                    Some(annotation) => self.contract_type(annotation.id).map(Some),
                    None => self.need_context(&decoder, value),
                }
            }
            super::super::FullDriverTag::LetRecord => Ok(self.driver_output_types(step)?.get(&binding).cloned()),
            _ => Ok(None),
        }
    }

    fn conditional_producer_type(&mut self, decoder: &FullDecoder<'a>, instruction: usize,
        arms: impl Iterator<Item = Result<usize, IrVerifyError>>) -> Result<Option<Arc<Type>>, IrVerifyError>
    {
        let mut joined = ConditionalProducerType::new();
        for arm in arms {
            self.budget.charge(1)?;
            let Some(arm) = decoder.checked_branch_value_instruction(arm?)? else { continue };
            joined.observe(self.need_context(decoder, arm)?);
        }
        let (candidate, unknown) = joined.finish(&mut self.budget)?;
        let Some(candidate) = candidate else { return Ok(None) };
        if unknown {
            ensure_substitution_type(&candidate)?;
            self.expression(decoder.owner, instruction, Arc::clone(&candidate), false);
        }
        Ok(Some(candidate))
    }

    fn mark_runtime_checked_literal_payloads(&mut self, decoder: &FullDecoder<'a>, root: usize) -> Result<(), IrVerifyError> {
        let mut pending = vec![root];
        while let Some(instruction) = pending.pop() {
            self.budget.charge(1)?;
            match decoder.expression_view(instruction)? {
                schema::ExprView::Record(mut entries) => {
                    for entry in &mut entries {
                        self.budget.charge(1)?;
                        if let super::super::decode::RecordEntryView::Field(_, value) = entry? { pending.push(value.raw() as usize); }
                    }
                }
                schema::ExprView::List(mut entries) => for entry in &mut entries { pending.push(entry?.raw() as usize); },
                schema::ExprView::ListBuild(mut entries) => for entry in &mut entries {
                    let (splice, value, _) = entry?;
                    if !splice { pending.push(value.raw() as usize); }
                },
                schema::ExprView::MapLiteral(mut entries) => for entry in &mut entries {
                    let (key, value, _) = entry?;
                    if let Some(key) = key {
                        pending.push(key.raw() as usize);
                        pending.push(value.raw() as usize);
                    }
                },
                schema::ExprView::ListComp { value, .. } => pending.push(value.raw() as usize),
                schema::ExprView::MapComp { key, value, .. } => {
                    pending.push(key.raw() as usize);
                    pending.push(value.raw() as usize);
                }
                schema::ExprView::CheckedValue { value, .. } => pending.push(value.raw() as usize),
                schema::ExprView::Null | schema::ExprView::Unit | schema::ExprView::Bool(_)
                | schema::ExprView::Int(_) | schema::ExprView::UInt(_) | schema::ExprView::Float(_)
                | schema::ExprView::Str(_) | schema::ExprView::Bytes(_) | schema::ExprView::Duration(_)
                | schema::ExprView::PreparedRegex(_) | schema::ExprView::Path(_) => {
                    self.runtime_checked_inputs.insert((decoder.owner, instruction));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn checked_list_accumulator(&mut self, decoder: &FullDecoder<'a>, instruction: usize, checked: &Arc<Type>, runtime_complete: bool) -> Result<bool, IrVerifyError> {
        let Type::List(expected_item) = checked.as_ref() else { return Ok(false) };
        let schema::ExprView::ValueBlock { mut body, .. } = decoder.expression_view(instruction)? else { return Ok(false) };
        if body.len() != 3 { return Ok(false); }
        self.budget.charge(3)?;
        let declaration = body.next().unwrap()?.raw() as usize;
        let run = body.next().unwrap()?.raw() as usize;
        let tail = body.next().unwrap()?.raw() as usize;
        let schema::StmtView::Let { slot, value: empty, annotation: None, .. } = decoder.statement_view(declaration)? else { return Ok(false) };
        if !decoder.is_empty_list_literal_expression(empty.raw() as usize)? { return Ok(false); }
        let schema::StmtView::Expr { value: inner, .. } = decoder.statement_view(run)? else { return Ok(false) };
        if self.store.tags[inner.raw() as usize] != FullTag::ExprValueBlock { return Ok(false); }
        let schema::StmtView::Value { value: result } = decoder.statement_view(tail)? else { return Ok(false) };
        if !matches!(decoder.expression_view(result.raw() as usize)?, schema::ExprView::Param(actual) if actual == slot) { return Ok(false); }
        let binding = self.binding(decoder.owner, result.raw() as usize, slot)?;
        if binding != ProducerBinding::ScopedDeclaration(declaration, slot) { return Ok(false); }
        let Some(writers) = self.index.owners[&decoder.owner].writers.get(&binding) else { return Ok(false) };
        if self.index.owners[&decoder.owner].storage_bindings.contains_key(&binding) { return Ok(false); }
        let mut initialized = false;
        let mut remaining_writers = BTreeSet::new();
        for &writer in writers {
            if writer != declaration {
                self.budget.charge(1)?;
                if !remaining_writers.insert(writer) { return Err(IrVerifyError::new("list accumulator has duplicate writers")); }
            }
        }
        let owner = &self.index.owners[&decoder.owner];
        let mut pending = Vec::new();
        if !remaining_writers.is_empty() {
            self.budget.charge(1)?;
            pending.push(inner.raw() as usize);
        }
        while let Some(instruction) = pending.pop() {
            self.budget.charge(1)?;
            if self.store.tags[instruction].kind() == schema::InstructionKind::Stmt {
                self.budget.charge(1)?;
                remaining_writers.remove(&instruction);
            }
            if remaining_writers.is_empty() { break; }
            let mut child = owner.children[instruction - owner.range.start];
            while child != IR_NONE {
                self.budget.charge(1)?;
                if self.store.tags[child as usize].kind() == schema::InstructionKind::Stmt
                    || owner.children[child as usize - owner.range.start] != IR_NONE
                {
                    pending.push(child as usize);
                }
                child = owner.siblings[child as usize - owner.range.start];
            }
        }
        if !remaining_writers.is_empty() { return Ok(false); }
        for &writer in writers {
            self.budget.charge(1)?;
            if writer == declaration {
                if initialized { return Err(IrVerifyError::new("list accumulator has duplicate initialization")); }
                initialized = true;
                continue;
            }
            let statement = decoder.statement_view(writer)?;
            let schema::StmtView::Assign { slot: written, op: super::super::AssignOp::Add, value, check: None, .. } = statement else { return Ok(false) };
            if written != slot { return Ok(false); }
            let value = value.raw() as usize;
            if decoder.is_empty_list_literal_expression(value)? {
                self.expression(decoder.owner, value, Arc::clone(checked), false);
                continue;
            }
            let Some(actual) = self.declared_type(decoder, value)? else { return Ok(false) };
            let Type::List(item) = actual.unvalidated() else { return Ok(false) };
            let fits = if runtime_complete {
                self.checked_record_field_fits(decoder, value, item, expected_item)?
            } else {
                decoder.checked_type_fits(item, expected_item)
                    && super::initializer_type_matches_annotation(item, expected_item, &mut self.budget, 0)?
            };
            if !fits { return Err(IrVerifyError::new("list accumulator append disagrees with its checked item contract")); }
            self.expression(decoder.owner, value, actual, false);
        }
        if !initialized { return Ok(false); }
        self.expression(decoder.owner, empty.raw() as usize, Arc::clone(checked), false);
        let inner_type = match self.declared_type(decoder, inner.raw() as usize)? {
            Some(actual) => actual,
            None => {
                let schema::ExprView::ValueBlock { mut body, .. } = decoder.expression_view(inner.raw() as usize)? else { unreachable!() };
                match decoder.checked_value_block_result(&mut body)? {
                    super::super::CheckedValueBlockResult::Unit | super::super::CheckedValueBlockResult::DoesNotComplete => Arc::new(Type::Unit),
                    super::super::CheckedValueBlockResult::Tail(_) => return Ok(false),
                }
            }
        };
        self.expression(decoder.owner, inner.raw() as usize, inner_type, false);
        Ok(true)
    }

    fn derive_unannotated_type(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        let literal = match decoder.expression_view(instruction)? {
            schema::ExprView::Null => Some(Type::Null),
            schema::ExprView::Unit => Some(Type::Unit),
            schema::ExprView::Bool(_) => Some(Type::Bool),
            schema::ExprView::Int(_) => Some(Type::Int),
            schema::ExprView::UInt(_) => Some(Type::UInt),
            schema::ExprView::Float(_) => Some(Type::Float),
            schema::ExprView::Str(_) => Some(Type::Str),
            schema::ExprView::Bytes(_) => Some(Type::Bytes),
            schema::ExprView::Duration(_) => Some(Type::Duration),
            schema::ExprView::PreparedRegex(_) => Some(Type::Regex),
            schema::ExprView::Path(_) => Some(Type::Path),
            schema::ExprView::Glob { .. } => Some(Type::List(Box::new(Type::Path))),
            schema::ExprView::LastStatus { .. } => Some(Type::Status),
            schema::ExprView::Range { .. } => Some(Type::Stream(Box::new(Type::Int))),
            schema::ExprView::Record(mut fields) => {
                let writers = decoder.checked_record_field_writers(&mut fields, |source| self.need_context(decoder, source)?.ok_or_else(|| IrVerifyError::new("record spread derivation has no input contract")))?;
                let mut fields = BTreeMap::new();
                for (name, writer) in writers {
                    let ty = match writer {
                        super::super::CheckedRecordFieldWriter::Value(value) => match self.need_context(decoder, value)? { Some(ty) => (*ty).clone(), None => return Ok(None) },
                        super::super::CheckedRecordFieldWriter::Spread { fields, .. } => {
                            let Type::Record(fields) = fields.unvalidated() else { unreachable!("record writer has a non-record spread") };
                            fields[&name].clone()
                        }
                    };
                    fields.insert(name, ty);
                }
                Some(Type::Record(Arc::new(fields)))
            }
            schema::ExprView::Capture { mut body, .. } => {
                let Some(value) = decoder.derive_checked_block_tail_expression(&mut body)? else { return Ok(None) };
                let Some(ok) = self.need_context(decoder, value)? else { return Ok(None) };
                self.budget.charge_clone(&ok)?;
                let errors = self.captured_error_types(decoder, instruction)?;
                Some(Type::Result(Box::new((*ok).clone()), Box::new(self.join_captured_errors(&errors)?)))
            }
            schema::ExprView::ProcessCommandArgv(_) | schema::ExprView::ProcessCommandBuilder { .. } => FullDecoder::derive_registered_api_operation_return_type(super::super::RuntimeOp::ProcessCommandArgv)?,
            schema::ExprView::SpawnRun(_) | schema::ExprView::SpawnCommand { .. } => Some(Type::Result(Box::new(Type::ProcessHandle), Box::new(Type::ProcessError))),
            schema::ExprView::Wait { target, .. } => Some(self.process_wait_contract(decoder, target.raw() as usize)?.1),
            schema::ExprView::RunCapture(run) => match run.kind {
                crate::syntax::node::RunKind::Status => Some(Type::Status),
                _ => Type::run_capture_result(run.kind),
            },
            schema::ExprView::RunPipeline { mut segments, .. } => {
                let first = segments.next().transpose()?.ok_or_else(|| IrVerifyError::new("ground capture pipeline has no head"))?;
                Type::run_capture_result(first.kind)
            }
            schema::ExprView::ErrorContext { mut body, .. } => {
                let block = body.block_identity();
                return match decoder.derive_checked_block_tail_expression(&mut body)? {
                    Some(tail) => self.need_context(decoder, tail),
                    None if super::super::indexed_block_can_return_with_decoder(self.store, block, Some(decoder))? => Ok(None),
                    None => Ok(Some(Arc::new(Type::Unit))),
                };
            }
            schema::ExprView::Retry { mut body, .. } => {
                self.budget.charge(body.len())?;
                let payload = match decoder.checked_value_block_result(&mut body)? {
                    super::super::CheckedValueBlockResult::Tail(tail) => match self.need_context(decoder, tail)? { Some(payload) => payload, None => return Ok(None) },
                    super::super::CheckedValueBlockResult::Unit => Arc::new(Type::Unit),
                    super::super::CheckedValueBlockResult::DoesNotComplete => return Ok(None),
                };
                self.budget.charge_clone(&payload)?;
                if matches!(payload.as_ref(), Type::Result(..)) { Some((*payload).clone()) }
                else {
                    let errors = self.captured_error_types(decoder, instruction)?;
                    Some(Type::Result(Box::new((*payload).clone()), Box::new(self.join_captured_errors(&errors)?)))
                }
            }
            schema::ExprView::ContextScope { mut body, .. } => {
                let block = body.block_identity();
                let payload = match decoder.derive_checked_block_tail_expression(&mut body)? {
                    Some(tail) => match self.need_context(decoder, tail)? { Some(payload) => payload, None => return Ok(None) },
                    None if super::super::indexed_block_can_return_with_decoder(self.store, block, Some(decoder))? => return Ok(None),
                    None => Arc::new(Type::Unit),
                };
                self.budget.charge_clone(&payload)?;
                Some(Type::Result(Box::new((*payload).clone()), Box::new(Type::Error)))
            }
            schema::ExprView::ResultFallback { left, .. } => {
                let Some(source) = self.need_context(decoder, left.raw() as usize)? else { return Ok(None) };
                let payload = match source.unvalidated() {
                    Type::Result(payload, _) | Type::Optional(payload) => payload,
                    _ => return Ok(None),
                };
                self.budget.charge_clone(payload)?;
                Some((**payload).clone())
            }
            schema::ExprView::ValueBlock { mut body, .. } => {
                return match decoder.checked_value_block_result(&mut body)? {
                    super::super::CheckedValueBlockResult::Tail(tail) => {
                        self.need_context(decoder, tail)
                    }
                    super::super::CheckedValueBlockResult::Unit => {
                        Ok(Some(Arc::new(Type::Unit)))
                    }
                    super::super::CheckedValueBlockResult::DoesNotComplete => Ok(None),
                };
            }
            schema::ExprView::IfExpr { branches, else_value, .. } => {
                return self.conditional_producer_type(decoder, instruction,
                    branches.map(|branch| branch.map(|(_, value)| value.raw() as usize))
                        .chain(std::iter::once(Ok(else_value.raw() as usize))));
            }
            schema::ExprView::PatternIf { branches, else_value, .. } => {
                return self.conditional_producer_type(decoder, instruction,
                    branches.map(|branch| branch.map(|(_, value, _)| value.raw() as usize))
                        .chain(std::iter::once(Ok(else_value.raw() as usize))));
            }
            schema::ExprView::MatchExpr { arms, .. } => {
                return self.conditional_producer_type(decoder, instruction,
                    arms.map(|arm| arm.map(|(_, _, value)| value.raw() as usize)));
            }
            schema::ExprView::StrMatchExpr { arms, fallback, .. } | schema::ExprView::TagMatchExpr { arms, fallback, .. } => {
                return self.conditional_producer_type(decoder, instruction,
                    arms.map(|arm| arm.map(|(_, value)| value.raw() as usize))
                        .chain(fallback.map(|value| Ok(value.raw() as usize))));
            }
            schema::ExprView::StrByteLen { .. } => Some(Type::Int),
            schema::ExprView::StrByteAt { .. } => FullDecoder::registered_method_return_type(&Type::Str, "byte_at")?,
            schema::ExprView::StrPredicate { .. } => Some(Type::Bool),
            schema::ExprView::ListComp { value, mut qualifiers, .. } => {
                let Some(item) = self.need_context(decoder, value.raw() as usize)? else { return Ok(None) };
                self.budget.charge_clone(&item)?;
                self.budget.charge(1)?;
                let list = Type::List(Box::new((*item).clone()));
                self.budget.charge(2)?;
                let validation = match qualifiers.next().transpose()? {
                    Some(super::super::decode::CompQualifierView::For { iter, .. })
                        if qualifiers.next().transpose()?.is_none() => self.need_context(decoder, iter.raw() as usize)?
                            .as_ref().and_then(|source| source.validated())
                            .map(|source| source.validation()).filter(|validation| validation.survives_mapping()),
                    _ => None,
                };
                match validation {
                    Some(validation) => {
                        self.budget.charge(1)?;
                        let validated = crate::sema::validated::ValidatedType::new(validation, list)
                            .map_err(|_| IrVerifyError::new("comprehension source validation has no mapped list domain"))?;
                        Some(Type::Validated(Box::new(validated)))
                    }
                    None => Some(list),
                }
            }
            schema::ExprView::List(mut values) => {
                let mut items = Vec::new();
                for value in &mut values {
                    let Some(item) = self.need_context(decoder, value?.raw() as usize)? else { return Ok(None) };
                    items.push((*item).clone());
                }
                return merge_list_item_types(&items, &mut self.budget).map(|ty| ty.map(Arc::new));
            }
            schema::ExprView::Require { check, .. } => Some(Type::Result(Box::new(check.ty.materialize()?), Box::new(Type::Error))),
            schema::ExprView::CheckedValue { check: ValueCheckView::Materialization(id), .. } => {
                let (_, destination) = super::materialization_contract(self.store, self.index.scheme_scopes(self.store)?, id, instruction, &mut self.budget)?;
                return Ok(destination.closed_ground_type().map(Arc::new));
            }
            schema::ExprView::CheckedValue { check: ValueCheckView::Ground(check), .. } => Some(check.ty.materialize()?),
            schema::ExprView::Tag { type_name, .. } => Some(Type::Tag(type_name)),
            schema::ExprView::ComparisonChain { .. } => Some(Type::Bool),
            schema::ExprView::Binary { op: super::super::BinaryOp::Eq | super::super::BinaryOp::Ne | super::super::BinaryOp::Lt | super::super::BinaryOp::Le | super::super::BinaryOp::Gt | super::super::BinaryOp::Ge | super::super::BinaryOp::And | super::super::BinaryOp::Or | super::super::BinaryOp::In | super::super::BinaryOp::NotIn, .. } => Some(Type::Bool),
            schema::ExprView::Binary { op, left, right, .. } => {
                let Some(left) = self.need_context(decoder, left.raw() as usize)? else { return Ok(None) };
                let Some(right) = self.need_context(decoder, right.raw() as usize)? else { return Ok(None) };
                FullDecoder::checked_binary_result_type(op, left.unvalidated(), right.unvalidated())
            }
            schema::ExprView::ListPipeline { .. } => return self.pipeline_type(decoder, instruction),
            schema::ExprView::Param(slot) => return self.slot_context_type(decoder, instruction, slot),
            schema::ExprView::Call { function, .. } | schema::ExprView::DirectPureCall { function, .. } => return self.contract_type(decoder.function_return_type_at(instruction, function.id.index())?).map(Some),
            schema::ExprView::SelfCall { .. } if super::super::driver_owner_index(decoder.owner).is_none() => return self.contract_type(decoder.function_return_type_at(instruction, function_index(decoder.owner)?)?).map(Some),
            schema::ExprView::ModuleCall { op, call_plan, args, .. } => {
                let arguments = args.map(|argument| argument.map(|value| value.map(|value| value.raw() as usize))).collect::<Result<Vec<_>, _>>()?;
                if call_plan.is_none() {
                    let mut result = None;
                    let mut templated = false;
                    for signature in crate::modules::signature::native_static_operation_signatures(op) {
                        if !signature.return_ty.contains_builtin_parameter() && !signature.params.iter().any(|parameter| parameter.ty.contains_builtin_parameter()) { continue; }
                        templated = true;
                        if arguments.len() > signature.params.len() { continue; }
                        let Some(instance) = self.native_signature(decoder, signature, &arguments, None)? else { continue };
                        if result.as_ref().is_some_and(|previous| *previous != instance.return_ty) { return Ok(None); }
                        self.budget.charge_clone(&instance.return_ty)?;
                        result = Some(instance.return_ty.clone());
                    }
                    if templated { return Ok(result.map(Arc::new)); }
                }
                return FullDecoder::checked_module_call_return_type(op, call_plan.map(AsRef::as_ref), &arguments,
                    |argument| self.need_context(decoder, argument).map(|ty| ty.map(|ty| (*ty).clone())))
                    .map(|ty| ty.map(Arc::new));
            }
            schema::ExprView::Try { value, .. } => {
                let Some(source) = self.need_context(decoder, value.raw() as usize)? else { return Ok(None) };
                let Some(payload) = source.result_ok() else { return Ok(None) };
                self.budget.charge_clone(payload)?;
                return Ok(Some(Arc::new(payload.clone())));
            }
            schema::ExprView::TypedCall { signature, .. } => {
                let contract = self.contract_type(signature.id)?;
                let Type::Callable(callable) = contract.as_ref() else {
                    return Err(IrVerifyError::new("typed producer call has no callable contract"));
                };
                self.budget.charge_clone(&callable.sig.return_ty)?;
                Some((*callable.sig.return_ty).clone())
            }
            schema::ExprView::DynamicCall { callee, .. } => {
                let Some((_, _, callable)) = self.module_callable(decoder, callee.raw() as usize)? else { return Ok(None) };
                Some(*callable.sig.return_ty)
            }

            schema::ExprView::Index { base, .. } | schema::ExprView::IndexFromEnd { base, .. } => {
                if let Some(base) = self.need_context(decoder, base.raw() as usize)? { return FullDecoder::checked_index_type((*base).clone()).map(|ty| Some(Arc::new(ty))); }
                None
            }
            schema::ExprView::Slice { base, .. } => {
                if let Some(base) = self.need_context(decoder, base.raw() as usize)? { return FullDecoder::checked_slice_type((*base).clone()).map(|ty| Some(Arc::new(ty))); }
                None
            }
            schema::ExprView::Method { receiver, name, args, .. } => {
                let arguments = args.collect::<Result<Vec<_>, _>>()?.into_iter().map(|argument| argument.raw() as usize).collect::<Vec<_>>();
                if let Some(receiver) = self.need_context(decoder, receiver.raw() as usize)? {
                    return decoder.checked_method_return_type(&receiver, name, &arguments).map(|ty| ty.map(Arc::new));
                }
                None
            }
            schema::ExprView::Field { base, name, .. } => {
                if let Some(ty) = self.scoped_field_type(decoder, instruction)? {
                    return Ok(ty.closed_ground_type().map(Arc::new));
                }
                let receiver_instruction = base.raw() as usize;
                if let Some(base) = self.need_context(decoder, receiver_instruction)? {
                    let receiver = match base.unvalidated() {
                        Type::Optional(payload) if self.optional_read_is_narrowed(decoder, receiver_instruction, &base)? => payload.unvalidated(),
                        receiver => receiver,
                    };
                    match receiver {
                        Type::Record(fields) => return Ok(fields.get(&Name::intern(name)).cloned().map(Arc::new)),
                        Type::Module(module) => return Ok(module.exports.get(&Name::intern(name)).and_then(crate::sema::types::ModuleExportType::field_value_type).map(Arc::new)),
                        receiver => return Ok(crate::sema::check::Checker::fixed_field_type(receiver, Name::intern(name)).flatten().map(Arc::new)),
                    }
                }
                None
            }
            _ => None,
        };
        if let Some(ty) = literal { return Ok(Some(Arc::new(ty))); }
        if let Some(id) = checked_expression_type_id(self.store, instruction as u32) { return self.contract_type(id).map(Some); }
        Ok(match checked_argument_type(self.store, decoder, instruction, instruction, &mut self.budget, 0)? {
            Some(CheckedArgumentType::Known(ty)) => Some(Arc::new(ty)),
            Some(CheckedArgumentType::ListItems(items)) => merge_list_item_types(&items, &mut self.budget)?.map(Arc::new),
            Some(CheckedArgumentType::EmptyList(ty)) => ty.map(Arc::new),
            Some(CheckedArgumentType::EmptyMap) => None,
            None => None,
        })
    }

    fn need_context(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        if let Some(ty) = self.declared_type(decoder, instruction)? { return self.declared_read_type(decoder, instruction, ty).map(Some); }
        self.contextual_producer_type(decoder, instruction)
    }

    fn declared_read_type(&self, decoder: &FullDecoder<'a>, instruction: usize, declared: Arc<Type>) -> Result<Arc<Type>, IrVerifyError> {
        if declared.as_ref() == &Type::Any
            && let Some(prefix) = &decoder.producer_prefix
            && let Some(narrowed) = prefix.runtime_list_read_type(decoder, instruction)?
        { return Ok(Arc::new(narrowed)); }
        if matches!(declared.as_ref(), Type::Union(_))
            && let Some(prefix) = &decoder.producer_prefix
            && let Some(narrowed) = prefix.union_member_read_type(decoder, instruction, &declared)?
        { return Ok(Arc::new(narrowed)); }
        Ok(declared)
    }

    fn contextual_producer_type(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        let key = instruction as u32;
        let owner = super::super::ExecutableOwner::from_raw(decoder.owner)?;
        let ty = match &self.contexts {
            ProducerContexts::Pending(candidates) => {
                match candidates.binary_search_by_key(&key, |(instruction, _, _)| *instruction) {
                    Ok(index) => {
                        let (_, candidate_owner, ty) = &candidates[index];
                        if *candidate_owner != owner
                            || index.checked_sub(1).is_some_and(|previous| candidates[previous].0 == key)
                            || candidates.get(index + 1).is_some_and(|next| next.0 == key)
                        {
                            return Err(IrVerifyError::new("pending producer context is duplicated or has a wrong owner"));
                        }
                        Some(Arc::clone(ty))
                    }
                    Err(_) => None,
                }
            }
            ProducerContexts::Published => {
                self.store.checked_expr_types.binary_search_by_key(&key, |fact| fact.instruction).ok()
                    .and_then(|index| {
                        let fact = self.store.checked_expr_types[index];
                        match fact.purpose {
                            super::super::FullCheckedExprPurpose::ProducerContext(candidate_owner) if candidate_owner == owner => Some(fact.type_id),
                            _ => None,
                        }
                    }).map(|id| self.ground_type(id)).transpose()?
            }
        };
        if let Some(ty) = &ty {
            ensure_substitution_type(ty)?;
            self.used_contexts.insert(key, (owner, Arc::clone(ty)));
            self.expression(decoder.owner, instruction, Arc::clone(ty), false);
        }
        Ok(ty)
    }

    fn binding_producer(&mut self, binding: u32, purpose: ProducerTargetPurpose, path: &mut Vec<FunctionProjection>) -> Result<(u32, CallableProducer), IrVerifyError> {
        let (_, step, _) = self.index.binding_writers.get(binding as usize).and_then(|writer| *writer).ok_or_else(|| IrVerifyError::new("callable capture has no lexical initializer"))?;
        let owner = driver_owner(step).map_err(|_| IrVerifyError::new("callable capture initializer owner is invalid"))?;
        let decoder = self.owner(owner)?;
        let row = self.store.driver_steps[step];
        if row.tag == super::super::FullDriverTag::Use {
            if purpose == ProducerTargetPurpose::LiteralPayload { return Err(IrVerifyError::new("generic record capture has no literal lexical initializer")); }
            let type_id = TypeId::from_raw(row.registered_module_type)
                .ok_or_else(|| IrVerifyError::new("callable import has no registered module contract"))?;
            let ty = self.contract_type(type_id)?;
            let Type::Module(module) = ty.as_ref() else { return Err(IrVerifyError::new("callable import registration is not a module")) };
            let Some(FunctionProjection::Field(field)) = path.pop() else { return Err(IrVerifyError::new("callable import has no selected export projection")) };
            if !module.exports.contains_key(&field) { return Err(IrVerifyError::new("callable import projection is absent from its registered module")); }
        self.binding_writer(binding, Arc::clone(&ty))?;
        let mut payload = super::super::FullCursor::new(self.store.payload(row.data.range())?);
        let _: Arc<str> = super::super::FullCodec::decode(&decoder, &mut payload)?;
        let _: Option<Name> = super::super::FullCodec::decode(&decoder, &mut payload)?;
        let _path: Vec<Name> = super::super::FullCodec::decode(&decoder, &mut payload)?;
        let _: Name = super::super::FullCodec::decode(&decoder, &mut payload)?;
        let exports: Vec<super::super::LoweredModuleExport> = super::super::FullCodec::decode(&decoder, &mut payload)?;
        self.budget.charge(exports.len())?;
        let child: u32 = super::super::FullCodec::decode(&decoder, &mut payload)?;
        if module.exact
            && path.is_empty()
            && let Some(export) = exports.iter().find(|export| match export {
                super::super::LoweredModuleExport::GenericCallable { name, .. } => *name == field,
                _ => false,
            })
        {
            let super::super::LoweredModuleExport::GenericCallable { kind, function_namespace, .. } = export else { unreachable!() };
            if !matches!(module.exports.get(&field), Some(ModuleExportType::GenericCallable { kind: registered, optional: false }) if registered == kind) {
                return Err(IrVerifyError::new("callable import registration disagrees with its generic export"));
            }
            let child_index = child.checked_sub(1).map(|index| index as usize)
                .filter(|index| *index < self.store.driver_programs.len())
                .ok_or_else(|| IrVerifyError::new("callable import child program is invalid"))?;
            let child_owner = self.store.driver_programs[child_index].owner;
            if child_owner == IR_NONE || self.store.string(child_owner)? != function_namespace.as_str() {
                return Err(IrVerifyError::new("generic callable import is not owned by its child module"));
            }
            let function = super::super::decode::FunctionNameView::Qualified(crate::symbol::QualifiedName::new(*function_namespace, field));
            let (target, contract, target_kind) = decoder.function_reference_contract(function)?;
            if target_kind != *kind { return Err(IrVerifyError::new("generic callable import kind differs from its target")); }
            let metadata = self.store.function_metadata.get(target.index())
                .ok_or_else(|| IrVerifyError::new("generic callable import target metadata is missing"))?;
            if metadata.owner != child_owner || metadata.callable_kind != *kind
                || metadata.flags != (0b10 | u8::from(*kind == CallableKind::Proc))
                || self.store.string(self.store.functions[target.index()].name)? != field.as_str()
            { return Err(IrVerifyError::new("generic callable import target differs from its export")); }
            let super::super::FullCallableContract::Scheme(scheme_id) = contract else {
                return Err(IrVerifyError::new("generic callable import has no scheme contract"));
            };
            let scheme = self.store.scheme(scheme_id)?;
            if scheme.callable_kind != *kind || !scheme.is_quantified(&self.store)? {
                return Err(IrVerifyError::new("generic callable import has no matching quantified contract"));
            }
            return Ok((owner, CallableProducer::Function(target.index())));
        }
            let exports = self.index.module_exports.get(&owner)
                .ok_or_else(|| IrVerifyError::new("callable import has no verified export census"))?;
            self.budget.charge(exports.len())?;
            let mut selected = exports.iter().filter(|(name, _)| *name == field);
            let export = selected.next().map(|(_, binding)| *binding)
                .ok_or_else(|| IrVerifyError::new("callable import projection has no value binding"))?;
            if selected.next().is_some() { return Err(IrVerifyError::new("callable import projection has duplicate value bindings")); }
            return Ok((owner, CallableProducer::Binding(export)));
        }
        if row.tag != super::super::FullDriverTag::Let { return Err(IrVerifyError::new("callable capture initializer is not a value declaration")); }
        let mut cursor = super::super::FullCursor::new(self.store.payload(row.data.range())?);
        let _: Name = super::super::FullCodec::decode(&decoder, &mut cursor)?;
        let _: Option<super::super::LoweredType> = super::super::FullCodec::decode(&decoder, &mut cursor)?;
        let _: Option<super::super::LoweredTypeCheck> = super::super::FullCodec::decode(&decoder, &mut cursor)?;
        let _: Option<Type> = super::super::FullCodec::decode(&decoder, &mut cursor)?;
        let mutable: bool = super::super::FullCodec::decode(&decoder, &mut cursor)?;
        if mutable && matches!(purpose, ProducerTargetPurpose::Callable | ProducerTargetPurpose::LiteralPayload) { return Err(IrVerifyError::new("callable capture has mutable lexical storage")); }
        Ok((owner, CallableProducer::Expression(cursor.raw()? as usize)))
    }

    fn callable_assignment_projection(
        &mut self,
        decoder: &FullDecoder<'a>,
        instruction: usize,
        written: &[FunctionProjection],
        requested: &[FunctionProjection],
    ) -> Result<Option<Vec<FunctionProjection>>, IrVerifyError> {
        self.budget.charge(1 + written.len())?;
        let mut remaining = requested.to_vec();
        if written.is_empty() { return Ok(Some(remaining)); }
        let source = self.need_context(decoder, instruction)?.ok_or_else(|| IrVerifyError::new("callable partial writer has no binding contract"))?;
        let mut current = source.as_ref();
        for &step in written {
            loop {
                current = match current { Type::Validated(validated) => validated.base(), Type::Optional(inner) => inner, _ => break };
            }
            let requested = remaining.pop();
            match (step, current) {
                (FunctionProjection::Field(field), Type::Record(fields)) if requested == Some(step) => {
                    let Some(next) = fields.get(&field) else { return Ok(None) };
                    current = next;
                }
                (FunctionProjection::Element, Type::List(item) | Type::Set(item)) if matches!(requested, Some(FunctionProjection::Element | FunctionProjection::IterationItem)) => current = item,
                (FunctionProjection::Element, Type::Map(_, item)) if matches!(requested, Some(FunctionProjection::Element | FunctionProjection::IterationItem)) => {
                    if requested == Some(FunctionProjection::IterationItem) && remaining.pop() != Some(FunctionProjection::Field(Name::intern("value"))) { return Ok(None); }
                    current = item;
                }
                _ => return Ok(None),
            }
        }
        Ok(Some(remaining))
    }

    fn mutable_storage_contract_source(&mut self, owner: u32, binding: ProducerBinding) -> Result<bool, IrVerifyError> {
        let Some(source) = self.index.owners[&owner].storage_bindings.get(&binding).copied().filter(|source| source.mutable) else { return Ok(false) };
        let ty = self.contract_type(source.ty)?;
        self.budget.charge_clone(&ty)?;
        if !ty.contains_typed_callable() { return Ok(false); }
        self.binding_writer(source.identity, ty)?;
        Ok(true)
    }

    fn function_targets(&mut self, owner: u32, instruction: usize) -> Result<BTreeSet<usize>, IrVerifyError> {
        Ok(self.producer_targets(owner, instruction, ProducerTargetPurpose::Callable, Vec::new())?.functions)
    }

    fn producer_targets(&mut self, owner: u32, instruction: usize, purpose: ProducerTargetPurpose, projection: Vec<FunctionProjection>) -> Result<ProducerTargets, IrVerifyError> {
        let mut summaries = StreamSummaries::default();
        let result = self.producer_target_walk(owner, instruction, purpose, projection.clone(), &mut summaries, None)?;
        if purpose == ProducerTargetPurpose::Callable { return Ok(result); }
        loop {
            while let Some(key) = summaries.pending.pop_first() {
                self.budget.charge(1 + key.1.len())?;
                let result = self.producer_target_walk(function_owner(key.0)?, instruction, purpose, key.1.clone(), &mut summaries, Some(&key))?;
                if summaries.values.get(&key).is_none_or(|previous| **previous != result) {
                    summaries.values.insert(key.clone(), Arc::new(result));
                    for dependent in summaries.dependents.get(&key).into_iter().flatten() {
                        self.budget.charge(1 + dependent.1.len())?;
                        summaries.pending.insert(dependent.clone());
                    }
                }
            }
            let result = self.producer_target_walk(owner, instruction, purpose, projection.clone(), &mut summaries, None)?;
            if summaries.pending.is_empty() { return Ok(result); }
        }
    }

    fn producer_target_walk(&mut self, owner: u32, instruction: usize, purpose: ProducerTargetPurpose, projection: Vec<FunctionProjection>, summaries: &mut StreamSummaries, summary: Option<&StreamSummaryKey>) -> Result<ProducerTargets, IrVerifyError> {
        let mut visited = BTreeSet::new();
        let mut pending = Vec::new();
        if summary.is_some() {
            for returned in self.index.owners[&owner].header_returns().copied().collect::<Vec<_>>() {
                self.budget.charge(1)?;
                let decoder = self.owner(owner)?;
                let schema::StmtView::Return { value } = decoder.statement_view(returned)? else { unreachable!() };
                let value = value.raw() as usize;
                if projection.is_empty() && !self.need_context(&decoder, value)?.is_some_and(|ty| matches!(ty.unvalidated(), Type::Stream(_))) { continue; }
                pending.push((owner, CallableProducer::Expression(value), projection.clone()));
            }
        } else { pending.push((owner, CallableProducer::Expression(instruction), projection)); }
        let mut parameters = BTreeSet::new();
        let mut targets = BTreeSet::new();
        let mut has_contract_source = false;
        while let Some((owner, source, mut path)) = pending.pop() {
            self.budget.charge(1 + path.len())?;
            if !visited.insert((owner, source, path.clone())) { continue; }
            let decoder = self.owner(owner)?;
            let instruction = match source {
                CallableProducer::Binding(binding) => {
                    let (source_owner, source) = self.binding_producer(binding, purpose, &mut path)?;
                    pending.push((source_owner, source, path));
                    continue;
                }
                CallableProducer::Expression(instruction) => instruction,
                CallableProducer::Function(function) => {
                    if !path.is_empty() { return Err(IrVerifyError::new("imported callable has an unresolved result projection")); }
                    targets.insert(function);
                    continue;
                }
                CallableProducer::PipelinePrefix(pipeline, end) => {
                    if end == 0 {
                        let schema::ExprView::ListPipeline { input, .. } = decoder.expression_view(pipeline)? else { unreachable!("callable pipeline input has another expression") };
                        path.push(FunctionProjection::IterationItem);
                        pending.push((owner, CallableProducer::Expression(input.raw() as usize), path));
                    } else {
                        let stage = self.pipeline_stage(&decoder, pipeline, end - 1)?;
                        use super::super::StageView as S;
                        match &stage {
                            S::Map { value, .. } | S::MapBlock { value, .. } | S::ParMap { value, .. } | S::ParMapBlock { value, .. }
                            | S::FlatMap { value, .. } | S::FlatMapBlock { value, .. } => {
                                if matches!(stage, S::FlatMap { .. } | S::FlatMapBlock { .. }) { path.push(FunctionProjection::Element); }
                                if super::super::indexed_stage_callback_can_complete(&decoder, &stage)? { pending.push((owner, CallableProducer::Expression(value.raw() as usize), path.clone())); }

                            }
                            S::Enumerate => {
                                if path.pop() != Some(FunctionProjection::Field(Name::intern("value"))) { return Err(IrVerifyError::new("callable enumeration projection has no value field")); }
                                pending.push((owner, CallableProducer::PipelinePrefix(pipeline, end - 1), path));
                            }
                            S::Zip { other } => match path.pop() {
                                Some(FunctionProjection::Field(name)) if name == Name::intern("left") => pending.push((owner, CallableProducer::PipelinePrefix(pipeline, end - 1), path)),
                                Some(FunctionProjection::Field(name)) if name == Name::intern("right") => { path.push(FunctionProjection::Element); pending.push((owner, CallableProducer::Expression(other.raw() as usize), path)); }
                                _ => return Err(IrVerifyError::new("callable zip projection has no item field")),
                            },
                            S::GroupBy { .. } => {
                                if path.pop() != Some(FunctionProjection::Field(Name::intern("items"))) || !matches!(path.pop(), Some(FunctionProjection::Element | FunctionProjection::IterationItem)) { return Err(IrVerifyError::new("callable group projection has no item")); }
                                pending.push((owner, CallableProducer::PipelinePrefix(pipeline, end - 1), path));
                            }
                            S::BatchCount { .. } | S::BatchMaxArgv { .. } | S::BatchMaxBytes { .. } | S::BatchLimits { .. } => {
                                if path.pop() != Some(FunctionProjection::Element) { return Err(IrVerifyError::new("callable batch projection has no item")); }
                                pending.push((owner, CallableProducer::PipelinePrefix(pipeline, end - 1), path));
                            }
                            S::Where { .. } | S::WhereBlock { .. } | S::Sort { .. } | S::SortBy { .. } | S::UniqueBy { .. } | S::Tee { .. }
                            | S::Take(_) | S::Drop(_) | S::Repeat { .. } | S::Shuffle { .. } => pending.push((owner, CallableProducer::PipelinePrefix(pipeline, end - 1), path)),
                            _ => return Err(IrVerifyError::new("callable pipeline stage has no value projection")),
                        }
                    }
                    continue;
                }
            };
            if purpose == ProducerTargetPurpose::Stream {
                let functions = match decoder.expression_view(instruction)? {
                    schema::ExprView::Call { .. } | schema::ExprView::DirectPureCall { .. } | schema::ExprView::SelfCall { .. } => Some(BTreeSet::from([call_parts(&decoder, instruction)?.0])),
                    schema::ExprView::TypedCall { callee, .. } => {
                        let result = self.producer_targets(owner, callee.raw() as usize, ProducerTargetPurpose::Callable, Vec::new())?;
                        has_contract_source |= result.opaque || result.functions.is_empty();
                        Some(result.functions)
                    }
                    schema::ExprView::Range { .. } if path.is_empty() => continue,
                    schema::ExprView::RunCapture(_) | schema::ExprView::RunPipeline { .. } | schema::ExprView::FsFiles { .. } | schema::ExprView::FsWalk { .. }
                        if path.is_empty() || path.as_slice() == [FunctionProjection::ResultPayload(true)] =>
                    {
                        self.verify_host_stream_projection(&decoder, instruction, &path)?;
                        has_contract_source = true;
                        continue;
                    }
                    schema::ExprView::ModuleCall { .. } if path.is_empty() || path.as_slice() == [FunctionProjection::ResultPayload(true)] => {
                        self.verify_host_stream_projection(&decoder, instruction, &path)?;
                        has_contract_source = true;
                        continue;
                    }
                    schema::ExprView::Method { .. } if path.is_empty() => {
                        self.verify_host_stream_projection(&decoder, instruction, &path)?;
                        has_contract_source = true;
                        continue;
                    }
                    _ => None,
                };
                if let Some(functions) = functions {
                    let arguments = call_values(&decoder, instruction)?;
                    self.budget.charge(1 + arguments.len())?;
                    for function in functions {
                        self.budget.charge(1 + path.len())?;
                        if path.is_empty() { targets.insert(function); }
                        let key = (function, path.clone());
                        if !summaries.values.contains_key(&key) {
                            summaries.values.insert(key.clone(), Arc::new(ProducerTargets::default()));
                            summaries.pending.insert(key.clone());
                        }
                        if let Some(parent) = summary { summaries.dependents.entry(key.clone()).or_default().insert(parent.clone()); }
                        let result = Arc::clone(&summaries.values[&key]);
                        has_contract_source |= result.opaque;
                        for target in &result.functions { self.budget.charge(1)?; targets.insert(*target); }
                        for (parameter, path) in &result.parameters {
                            self.budget.charge(1 + path.len())?;
                            match arguments.get(*parameter) {
                                Some(ProducerArgument::Single(value)) => pending.push((owner, CallableProducer::Expression(*value), path.clone())),
                                Some(ProducerArgument::Default(_)) => {
                                    let target = function_owner(function)?;
                                    let decoder = self.owner(target)?;
                                    let binding = ProducerBinding::HeaderParameter(*parameter);
                                    for writer in self.index.owners[&target].writers.get(&binding).into_iter().flatten() {
                                        if let schema::StmtView::DefaultParameter { value, .. } = decoder.statement_view(*writer)? { pending.push((target, CallableProducer::Expression(value.raw() as usize), path.clone())); }
                                    }
                                }
                                _ => return Err(IrVerifyError::new("stream relay parameter has no actual argument")),
                            }
                        }
                    }
                    continue;
                }
            }
            match decoder.expression_view(instruction)? {
                schema::ExprView::FunctionRef { function, kind } if path.is_empty() => {
                    let (target, _, selected_kind) = decoder.function_reference_contract(function)?;
                    if kind != selected_kind { return Err(IrVerifyError::new("ground callable producer kind disagrees with its function")); }
                    targets.insert(target.index());
                }
                schema::ExprView::Param(slot) => {
                    let binding = self.binding(owner, instruction, slot)?;
                    if purpose == ProducerTargetPurpose::Callable && self.mutable_storage_contract_source(owner, binding)? { has_contract_source = true; continue; }
                    if purpose == ProducerTargetPurpose::Stream
                        && let Some(storage) = self.index.owners[&owner].storage_bindings.get(&binding).copied().filter(|storage| storage.identity != IR_NONE)
                    {
                        let contract = self.contract_type(storage.ty)?;
                        self.binding_writer(storage.identity, contract)?;
                        for writer in self.index.storage_writers.get(&storage.identity).cloned().unwrap_or_default() {
                            self.budget.charge(1)?;
                            match writer {
                                StorageWriter::Statement { owner: writer_owner, instruction: writer, .. } => {
                                    let writer_decoder = self.owner(writer_owner)?;
                                    let (written, value) = match writer_decoder.statement_view(writer)? {
                                        schema::StmtView::Assign { value, .. } => (Vec::new(), value.raw() as usize),
                                        schema::StmtView::AssignField { field, value, .. } => (vec![FunctionProjection::Field(Name::intern(field))], value.raw() as usize),
                                        schema::StmtView::AssignPath { path, value, .. } => (path.map(|step| step.map(|step| match step { super::super::decode::AssignStepView::Field(field) => FunctionProjection::Field(field), super::super::decode::AssignStepView::Index(_) => FunctionProjection::Element })).collect::<Result<Vec<_>, _>>()?, value.raw() as usize),
                                        _ => continue,
                                    };
                                    if let Some(remaining) = self.callable_assignment_projection(&decoder, instruction, &written, &path)? { pending.push((writer_owner, CallableProducer::Expression(value), remaining)); }
                                }
                                StorageWriter::DriverAssign { step } => {
                                    let driver = self.store.driver_steps[step];
                                    let writer_decoder = self.owner(driver_owner(step).map_err(|_| IrVerifyError::new("stream writer owner is invalid"))?)?;
                                    let mut payload = super::super::FullCursor::new(self.store.payload(driver.data.range())?);
                                    let _: Name = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
                                    let _ = payload.raw()?;
                                    let _: crate::syntax::node::AssignOp = super::super::FullCodec::decode(&writer_decoder, &mut payload)?;
                                    pending.push((writer_decoder.owner, CallableProducer::Expression(payload.raw()? as usize), path.clone()));
                                }
                            }
                        }
                    }
                    for writer in self.index.owners[&owner].writers.get(&binding).cloned().unwrap_or_default() {
                        self.budget.charge(1)?;
                        let (written, value) = match decoder.statement_view(writer)? {
                            schema::StmtView::Assign { value, .. } => (Vec::new(), value.raw() as usize),
                            schema::StmtView::AssignField { field, value, .. } => (vec![FunctionProjection::Field(Name::intern(field))], value.raw() as usize),
                            schema::StmtView::AssignPath { path, value, .. } => {
                                let written = path.map(|step| step.map(|step| match step { super::super::decode::AssignStepView::Field(field) => FunctionProjection::Field(field), super::super::decode::AssignStepView::Index(_) => FunctionProjection::Element })).collect::<Result<Vec<_>, _>>()?;
                                (written, value.raw() as usize)
                            }
                            _ => continue,
                        };
                        if let Some(remaining) = self.callable_assignment_projection(&decoder, instruction, &written, &path)? {
                            pending.push((owner, CallableProducer::Expression(value), remaining));
                        }
                    }
                    if let Some(source) = self.index.owners[&owner].pattern_sources.get(&binding) {
                        let (source, pattern, preceding) = (source.source, source.pattern, source.preceding.clone());
                        if self.pattern_binding_type(&decoder, binding, slot)?.is_some_and(|ty| ty.contains_typed_callable()) {
                            has_contract_source = true;
                        }
                        let ty = self.need_context(&decoder, source)?.ok_or_else(|| IrVerifyError::new("callable pattern subject has no input contract"))?;
                        let mut subject = Some((*ty).clone());
                        for (pattern, guarded) in preceding { subject = decoder.subject_after_arm(pattern, guarded, subject)?; }
                        for projection in decoder.checked_pattern_binding_paths(pattern, slot, subject.as_ref(), |amount| self.budget.charge(amount))? {
                            let mut path = path.clone();
                            path.extend(projection.into_iter().rev());
                            pending.push((owner, CallableProducer::Expression(source), path));
                        }
                        continue;
                    }
                    if let Some(&(comprehension, iter)) = self.index.owners[&owner].comprehension_sources.get(&binding) {
                        let mut qualifiers = match decoder.expression_view(comprehension)? {
                            schema::ExprView::ListComp { qualifiers, .. } | schema::ExprView::MapComp { qualifiers, .. } => qualifiers,
                            _ => unreachable!("callable comprehension binding has no comprehension"),
                        };
                        for qualifier in &mut qualifiers {
                            self.budget.charge(1)?;
                            if let super::super::decode::CompQualifierView::For { target, iter: source, .. } = qualifier?
                                && source.raw() as usize == iter
                                && let Some(fields) = FullDecoder::checked_target_field_path(&target.materialize()?, slot)
                            {
                                let mut path = path.clone();
                                path.extend(fields.into_iter().rev().map(FunctionProjection::Field));
                                path.push(FunctionProjection::IterationItem);
                                pending.push((owner, CallableProducer::Expression(iter), path));
                            }
                        }
                        continue;
                    }
                    if let Some(source) = self.index.owners[&owner].pipeline_sources.get(&binding).copied() {
                        match source.role {
                            super::super::PipelineStageSlotRole::Item => pending.push((owner, CallableProducer::PipelinePrefix(source.pipeline, source.stage), path)),
                            super::super::PipelineStageSlotRole::Accumulator => {
                                let stage = self.pipeline_stage(&decoder, source.pipeline, source.stage)?;
                                let super::super::StageView::Fold { initial, value, .. } = stage else { unreachable!("callable accumulator has a non-fold stage") };
                                pending.push((owner, CallableProducer::Expression(initial.raw() as usize), path.clone()));
                                pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                            }
                            super::super::PipelineStageSlotRole::ReducedItem { value, flatten } => {
                                if flatten { path.push(FunctionProjection::Element); }
                                pending.push((owner, CallableProducer::Expression(value), path));
                            }
                        }
                        continue;
                    }
                    match binding {
                        ProducerBinding::ScopedDeclaration(declaration, _) => {
                            let writers = self.index.owners[&owner].writers.get(&binding).cloned().unwrap_or_default();
                            for writer in writers {
                                match decoder.statement_view(writer)? {
                                    schema::StmtView::Let { value, .. } | schema::StmtView::DefaultParameter { value, .. } => pending.push((owner, CallableProducer::Expression(value.raw() as usize), path.clone())),
                                    schema::StmtView::Assign { .. } | schema::StmtView::AssignField { .. } | schema::StmtView::AssignPath { .. } => {}
                                    schema::StmtView::For { iter, .. } => {
                                        let mut path = path.clone();
                                        path.push(FunctionProjection::IterationItem);
                                        pending.push((owner, CallableProducer::Expression(iter.raw() as usize), path));
                                    }
                                    schema::StmtView::ForRecord { target, iter, .. } => {
                                        let fields = FullDecoder::checked_target_field_path(&target.materialize()?, slot).ok_or_else(|| IrVerifyError::new("callable loop binder has no field projection"))?;
                                        let mut path = path.clone();
                                        path.extend(fields.into_iter().rev().map(FunctionProjection::Field));
                                        path.push(FunctionProjection::IterationItem);
                                        pending.push((owner, CallableProducer::Expression(iter.raw() as usize), path));
                                    }
                                    schema::StmtView::LetRecord { target, source, .. } => {
                                        let fields = FullDecoder::checked_target_field_path(&target.materialize()?, slot).ok_or_else(|| IrVerifyError::new("callable destructuring binder has no field projection"))?;
                                        let mut path = path.clone();
                                        path.extend(fields.into_iter().rev().map(FunctionProjection::Field));
                                        pending.push((owner, CallableProducer::Expression(source.raw() as usize), path));
                                    }
                                    schema::StmtView::Guard { target, value, optional, else_param_slot, .. } => {
                                        let mut path = path.clone();
                                        if else_param_slot == Some(slot) {
                                            if optional { return Err(IrVerifyError::new("callable optional guard has an error binder")); }
                                            path.push(FunctionProjection::ResultPayload(false));
                                        } else {
                                            let fields = FullDecoder::checked_target_field_path(&target.materialize()?, slot).ok_or_else(|| IrVerifyError::new("callable guard binder has no field projection"))?;
                                            path.extend(fields.into_iter().rev().map(FunctionProjection::Field));
                                            if !optional { path.push(FunctionProjection::ResultPayload(true)); }
                                        }
                                        pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                                    }
                                    schema::StmtView::With { bindings, else_param_slot, .. } => {
                                        let failure = else_param_slot == Some(slot);
                                        let mut found = failure;
                                        for binding in bindings {
                                            self.budget.charge(1)?;
                                            let (bound, value) = binding?;
                                            if bound != slot && !failure { continue; }
                                            found = true;
                                            let source = self.need_context(&decoder, value.raw() as usize)?;
                                            if failure && !source.as_ref().is_some_and(|ty| ty.is_result()) { continue; }
                                            let mut path = path.clone();
                                            if source.as_ref().is_some_and(|ty| ty.is_result()) { path.push(FunctionProjection::ResultPayload(!failure)); }
                                            pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                                        }
                                        if !found { return Err(IrVerifyError::new("callable with binder has no value producer")); }
                                        if failure {
                                            let contract = self.index.owners[&owner].with_errors.get(&writer).ok_or_else(|| IrVerifyError::new("callable With failure has no verified error contract"))?;
                                            if contract.contains_typed_callable() { has_contract_source = true; }
                                        }
                                    }
                                    _ => return Err(IrVerifyError::new("ground callable lexical writer is not a value transfer")),
                                }
                            }
                            if !self.index.owners[&owner].writers.contains_key(&binding) && !self.index.owners[&owner].pattern_sources.contains_key(&binding) { return Err(IrVerifyError::new(format!("ground callable declaration {declaration} has no lexical writer"))); }
                        }
                        ProducerBinding::HeaderParameter(parameter) => {
                            if purpose == ProducerTargetPurpose::Stream {
                                if summary.is_some_and(|key| function_owner(key.0).ok() == Some(owner)) { parameters.insert((parameter, path)); }
                                else { has_contract_source = true; }
                                continue;
                            }
                            let function = function_index(owner)?;
                            let parameter_type = match self.store.functions[function].contract {
                                FullCallableContract::Ground(signature) => {
                                    let (_, ty, _) = self.store.semantic.signature_param(signature, parameter)?;
                                    Some(self.contract_type(ty)?)
                                }
                                FullCallableContract::Scheme(_) => self.scheme_parameter(owner, parameter)?,
                            };
                            if parameter_type.is_some_and(|ty| ty.contains_typed_callable()) { has_contract_source = true; }
                            let parameter_flags = bounded(self.store.functions[function].params, &self.store.params, "callable parameter range is invalid")?
                                .get(parameter).ok_or_else(|| IrVerifyError::new("callable parameter is out of bounds"))?.flags;
                            self.budget.charge(self.incoming.get(&function).map_or(0, Vec::len))?;
                            for caller in self.incoming.get(&function).cloned().unwrap_or_default() {
                                let caller_owner = self.index.instruction_owner(caller)?;
                                let caller_decoder = self.owner(caller_owner)?;
                                let values = call_values(&caller_decoder, caller)?;
                                if parameter_flags & 1 != 0 {
                                    for argument in values.iter().skip(parameter) {
                                        let mut path = path.clone();
                                        match argument {
                                            ProducerArgument::Single(value) => {
                                                if !matches!(path.pop(), Some(FunctionProjection::Element | FunctionProjection::IterationItem)) { return Err(IrVerifyError::new("callable rest parameter has no item projection")); }
                                                pending.push((caller_owner, CallableProducer::Expression(*value), path));
                                            }
                                            ProducerArgument::Splice(value) => pending.push((caller_owner, CallableProducer::Expression(*value), path)),
                                            ProducerArgument::Default(_) => return Err(IrVerifyError::new("callable rest source contains a default argument")),
                                        }
                                    }
                                } else if let Some(ProducerArgument::Single(value)) = values.get(parameter) {
                                    pending.push((caller_owner, CallableProducer::Expression(*value), path.clone()));
                                }
                            }
                            for writer in self.index.owners[&owner].writers.get(&binding).cloned().unwrap_or_default() {
                                if let schema::StmtView::DefaultParameter { value, .. } = decoder.statement_view(writer)? { pending.push((owner, CallableProducer::Expression(value.raw() as usize), path.clone())); }
                            }
                        }
                        ProducerBinding::HeaderCapture(binding) => {
                            pending.push((owner, CallableProducer::Binding(binding), path));
                        }
                        ProducerBinding::DriverBinding(slot) => {
                            if let Some(source) = decoder.driver_local_let_source(slot, |amount| self.budget.charge(amount))? {
                                pending.push((driver_owner(source.step).map_err(|_| IrVerifyError::new("callable driver initializer owner is invalid"))?, CallableProducer::Expression(source.initializer), path));
                            } else {
                                let step = super::super::driver_owner_index(owner).ok_or_else(|| IrVerifyError::new("callable driver owner is invalid"))?;
                                let slots = bounded(self.store.driver_steps[step].slots, &self.store.driver_slots, "callable driver binding range is invalid")?;
                                let binding = slots.iter().find(|binding| binding.slot as usize == slot).ok_or_else(|| IrVerifyError::new("callable driver slot has no lexical identity"))?;
                                pending.push((owner, CallableProducer::Binding(binding.binding_identity), path));
                            }
                        }
                    }
                }
                schema::ExprView::Try { value, .. } => {
                    if purpose == ProducerTargetPurpose::Stream { path.push(FunctionProjection::ResultPayload(true)); }
                    pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                }
                schema::ExprView::Require { value, .. } | schema::ExprView::CheckedValue { value, .. } => pending.push((owner, CallableProducer::Expression(value.raw() as usize), path)),
                schema::ExprView::Ok(value) => {
                    if path.last() == Some(&FunctionProjection::ResultPayload(false)) { continue; }
                    if path.last() == Some(&FunctionProjection::ResultPayload(true)) { path.pop(); }
                    pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                }
                schema::ExprView::Err { value, .. } => {
                    if path.pop() != Some(FunctionProjection::ResultPayload(false)) { continue; }
                    pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                }
                schema::ExprView::Tag { type_name, name, mut fields, .. } => {
                    let Some(FunctionProjection::TagField { type_name: selected_type, name: selected_name, field }) = path.pop() else { return Err(IrVerifyError::new("callable tag projection has no payload field")) };
                    if type_name != selected_type || Name::intern(name) != selected_name { continue; }
                    let value = fields.nth(field).ok_or_else(|| IrVerifyError::new("callable tag payload field is absent"))??;
                    pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                }
                schema::ExprView::Error(super::super::ErrorExprView::Structured { family_identity, variant, fields, .. }) => {
                    let Some(FunctionProjection::ErrorField { family, variant: selected_variant, field }) = path.pop() else { return Err(IrVerifyError::new("callable error projection has no payload field")) };
                    let actual_family = Name::intern(family_identity);
                    let actual_variant = Name::intern(variant);
                    let matches = actual_family == family && actual_variant == selected_variant;
                    let selected = self.structured_error_payload(owner, actual_family, actual_variant, fields.map(|field| field.map(|(name, value)| (Name::intern(name), value.raw() as usize))), matches.then_some(field))?;
                    if let Some(value) = selected { pending.push((owner, CallableProducer::Expression(value), path)); }
                }
                schema::ExprView::Field { base, name, .. } => {
                    path.push(FunctionProjection::Field(Name::intern(name)));
                    pending.push((owner, CallableProducer::Expression(base.raw() as usize), path));
                }
                schema::ExprView::Record(mut entries) if !path.is_empty() => {
                    let Some(FunctionProjection::Field(field)) = path.pop() else { return Err(IrVerifyError::new("callable record projection has no field")) };
                    let writers = decoder.checked_record_field_writers(&mut entries, |source| self.need_context(&decoder, source)?.ok_or_else(|| IrVerifyError::new("callable record spread has no source contract")))?;
                    match writers.get(&field) {
                        Some(super::super::CheckedRecordFieldWriter::Value(value)) => pending.push((owner, CallableProducer::Expression(*value), path)),
                        Some(super::super::CheckedRecordFieldWriter::Spread { instruction, .. }) => { path.push(FunctionProjection::Field(field)); pending.push((owner, CallableProducer::Expression(*instruction), path)); }
                        None => return Err(IrVerifyError::new("callable record producer omits its projected field")),
                    }
                }
                schema::ExprView::RecordUpdate { base, mut updates, .. } => {
                    let mut selected = None;
                    for update in &mut updates {
                        let (fields, value, _) = update?;
                        let fields = fields.collect::<Result<Vec<_>, _>>()?;
                        self.budget.charge(1 + fields.len())?;
                        if fields.iter().zip(path.iter().rev()).all(|(field, step)| *step == FunctionProjection::Field(*field)) && fields.len() <= path.len() {
                            selected = Some((fields.len(), value.raw() as usize));
                        }
                    }
                    if let Some((length, value)) = selected {
                        path.truncate(path.len() - length);
                        pending.push((owner, CallableProducer::Expression(value), path));
                    } else { pending.push((owner, CallableProducer::Expression(base.raw() as usize), path)); }
                }
                schema::ExprView::Slice { base, .. } => pending.push((owner, CallableProducer::Expression(base.raw() as usize), path)),
                schema::ExprView::Method { receiver, name, args, .. } => {
                    let source = self.need_context(&decoder, receiver.raw() as usize)?.ok_or_else(|| IrVerifyError::new("callable collection method has no receiver contract"))?;
                    let (domain, _) = FullDecoder::registered_method_receiver(&source, name).ok_or_else(|| IrVerifyError::new("callable collection method has no registry receiver"))?;
                    if path.last() == Some(&FunctionProjection::ResultPayload(false)) { continue; }
                    let operation = crate::modules::api_spec().method_op(domain, name);
                    if operation == Some(super::super::RuntimeOp::ResultContext) {
                        pending.push((owner, CallableProducer::Expression(receiver.raw() as usize), path));
                        continue;
                    }
                    let host_stream = purpose == ProducerTargetPurpose::Stream && path.as_slice() == [FunctionProjection::ResultPayload(true)];
                    if path.last() == Some(&FunctionProjection::ResultPayload(true)) { path.pop(); }
                    match operation {
                        Some(super::super::RuntimeOp::MapGet | super::super::RuntimeOp::ListGet) => path.push(FunctionProjection::Element),
                        Some(super::super::RuntimeOp::RecordGet) => {
                            let argument = args.into_iter().next().ok_or_else(|| IrVerifyError::new("callable record get has no field argument"))??;
                            let field = decoder.checked_literal_string(argument.raw() as usize)?.ok_or_else(|| IrVerifyError::new("callable record get has no literal field"))?;
                            path.push(FunctionProjection::Field(Name::intern(&field)));
                        }
                        _ if host_stream => {
                            self.verify_host_stream_projection(&decoder, instruction, &[FunctionProjection::ResultPayload(true)])?;
                            has_contract_source = true;
                            continue;
                        }
                        _ => return Err(IrVerifyError::new("callable registered method has no value projection")),
                    }
                    pending.push((owner, CallableProducer::Expression(receiver.raw() as usize), path));
                }
                schema::ExprView::Index { base, index, .. } => {
                    let source = self.need_context(&decoder, base.raw() as usize)?.ok_or_else(|| IrVerifyError::new("callable index has no receiver contract"))?;
                    let field = if let Type::Module(module) = source.unvalidated()
                        && module.exact
                    {
                        let key = decoder.checked_literal_string(index.raw() as usize)?.ok_or_else(|| IrVerifyError::new("callable module index has no literal key"))?;
                        let field = Name::intern(&key);
                        matches!(module.exports.get(&field), Some(ModuleExportType::GenericCallable { optional: false, .. })).then_some(field)
                    } else { None };
                    path.push(field.map_or(FunctionProjection::Element, FunctionProjection::Field));
                    pending.push((owner, CallableProducer::Expression(base.raw() as usize), path));
                }
                schema::ExprView::IndexFromEnd { base, .. } => {
                    path.push(FunctionProjection::Element);
                    pending.push((owner, CallableProducer::Expression(base.raw() as usize), path));
                }
                schema::ExprView::MapLiteral(mut entries) => {
                    let projection = path.pop();
                    if projection == Some(FunctionProjection::IterationItem) {
                        if path.pop() != Some(FunctionProjection::Field(Name::intern("value"))) { return Err(IrVerifyError::new("callable map iteration has no value field")); }
                    } else if projection != Some(FunctionProjection::Element) { return Err(IrVerifyError::new("callable map projection has no value")); }
                    for entry in &mut entries {
                        let (key, value, _) = entry?;
                        let mut path = path.clone();
                        if key.is_none() { path.push(FunctionProjection::Element); }
                        pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                    }
                }
                schema::ExprView::ListComp { value, .. } => {
                    if !matches!(path.pop(), Some(FunctionProjection::Element | FunctionProjection::IterationItem)) { return Err(IrVerifyError::new("callable list comprehension projection has no value")); }
                    pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                }
                schema::ExprView::MapComp { value, .. } => {
                    let projection = path.pop();
                    if projection == Some(FunctionProjection::IterationItem) {
                        if path.pop() != Some(FunctionProjection::Field(Name::intern("value"))) { return Err(IrVerifyError::new("callable map comprehension iteration has no value field")); }
                    } else if projection != Some(FunctionProjection::Element) { return Err(IrVerifyError::new("callable map comprehension projection has no value")); }
                    pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                }
                schema::ExprView::List(mut values) => {
                    if !matches!(path.pop(), Some(FunctionProjection::Element | FunctionProjection::IterationItem)) { return Err(IrVerifyError::new("callable list projection has no element")); }
                    for value in &mut values { pending.push((owner, CallableProducer::Expression(value?.raw() as usize), path.clone())); }
                }
                schema::ExprView::ListBuild(mut values) => {
                    if !matches!(path.pop(), Some(FunctionProjection::Element | FunctionProjection::IterationItem)) { return Err(IrVerifyError::new("callable list-build projection has no element")); }
                    for value in &mut values {
                        let (splice, value, _) = value?;
                        let mut path = path.clone();
                        if splice { path.push(FunctionProjection::Element); }
                        pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                    }
                }
                schema::ExprView::ListPipeline { .. } => {
                    let len = self.index.owners[&owner].pipeline_stages[&instruction].len();
                    let stage = self.pipeline_stage(&decoder, instruction, len - 1)?;
                    if let super::super::StageView::Fold { initial, value, .. } = stage {
                        pending.push((owner, CallableProducer::Expression(initial.raw() as usize), path.clone()));
                        if super::super::indexed_stage_callback_can_complete(&decoder, &stage)? { pending.push((owner, CallableProducer::Expression(value.raw() as usize), path)); }
                    } else {
                        if !matches!(path.pop(), Some(FunctionProjection::Element | FunctionProjection::IterationItem)) { return Err(IrVerifyError::new("callable pipeline projection has no element")); }
                        pending.push((owner, CallableProducer::PipelinePrefix(instruction, if matches!(stage, super::super::StageView::Collect) { len - 1 } else { len }), path));
                    }
                }
                schema::ExprView::Loop { .. } => {
                    for value in self.index.owners[&owner].return_analysis.loop_values.as_ref().and_then(|values| values.get(&instruction)).into_iter().flatten() {
                        pending.push((owner, CallableProducer::Expression(*value), path.clone()));
                    }
                }
                schema::ExprView::ResultFallback { left, right } => {
                    let mut success = path.clone();
                    if self.need_context(&decoder, left.raw() as usize)?.is_some_and(|ty| ty.is_result()) { success.push(FunctionProjection::ResultPayload(true)); }
                    pending.push((owner, CallableProducer::Expression(left.raw() as usize), success));
                    pending.push((owner, CallableProducer::Expression(right.raw() as usize), path));
                }
                schema::ExprView::Null => {}
                schema::ExprView::MatchExpr { mut arms, .. } => { for arm in &mut arms { pending.push((owner, CallableProducer::Expression(arm?.2.raw() as usize), path.clone())); } }
                schema::ExprView::PatternIf { mut branches, else_value, .. } => {
                    for branch in &mut branches { pending.push((owner, CallableProducer::Expression(branch?.1.raw() as usize), path.clone())); }
                    pending.push((owner, CallableProducer::Expression(else_value.raw() as usize), path));
                }
                schema::ExprView::Capture { mut body, .. } if purpose == ProducerTargetPurpose::Stream => {
                    if path.pop() != Some(FunctionProjection::ResultPayload(true)) { return Err(IrVerifyError::new("captured Stream producer has no Result success projection")); }
                    let block = body.block_identity();
                    match decoder.derive_checked_block_tail_expression(&mut body)? {
                        Some(value) => pending.push((owner, CallableProducer::Expression(value), path)),
                        None if super::super::indexed_block_can_return_with_decoder(self.store, block, Some(&decoder))? => {}
                        None => return Err(IrVerifyError::new("captured Stream producer completes without a value tail")),
                    }
                }
                schema::ExprView::ErrorContext { mut body, .. } if purpose == ProducerTargetPurpose::Stream => {
                    let block = body.block_identity();
                    match decoder.derive_checked_block_tail_expression(&mut body)? {
                        Some(value) => pending.push((owner, CallableProducer::Expression(value), path)),
                        None if super::super::indexed_block_can_return_with_decoder(self.store, block, Some(&decoder))? => {}
                        None => return Err(IrVerifyError::new("context Stream producer completes without a value tail")),
                    }
                }
                schema::ExprView::ContextScope { mut body, .. } => {
                    if path.last() == Some(&FunctionProjection::ResultPayload(false)) { continue; }
                    if path.last() == Some(&FunctionProjection::ResultPayload(true)) { path.pop(); }
                    let actual = self.need_context(&decoder, instruction)?.ok_or_else(|| IrVerifyError::new("callable context scope has no producer contract"))?;
                    self.expression(owner, instruction, actual, false);
                    pending.push((owner, CallableProducer::Expression(decoder.checked_block_tail_expression(&mut body)?), path));
                }
                schema::ExprView::ValueBlock { mut body, .. } => pending.push((owner, CallableProducer::Expression(decoder.checked_block_tail_expression(&mut body)?), path)),
                schema::ExprView::IfExpr { mut branches, else_value, .. } => {
                    for branch in &mut branches { pending.push((owner, CallableProducer::Expression(branch?.1.raw() as usize), path.clone())); }
                    pending.push((owner, CallableProducer::Expression(else_value.raw() as usize), path));
                }
                schema::ExprView::Call { function, .. } | schema::ExprView::DirectPureCall { function, .. } => {
                    let target = function_owner(function.id.index())?;
                    for returned in self.index.owners[&target].header_returns().copied().collect::<Vec<_>>() {
                        let target_decoder = self.owner(target)?;
                        let schema::StmtView::Return { value } = target_decoder.statement_view(returned)? else { unreachable!("callable function return index contains a non-return") };
                        pending.push((target, CallableProducer::Expression(value.raw() as usize), path.clone()));
                    }
                }
                _ => return Err(IrVerifyError::new(format!("ground callable has no resolved lexical function identity for {:?}", self.store.tags[instruction]))),
            }
        }
        if purpose == ProducerTargetPurpose::Callable && targets.is_empty() && !has_contract_source { return Err(IrVerifyError::new("ground callable has no executable function target")); }
        Ok(ProducerTargets { functions: targets, opaque: has_contract_source, parameters })
    }

    fn call_arguments(&mut self, decoder: &FullDecoder<'a>, function: usize, arguments: &[ProducerArgument]) -> Result<(), IrVerifyError> {
        if super::super::driver_owner_index(decoder.owner).is_none() && matches!(self.store.functions[function_index(decoder.owner)?].contract, FullCallableContract::Scheme(_)) { return Ok(()); }
        let FullCallableContract::Ground(signature) = self.store.functions[function].contract else { return Ok(()) };
        let count = self.store.semantic.signature_param_count(signature)?;
        let store = self.store;
        self.call_arguments_with(decoder, arguments, count,
            |position| store.semantic.signature_param(signature, position).map(|(_, _, flags)| flags),
            |proof, position| proof.contract_type(store.semantic.signature_param(signature, position)?.1))
    }

    fn call_arguments_with(&mut self, decoder: &FullDecoder<'a>, arguments: &[ProducerArgument], count: usize,
        flags_at: impl Fn(usize) -> Result<u32, IrVerifyError>,
        mut type_at: impl FnMut(&mut Self, usize) -> Result<Arc<Type>, IrVerifyError>) -> Result<(), IrVerifyError> {
        let mut position = 0;
        for &argument in arguments {
            if position >= count { return Err(IrVerifyError::new("ground producer call has excess arguments")); }
            let flags = flags_at(position)?;
            let expected = type_at(self, position)?;
            let rest = flags & 2 != 0;
            match argument {
                ProducerArgument::Default(slot) => {
                    if slot != position || flags & 1 == 0 || rest { return Err(IrVerifyError::new("ground producer default argument has no parameter default")); }
                }
                ProducerArgument::Single(instruction) => {
                    let expected = if rest {
                        let Type::List(item) = expected.as_ref() else { return Err(IrVerifyError::new("ground rest parameter has no list contract")) };
                        Arc::new((**item).clone())
                    } else { expected };
                    self.contextual_argument(decoder, instruction, &expected)?;
                    self.expression(decoder.owner, instruction, expected, false);
                }
                ProducerArgument::Splice(instruction) => {
                    if !rest { return Err(IrVerifyError::new("ground producer splice has no rest parameter")); }
                    self.contextual_argument(decoder, instruction, &expected)?;
                    self.expression(decoder.owner, instruction, expected, false);
                }
            }
            if !rest { position += 1; }
        }
        for index in position..count {
            let flags = flags_at(index)?;
            if flags & 3 == 0 { return Err(IrVerifyError::new("ground producer call omits a required argument")); }
        }
        Ok(())
    }

    fn verify_stream_yields(&mut self, function: usize, decoder: &FullDecoder<'a>, result: Arc<Type>) -> Result<(), IrVerifyError> {
        let Type::Stream(item) = result.as_ref() else { return Err(IrVerifyError::new("stream producer header has no item contract")); };
        self.budget.charge_clone(item)?;
        let item = Arc::new((**item).clone());
        let guarded = item.has_unsigned_constraint() && super::runtime_complete_value_check_type(&item, &mut self.budget, 0)?;
        let owner = function_owner(function)?;
        self.budget.charge(self.index.owners[&owner].yield_sites.len())?;
        let sites = self.index.owners[&owner].yield_sites.clone();
        for instruction in sites {
            self.budget.charge(1)?;
            match decoder.statement_view(instruction)? {
                schema::StmtView::Yield { value } => {
                    let value = value.raw() as usize;
                    if guarded && let Some(actual) = self.declared_type(decoder, value)? {
                        if !actual.matches_expected(&item) { return Err(IrVerifyError::new("stream yield item disagrees with its checked UInt boundary")); }
                        self.expression(owner, value, actual, false);
                    } else { self.expression(owner, value, Arc::clone(&item), false); }
                }
                schema::StmtView::YieldDelegate { value, .. } => {
                    let value = value.raw() as usize;
                    let actual = self.declared_type(decoder, value)?;
                    let stream = match actual.as_deref().map(Type::unvalidated) {
                        Some(Type::List(_)) => false,
                        Some(Type::Stream(_)) => true,
                        None if matches!(decoder.expression_view(value)?, schema::ExprView::List(ref values) if values.len() == 0) => false,
                        _ => return Err(IrVerifyError::new("stream delegation has no actual List or Stream contract")),
                    };
                    if stream && self.stream_may_propagate_error(decoder, StreamProducer::Value(value))? {
                        self.require_callable_effects(decoder, instruction, Some(&[crate::syntax::node::Effect::Error]), false)?;
                    }
                    if guarded && let Some(actual) = actual {
                        let actual_item = match actual.unvalidated() { Type::List(item) | Type::Stream(item) => item, _ => unreachable!() };
                        if !actual_item.matches_expected(&item) { return Err(IrVerifyError::new("stream delegated item disagrees with its checked UInt boundary")); }
                        self.expression(owner, value, actual, false);
                    } else {
                        self.budget.charge_clone(&item)?;
                        self.budget.charge(1)?;
                        let expected = if stream { Type::Stream(Box::new((*item).clone())) } else { Type::List(Box::new((*item).clone())) };
                        self.expression(owner, value, Arc::new(expected), false);
                    }
                }
                _ => return Err(IrVerifyError::new("stream yield census names another statement")),
            }
        }
        self.budget.charge(self.index.owners[&owner].returns.len())?;
        let returns = self.index.owners[&owner].returns.clone();
        for returned in returns {
            self.budget.charge(1)?;
            let schema::StmtView::Return { value } = decoder.statement_view(returned)? else { return Err(IrVerifyError::new("stream completion census names another statement")); };
            self.expression(owner, value.raw() as usize, Arc::new(Type::Unit), false);
        }
        Ok(())
    }

    fn verify_literal_payload(&mut self, owner: u32, instruction: usize) -> Result<(), IrVerifyError> {
        let decoder = self.owner(owner)?;
        if !decoder.instruction_range.contains(&instruction) { return Err(IrVerifyError::new("literal capture payload escapes its lexical owner")); }
        match decoder.expression_view(instruction)? {
            schema::ExprView::Null | schema::ExprView::Unit | schema::ExprView::Bool(_)
            | schema::ExprView::Int(_) | schema::ExprView::UInt(_) | schema::ExprView::Float(_)
            | schema::ExprView::Str(_) | schema::ExprView::Bytes(_) | schema::ExprView::Duration(_)
            | schema::ExprView::PreparedRegex(_) | schema::ExprView::Path(_) | schema::ExprView::EmptyMap => {}
            schema::ExprView::Record(mut entries) => {
                for entry in &mut entries {
                    self.budget.charge(1)?;
                    let value = match entry? {
                        super::super::decode::RecordEntryView::Field(_, value) | super::super::decode::RecordEntryView::Spread(value) => value,
                    };
                    self.pending.push(Obligation::LiteralPayload(owner, value.raw() as usize));
                }
            }
            schema::ExprView::List(mut values) => {
                for value in &mut values {
                    self.budget.charge(1)?;
                    self.pending.push(Obligation::LiteralPayload(owner, value?.raw() as usize));
                }
            }
            schema::ExprView::ListBuild(mut values) => {
                for value in &mut values {
                    self.budget.charge(1)?;
                    let (_, value, _) = value?;
                    self.pending.push(Obligation::LiteralPayload(owner, value.raw() as usize));
                }
            }
            schema::ExprView::MapLiteral(mut entries) => {
                for entry in &mut entries {
                    self.budget.charge(1)?;
                    let (key, value, _) = entry?;
                    if let Some(key) = key { self.pending.push(Obligation::LiteralPayload(owner, key.raw() as usize)); }
                    self.pending.push(Obligation::LiteralPayload(owner, value.raw() as usize));
                }
            }
            schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::Add | crate::syntax::node::BinaryOp::Sub
                | crate::syntax::node::BinaryOp::Mul | crate::syntax::node::BinaryOp::Div | crate::syntax::node::BinaryOp::Rem
                | crate::syntax::node::BinaryOp::Eq | crate::syntax::node::BinaryOp::Ne | crate::syntax::node::BinaryOp::Lt
                | crate::syntax::node::BinaryOp::Le | crate::syntax::node::BinaryOp::Gt | crate::syntax::node::BinaryOp::Ge
                | crate::syntax::node::BinaryOp::And | crate::syntax::node::BinaryOp::Or
                | crate::syntax::node::BinaryOp::In | crate::syntax::node::BinaryOp::NotIn, left, right, .. } => {
                self.pending.push(Obligation::LiteralPayload(owner, left.raw() as usize));
                self.pending.push(Obligation::LiteralPayload(owner, right.raw() as usize));
            }
            schema::ExprView::Field { base, .. } => self.pending.push(Obligation::LiteralPayload(owner, base.raw() as usize)),
            schema::ExprView::IfExpr { mut branches, else_value, .. } => {
                self.budget.charge(branches.len() + 2)?;
                let Some((condition, value)) = branches.next().transpose()? else { return Err(IrVerifyError::new("generic record capture has a nonliteral payload")); };
                if branches.next().is_some()
                    || !matches!(decoder.expression_view(value.raw() as usize)?, schema::ExprView::Bool(false))
                    || !matches!(decoder.expression_view(else_value.raw() as usize)?, schema::ExprView::Bool(true))
                { return Err(IrVerifyError::new("generic record capture has a nonliteral payload")); }
                self.verify_condition(owner, condition.raw() as usize, ConditionRole::Boolean)?;
                self.pending.push(Obligation::LiteralPayload(owner, condition.raw() as usize));
                self.pending.push(Obligation::LiteralPayload(owner, value.raw() as usize));
                self.pending.push(Obligation::LiteralPayload(owner, else_value.raw() as usize));
            }
            schema::ExprView::PreparedConstant(value) => {
                let work = super::super::prepared_constant_data_work(&value.0, 0).ok_or_else(|| IrVerifyError::new("generic capture literal contains a runtime payload"))?;
                self.budget.charge(work)?;
            }
            _ => return Err(IrVerifyError::new("generic record capture has a nonliteral payload")),
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), IrVerifyError> {
        loop {
            while let Some(obligation) = self.pending.pop() {
                self.budget.charge(1)?;
                match obligation {
                    Obligation::GroundFunction(index) => {
                        let owner = function_owner(index)?;
                        let decoder = self.owner(owner)?;
                        let expected = self.contract_type(decoder.function_return_type(index)?)?;
                        if self.store.function_metadata[index].callable_kind == CallableKind::Stream {
                            self.verify_stream_yields(index, &decoder, expected)?;
                            continue;
                        }
                        for returned in self.index.owners[&owner].header_returns().copied().collect::<Vec<_>>() {
                            let schema::StmtView::Return { value } = decoder.statement_view(returned)? else { unreachable!("ground producer return index contains a non-return") };
                            self.expression(owner, value.raw() as usize, Arc::clone(&expected), true);
                        }
                    }
                    Obligation::LiteralPayload(owner, instruction) => self.verify_literal_payload(owner, instruction)?,
                    Obligation::Expression(owner, instruction, expected, returned, fact) => self.verify_expression(owner, instruction, expected, returned, fact)?,
                    Obligation::Slot(owner, binding, slot, expected) => self.verify_slot(owner, binding, slot, expected)?,
                    Obligation::PipelinePrefix(owner, pipeline, end, expected) => self.verify_pipeline_prefix(owner, pipeline, end, expected)?,
                    Obligation::Storage(binding, expected, relation) => self.verify_storage(binding, expected, relation)?,
                }
            }
            if self.deferred_sources.is_empty() { break; }
            for ((owner, binding), source) in std::mem::take(&mut self.deferred_sources) {
                self.budget.charge(1)?;
                let (actual_source, expected) = self.contextual_sources.get(&(owner, binding)).cloned()
                    .ok_or_else(|| IrVerifyError::new("contextual aggregate subject has no authenticated consumer contract"))?;
                if actual_source != source { return Err(IrVerifyError::new("contextual aggregate subject has a different source binding")); }
                for required in self.contextual_reads.remove(&(owner, binding)).into_iter().flatten() {
                    self.budget.charge_clone(&required)?;
                    if !expected.matches_expected(&required)
                        || !super::initializer_type_matches_annotation(&expected, &required, &mut self.budget, 0)?
                    { return Err(IrVerifyError::new("contextual aggregate read disagrees with its consumer contract")); }
                }
                let decoder = self.owner(owner)?;
                if let Some(context) = self.need_context(&decoder, source)? && context != expected {
                    return Err(IrVerifyError::new("contextual aggregate hint disagrees with its consumer contract"));
                }
                self.expression(owner, source, expected, false);
            }
        }
        Ok(())
    }

    fn optional_read_is_narrowed(&mut self, decoder: &FullDecoder<'a>, instruction: usize, actual: &Type) -> Result<bool, IrVerifyError> {
        let Type::Optional(payload) = actual.unvalidated() else { return Ok(false) };
        let Some(target) = decoder.checked_binding_path(instruction, &mut |amount| self.budget.charge(amount))? else { return Ok(false) };
        let binding = self.binding(decoder.owner, instruction, target.0)?;
        if target.1.is_empty()
            && self.index.owners[&decoder.owner].optional_binding_read_is_narrowed(decoder, instruction, target.0, binding, &mut |amount| self.budget.charge(amount))?
        { return Ok(true); }
        self.index.owners[&decoder.owner].optional_path_read_is_narrowed(decoder, instruction, &target, payload, binding, &mut |amount| self.budget.charge(amount))
    }

    fn is_conditional_record_source(&mut self, decoder: &FullDecoder<'a>, mut instruction: usize) -> Result<bool, IrVerifyError> {
        for _ in decoder.instruction_range.clone() {
            self.budget.charge(1)?;
            if !decoder.instruction_range.contains(&instruction) { return Err(IrVerifyError::new("conditional record source belongs to another owner")); }
            match decoder.expression_view(instruction)? {
                schema::ExprView::IfExpr { .. } | schema::ExprView::PatternIf { .. } | schema::ExprView::MatchExpr { .. }
                | schema::ExprView::StrMatchExpr { .. } | schema::ExprView::TagMatchExpr { .. } => return Ok(true),
                schema::ExprView::ValueBlock { mut body, .. } => {
                    self.budget.charge(body.len())?;
                    let Some(tail) = decoder.derive_checked_block_tail_expression(&mut body)? else { return Ok(false) };
                    instruction = tail;
                }
                _ => return Ok(false),
            }
        }
        Err(IrVerifyError::new("conditional record source is cyclic"))
    }

    fn verify_expression(&mut self, owner: u32, instruction: usize, expected: Arc<Type>, returned: bool, fact: Option<DeclaredProducerFact>) -> Result<(), IrVerifyError> {
        let owned_declared_fact = fact.is_some();
        let (declared, declared_binding) = match fact {
            Some(fact) => { let (ty, binding) = fact.into_evidence(owner, instruction)?; (Some(ty), binding) }
            None => (None, None),
        };
        let decoder = self.owner(owner)?;
        if let Some(binding) = declared_binding
            && !matches!(decoder.expression_view(instruction)?, schema::ExprView::Param(slot) if slot == binding.slot)
        { return Err(IrVerifyError::new("declared producer binding belongs to another slot")); }
        self.budget.charge(1)?;
        let expected = if expected.as_ref() == &Type::Any && self.store.tags[instruction] == FullTag::ExprCapture {
            match self.need_context(&decoder, instruction)? {
                Some(contract) => contract,
                None => {
                    let errors = self.captured_error_types(&decoder, instruction)?;
                    Arc::new(Type::Result(Box::new(Type::Unit), Box::new(self.join_captured_errors(&errors)?)))
                }
            }
        } else { expected };
        if *expected == Type::Any && !super::super::verify_effects::needs_operand_proof(self.store, instruction)? { return Ok(()); }
        if let schema::ExprView::Abort { status, .. } = decoder.expression_view(instruction)? {
            self.expression(owner, status.raw() as usize, Arc::new(Type::Int), false);
            return Ok(());
        }
        if let schema::ExprView::Param(slot) = decoder.expression_view(instruction)?
            && let Type::Callable(callable) = expected.as_ref()
            && self.store.call_instantiations.binary_search_by_key(&(instruction as u32), |instance| instance.instruction).is_ok()
        {
            decoder.function_reference_matches_callable(instruction, &expected)?;
            self.slot(owner, instruction, slot, Arc::new(Type::Function(callable.kind)))?;
            return Ok(());
        }
        if let schema::ExprView::FunctionRef { function, kind } = decoder.expression_view(instruction)?
            && let Type::Callable(callable) = expected.as_ref()
        {
            let (target, contract, selected_kind) = decoder.function_reference_contract(function)?;
            let fits = match contract {
                super::super::FullCallableContract::Ground(signature) => self.store.semantic.signature_fits_callable(signature, kind, callable)?,
                super::super::FullCallableContract::Scheme(_) => { decoder.function_reference_matches_callable(instruction, &expected)?; true },
            };
            if kind != selected_kind || kind != callable.kind || !fits
            { return Err(IrVerifyError::new("ground callable producer disagrees with its executable signature")); }
            if matches!(contract, super::super::FullCallableContract::Ground(_)) { self.ground_function(target.index())?; }
            return Ok(());
        }
        if let Type::SchemeCallable(id) = expected.as_ref() {
            let relationship = self.store.semantic.scheme_callable_index(*id)?;
            let relationship = self.store.scheme_callable_values.get(relationship as usize).ok_or_else(|| IrVerifyError::new("scheme callable producer relationship is out of bounds"))?;
            let range = relationship.targets.bounds(self.store.scheme_callable_targets.len()).ok_or_else(|| IrVerifyError::new("scheme callable producer target range is invalid"))?;
            self.budget.charge(range.len())?;
            let expected = self.store.scheme_callable_targets[range].iter().map(|target| target.function.index()).collect::<BTreeSet<_>>();
            let actual = self.producer_targets(owner, instruction, ProducerTargetPurpose::Callable, Vec::new())?;
            if actual.opaque || !actual.parameters.is_empty() || actual.functions != expected { return Err(IrVerifyError::new("scheme callable relationship disagrees with its exact physical producers")); }
            return Ok(());
        }
        let actual = match declared {
            Some(actual) => Some(actual),
            None => self.declared_type(&decoder, instruction)?,
        };
        if matches!(expected.as_ref(), Type::List(_) | Type::Map(_, _))
            && let schema::ExprView::Param(slot) = decoder.expression_view(instruction)?
            && let Some(source) = self.fresh_contextual_argument_source(&decoder, instruction)?
            && self.declared_type(&decoder, source)?.is_none()
        {
            let binding = self.binding(owner, instruction, slot)?;
            self.deferred_sources.insert((owner, binding), source);
            self.budget.charge_clone(&expected)?;
            self.contextual_reads.entry((owner, binding)).or_default().push(Arc::clone(&expected));
            return Ok(());
        }
        let contextual_list_source = if matches!(expected.as_ref(), Type::List(_))
            && actual.as_ref().is_none_or(|actual| !actual.matches_expected(&expected))
        { self.fresh_contextual_list_source(&decoder, instruction)? } else { None };
        if let Some(actual) = &actual {
            let fits = if returned { crate::sema::types::tail_type_matches_expected(&expected, actual) } else { actual.matches_expected(&expected) };
            let constructor_payload_contract = owned_declared_fact && matches!(
                (decoder.expression_view(instruction)?, actual.as_ref(), expected.as_ref()),
                (schema::ExprView::Ok(_), Type::Result(actual, _), Type::Result(expected, _)) if actual.matches_expected(expected)
            ) || owned_declared_fact && matches!(
                (decoder.expression_view(instruction)?, actual.as_ref(), expected.as_ref()),
                (schema::ExprView::Err { .. }, Type::Result(_, actual), Type::Result(_, expected)) if actual.matches_expected(expected)
            );
            let validated_path_literal = !fits
                && actual.as_ref() == &Type::Path
                && matches!(expected.as_ref(), Type::Validated(validated) if validated.validation() == crate::sema::validated::Validation::RelPath)
                && self.store.tags[instruction] == FullTag::ExprPath
                && matches!(decoder.expression_view(instruction)?, schema::ExprView::Path(path) if crate::sema::validated::is_rel_path(path));
            let container = if returned && !actual.is_result() { expected.result_ok().unwrap_or(expected.as_ref()) } else { expected.as_ref() };
            let literal_list = matches!(decoder.expression_view(instruction)?, schema::ExprView::List(_) | schema::ExprView::ListBuild(_))
                && expected_list_item(container, true).is_some();
            let fresh_comprehension = self.store.tags[instruction] == FullTag::ExprListComp
                && matches!((actual.unvalidated(), container.unvalidated()), (Type::List(_), Type::List(_)));
            let literal_record = self.store.tags[instruction] == FullTag::ExprRecord
                && matches!(actual.as_ref(), Type::Record(_) | Type::ErasedRecord) && matches!(container, Type::Record(_));
            let record_branches = actual.as_ref() == &Type::ErasedRecord && matches!(container, Type::Record(_))
                && self.is_conditional_record_source(&decoder, instruction)?;
            let checked_comprehension = if matches!((self.store.tags[instruction], container),
                (FullTag::ExprListComp, Type::List(_)) | (FullTag::ExprMapComp, Type::Map(_, _)))
                && let Some(parent) = decoder.instruction_parent_of(instruction)?
                && self.store.tags[parent].kind() == schema::InstructionKind::Expr
                && let schema::ExprView::CheckedValue { check: ValueCheckView::Ground(check), value, .. } = decoder.expression_view(parent)?
                && value.raw() as usize == instruction && check.schema.is_none()
            { self.contract_type(check.ty.id)?.as_ref() == container } else { false };
            let narrowed_read = if !fits && matches!(decoder.expression_view(instruction)?, schema::ExprView::Param(_) | schema::ExprView::Field { .. }) {
                if decoder.checked_guarded_record_read_matches_type(instruction, actual, container, &mut |amount| self.budget.charge(amount))? {
                    true
                } else { match actual.unvalidated() {
                    Type::Any | Type::Union(_) => self.declared_read_type(&decoder, instruction, Arc::clone(actual))?.as_ref() == container,
                    Type::Optional(payload) if payload.matches_expected(container)
                        && super::initializer_type_matches_annotation(payload, container, &mut self.budget, 0)? => self.optional_read_is_narrowed(&decoder, instruction, actual)?,
                    _ => false,
                } }
            } else { false };
            let runtime_checked_input = self.runtime_checked_inputs.contains(&(owner, instruction));
            let literal_map = matches!(decoder.expression_view(instruction)?, schema::ExprView::MapLiteral(_))
                && matches!((actual.unvalidated(), container.unvalidated()), (Type::Map(_, _), Type::Map(_, _)));
            if !fits && !constructor_payload_contract && !validated_path_literal && !runtime_checked_input && !narrowed_read && !literal_list && !fresh_comprehension && !literal_record && !literal_map && !record_branches && !checked_comprehension && contextual_list_source.is_none() && self.store.tags[instruction] != FullTag::ExprCapture {
                if self.store.tags[instruction] == FullTag::ExprListComp
                    && let Some(required) = container.validated()
                    && actual.unvalidated().matches_expected(required.base())
                {
                    return Err(IrVerifyError::new("ground list comprehension does not preserve its validation"));
                }
                return Err(IrVerifyError::new("ground producer value disagrees with its checked contract"));
            }
        }
        let preserves_result = matches!(decoder.expression_view(instruction)?, schema::ExprView::Fail { .. } | schema::ExprView::Ok(_) | schema::ExprView::Err { .. } | schema::ExprView::Capture { .. } | schema::ExprView::ContextScope { .. } | schema::ExprView::IfExpr { .. } | schema::ExprView::PatternIf { .. } | schema::ExprView::MatchExpr { .. } | schema::ExprView::StrMatchExpr { .. } | schema::ExprView::TagMatchExpr { .. } | schema::ExprView::ValueBlock { .. });
        let expected = if returned && !preserves_result && !actual.as_ref().is_some_and(|actual| actual.is_result()) {
            match expected.as_ref() {
                Type::Result(ok, _) => {
                    self.budget.charge_clone(ok)?;
                    Arc::new((**ok).clone())
                }
                _ => expected,
            }
        } else { expected };
        let expected = if let Type::Optional(payload) = expected.unvalidated()
            && (actual.as_ref().is_some_and(|actual| actual.unvalidated() != &Type::Null && !matches!(actual.unvalidated(), Type::Optional(_)) && actual.matches_expected(payload))
                || (matches!(expected.as_ref(), Type::Optional(_))
                    && matches!((decoder.expression_view(instruction)?, payload.as_ref()),
                    (schema::ExprView::Record(_), Type::Record(_))
                    | (schema::ExprView::List(_) | schema::ExprView::ListBuild(_), Type::List(_))
                    | (schema::ExprView::MapLiteral(_), Type::Map(_, _)))))
        {
            self.budget.charge_clone(payload)?;
            Arc::new((**payload).clone())
        } else { expected };
        match decoder.expression_view(instruction)? {
            schema::ExprView::Param(_) if contextual_list_source.is_some() => {
                self.expression(owner, contextual_list_source.unwrap(), expected, false);
                Ok(())
            }
            schema::ExprView::Param(slot) => {
                let binding = match declared_binding { Some(binding) => binding.binding, None => self.binding(owner, instruction, slot)? };
                if let Some((source, contract)) = self.contextual_sources.get(&(owner, binding)).cloned() {
                    if contract.as_ref() != expected.as_ref() { return Err(IrVerifyError::new("contextual aggregate read disagrees with its consumer contract")); }
                    self.expression(owner, source, contract, false);
                    Ok(())
                } else {
                    if matches!(binding, ProducerBinding::HeaderParameter(_))
                        && matches!(self.store.functions[function_index(owner)?].contract, FullCallableContract::Ground(_))
                        && matches!(expected.as_ref(), Type::Record(_))
                        && actual.as_deref() == Some(expected.as_ref())
                        && let Some(declared) = self.slot_context_type_for_binding(&decoder, instruction, slot, binding)?
                        && declared.as_ref() != expected.as_ref()
                        && let Some(checked_slot_type) = decoder.checked_slot_type(slot)?
                        && self.contract_type(checked_slot_type)?.as_ref() == declared.as_ref()
                    {
                        return self.binding_slot(owner, binding, slot, declared);
                    }
                    if matches!(expected.as_ref(), Type::Record(_))
                        && self.index.owners[&owner].writers.get(&binding).is_some_and(|writers| writers.len() > 1)
                        && let Some(stored) = self.slot_context_type_for_binding(&decoder, instruction, slot, binding)?
                        && stored.as_ref() != expected.as_ref()
                    {
                        if !decoder.checked_declared_type_matches_narrowed_fact_with(instruction, (slot, Vec::new()), &stored, &expected, &mut |amount| self.budget.charge(amount))? {
                            return Err(IrVerifyError::new("ground producer read has no authenticated narrowed storage view"));
                        }
                        return self.binding_slot(owner, binding, slot, stored);
                    }
                    self.binding_slot(owner, binding, slot, actual.unwrap_or(expected))
                }
            },
            schema::ExprView::Glob { .. } | schema::ExprView::LastStatus { .. } => Ok(()),
            schema::ExprView::Range { start, end, .. } => {
                if !Type::Stream(Box::new(Type::Int)).matches_expected(&expected) { return Err(IrVerifyError::new("range producer has an incompatible stream contract")); }
                self.expression(owner, start.raw() as usize, Arc::new(Type::Int), false);
                self.expression(owner, end.raw() as usize, Arc::new(Type::Int), false);
                Ok(())
            }
            schema::ExprView::Call { .. } | schema::ExprView::DirectPureCall { .. } | schema::ExprView::SelfCall { .. } => {
                let (function, arguments) = call_parts(&decoder, instruction)?;
                match self.store.functions[function].contract {
                    FullCallableContract::Ground(_) => { self.ground_function(function)?; self.call_arguments(&decoder, function, &arguments)?; }
                    FullCallableContract::Scheme(_) => {
                        for argument in arguments {
                            let (ProducerArgument::Single(argument) | ProducerArgument::Splice(argument)) = argument else { continue };
                            let ty = self.need_context(&decoder, argument)?.ok_or_else(|| IrVerifyError::new("generic argument producer has no ground type"))?;
                            self.expression(owner, argument, ty, false);
                        }
                    }
                }
                Ok(())
            }
            schema::ExprView::ProcessCommandArgv(command) => self.verify_process_command_argv(&decoder, instruction, command, &expected),
            schema::ExprView::ModuleCall { op, call_plan, args, .. } => {
                let arguments = args.map(|argument| argument.map(|value| value.map(|value| value.raw() as usize))).collect::<Result<Vec<_>, _>>()?;
                self.verify_native_call(&decoder, instruction, op, call_plan.map(AsRef::as_ref), arguments, &expected)
            }
            schema::ExprView::FsFiles { root, gitignore, stat, hidden, exts, result_wrapped, .. }
            | schema::ExprView::FsWalk { root, gitignore, stat, hidden, exts, result_wrapped, .. } => {
                let op = if self.store.tags[instruction] == FullTag::ExprFsFiles { super::super::RuntimeOp::FsFiles } else { super::super::RuntimeOp::FsWalk };
                let result = FullDecoder::registered_api_operation_return_type(op)?;
                let physical = if result_wrapped { &result } else { result.result_ok().ok_or_else(|| IrVerifyError::new("filesystem stream contract has no Result payload"))? };
                if !physical.matches_expected(&expected) { return Err(IrVerifyError::new("filesystem stream producer disagrees with its result contract")); }
                let arguments = if op == super::super::RuntimeOp::FsFiles { vec![Some(root), gitignore, stat, exts, hidden] }
                else {
                    if exts.is_some() { return Err(IrVerifyError::new("filesystem walk has an unregistered extension option")); }
                    vec![Some(root), gitignore, stat, hidden]
                }.into_iter().map(|value| value.map(|value| value.raw() as usize)).collect();
                self.verify_native_call(&decoder, instruction, op, None, arguments, &result)
            }
            schema::ExprView::TypedCall { callee, signature, kind, .. } => {
                let targets = self.function_targets(owner, callee.raw() as usize)?;
                let contract = self.contract_type(signature.id)?;
                let Type::Callable(callable) = contract.as_ref() else { return Err(IrVerifyError::new("typed producer call has no callable contract")) };
                if callable.kind != kind { return Err(IrVerifyError::new("typed producer call kind disagrees with its contract")); }
                if !callable.sig.return_ty.matches_expected(&expected) { return Err(IrVerifyError::new("typed producer call return disagrees with its checked contract")); }
                self.expression(owner, callee.raw() as usize, Arc::clone(&contract), false);
                let arguments = call_values(&decoder, instruction)?;
                self.call_arguments_with(&decoder, &arguments, callable.sig.params.len(),
                    |position| {
                        let parameter = &callable.sig.params[position];
                        Ok(u32::from(parameter.defaulted) | (u32::from(parameter.rest) << 1))
                    },
                    |proof, position| {
                        let ty = &callable.sig.params[position].ty;
                        proof.budget.charge_clone(ty)?;
                        Ok(Arc::new(ty.clone()))
                    })?;
                for target in targets {
                    let incoming = self.incoming.entry(target).or_default();
                    if !incoming.contains(&instruction) { incoming.push(instruction); }
                    if matches!(self.store.functions[target].contract, FullCallableContract::Ground(_)) {
                        self.ground_function(target)?;
                        self.call_arguments(&decoder, target, &arguments)?;
                    }
                }
                Ok(())
            }
            schema::ExprView::Method { receiver, name, args, .. } => {
                let arguments = args.collect::<Result<Vec<_>, _>>()?.into_iter().map(|argument| argument.raw() as usize).collect::<Vec<_>>();
                self.verify_registered_method(&decoder, instruction, receiver.raw() as usize, name, &arguments, &expected)
            }
            schema::ExprView::ComparisonChain { first, mut pairs } => {
                let mut left = self.need_context(&decoder, first.raw() as usize)?.ok_or_else(|| IrVerifyError::new("comparison chain operand has no producer contract"))?;
                self.expression(owner, first.raw() as usize, Arc::clone(&left), false);
                for pair in &mut pairs {
                    self.budget.charge(1)?;
                    let (op, right, _) = pair?;
                    let actual = self.need_context(&decoder, right.raw() as usize)?.ok_or_else(|| IrVerifyError::new("comparison chain operand has no producer contract"))?;
                    if !matches!(op, crate::syntax::node::BinaryOp::Lt | crate::syntax::node::BinaryOp::Le | crate::syntax::node::BinaryOp::Gt | crate::syntax::node::BinaryOp::Ge)
                        || !left.can_be_ordering_operand() || !actual.can_be_ordering_operand()
                        || !actual.unvalidated().matches_expected(left.unvalidated())
                    { return Err(IrVerifyError::new("comparison chain operands have incompatible ordering domains")); }
                    self.expression(owner, right.raw() as usize, Arc::clone(&actual), false);
                    left = actual;
                }
                Ok(())
            }
            schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::Add, left, right, .. }
                if matches!(expected.as_ref(), Type::List(_)) => {
                self.expression(owner, left.raw() as usize, Arc::clone(&expected), false);
                self.expression(owner, right.raw() as usize, expected, false);
                Ok(())
            }
            schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::Eq | crate::syntax::node::BinaryOp::Ne, left, right, .. } => {
                let left_type = self.need_context(&decoder, left.raw() as usize)?;
                let right_type = self.need_context(&decoder, right.raw() as usize)?;
                let left_type = left_type.or_else(|| right_type.clone()).ok_or_else(|| IrVerifyError::new("equality operands have no producer contract"))?;
                let right_type = right_type.unwrap_or_else(|| Arc::clone(&left_type));
                self.budget.charge_clone(&left_type)?;
                self.budget.charge_clone(&right_type)?;
                let left_operand = left_type.equality_operand_type();
                let right_operand = right_type.equality_operand_type();
                if left_operand != Type::Any && right_operand != Type::Any
                    && !left_operand.matches_expected(&right_operand)
                    && !right_operand.matches_expected(&left_operand)
                { return Err(IrVerifyError::new("equality operands have incompatible checked domains")); }
                self.expression(owner, left.raw() as usize, left_type, false);
                self.expression(owner, right.raw() as usize, right_type, false);
                Ok(())
            }
            schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::And | crate::syntax::node::BinaryOp::Or, left, right, .. } => {
                self.verify_condition(owner, left.raw() as usize, ConditionRole::Boolean)?;
                self.verify_condition(owner, right.raw() as usize, ConditionRole::Boolean)
            }
            schema::ExprView::Binary { op: crate::syntax::node::BinaryOp::In | crate::syntax::node::BinaryOp::NotIn, left, right, .. } => {
                let container = match self.need_context(&decoder, right.raw() as usize)? {
                    Some(container) => container,
                    None => {
                        return Err(IrVerifyError::new(format!("membership container has no producer contract at owner {owner}, instruction {instruction}, container {} {:?}", right.raw(), self.store.tags[right.raw() as usize])));
                    }
                };
                let needle = self.need_context(&decoder, left.raw() as usize)?.ok_or_else(|| IrVerifyError::new("membership needle has no producer contract"))?;
                use crate::sema::types::MembershipCheck as M;
                let required = match container.membership_needle_type(&needle) {
                    M::Expected(required) => {
                        self.budget.charge(1)?;
                        let actual = self.derive_unannotated_type(&decoder, left.raw() as usize)?.ok_or_else(|| IrVerifyError::new("membership key has no independent producer contract"))?;
                        if !container.membership_needle_type(&needle).accepts_producer(&actual) { return Err(IrVerifyError::new("membership key producer does not preserve its checked domain")); }
                        self.budget.charge_clone(required)?;
                        Arc::new(required.clone())
                    }
                    M::Compared(required) if needle.matches_expected(required) => Arc::clone(&needle),
                    _ => return Err(IrVerifyError::new("membership operands have no checked domain")),
                };
                self.expression(owner, left.raw() as usize, required, false);
                self.expression(owner, right.raw() as usize, container, false);
                Ok(())
            }
            schema::ExprView::StrPredicate { receiver, predicate, needle, .. } => {
                self.verify_registered_method(&decoder, instruction, receiver.raw() as usize, predicate.method_name(), &[needle.raw() as usize], &expected)
            }
            schema::ExprView::DynamicCall { callee, .. } => {
                if self.store.scheme_callable_calls.binary_search_by_key(&(instruction as u32), |call| call.instruction).is_ok() {
                    let result = self.declared_type(&decoder, instruction)?.ok_or_else(|| IrVerifyError::new("scheme callable invocation has no checked result"))?;
                    if !result.matches_expected(&expected) { return Err(IrVerifyError::new("scheme callable invocation result disagrees with its producer contract")); }
                    let callee_type = self.need_context(&decoder, callee.raw() as usize)?.ok_or_else(|| IrVerifyError::new("scheme callable invocation has no callee producer contract"))?;
                    if !matches!(callee_type.as_ref(), Type::SchemeCallable(_)) { return Err(IrVerifyError::new("scheme callable invocation has a monomorphic callee producer")); }
                    self.expression(owner, callee.raw() as usize, callee_type, false);
                    return Ok(());
                }
                if let Some((base, receiver, callable)) = self.module_callable(&decoder, callee.raw() as usize)? {
                    let effects = callable.sig.effects.concrete()
                        .map_err(|_| IrVerifyError::new("callable effect contract is unresolved"))?;
                    self.require_callable_effects(&decoder, instruction, effects.as_deref(), callable.kind == crate::sema::types::CallableKind::Pure)?;
                    if !callable.sig.return_ty.matches_expected(&expected) { return Err(IrVerifyError::new("module callable result disagrees with its export contract")); }
                    self.expression(owner, base, receiver, false);
                    self.producer_owner_contract(owner)?;
                    let arguments = call_values(&decoder, instruction)?;
                    return self.call_arguments_with(&decoder, &arguments, callable.sig.params.len(),
                        |position| {
                            let parameter = &callable.sig.params[position];
                            Ok(u32::from(parameter.defaulted) | (u32::from(parameter.rest) << 1))
                        },
                        |proof, position| {
                            let ty = &callable.sig.params[position].ty;
                            proof.budget.charge_clone(ty)?;
                            Ok(Arc::new(ty.clone()))
                        });
                }
                if expected.as_ref() != &Type::Any { return Err(IrVerifyError::new("erased callable result has no concrete producer contract")); }
                let pure = super::super::driver_owner_index(owner).is_none()
                    && self.store.function_metadata[function_index(owner)?].callable_kind == crate::sema::types::CallableKind::Pure;
                self.require_callable_effects(&decoder, instruction, None, pure)?;
                if pure {
                    self.expression(owner, callee.raw() as usize, Arc::new(Type::Function(crate::sema::types::CallableKind::Pure)), false);
                }
                Ok(())
            }
            schema::ExprView::FunctionRef { function, kind } if matches!(expected.as_ref(), Type::Function(expected_kind) if *expected_kind == kind) => {
                let (target, contract, target_kind) = decoder.function_reference_contract(function)?;
                if kind != target_kind { return Err(IrVerifyError::new("callable producer kind disagrees with its target")); }
                if matches!(contract, super::super::FullCallableContract::Ground(_)) { self.ground_function(target.index())?; }
                Ok(())
            }
            schema::ExprView::Field { base, .. } => {
                if let Some(ty) = self.scoped_field_type(&decoder, instruction)? {
                    let actual = ty.closed_ground_type().ok_or_else(|| IrVerifyError::new("scoped field has no closed producer contract"))?;
                    if !decoder.checked_type_fits(&actual, &expected) {
                        return Err(IrVerifyError::new("scoped field producer disagrees with its expected contract"));
                    }
                    return Ok(());
                }
                let (ty, fact) = self.operand_context(&decoder, base.raw() as usize)?.into_parts();
                let ty = ty.ok_or_else(|| IrVerifyError::new("ground field producer has no receiver contract"))?;
                self.expression_with_fact(owner, base.raw() as usize, ty, false, fact);
                Ok(())
            }
            schema::ExprView::Record(_) => {
                let RecordProducerWriters { writers, mut unresolved, .. } = self.record_field_writers(&decoder, instruction)?;
                if expected.unvalidated() == &Type::ErasedRecord {
                    if !unresolved.is_empty() { return Err(IrVerifyError::new("record field has no producer contract")); }
                    return Ok(());
                }
                let Type::Record(expected_fields) = expected.unvalidated() else { return Err(IrVerifyError::new("ground record producer has no record contract")) };
                for (name, expected) in expected_fields.iter() {
                    match writers.get(name) {
                        Some(super::super::CheckedRecordFieldWriter::Value(value)) => {
                            unresolved.remove(value);
                            self.expression(owner, *value, Arc::new(expected.clone()), false);
                        },
                        Some(super::super::CheckedRecordFieldWriter::Spread { instruction, fields }) => {
                            let Type::Record(source) = fields.unvalidated() else { unreachable!("record spread writer has a non-record field contract") };
                            if !decoder.checked_type_fits(&source[name], expected) { return Err(IrVerifyError::new("ground record last spread writer disagrees with its result field contract")); }
                            self.expression(owner, *instruction, Arc::clone(fields), false);
                        }
                        None => return Err(IrVerifyError::new("ground record producer omits a required field")),
                    }
                }
                if !unresolved.is_empty() { return Err(IrVerifyError::new("record field has no producer contract")); }
                Ok(())
            }
            schema::ExprView::Loop { .. } => {
                for value in self.index.owners[&owner].return_analysis.loop_values.as_ref().and_then(|values| values.get(&instruction)).into_iter().flatten().copied().collect::<Vec<_>>() { self.expression(owner, value, Arc::clone(&expected), returned); }
                Ok(())
            }
            schema::ExprView::ListPipeline { .. } => self.verify_pipeline_result(owner, instruction, expected),
            schema::ExprView::ListComp { value, mut qualifiers, .. } => {
                let Type::List(item) = expected.unvalidated() else { return Err(IrVerifyError::new("ground list comprehension has no list contract")) };
                let mut qualifier_count = 0;
                let mut source_validation = None;
                for qualifier in &mut qualifiers {
                    qualifier_count += 1;
                    match qualifier? {
                        super::super::decode::CompQualifierView::For { target, iter, .. } => {
                            let source = self.need_context(&decoder, iter.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground comprehension has no iterable input contract"))?;
                            source_validation = source.validated().map(|validated| validated.validation());
                            self.budget.charge_clone(&source)?;
                            let item = source.iteration_item_type().ok_or_else(|| IrVerifyError::new("ground comprehension source has no item type"))?;
                            self.verify_comprehension_target(target, &item)?;
                            self.expression(owner, iter.raw() as usize, source, false);
                        }
                        super::super::decode::CompQualifierView::If { condition, .. } => self.verify_condition(owner, condition.raw() as usize, ConditionRole::Control)?,
                    }
                }
                if let Some(validated) = expected.validated()
                    && (qualifier_count != 1 || !validated.validation().survives_mapping()
                        || source_validation != Some(validated.validation()))
                {
                    return Err(IrVerifyError::new("ground list comprehension does not preserve its validation"));
                }
                self.expression(owner, value.raw() as usize, Arc::new((**item).clone()), false);
                Ok(())
            }
            schema::ExprView::MapComp { key, value, mut qualifiers, .. } => {
                let Type::Map(key_type, value_type) = expected.unvalidated() else { return Err(IrVerifyError::new("ground map comprehension has no map contract")) };
                for qualifier in &mut qualifiers {
                    match qualifier? {
                        super::super::decode::CompQualifierView::For { target, iter, .. } => {
                            let source = self.need_context(&decoder, iter.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground comprehension has no iterable input contract"))?;
                            self.budget.charge_clone(&source)?;
                            let item = source.iteration_item_type().ok_or_else(|| IrVerifyError::new("ground comprehension source has no item type"))?;
                            self.verify_comprehension_target(target, &item)?;
                            self.expression(owner, iter.raw() as usize, source, false);
                        }
                        super::super::decode::CompQualifierView::If { condition, .. } => self.verify_condition(owner, condition.raw() as usize, ConditionRole::Control)?,
                    }
                }
                self.expression(owner, key.raw() as usize, Arc::new((**key_type).clone()), false);
                self.expression(owner, value.raw() as usize, Arc::new((**value_type).clone()), false);
                Ok(())
            }
            schema::ExprView::MapLiteral(mut entries) => {
                let Type::Map(key_type, value_type) = expected.unvalidated() else { return Err(IrVerifyError::new("ground map literal has no map contract")) };
                for entry in &mut entries {
                    let (key, value, _) = entry?;
                    if let Some(key) = key {
                        self.expression(owner, key.raw() as usize, Arc::new((**key_type).clone()), false);
                        self.expression(owner, value.raw() as usize, Arc::new((**value_type).clone()), false);
                    } else {
                        self.expression(owner, value.raw() as usize, Arc::clone(&expected), false);
                    }
                }
                Ok(())
            }
            schema::ExprView::EmptyMap => {
                if !matches!(expected.unvalidated(), Type::Map(_, _)) { return Err(IrVerifyError::new("empty map producer has no map contract")); }
                Ok(())
            }
            schema::ExprView::ListBuild(mut entries) => {
                let Some(item) = expected_list_item(&expected, true) else { return Err(IrVerifyError::new("ground list build has no list contract")) };
                let mut nonempty = false;
                for entry in &mut entries {
                    let (splice, value, _) = entry?;
                    let value = value.raw() as usize;
                    let ty = if splice {
                        match self.need_context(&decoder, value)? {
                            Some(actual) if matches!(actual.unvalidated(), Type::List(actual_item) if actual_item.matches_expected(item)) => actual,
                            _ if self.contextual_list_argument(&decoder, value, &expected)? => Arc::clone(&expected),
                            _ => return Err(IrVerifyError::new("fresh list splice disagrees with its element contract")),
                        }
                    } else { Arc::new(item.clone()) };
                    nonempty |= !splice || ty.validated().is_some_and(|validated| validated.validation().implies(crate::sema::validated::Validation::NonEmpty));
                    self.expression(owner, value, ty, false);
                }
                if expected_list_item(&expected, nonempty).is_none() {
                    return Err(IrVerifyError::new("ground list build has no non-empty producer"));
                }
                Ok(())
            }
            schema::ExprView::Index { base, index, .. } => {
                let source = self.need_context(&decoder, base.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground index has no collection contract"))?;
                let key = match source.unvalidated() { Type::Map(key, _) => (**key).clone(), Type::List(_) => Type::Int, _ => return Err(IrVerifyError::new("ground index source is not a keyed collection")) };
                self.expression(owner, base.raw() as usize, source, false);
                self.expression(owner, index.raw() as usize, Arc::new(key), false);
                Ok(())
            }
            schema::ExprView::IndexFromEnd { base, .. } => {
                let source = self.need_context(&decoder, base.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground from-end index has no collection contract"))?;
                if !matches!(source.unvalidated(), Type::List(_)) { return Err(IrVerifyError::new("ground from-end index source is not a list")); }
                self.expression(owner, base.raw() as usize, source, false);
                Ok(())
            }
            schema::ExprView::Slice { base, start, end, .. } => {
                let source = self.need_context(&decoder, base.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground slice has no collection contract"))?;
                if !matches!(source.unvalidated(), Type::List(_) | Type::Str | Type::Bytes) { return Err(IrVerifyError::new("ground slice source is not sliceable")); }
                self.expression(owner, base.raw() as usize, source, false);
                for bound in [start, end].into_iter().flatten() { self.expression(owner, bound.raw() as usize, Arc::new(Type::Int), false); }
                Ok(())
            }
            schema::ExprView::List(mut values) => {
                let Some(item) = expected_list_item(&expected, values.len() > 0) else { return Err(IrVerifyError::new("ground list producer has no item contract")) };
                for value in &mut values { self.expression(owner, value?.raw() as usize, Arc::new(item.clone()), false); }
                Ok(())
            }
            schema::ExprView::RunCapture(run) => {
                let result = (match run.kind { crate::syntax::node::RunKind::Status => Some(Type::Status), _ => Type::run_capture_result(run.kind) }).ok_or_else(|| IrVerifyError::new("ground run value has no capture contract"))?;
                if !result.matches_expected(&expected) { return Err(IrVerifyError::new("ground capture kind disagrees with its result contract")); }
                self.verify_operands(&decoder, instruction)
            }
            schema::ExprView::SpawnRun(run) => {
                self.verify_spawn_result(&expected)?;
                self.require_callable_effects(&decoder, instruction, Some(&[crate::syntax::node::Effect::Process]), false)?;
                if !self.operand_expansions.insert((owner, instruction)) { return Ok(()); }
                if self.verify_process_run_operands(&decoder, run.operands)? {
                    self.require_callable_effects(&decoder, instruction, Some(&[crate::syntax::node::Effect::Error]), false)?;
                }
                Ok(())
            }
            schema::ExprView::SpawnCommand { command, .. } => {
                self.verify_spawn_result(&expected)?;
                self.require_callable_effects(&decoder, instruction, Some(&[crate::syntax::node::Effect::Process]), false)?;
                self.expression(owner, command.raw() as usize, Arc::new(Type::Command), false);
                Ok(())
            }
            schema::ExprView::Wait { target, .. } => {
                let (target_type, result) = self.process_wait_contract(&decoder, target.raw() as usize)?;
                if !result.matches_expected(&expected) { return Err(IrVerifyError::new("wait result disagrees with its actual handle contract")); }
                self.require_callable_effects(&decoder, instruction, Some(&[crate::syntax::node::Effect::Process]), false)?;
                self.expression(owner, target.raw() as usize, target_type, false);
                Ok(())
            }
            schema::ExprView::ProcessCommandBuilder { entries, .. } => self.verify_process_command_builder(&decoder, instruction, entries, &expected),
            schema::ExprView::RunPipeline { mut segments, .. } => {
                let first = segments.next().transpose()?.ok_or_else(|| IrVerifyError::new("ground capture pipeline has no head"))?;
                let result = Type::run_capture_result(first.kind).ok_or_else(|| IrVerifyError::new("ground pipeline head has no capture contract"))?;
                if !result.matches_expected(&expected) { return Err(IrVerifyError::new("ground pipeline head disagrees with its result contract")); }
                self.verify_operands(&decoder, instruction)
            }
            schema::ExprView::Capture { body, .. } => {
                let Type::Result(ok, error) = expected.unvalidated() else { return Err(IrVerifyError::new("ground capture has no Result contract")) };
                self.budget.charge_clone(ok)?;
                self.budget.charge_clone(error)?;
                let errors = self.captured_error_types(&decoder, instruction)?;
                if !errors.is_empty() {
                    let actual = self.join_captured_errors(errors.as_ref())?;
                    if !actual.matches_expected(error) { return Err(IrVerifyError::new("captured error producer disagrees with its nominal error contract")); }
                }
                self.verify_block_value(&decoder, body, Arc::new((**ok).clone()), false)
            }
            schema::ExprView::Retry { delays, mut body, .. } => {
                let Type::Result(ok, error) = expected.unvalidated() else { return Err(IrVerifyError::new("ground retry has no Result contract")) };
                self.budget.charge_clone(ok)?;
                self.budget.charge_clone(error)?;
                for delay in delays {
                    self.budget.charge(1)?;
                    self.expression(owner, delay?.raw() as usize, Arc::new(Type::Duration), false);
                }
                let errors = self.captured_error_types(&decoder, instruction)?;
                if !errors.is_empty() {
                    let actual = self.join_captured_errors(errors.as_ref())?;
                    if !actual.matches_expected(error) { return Err(IrVerifyError::new("retry error producer disagrees with its nominal error contract")); }
                }
                self.budget.charge(body.len())?;
                let body_expected = match decoder.checked_value_block_result(&mut body)? {
                    super::super::CheckedValueBlockResult::Tail(tail) => {
                        let actual = self.need_context(&decoder, tail)?.ok_or_else(|| IrVerifyError::new("ground retry body has no producer contract"))?;
                        if matches!(actual.as_ref(), Type::Result(..)) { Arc::clone(&expected) }
                        else { Arc::new((**ok).clone()) }
                    }
                    super::super::CheckedValueBlockResult::Unit => Arc::new((**ok).clone()),
                    super::super::CheckedValueBlockResult::DoesNotComplete => Arc::new((**ok).clone()),
                };
                let schema::ExprView::Retry { body, .. } = decoder.expression_view(instruction)? else { unreachable!() };
                self.verify_block_value(&decoder, body, body_expected, false)
            }
            schema::ExprView::ErrorContext { message, body, .. } => {
                self.expression(owner, message.raw() as usize, Arc::new(Type::Str), false);
                self.verify_block_value(&decoder, body, expected, returned)
            }
            schema::ExprView::ContextScope { kind, input, body, .. } => {
                let source = self.need_context(&decoder, input.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground scope input has no producer contract"))?;
                if !source.accepts_context_scope_input(kind) { return Err(IrVerifyError::new("ground scope input disagrees with its scope kind")); }
                self.expression(owner, input.raw() as usize, source, false);
                let Type::Result(payload, error) = expected.as_ref() else { return Err(IrVerifyError::new("ground scope result has no Result contract")) };
                if !Type::Error.matches_expected(error) { return Err(IrVerifyError::new("ground scope error disagrees with its Result contract")); }
                self.budget.charge_clone(payload)?;
                self.verify_block_value(&decoder, body, Arc::new((**payload).clone()), false)
            }
            schema::ExprView::ValueBlock { body, .. } => self.verify_block_value(&decoder, body, expected, returned),
            schema::ExprView::IfExpr { mut branches, else_value, .. } => {
                for branch in &mut branches {
                    let (condition, value) = branch?;
                    self.verify_condition(owner, condition.raw() as usize, ConditionRole::Control)?;
                    self.expression(owner, value.raw() as usize, Arc::clone(&expected), returned);
                }
                self.expression(owner, else_value.raw() as usize, expected, returned);
                Ok(())
            }
            schema::ExprView::PatternIf { mut branches, else_value, .. } => {
                for branch in &mut branches {
                    let (condition, value, _) = branch?;
                    self.verify_condition(owner, condition.raw() as usize, ConditionRole::Control)?;
                    self.expression(owner, value.raw() as usize, Arc::clone(&expected), returned);
                }
                self.expression(owner, else_value.raw() as usize, expected, returned);
                Ok(())
            }
            schema::ExprView::MatchExpr { value, mut arms, .. } => {
                if let Some(subject) = super::super::indexed_identity_match_subject(&decoder, instruction)? {
                    self.expression(owner, subject as usize, expected, returned);
                    return Ok(());
                }
                let declared = self.declared_type(&decoder, value.raw() as usize)?;
                if declared.is_none()
                    && let Some((binding, source)) = self.contextual_match_binding(&decoder, instruction)?
                {
                    self.deferred_sources.insert((owner, binding), source);
                } else {
                    let (subject, fact) = match declared {
                        Some(ty) => (ty.clone(), Some(DeclaredProducerFact { owner, instruction: value.raw() as usize, ty, binding: None })),
                        None => (self.contextual_producer_type(&decoder, value.raw() as usize)?.ok_or_else(|| IrVerifyError::new(format!("ground match expression has no subject contract at owner {owner}, instruction {instruction}, subject {} {:?}", value.raw(), self.store.tags[value.raw() as usize])))?, None),
                    };
                    self.expression_with_fact(owner, value.raw() as usize, subject, false, fact);
                }
                for arm in &mut arms {
                    let (_, guard, value) = arm?;
                    if let Some(guard) = guard { self.expression(owner, guard.raw() as usize, Arc::new(Type::Bool), false); }
                    self.expression(owner, value.raw() as usize, Arc::clone(&expected), returned);
                }
                Ok(())
            }
            schema::ExprView::StrMatchExpr { value, mut arms, fallback, .. } | schema::ExprView::TagMatchExpr { value, mut arms, fallback, .. } => {
                let subject = self.need_context(&decoder, value.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground keyed match has no subject contract"))?;
                self.expression(owner, value.raw() as usize, subject, false);
                for arm in &mut arms { self.expression(owner, arm?.1.raw() as usize, Arc::clone(&expected), returned); }
                if let Some(fallback) = fallback { self.expression(owner, fallback.raw() as usize, expected, returned); }
                Ok(())
            }
            schema::ExprView::Try { value, .. } => {
                if self.prove_error_transfer(&decoder, instruction)?.is_some() { return Ok(()); }
                let source = self.need_context(&decoder, value.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground propagation has no result contract"))?;
                let Type::Result(ok, _) = source.unvalidated() else { return Err(IrVerifyError::new("ground propagation source is not a result")) };
                if !ok.matches_expected(&expected) { return Err(IrVerifyError::new("ground propagation payload disagrees with its expected contract")); }
                self.expression(owner, value.raw() as usize, source, false);
                Ok(())
            }
            schema::ExprView::ResultFallback { left, right } => {
                let source = self.need_context(&decoder, left.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground fallback has no input envelope contract"))?;
                let payload = match source.unvalidated() { Type::Result(ok, _) | Type::Optional(ok) => ok.as_ref(), _ => return Err(IrVerifyError::new("ground fallback input has no payload")) };
                if !payload.matches_expected(&expected) { return Err(IrVerifyError::new("ground fallback payload disagrees with its expected contract")); }
                self.expression(owner, left.raw() as usize, source, false);
                self.expression(owner, right.raw() as usize, expected, returned);
                Ok(())
            }
            schema::ExprView::Ok(value) => {
                let Type::Result(ok, _) = expected.unvalidated() else { return Err(IrVerifyError::new("ground success producer has no result contract")) };
                self.expression(owner, value.raw() as usize, Arc::new((**ok).clone()), false);
                Ok(())
            }
            schema::ExprView::Err { value, cause } => {
                let Type::Result(_, error) = expected.unvalidated() else { return Err(IrVerifyError::new("ground error producer has no result contract")) };
                self.expression(owner, value.raw() as usize, Arc::new((**error).clone()), false);
                if let Some(cause) = cause { self.expression(owner, cause.raw() as usize, Arc::new(Type::Error), false); }
                Ok(())
            }
            schema::ExprView::Tag { type_name, name, mut fields, .. } => {
                if expected.unvalidated() != &Type::Tag(type_name) { return Err(IrVerifyError::new("ground tag producer has an incompatible family contract")); }
                let key = super::super::CheckedPatternVariantKey { kind: super::super::CheckedPatternVariantKind::Tag, owner: type_name, variant: Name::intern(name) };
                let layout = self.store.declared_pattern_variants.get(&key).ok_or_else(|| IrVerifyError::new("ground tag producer has no declared layout"))?;
                let fields = (&mut fields).collect::<Result<Vec<_>, _>>()?;
                if fields.len() != layout.fields.len() { return Err(IrVerifyError::new("ground tag producer disagrees with its declared payload count")); }
                let types = layout.fields.iter().map(|field| field.type_id).collect::<Vec<_>>();
                for (field, ty) in fields.iter().zip(types) { let ty = self.ground_type(ty)?; self.expression(owner, field.raw() as usize, ty, false); }
                Ok(())
            }
            schema::ExprView::Error(super::super::ErrorExprView::Structured { family_identity, variant, mut fields, .. }) => {
                self.structured_error_payload(owner, Name::intern(family_identity), Name::intern(variant), (&mut fields).map(|field| field.map(|(name, value)| (Name::intern(name), value.raw() as usize))), None)?;
                Ok(())
            }
            schema::ExprView::PreparedConstant(value) => decoder.verify_prepared_constant_type(value, &expected, &mut |amount| self.budget.charge(amount)),
            schema::ExprView::CheckedValue { check: ValueCheckView::Materialization(id), value, .. } => {
                let (source, destination) = super::materialization_endpoints(self.store, id, instruction, &mut self.budget)?;
                if !destination.matches_expected(&expected) { return Err(IrVerifyError::new("materialization destination disagrees with its ground producer contract")); }
                self.expression(owner, value.raw() as usize, Arc::new(source), false);
                Ok(())
            }
            schema::ExprView::CheckedValue { check: ValueCheckView::Ground(check), value, .. } => {
                let checked = self.contract_type(check.ty.id)?;
                if !checked.matches_expected(&expected) { return Err(IrVerifyError::new("ground checked producer disagrees with its contract")); }
                if self.store.scheme_callable_calls.binary_search_by_key(&(instruction as u32), |site| site.instruction).is_ok() {
                    let source = super::checked_expression_type_id(self.store, value.raw())
                        .ok_or_else(|| IrVerifyError::new("scheme callable value materialization has no source contract"))?;
                    let source = self.contract_type(source)?;
                    if check.schema.is_some() || !matches!(checked.as_ref(), Type::Callable(_)) || !matches!(source.as_ref(), Type::SchemeCallable(_)) {
                        return Err(IrVerifyError::new("scheme callable value materialization has an invalid storage contract"));
                    }
                    self.expression(owner, value.raw() as usize, source, false);
                    return Ok(());
                }
                let runtime_complete = check.schema.is_none() && super::runtime_complete_storage_check_type(&checked, &mut self.budget)?;
                let mut checked_record = false;
                if check.schema.is_none() {
                    let input = value.raw() as usize;
                    if runtime_complete { self.mark_runtime_checked_literal_payloads(&decoder, input)?; }
                    checked_record = self.checked_record_arguments(&decoder, input, &checked)?;
                    if !checked_record
                        && !self.checked_list_accumulator(&decoder, input, &checked, runtime_complete)?
                    {
                        let fresh_set = matches!(checked.as_ref(), Type::Set(_))
                            && self.checked_fresh_list_method_receiver(&decoder, input, &checked)?.is_some();
                        let empty_set = matches!(checked.as_ref(), Type::Set(_))
                            && self.checked_empty_set_module_call(&decoder, input, &checked)?;
                        if fresh_set || empty_set || matches!((self.store.tags[input], checked.as_ref()),
                            (FullTag::ExprList | FullTag::ExprListBuild | FullTag::ExprListComp, Type::List(_))
                            | (FullTag::ExprMapLiteral | FullTag::ExprEmptyMap | FullTag::ExprMapComp, Type::Map(_, _))
                            | (FullTag::ExprCapture, Type::Result(_, _)))
                        {
                            self.expression(owner, input, Arc::clone(&checked), false);
                        } else {
                        let (actual, source_fact) = self.operand_context(&decoder, input)?.into_parts();
                        let actual = match actual {
                            Some(actual) => actual,
                            None => return Err(IrVerifyError::new("checked value input has no producer contract")),
                        };
                        let optional_payload = !runtime_complete && Self::checked_optional_payload_source(&actual, &checked);
                        if optional_payload {
                            let prefix = decoder.producer_prefix.as_ref()
                                .ok_or_else(|| IrVerifyError::new("checked optional payload has no verified owner prefix"))?;
                            let target = decoder.checked_binding_path(input, &mut |amount| prefix.charge_optional_path(amount))?
                                .ok_or_else(|| IrVerifyError::new("checked optional field receiver has no non-null branch proof"))?;
                            if !prefix.optional_source_is_narrowed(&decoder, instruction, input, check.ty.id, &checked, &target)? {
                                return Err(IrVerifyError::new("checked optional field receiver has no non-null branch proof"));
                            }
                        }
                        if !optional_payload && !self.checked_record_field_fits(&decoder, input, &actual, &checked)? {
                            return Err(IrVerifyError::new("checked value input disagrees with its contract"));
                        }
                        self.expression_with_fact(owner, input, actual, false, source_fact);
                        }
                    }
                }
                if !runtime_complete {
                    let source = if check.schema.is_none() {
                        self.need_context(&decoder, value.raw() as usize)?
                            .filter(|source| Self::checked_optional_payload_source(source, &checked))
                            .or_else(|| (checked_record && checked.nominal_name().is_some()).then(|| Arc::new(checked.unvalidated().clone())))
                            .unwrap_or(checked)
                    } else { checked };
                    self.expression(owner, value.raw() as usize, source, false);
                }
                Ok(())
            }
            schema::ExprView::Fail { message, .. } => {
                let Type::Result(_, expected_error) = expected.unvalidated() else { return Err(IrVerifyError::new("ground failure producer has no result contract")); };
                let error = FullDecoder::registered_api_operation_return_type(super::super::RuntimeOp::ErrorFailure)?;
                if !error.matches_expected(expected_error) { return Err(IrVerifyError::new("ground failure producer has an incompatible error contract")); }
                self.expression(owner, message.raw() as usize, Arc::new(Type::Str), false);
                Ok(())
            }
            schema::ExprView::Require { check, .. } => {
                let checked = check.ty.materialize()?;
                if !checked.matches_expected(&expected) && !matches!(expected.as_ref(), Type::Result(ok, _) if checked.matches_expected(ok)) { return Err(IrVerifyError::new("ground checked producer disagrees with its contract")); }
                Ok(())
            }
            _ if actual.is_some() => self.verify_operands(&decoder, instruction),
            _ => Err(IrVerifyError::new(format!("ground producer form has no executable type proof: {:?} at instruction {instruction}, owner {owner}", self.store.tags[instruction]))),
        }
    }

    fn producer_owner_contract(&mut self, owner: u32) -> Result<(), IrVerifyError> {
        if !self.contract_owners.insert(owner) { return Ok(()); }
        self.budget.charge(1)?;
        let Some(step) = super::super::driver_owner_index(owner) else {
            self.ground_function(function_index(owner)?)?;
            return Ok(());
        };
        let driver = self.store.driver_steps[step];
        if driver.tag != super::super::FullDriverTag::Let { return Ok(()); }
        let decoder = self.owner(owner)?;
        let mut payload = super::super::FullCursor::new(self.store.payload(driver.data.range())?);
        let _: Name = super::super::FullCodec::decode(&decoder, &mut payload)?;
        let _: Option<super::super::LoweredType> = super::super::FullCodec::decode(&decoder, &mut payload)?;
        let context = super::super::DecodeContext::from_decoder(&decoder);
        let _ = Option::<super::super::LoweredTypeCheck>::borrow(context, &mut payload)?;
        let annotation = Option::<Type>::borrow(context, &mut payload)?;
        if let Some(annotation) = annotation {
            let expected = self.contract_type(annotation.id)?;
            for writer in bounded(driver.writers, &self.store.driver_writers, "capture declaration writer range is invalid")? {
                self.binding_writer(writer.binding_identity, Arc::clone(&expected))?;
            }
        }
        Ok(())
    }

    fn callee_error_flow(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<crate::sema::types::CalleeErrorFlow, IrVerifyError> {
        if self.scheme_applications.contains(&(decoder.owner, instruction)) {
            let may_error = super::ScopedEffectProof::new(self.store, self.index.scheme_scopes(self.store)?)
                .application_may_error(function_index(decoder.owner)?, instruction, &mut self.budget)?;
            return Ok(if may_error { crate::sema::types::CalleeErrorFlow::Immediate } else { crate::sema::types::CalleeErrorFlow::None });
        }
        let may_error = match decoder.expression_view(instruction)? {
            schema::ExprView::Call { function, .. } | schema::ExprView::DirectPureCall { function, .. } => {
                self.direct_callee_may_error(decoder, instruction, function.id.index())?
            }
            schema::ExprView::SelfCall { .. } => {
                self.direct_callee_may_error(decoder, instruction, function_index(decoder.owner)?)?
            }
            schema::ExprView::TypedCall { signature, kind, .. } => {
                let signature = self.store.semantic.typed_callable_signature(signature.id)?;
                let effects = self.store.semantic.signature_effects(signature)?;
                self.budget.charge(1 + effects.as_ref().map_or(0, Vec::len))?;
                if kind == crate::sema::types::CallableKind::Pure {
                    effects.unwrap_or_default().contains(&crate::syntax::node::Effect::Error)
                } else {
                    effects.is_none_or(|effects| effects.contains(&crate::syntax::node::Effect::Error))
                }
            }
            _ => return Ok(crate::sema::types::CalleeErrorFlow::None),
        };
        self.budget.charge(1)?;
        let ty = self.need_context(decoder, instruction)?.ok_or_else(|| IrVerifyError::new("captured callee has no executable return contract"))?;
        let effects = if may_error { &[crate::syntax::node::Effect::Error][..] } else { &[] };
        let flow = ty.callee_error_flow(Some(effects));
        self.expression(decoder.owner, instruction, ty, false);
        Ok(flow)
    }

    fn direct_callee_may_error(&mut self, decoder: &FullDecoder<'a>, instruction: usize, target: usize) -> Result<bool, IrVerifyError> {
        let function = self.store.functions.get(target).ok_or_else(|| IrVerifyError::new("captured call target is out of bounds"))?;
        if matches!(function.contract, super::super::FullCallableContract::Scheme(_)) {
            return super::ScopedEffectProof::new(self.store, self.index.scheme_scopes(self.store)?)
                .call_may_error(decoder, instruction, target, &mut self.budget);
        }
        let effects = super::super::verify_effects::function_effects(self.store, target)?;
        self.budget.charge(1 + effects.as_ref().map_or(0, Vec::len))?;
        Ok(effects.as_ref().is_none_or(|effects| effects.contains(&crate::syntax::node::Effect::Error)))
    }

    fn verify_host_stream_projection(&mut self, decoder: &FullDecoder<'a>, instruction: usize, projection: &[FunctionProjection]) -> Result<(), IrVerifyError> {
        let ty = self.need_context(decoder, instruction)?.ok_or_else(|| IrVerifyError::new("host stream source has no canonical output contract"))?;
        let projected = match projection {
            [] => Some(ty.as_ref()),
            [FunctionProjection::ResultPayload(true)] => ty.result_ok(),
            _ => None,
        };
        if !projected.is_some_and(|ty| matches!(ty.unvalidated(), Type::Stream(_))) { return Err(IrVerifyError::new("host stream projection disagrees with its canonical output contract")); }
        Ok(())
    }

    fn stream_may_propagate_error(&mut self, decoder: &FullDecoder<'a>, source: StreamProducer) -> Result<bool, IrVerifyError> {
        let (instruction, projection) = match source {
            StreamProducer::Value(instruction) => (instruction, Vec::new()),
            StreamProducer::ResultSuccess(instruction) => (instruction, vec![FunctionProjection::ResultPayload(true)]),
        };
        let targets = self.producer_targets(decoder.owner, instruction, ProducerTargetPurpose::Stream, projection)?;
        let mut may_error = targets.opaque;
        for target in targets.functions {
            let effects = super::super::verify_effects::function_effects(self.store, target)?;
            self.budget.charge(1 + effects.as_ref().map_or(0, Vec::len))?;
            may_error |= effects.as_ref().is_none_or(|effects| effects.contains(&crate::syntax::node::Effect::Error));
            self.ground_function(target)?;
        }
        Ok(may_error)
    }

    fn run_capture_error_policy<'view>(&mut self, operands: super::super::decode::RunOperandsView<'view>) -> Result<(bool, bool), IrVerifyError> {
        self.budget.charge(1 + operands.options.len())?;
        let spliced = matches!(operands.target.kind, super::super::decode::RunArgKindView::Splice(_) | super::super::decode::RunArgKindView::SingleOrSplice(_));
        let mut accepts = false;
        for option in operands.options { accepts |= matches!(option?, super::super::decode::RunOptionView::Accept(_)); }
        Ok((spliced, accepts))
    }

    fn captured_pipeline_errors(&mut self, decoder: &FullDecoder<'a>, instruction: usize, errors: &mut Vec<Type>, skipped: &mut BTreeSet<usize>) -> Result<(), IrVerifyError> {
        let stages = &self.index.owners[&decoder.owner].pipeline_stages[&instruction];
        self.budget.charge(1 + stages.len())?;
        let stages = stages.clone();
        let schema::ExprView::ListPipeline { input, .. } = decoder.expression_view(instruction)? else { unreachable!() };
        let mut query = super::super::IndexedCardinalityQuery::new(decoder);
        let pulls = super::super::indexed_pipeline_downstream_pulls(decoder, &stages, &mut query)?;
        let completes = super::super::indexed_expr_can_complete(decoder, input.raw() as usize, 0)?;
        let mut cardinality = query.expression(input.raw() as usize)?;
        for (position, stage) in stages.into_iter().enumerate() {
            self.budget.charge(1)?;
            let stage = super::super::StageView::decode(super::super::DecodeContext::from_decoder(decoder), stage)?;
            if !completes || super::super::indexed_stage_callback_demand(pulls[position + 1], cardinality, &stage) == super::super::IndexedCallbackDemand::Never {
                let mut roots = rustc_hash::FxHashSet::default();
                super::super::indexed_stage_callback_roots(decoder, &stage, &mut roots)?;
                self.budget.charge(roots.len())?;
                skipped.extend(roots);
            }
            cardinality = query.stage(cardinality, &stage)?;
        }
        self.budget.charge(query.memo.len())?;
        if completes {
            self.budget.charge(1)?;
            if decoder.is_empty_list_literal_expression(input.raw() as usize)? { return Ok(()); }
            let source = self.need_context(decoder, input.raw() as usize)?.ok_or_else(|| IrVerifyError::new("captured pipeline input has no executable contract"))?;
            if matches!(source.unvalidated(), Type::Stream(_)) {
                if !pulls[0].never && self.stream_may_propagate_error(decoder, StreamProducer::Value(input.raw() as usize))? { errors.push(Type::Error); }
                self.expression(decoder.owner, input.raw() as usize, source, false);
            }
        }
        Ok(())
    }

    fn prove_error_transfer(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        self.budget.charge(2)?;
        let Some(mut operand) = self.store.try_operand(instruction)? else { return Ok(None) };
        let mut bindings = Vec::new();
        let (value, cause) = loop {
            self.budget.charge(1)?;
            if !decoder.instruction_range.contains(&operand) {
                return Err(IrVerifyError::new("error transfer source belongs to another owner"));
            }
            match decoder.expression_view(operand)? {
                schema::ExprView::Err { value, cause } => break (value.raw() as usize, cause.map(|cause| cause.raw() as usize)),
                schema::ExprView::MatchExpr { .. } => {
                    self.budget.charge(1)?;
                    let Some(binding) = decoder.argument_binding_match(operand)? else { return Ok(None) };
                    if bindings.len() >= decoder.instruction_range.len() {
                        return Err(IrVerifyError::new("error transfer binding chain exceeds its owner"));
                    }
                    operand = binding.result;
                    self.budget.charge(1)?;
                    bindings.push(binding);
                }
                _ => return Ok(None),
            }
        };
        for binding in bindings {
            self.budget.charge(5)?;
            let identity = ProducerBinding::ScopedDeclaration(binding.result, binding.slot);
            let owner = &self.index.owners[&decoder.owner];
            let source = owner.pattern_sources.get(&identity)
                .ok_or_else(|| IrVerifyError::new("error transfer binding has no authenticated pattern source"))?;
            if source.source != binding.source || source.pattern != binding.pattern || !source.preceding.is_empty()
                || owner.parents[binding.source - owner.range.start] != Some(binding.instruction)
                || owner.parents[binding.result - owner.range.start] != Some(binding.instruction)
                || owner.region_bindings.get(&(binding.result, binding.slot)) != Some(&identity)
                || owner.writers.get(&identity).is_some_and(|writers| !writers.is_empty())
                || owner.storage_bindings.contains_key(&identity)
                || matches!(owner.header_binding(self.store, decoder.owner, binding.slot)?, Some(ProducerBinding::HeaderParameter(_) | ProducerBinding::HeaderCapture(_)))
            { return Err(IrVerifyError::new("error transfer binding differs from its physical source")); }
            let actual = self.declared_type(decoder, binding.source)?
                .ok_or_else(|| IrVerifyError::new("error transfer argument has no actual producer contract"))?;
            let fact = DeclaredProducerFact { owner: decoder.owner, instruction: binding.source, ty: Arc::clone(&actual), binding: None };
            self.expression_with_fact(decoder.owner, binding.source, actual, false, Some(fact));
        }
        let error = self.need_context(decoder, value)?.ok_or_else(|| IrVerifyError::new("direct error transfer has no actual error contract"))?;
        if !error.matches_expected(&Type::Error) { return Err(IrVerifyError::new("direct error transfer payload is not an error")); }
        self.expression(decoder.owner, value, Arc::clone(&error), false);
        if let Some(cause) = cause { self.expression(decoder.owner, cause, Arc::new(Type::Error), false); }
        let mut child = instruction;
        while let Some(parent) = self.index.owners[&decoder.owner].parents[child - decoder.instruction_range.start] {
            self.budget.charge(1)?;
            if super::super::verify_effects::captures_child_error(decoder, parent, child)? { return Ok(Some(error)); }
            child = parent;
        }
        if super::super::driver_owner_index(decoder.owner).is_none() {
            let function = function_index(decoder.owner)?;
            if matches!(self.store.functions[function].contract, FullCallableContract::Ground(_)) {
                let expected = self.contract_type(decoder.function_return_type(function)?)?;
                self.budget.charge_clone(&error)?;
                self.budget.charge_clone(&expected)?;
                let boundary = super::PropagationBoundary::for_function(self.store, decoder.owner, &crate::sema::schemes::SchemeType::Ground((*expected).clone()), &mut self.budget)?;
                super::body_propagated_error_matches_owner(
                    self.store,
                    &crate::sema::schemes::SchemeType::Ground((*error).clone()),
                    &boundary,
                    &mut self.budget,
                )?;
            }
        }
        Ok(Some(error))
    }

    fn join_captured_errors(&mut self, errors: &[Type]) -> Result<Type, IrVerifyError> {
        for error in errors { self.budget.charge_clone(error)?; }
        Ok(Type::captured_error_type(errors.iter().cloned()))
    }

    fn captured_error_types(&mut self, decoder: &FullDecoder<'a>, capture: usize) -> Result<Arc<[Type]>, IrVerifyError> {
        let key = (decoder.owner, capture);
        if let Some(derived) = self.captured_errors.get(&key) {
            return match derived {
                CapturedErrorDerivation::Pending => Err(IrVerifyError::new("capture error provenance is cyclic")),
                CapturedErrorDerivation::Resolved(errors) => Ok(Arc::clone(errors)),
            };
        }
        self.budget.charge(1)?;
        self.captured_errors.insert(key, CapturedErrorDerivation::Pending);
        let errors = self.derive_captured_error_types(decoder, capture)?;
        self.budget.charge(errors.len())?;
        let errors: Arc<[Type]> = errors.into();
        self.captured_errors.insert(key, CapturedErrorDerivation::Resolved(Arc::clone(&errors)));
        Ok(errors)
    }

    fn derive_captured_error_types(&mut self, decoder: &FullDecoder<'a>, capture: usize) -> Result<Vec<Type>, IrVerifyError> {
        Ok(self.derive_captured_error_events(decoder, capture, &[])?.into_iter().map(|(_, error)| error).collect())
    }

    fn derive_captured_error_events(&mut self, decoder: &FullDecoder<'a>, capture: usize, transfers: &[usize]) -> Result<Vec<(usize, Type)>, IrVerifyError> {
        let mut events = Vec::new();
        let mut skipped = BTreeSet::new();
        let mut recorded_transfers = BTreeSet::new();
        let mut pending = vec![capture];
        while let Some(instruction) = pending.pop() {
            self.budget.charge(1)?;
            if skipped.contains(&instruction) || self.index.owners[&decoder.owner].unreachable_roots.contains(&instruction) { continue; }
            let mut errors = Vec::new();
            if self.store.tags[instruction] == FullTag::ExprPipeline {
                self.captured_pipeline_errors(decoder, instruction, &mut errors, &mut skipped)?;
            }
            if instruction != capture && !charged_capture_transfer_contains(transfers, instruction, &mut self.budget)? {
                if self.store.tags[instruction].kind() == schema::InstructionKind::Expr
                    && self.callee_error_flow(decoder, instruction)? == crate::sema::types::CalleeErrorFlow::Immediate
                { errors.push(Type::Error); }
                let result = match self.store.tags[instruction] {
                    FullTag::ExprTry => {
                        self.budget.charge(1)?;
                        if recorded_transfers.contains(&instruction) { None }
                        else if let Some(error) = self.prove_error_transfer(decoder, instruction)? {
                            self.budget.charge_clone(&error)?;
                            errors.push((*error).clone());
                            None
                        } else {
                            let schema::ExprView::Try { value, .. } = decoder.expression_view(instruction)? else { unreachable!() };
                            Some((value.raw() as usize, CapturedResultUse::Propagate))
                        }
                    }
                    FullTag::StmtExpr => {
                        let schema::StmtView::Expr { value, .. } = decoder.statement_view(instruction)? else { unreachable!() };
                        let value = value.raw() as usize;
                        if charged_capture_transfer_contains(transfers, value, &mut self.budget)? { None }
                        else if self.store.tags[value] == FullTag::ExprTry
                            && let Some(error) = self.prove_error_transfer(decoder, value)?
                        {
                            self.budget.charge_clone(&error)?;
                            errors.push((*error).clone());
                            self.budget.charge(1)?;
                            recorded_transfers.insert(value);
                            None
                        } else { Some((value, CapturedResultUse::Discard)) }
                    }
                    FullTag::StmtFor | FullTag::StmtForRecord => {
                        let (schema::StmtView::For { iter, .. } | schema::StmtView::ForRecord { iter, .. }) = decoder.statement_view(instruction)? else { unreachable!() };
                        Some((iter.raw() as usize, CapturedResultUse::Iterate))
                    }
                    FullTag::ExprRange => {
                        errors.push(Type::Error);
                        None
                    }
                    FullTag::StmtAssert => {
                        errors.push(Type::ErrorFamily(Type::assertion_error_family()));
                        None
                    }
                    FullTag::ExprFsFiles | FullTag::ExprFsWalk => {
                        let (schema::ExprView::FsFiles { result_wrapped, .. } | schema::ExprView::FsWalk { result_wrapped, .. }) = decoder.expression_view(instruction)? else { unreachable!() };
                        if !result_wrapped {
                            let op = if self.store.tags[instruction] == FullTag::ExprFsFiles { super::super::RuntimeOp::FsFiles } else { super::super::RuntimeOp::FsWalk };
                            let Type::Result(_, error) = FullDecoder::registered_api_operation_return_type(op)? else { return Err(IrVerifyError::new("filesystem stream has no canonical error contract")) };
                            errors.push(*error);
                            let source = self.need_context(decoder, instruction)?.ok_or_else(|| IrVerifyError::new("filesystem stream has no physical output contract"))?;
                            self.expression(decoder.owner, instruction, source, false);
                        }
                        None
                    }
                    FullTag::StmtCd => {
                        let Type::Result(_, error) = Type::core_command_result(crate::syntax::node::CoreCommand::Cd) else { unreachable!() };
                        errors.push(*error);
                        None
                    }
                    FullTag::ExprRunCapture => {
                        let schema::ExprView::RunCapture(run) = decoder.expression_view(instruction)? else { unreachable!() };
                        let (spliced, accepts) = self.run_capture_error_policy(run.operands)?;
                        if spliced || (crate::runtime::run::RunErrorBoundary::Capture { kind: run.kind, propagate: run.propagate }).propagates(accepts) {
                            let Type::Result(_, error) = Type::run_statement_result() else { unreachable!() };
                            errors.push(*error);
                        }
                        None
                    }
                    FullTag::ExprRunPipeline => {
                        let schema::ExprView::RunPipeline { mut segments, .. } = decoder.expression_view(instruction)? else { unreachable!() };
                        let mut head = None;
                        let mut spliced = false;
                        let mut accepts = false;
                        for segment in &mut segments {
                            let segment = segment?;
                            head.get_or_insert(segment.kind);
                            let (splice, accept) = self.run_capture_error_policy(segment.operands)?;
                            spliced |= splice;
                            accepts |= accept;
                        }
                        if spliced || head.is_some_and(|head| (crate::runtime::run::RunErrorBoundary::Pipeline { head }).propagates(accepts)) {
                            let Type::Result(_, error) = Type::run_statement_result() else { unreachable!() };
                            errors.push(*error);
                        }
                        None
                    }
                    FullTag::StmtRun => {
                        let schema::StmtView::Run { value, propagate_result } = decoder.statement_view(instruction)? else { unreachable!() };
                        if propagate_result {
                            if self.store.tags[value.raw() as usize] == FullTag::ExprRunCapture {
                                let Type::Result(_, error) = Type::run_statement_result() else { unreachable!() };
                                errors.push(*error);
                                None
                            } else { Some((value.raw() as usize, CapturedResultUse::Propagate)) }
                        } else { None }
                    }
                    FullTag::StmtPrint => {
                        let schema::StmtView::Print { propagate_result, .. } = decoder.statement_view(instruction)? else { unreachable!() };
                        if propagate_result { return Err(IrVerifyError::new("captured print propagation has no canonical error contract")); }
                        None
                    }
                    _ => None,
                };
                if let Some((value, use_kind)) = result {
                    let source = self.need_context(decoder, value)?.ok_or_else(|| IrVerifyError::new("captured propagation has no executable source contract"))?;
                    if let Type::Result(ok, error) = source.unvalidated() {
                        self.budget.charge_clone(error)?;
                        errors.push((**error).clone());
                        if use_kind == CapturedResultUse::Iterate && matches!(ok.unvalidated(), Type::Stream(_))
                            && self.stream_may_propagate_error(decoder, StreamProducer::ResultSuccess(value))?
                        { errors.push(Type::Error); }
                        self.expression(decoder.owner, value, source, false);
                    } else if use_kind == CapturedResultUse::Propagate || source.as_ref() == &Type::Any {
                        return Err(IrVerifyError::new("captured propagation source has no Result contract"));
                    } else if use_kind == CapturedResultUse::Iterate && matches!(source.unvalidated(), Type::Stream(_)) {
                        if self.stream_may_propagate_error(decoder, StreamProducer::Value(value))? { errors.push(Type::Error); }
                        self.expression(decoder.owner, value, source, false);
                    }
                }
            }
            events.extend(errors.into_iter().map(|error| (instruction, error)));
            let owner = &self.index.owners[&decoder.owner];
            let mut child = owner.children[instruction - owner.range.start];
            while child != IR_NONE {
                let next = child as usize;
                let captured = super::super::verify_effects::captures_child_error(decoder, instruction, next)?;
                let visit = if instruction == capture && self.store.tags[instruction] == FullTag::ExprRetry { captured }
                    else { instruction == capture || !captured };
                if visit { pending.push(next); }
                child = owner.siblings[next - owner.range.start];
            }
        }
        Ok(events)
    }

    fn verify_block_value<'view>(&mut self, decoder: &FullDecoder<'view>, mut body: super::super::decode::ListView<'view, super::super::BuildStmtId>, expected: Arc<Type>, returned: bool) -> Result<(), IrVerifyError> {
        let block = body.block_identity();
        if let Some(tail) = decoder.derive_checked_block_tail_expression(&mut body)? {
            self.expression(decoder.owner, tail, expected, returned);
            return Ok(());
        }
        if super::super::indexed_block_can_return_with_decoder(self.store, block, Some(decoder))? {
            if super::super::driver_owner_index(decoder.owner).is_none() {
                self.ground_function(function_index(decoder.owner)?)?;
            }
            return Ok(());
        }
        if Type::Unit.matches_expected(&expected) { return Ok(()); }
        Err(IrVerifyError::new("ground completing block produces Unit instead of its checked contract"))
    }

    fn verify_operands(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<(), IrVerifyError> {
        if !self.operand_expansions.insert((decoder.owner, instruction)) { return Ok(()); }
        let owner = &self.index.owners[&decoder.owner];
        let mut pending = Vec::new();
        let mut child = owner.children[instruction - owner.range.start];
        while child != IR_NONE { pending.push(child as usize); child = owner.siblings[child as usize - owner.range.start]; }
        while let Some(instruction) = pending.pop() {
            self.budget.charge(1)?;
            match self.store.tags[instruction].kind() {
                schema::InstructionKind::Expr => {
                    let (ty, fact) = self.operand_context(decoder, instruction)?.into_parts();
                    let ty = match ty {
                        Some(ty) => ty,
                        None => return Err(IrVerifyError::new("ground producer operand has no checked contract")),
                    };
                    self.expression_with_fact(decoder.owner, instruction, ty, false, fact);
                }
                schema::InstructionKind::Int | schema::InstructionKind::Bool => {
                    if !self.operand_expansions.insert((decoder.owner, instruction)) { continue; }
                    self.verify_scalar(decoder, instruction)?;
                    let mut child = owner.children[instruction - owner.range.start];
                    while child != IR_NONE { pending.push(child as usize); child = owner.siblings[child as usize - owner.range.start]; }
                }
                _ => return Err(IrVerifyError::new("ground producer operand has no value transfer contract")),
            }
        }
        Ok(())
    }

    fn verify_condition(&mut self, owner: u32, instruction: usize, role: ConditionRole) -> Result<(), IrVerifyError> {
        if !self.conditions.insert((owner, instruction, role)) { return Ok(()); }
        self.budget.charge(1)?;
        let decoder = self.owner(owner)?;
        if self.store.tags[instruction].kind() == schema::InstructionKind::Bool {
            self.verify_scalar(&decoder, instruction)?;
            self.verify_operands(&decoder, instruction)
        } else {
            let ty = self.need_context(&decoder, instruction)?.ok_or_else(|| IrVerifyError::new(format!("condition producer has no checked contract at owner {owner}, instruction {instruction} {:?}", self.store.tags[instruction])))?;
            if !role.accepts(&ty) { return Err(IrVerifyError::new("condition producer disagrees with its control role")); }
            self.expression(owner, instruction, ty, false);
            Ok(())
        }
    }

    fn operand_context(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<ProducerOperandContext, IrVerifyError> {
        let (ty, binding) = self.declared_type_evidence(decoder, instruction)?;
        Ok(match ty {
            Some(ty) => {
                let read = self.declared_read_type(decoder, instruction, Arc::clone(&ty))?;
                if read != ty { ProducerOperandContext::Contextual(Some(read)) }
                else { ProducerOperandContext::Declared(DeclaredProducerFact { owner: decoder.owner, instruction, ty, binding }) }
            }
            None => ProducerOperandContext::Contextual(self.contextual_producer_type(decoder, instruction)?),
        })
    }

    fn verify_native_call(&mut self, decoder: &FullDecoder<'a>, instruction: usize, op: super::super::RuntimeOp, call_plan: Option<&crate::runtime::eval::ModuleCallPlan>, arguments: Vec<Option<usize>>, expected: &Type) -> Result<(), IrVerifyError> {
        let actuals = (0..arguments.len()).map(|_| OnceCell::new()).collect::<Vec<OnceCell<ProducerOperandContext>>>();
        let (result, parameters, effects, pure) = if let Some(bridge) = crate::stdlib::bridge_contract(op) {
            let effects = bridge.sig.effects.concrete()
                .map_err(|_| IrVerifyError::new("bridge effect contract is unresolved"))?;
            (*bridge.sig.return_ty, bridge.sig.params.into_iter().map(|parameter| { let domain = NativeParameterDomain::declared(&parameter.ty); (parameter.ty, parameter.defaulted, domain) }).collect::<Vec<_>>(), effects.unwrap_or_default(), bridge.kind == crate::sema::types::CallableKind::Pure)
        } else if let Some(crate::runtime::eval::ModuleCallPlan::CommandArguments(plan)) = call_plan {
            (plan.signature.return_ty.clone(), plan.signature.params.iter().enumerate().map(|(slot, parameter)| {
                let ty = if plan.word_conversions.iter().any(|conversion| conversion.slot == slot) {
                    Type::Any
                } else { parameter.ty.clone() };
                (ty, parameter.defaulted, NativeParameterDomain::declared(&parameter.ty))
            }).collect::<Vec<_>>(), plan.signature.effects.to_vec(), plan.signature.pure)
        } else {
            let mut selected: Option<(&'static crate::modules::signature::ModuleFnSig, std::borrow::Cow<'static, crate::modules::signature::ModuleFnSig>)> = None;
            for signature in crate::modules::signature::native_static_operation_signatures(op) {
                self.budget.charge(1 + signature.params.len())?;
                if arguments.len() > signature.params.len() { continue; }
                let Some(specialized) = self.native_signature(&decoder, signature, &arguments, Some(&expected))? else { continue };
                let original = signature;
                let signature = &specialized;
                let mut fits = true;
                for (position, parameter) in signature.params.iter().enumerate() {
                    match arguments.get(position).copied().flatten() {
                        Some(argument) if parameter.ty != Type::Any => {
                            if actuals[position].get().is_none() {
                                let context = self.operand_context(decoder, argument)?;
                                if actuals[position].set(context).is_err() { return Err(IrVerifyError::new("native argument context was already derived")); }
                            }
                            if let Some(actual) = actuals[position].get().unwrap().ty()
                                && !actual.matches_expected(&parameter.ty)
                                && !NativeParameterDomain::declared(&original.params[position].ty).accepts(&parameter.ty, actual)
                                && !self.contextual_list_argument(&decoder, argument, &parameter.ty)?
                            {
                                fits = false;
                                break;
                            }
                        }
                        Some(_) => {}
                        None if parameter.defaulted => {}
                        None => { fits = false; break; }
                    }
                }
                if !fits { continue; }
                if let Some((previous_original, previous)) = &selected {
                    if previous.return_ty != signature.return_ty || previous.pure != signature.pure
                        || previous.effects != signature.effects || previous.params.len() != signature.params.len()
                        || previous.params.iter().zip(&signature.params).any(|(left, right)| left.ty != right.ty || left.defaulted != right.defaulted)
                        || previous_original.params.iter().zip(&original.params).any(|(left, right)| NativeParameterDomain::declared(&left.ty) != NativeParameterDomain::declared(&right.ty))
                    {
                        return Err(IrVerifyError::new("ground native operation has ambiguous parameter contracts"));
                    }
                } else { selected = Some((original, specialized)); }
            }
            let (original, signature) = selected.ok_or_else(|| IrVerifyError::new("ground native operation arguments have no matching contract"))?;
            let result = match call_plan {
                Some(crate::runtime::eval::ModuleCallPlan::SignatureCli(descriptor)) => descriptor.return_type(op == super::super::RuntimeOp::CliParseFull),
                _ => signature.return_ty.clone(),
            };
            (result, signature.params.iter().zip(&original.params).map(|(parameter, declared)| (parameter.ty.clone(), parameter.defaulted, NativeParameterDomain::declared(&declared.ty))).collect::<Vec<_>>(), signature.effects.to_vec(), signature.pure)
        };
        self.require_callable_effects(&decoder, instruction, Some(&effects), pure)?;
        if !result.matches_expected(&expected) { return Err(IrVerifyError::new("ground native result disagrees with its operation contract")); }
        if arguments.len() > parameters.len() { return Err(IrVerifyError::new("ground native operation has excess arguments")); }
        let mut actuals = actuals.into_iter();
        for (position, (parameter, defaulted, domain)) in parameters.into_iter().enumerate() {
            let actual = actuals.next().and_then(OnceCell::into_inner);
            match arguments.get(position).copied().flatten() {
                Some(argument) => {
                    self.contextual_argument(decoder, argument, &parameter)?;
                    let (actual, fact) = if parameter == Type::Any { (None, None) } else {
                        match actual {
                            Some(actual) => actual.into_parts(),
                            None => self.operand_context(decoder, argument)?.into_parts(),
                        }
                    };
                    let expected = if parameter == Type::Any { Arc::new(parameter) }
                    else if let Some(actual) = actual {
                        if actual.matches_expected(&parameter) || domain.accepts(&parameter, &actual) {
                            actual
                        } else if self.contextual_list_argument(&decoder, argument, &parameter)? {
                            Arc::new(parameter)
                        } else {
                            return Err(IrVerifyError::new("ground native argument disagrees with its operation contract"));
                        }
                    } else { Arc::new(parameter) };
                    self.expression_with_fact(decoder.owner, argument, expected, false, fact);
                }
                None if defaulted => {}
                None => return Err(IrVerifyError::new("ground native operation omits a required argument")),
            }
        }
        Ok(())
    }

    fn contextual_match_binding(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<Option<(ProducerBinding, usize)>, IrVerifyError> {
        self.budget.charge(1)?;
        if self.store.tags[instruction] != FullTag::ExprMatch { return Ok(None); }
        let schema::ExprView::MatchExpr { value, mut arms, .. } = decoder.expression_view(instruction)? else { unreachable!("contextual match tag has another view") };
        if !matches!(decoder.expression_view(value.raw() as usize)?, schema::ExprView::List(_) | schema::ExprView::ListBuild(_) | schema::ExprView::MapLiteral(_) | schema::ExprView::EmptyMap) { return Ok(None); }
        let Some((pattern, guard, body)) = arms.next().transpose()? else { return Ok(None) };
        if guard.is_some() || arms.next().transpose()?.is_some() { return Ok(None); }
        let schema::PatternView::Bind { slot } = decoder.checked_pattern_view(pattern.raw())? else { return Ok(None) };
        let binding = ProducerBinding::ScopedDeclaration(body.raw() as usize, slot);
        let owner = &self.index.owners[&decoder.owner];
        let Some(source) = owner.pattern_sources.get(&binding) else { return Ok(None) };
        if source.source != value.raw() as usize || source.pattern != pattern.raw() || !source.preceding.is_empty()
            || owner.parents[body.raw() as usize - owner.range.start] != Some(instruction)
            || owner.writers.get(&binding).is_some_and(|writers| !writers.is_empty())
            || matches!(owner.header_binding(self.store, decoder.owner, slot)?, Some(ProducerBinding::HeaderParameter(_) | ProducerBinding::HeaderCapture(_)))
        { return Ok(None); }
        Ok(Some((binding, source.source)))
    }

    fn fresh_contextual_argument_source(&mut self, decoder: &FullDecoder<'a>, argument: usize) -> Result<Option<usize>, IrVerifyError> {
        self.budget.charge(1)?;
        let key = (decoder.owner, argument);
        if let Some(source) = self.fresh_lists.get(&key) { return Ok(*source); }
        let source = match decoder.expression_view(argument)? {
            schema::ExprView::List(_) | schema::ExprView::ListBuild(_) | schema::ExprView::MapLiteral(_) | schema::ExprView::EmptyMap => Some(argument),
            schema::ExprView::Param(slot) => {
                let binding = self.binding(decoder.owner, argument, slot)?;
                match binding {
                    ProducerBinding::ScopedDeclaration(result, _) => {
                        let owner = &self.index.owners[&decoder.owner];
                        match owner.parents[result - owner.range.start] {
                            Some(parent) => self.contextual_match_binding(decoder, parent)?.filter(|(actual, _)| *actual == binding).map(|(_, source)| source),
                            None => None,
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        self.fresh_lists.insert(key, source);
        Ok(source)
    }

    fn fresh_contextual_list_source(&mut self, decoder: &FullDecoder<'a>, argument: usize) -> Result<Option<usize>, IrVerifyError> {
        Ok(self.fresh_contextual_argument_source(decoder, argument)?.filter(|&source| matches!(self.store.tags[source], FullTag::ExprList | FullTag::ExprListBuild)))
    }

    fn contextual_argument(&mut self, decoder: &FullDecoder<'a>, argument: usize, expected: &Type) -> Result<bool, IrVerifyError> {
        if !matches!(expected, Type::List(_) | Type::Map(_, _)) { return Ok(false); }
        let schema::ExprView::Param(slot) = decoder.expression_view(argument)? else { return Ok(false) };
        let Some(source) = self.fresh_contextual_argument_source(decoder, argument)? else { return Ok(false) };
        if self.declared_type(decoder, source)?.is_some() { return Ok(false); }
        if !matches!((self.store.tags[source], expected),
            (FullTag::ExprList | FullTag::ExprListBuild, Type::List(_)) | (FullTag::ExprMapLiteral | FullTag::ExprEmptyMap, Type::Map(_, _)))
        { return Err(IrVerifyError::new("contextual aggregate argument has an incompatible consumer contract")); }
        ensure_substitution_type(expected)?;
        let binding = self.binding(decoder.owner, argument, slot)?;
        let key = (decoder.owner, binding);
        self.deferred_sources.insert(key, source);
        match self.contextual_sources.get(&key) {
            Some((actual_source, actual)) if *actual_source != source || actual.as_ref() != expected => return Err(IrVerifyError::new("contextual aggregate argument has conflicting consumer contracts")),
            Some(_) => {},
            None => {
                self.budget.charge_clone(expected)?;
                self.contextual_sources.insert(key, (source, Arc::new(expected.clone())));
            },
        }
        Ok(true)
    }

    fn record_field_writers(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<RecordProducerWriters, IrVerifyError> {
        let schema::ExprView::Record(mut fields) = decoder.expression_view(instruction)? else { return Err(IrVerifyError::new("record producer has no physical record")) };
        self.budget.charge(fields.len())?;
        let writers = decoder.checked_record_field_writers(&mut fields, |source| self.need_context(decoder, source)?.ok_or_else(|| IrVerifyError::new("record spread has no producer contract")))?;
        let schema::ExprView::Record(mut fields) = decoder.expression_view(instruction)? else { unreachable!() };
        self.budget.charge(fields.len())?;
        let mut unresolved = BTreeSet::new();
        let mut types = BTreeMap::new();
        for entry in decoder.checked_record_entries(&mut fields)? {
            let (value, effective) = match entry {
                super::super::CheckedRecordEntry::Field(name, value) => (value, matches!(writers.get(&name), Some(super::super::CheckedRecordFieldWriter::Value(actual)) if *actual == value)),
                super::super::CheckedRecordEntry::Spread(value) => (value, false),
            };
            let (actual, fact) = self.operand_context(decoder, value)?.into_parts();
            match actual {
                Some(actual) => {
                    types.insert(value, Arc::clone(&actual));
                    self.expression_with_fact(decoder.owner, value, actual, false, fact);
                }
                None if effective => { unresolved.insert(value); },
                None => return Err(IrVerifyError::new("overwritten record entry has no producer contract")),
            }
        }
        Ok(RecordProducerWriters { writers, unresolved, types })
    }

    fn checked_record_arguments(&mut self, decoder: &FullDecoder<'a>, instruction: usize, checked: &Type) -> Result<bool, IrVerifyError> {
        if self.store.tags[instruction] != FullTag::ExprRecord { return Ok(false); }
        let record = match checked.unvalidated() {
            Type::Record(_) => checked.unvalidated(),
            Type::Optional(payload) if matches!(payload.as_ref(), Type::Record(_)) => payload,
            _ => return Ok(false),
        };
        let Type::Record(expected) = record else { unreachable!() };
        let RecordProducerWriters { writers, mut unresolved, types } = self.record_field_writers(decoder, instruction)?;
        for name in expected.keys() {
            self.budget.charge(1)?;
            if !writers.contains_key(name) { return Err(IrVerifyError::new("checked record producer omits a required field")); }
        }
        for (name, writer) in writers {
            self.budget.charge(1)?;
            match writer {
                super::super::CheckedRecordFieldWriter::Value(value) => {
                    let contextual = match expected.get(&name) {
                        Some(ty) => self.contextual_argument(decoder, value, ty)?,
                        None => false,
                    };
                    if let (Some(actual), Some(expected)) = (types.get(&value), expected.get(&name)) {
                        if !self.checked_record_field_fits(decoder, value, actual, expected)? { return Err(IrVerifyError::new("checked record field disagrees with its contract")); }
                    }
                    if unresolved.remove(&value) {
                        let ty = expected.get(&name).ok_or_else(|| IrVerifyError::new("checked record field has no producer contract"))?;
                        if !contextual && !matches!(self.store.tags[value], FullTag::ExprList | FullTag::ExprListBuild | FullTag::ExprMapLiteral | FullTag::ExprEmptyMap | FullTag::ExprRecord | FullTag::ExprMethod) {
                            return Err(IrVerifyError::new("checked record field has no producer contract"));
                        }
                        self.budget.charge_clone(ty)?;
                        self.expression(decoder.owner, value, Arc::new(ty.clone()), false);
                    }
                }
                super::super::CheckedRecordFieldWriter::Spread { instruction, fields } => {
                    let Type::Record(actual) = fields.unvalidated() else { unreachable!() };
                    if let Some(expected) = expected.get(&name) {
                        let expected = match expected { Type::Validated(validated) => validated.base(), _ => expected };
                        if !decoder.checked_type_fits(&actual[&name], expected) && !self.checked_storage_record_fields_fit(decoder, &actual[&name], expected, 0)? { return Err(IrVerifyError::new("checked record spread field disagrees with its contract")); }
                    }
                    self.expression(decoder.owner, instruction, fields, false);
                }
            }
        }
        if !unresolved.is_empty() { return Err(IrVerifyError::new("checked record field has no producer contract")); }
        Ok(true)
    }

    fn checked_optional_payload_source(source: &Type, checked: &Type) -> bool {
        matches!(source, Type::Optional(payload) if payload.as_ref() == checked)
    }

    fn checked_record_field_fits(&mut self, decoder: &FullDecoder<'a>, value: usize, actual: &Type, expected: &Type) -> Result<bool, IrVerifyError> {
        let expected = match expected { Type::Validated(validated) => validated.base(), _ => expected };
        if decoder.checked_type_fits(actual, expected) { return Ok(true); }
        if matches!((actual, expected), (Type::Record(_), Type::Record(_))) && self.checked_storage_record_fields_fit(decoder, actual, expected, 0)? { return Ok(true); }
        if let Type::Optional(payload) = actual
            && decoder.checked_type_fits(payload, expected)
        {
            if self.optional_read_is_narrowed(decoder, value, actual)? { return Ok(true); }
            if let Some(target) = decoder.checked_binding_path(value, &mut |amount| self.budget.charge(amount))? {
                let binding = self.binding(decoder.owner, value, target.0)?;
                let result = self.index.owners[&decoder.owner].optional_path_read_is_narrowed(decoder, value, &target, payload, binding, &mut |amount| self.budget.charge(amount))?;
                if result { return Ok(true); }
            }
        }
        Ok(false)
    }

    fn checked_storage_record_fields_fit(&mut self, decoder: &FullDecoder<'a>, actual: &Type, expected: &Type, depth: usize) -> Result<bool, IrVerifyError> {
        if depth > 128 { return Err(IrVerifyError::new("checked storage record nesting is too deep")); }
        self.budget.charge(1)?;
        let expected = match expected {
            Type::Validated(validated) if !matches!(validated.validation(), crate::sema::validated::Validation::Nominal(_)) => validated.base(),
            _ => expected,
        };
        if decoder.checked_type_fits(actual, expected) { return Ok(true); }
        let (Type::Record(actual), Type::Record(expected)) = (actual, expected) else { return Ok(false) };
        for (name, expected) in expected.iter() {
            self.budget.charge(1)?;
            let Some(actual) = actual.get(name) else { return Ok(false) };
            if !self.checked_storage_record_fields_fit(decoder, actual, expected, depth + 1)? { return Ok(false); }
        }
        Ok(true)
    }

    fn contextual_list_argument(&mut self, decoder: &FullDecoder<'a>, argument: usize, expected: &Type) -> Result<bool, IrVerifyError> {
        if !matches!(expected, Type::List(_)) { return Ok(false); }
        Ok(self.fresh_contextual_list_source(decoder, argument)?.is_some())
    }

    fn native_signature(&mut self, decoder: &FullDecoder<'a>, signature: &'static crate::modules::signature::ModuleFnSig, arguments: &[Option<usize>], expected: Option<&Type>) -> Result<Option<std::borrow::Cow<'static, crate::modules::signature::ModuleFnSig>>, IrVerifyError> {
        if signature.op == super::super::RuntimeOp::EnvSet {
            let Some(Some(value)) = arguments.get(1) else { return Ok(None) };
            let actual = self.need_context(decoder, *value)?.ok_or_else(|| IrVerifyError::new("environment assignment value has no closed conversion domain"))?;
            self.budget.charge_clone(&actual)?;
            if ensure_closed_type(&actual).is_err() || !actual.can_be_env_value() {
                return Err(IrVerifyError::new("environment assignment value has no closed conversion domain"));
            }
        }
        if !signature.return_ty.contains_builtin_parameter() && !signature.params.iter().any(|parameter| parameter.ty.contains_builtin_parameter()) { return Ok(Some(std::borrow::Cow::Borrowed(signature))); }
        self.budget.charge_clone(&signature.return_ty)?;
        for parameter in &signature.params { self.budget.charge_clone(&parameter.ty)?; }
        let actual = arguments.iter().map(|argument| {
            let Some(argument) = argument else { return Ok(None) };
            let Some(ty) = self.need_context(decoder, *argument)? else { return Ok(None) };
            self.budget.charge_clone(&ty)?;
            Ok(Some((*ty).clone()))
        }).collect::<Result<Vec<_>, IrVerifyError>>()?;
        if let Some(expected) = expected { self.budget.charge_clone(expected)?; }
        crate::sema::builtin_templates::checked_module_signature(signature, &actual, expected)
            .map(|signature| signature.map(std::borrow::Cow::Owned))
            .map_err(|_| IrVerifyError::new("native template instance disagrees with its checked contract"))
    }

    fn verify_process_command_contract(&mut self, decoder: &FullDecoder<'a>, instruction: usize, expected: &Type) -> Result<(), IrVerifyError> {
        let op = super::super::RuntimeOp::ProcessCommandArgv;
        let result = FullDecoder::derive_registered_api_operation_return_type(op)?.ok_or_else(|| IrVerifyError::new("command constructor has no native result contract"))?;
        if !result.matches_expected(expected) { return Err(IrVerifyError::new("command result disagrees with its native contract")); }
        let Some(crate::modules::signature::NativeEffectRule::Uniform { effects, pure }) = crate::modules::signature::native_static_operation_effect_rule(op) else {
            return Err(IrVerifyError::new("command constructor has no uniform native effect contract"));
        };
        self.require_callable_effects(decoder, instruction, Some(effects), pure)
    }

    fn verify_spawn_result(&self, expected: &Type) -> Result<(), IrVerifyError> {
        let result = Type::Result(Box::new(Type::ProcessHandle), Box::new(Type::ProcessError));
        if !result.matches_expected(expected) { return Err(IrVerifyError::new("spawn result disagrees with its process handle contract")); }
        Ok(())
    }

    fn process_wait_contract(&mut self, decoder: &FullDecoder<'a>, target: usize) -> Result<(Arc<Type>, Type), IrVerifyError> {
        let target_type = match self.need_context(decoder, target)? {
            Some(ty) => ty,
            None if matches!(decoder.expression_view(target)?, schema::ExprView::List(_) | schema::ExprView::ListBuild(_)) => Arc::new(Type::List(Box::new(Type::ProcessHandle))),
            None => return Err(IrVerifyError::new("wait target has no executable handle contract")),
        };
        self.budget.charge_clone(&target_type)?;
        ensure_substitution_type(&target_type)?;
        let payload = match target_type.unvalidated() {
            Type::ProcessHandle => Type::Status,
            Type::List(item) if item.unvalidated() == &Type::ProcessHandle => Type::List(Box::new(Type::Status)),
            _ => return Err(IrVerifyError::new("wait target is neither ProcessHandle nor List[ProcessHandle]")),
        };
        Ok((target_type, Type::Result(Box::new(payload), Box::new(Type::ProcessError))))
    }

    fn process_option_contract(&mut self, name: &str, actual: Option<&Type>) -> Result<Arc<Type>, IrVerifyError> {
        process_option_contract(name, actual, &mut self.budget)
    }

    fn verify_process_word(&mut self, decoder: &FullDecoder<'a>, argument: super::super::decode::RunArgView<'_>, role: ProcessWordRole) -> Result<bool, IrVerifyError> {
        use super::super::decode::RunArgKindView as A;
        self.budget.charge(1)?;
        let (value, spliced, optional_splice) = match argument.kind {
            A::Single(value) => (value.raw() as usize, false, false),
            A::SingleOrSplice(value) => (value.raw() as usize, false, true),
            A::Splice(value) => (value.raw() as usize, true, false),
        };
        let ty = match self.need_context(decoder, value)? {
            Some(ty) => ty,
            None if (spliced || optional_splice) && matches!(decoder.expression_view(value)?, schema::ExprView::List(_) | schema::ExprView::ListBuild(_)) => Arc::new(Type::List(Box::new(Type::Str))),
            None => return Err(IrVerifyError::new("process word has no executable conversion contract")),
        };
        self.budget.charge_clone(&ty)?;
        ensure_substitution_type(&ty)?;
        let bytes_input = process_word_type(&ty, spliced, optional_splice, role)?;
        self.expression(decoder.owner, value, ty, false);
        Ok(bytes_input)
    }

    fn verify_process_run_operands(&mut self, decoder: &FullDecoder<'a>, operands: super::super::decode::RunOperandsView<'_>) -> Result<bool, IrVerifyError> {
        use super::super::decode::{RunOptionView as O, RunTailOperandView as T};
        self.budget.charge(1 + operands.options.len() + operands.env.len() + operands.tail.len())?;
        let mut accepts = false;
        for option in operands.options {
            let (name, value) = match option? {
                O::Timeout(value) => ("timeout", value),
                O::CpuMax(value) => ("cpu_max", value),
                O::Accept(value) => { accepts = true; ("accept", value) },
            };
            let expected = self.process_option_contract(name, None)?;
            self.expression(decoder.owner, value.raw() as usize, expected, false);
        }
        for assignment in operands.env { self.verify_process_word(decoder, assignment?.value, ProcessWordRole::Environment)?; }
        self.verify_process_word(decoder, operands.target, ProcessWordRole::Argument)?;
        let mut stdin_sources = 0usize;
        let mut bytes_input = false;
        for operand in operands.tail {
            match operand? {
                T::Argument(argument) => { self.verify_process_word(decoder, argument, ProcessWordRole::Argument)?; }
                T::Redirection(redirection) => {
                    stdin_sources += usize::from(matches!(redirection.kind, crate::syntax::node::RedirectionKind::StdinRead | crate::syntax::node::RedirectionKind::StdinDup));
                    bytes_input |= self.verify_process_word(decoder, redirection.target, ProcessWordRole::Redirection(redirection.kind))?;
                }
            }
        }
        if bytes_input && stdin_sources > 1 { return Err(IrVerifyError::new("process Bytes input competes with another stdin source")); }
        Ok(accepts)
    }

    fn verify_process_command_builder(&mut self, decoder: &FullDecoder<'a>, instruction: usize, entries: super::super::decode::ListView<'_, crate::runtime::eval::LoweredProcessCommandBuilderEntry>, expected: &Type) -> Result<(), IrVerifyError> {
        use super::super::decode::ProcessCommandBuilderEntryView as E;
        self.verify_process_command_contract(decoder, instruction, expected)?;
        if !self.operand_expansions.insert((decoder.owner, instruction)) { return Ok(()); }
        self.budget.charge(1 + entries.len())?;
        let mut fields = BTreeSet::new();
        let mut runs = 0usize;
        let mut field_accepts = false;
        let mut run_accepts = false;
        for entry in entries {
            match entry? {
                E::Run { mut operands, .. } => {
                    runs += 1;
                    let position = operands.tail.position();
                    for operand in &mut operands.tail {
                        self.budget.charge(1)?;
                        if matches!(operand?, super::super::decode::RunTailOperandView::Redirection(_)) {
                            return Err(IrVerifyError::new("command builder run contains a redirection"));
                        }
                    }
                    operands.tail.resume(position)?;
                    run_accepts |= self.verify_process_run_operands(decoder, operands)?;
                }
                E::Field { name, value, .. } => {
                    if !fields.insert(name) { return Err(IrVerifyError::new("command builder repeats an option field")); }
                    let name = name.as_str();
                    if matches!(name.as_str(), "target" | "argv") { return Err(IrVerifyError::new("command builder has an unknown option field")); }
                    field_accepts |= name.as_str() == "accept";
                    let actual = self.need_context(decoder, value.raw() as usize)?;
                    let parameter = self.process_option_contract(name.as_str(), actual.as_deref())?;
                    let ty = actual.unwrap_or(parameter);
                    self.budget.charge_clone(&ty)?;
                    ensure_substitution_type(&ty)?;
                    self.expression(decoder.owner, value.raw() as usize, ty, false);
                }
            }
        }
        if runs != 1 { return Err(IrVerifyError::new("command builder does not contain exactly one run entry")); }
        if field_accepts && run_accepts { return Err(IrVerifyError::new("command builder repeats its completion policy")); }
        Ok(())
    }

    fn verify_fresh_process_word_list(&mut self, decoder: &FullDecoder<'a>, source: usize, expected: Arc<Type>) -> Result<(), IrVerifyError> {
        let mut pending = vec![source];
        while let Some(source) = pending.pop() {
            self.budget.charge(1)?;
            let mut elements = Vec::new();
            match decoder.expression_view(source)? {
                schema::ExprView::List(mut values) => {
                    for value in &mut values { elements.push((false, value?.raw() as usize)); }
                }
                schema::ExprView::ListBuild(mut values) => {
                    for value in &mut values { let (splice, value, _) = value?; elements.push((splice, value.raw() as usize)); }
                }
                _ => return Err(IrVerifyError::new("contextual process words have no fresh list producer")),
            }
            for (splice, value) in elements {
                self.budget.charge(1)?;
                if splice && let Some(source) = self.fresh_contextual_list_source(decoder, value)? {
                    pending.push(source);
                    continue;
                }
                let ty = self.need_context(decoder, value)?.ok_or_else(|| IrVerifyError::new("command argv element has no concrete word contract"))?;
                ensure_closed_type(&ty)?;
                let word = match ty.unvalidated() {
                    Type::List(item) if splice => item.as_ref(),
                    _ if !splice => ty.as_ref(),
                    _ => return Err(IrVerifyError::new("command argv splice is not a word list")),
                };
                if word.contains_any() || !word.can_be_process_command_argv_item() {
                    return Err(IrVerifyError::new("command argv element is not a concrete process word"));
                }
            }
            self.expression(decoder.owner, source, Arc::clone(&expected), false);
        }
        Ok(())
    }

    fn verify_process_command_argv(&mut self, decoder: &FullDecoder<'a>, instruction: usize, command: super::super::decode::ProcessCommandArgvView<'_>, expected: &Type) -> Result<(), IrVerifyError> {
        self.verify_process_command_contract(decoder, instruction, expected)?;
        if !self.operand_expansions.insert((decoder.owner, instruction)) { return Ok(()); }
        let arguments = [Some(command.target), Some(command.argv), command.cwd, command.env, command.stdin,
            command.stdout, command.stderr, command.stdout_append, command.stderr_append, command.timeout,
            command.detach, command.new_session, command.ignore_hup, command.cpu_max, command.accept, command.same_group];
        let actual = arguments.iter().map(|argument| match argument {
            Some(argument) => self.need_context(decoder, argument.raw() as usize),
            None => Ok(None),
        }).collect::<Result<Vec<_>, _>>()?;
        if let Some(actual) = &actual[1] {
            self.budget.charge_clone(actual)?;
            ensure_closed_type(actual)?;
        }
        let fresh_argv = if matches!(&actual[1], Some(actual) if matches!(actual.unvalidated(), Type::List(item) if item.can_be_process_command_argv_item())) {
            None
        } else { self.fresh_contextual_list_source(decoder, command.argv.raw() as usize)? };
        let argv = match &actual[1] {
            Some(actual) if matches!(actual.unvalidated(), Type::List(item) if item.can_be_process_command_argv_item()) => Arc::clone(actual),
            _ if fresh_argv.is_some() => {
                let word = Type::process_command_argv_item_type();
                self.budget.charge_clone(word)?;
                Arc::new(Type::List(Box::new(word.clone())))
            }
            _ => return Err(IrVerifyError::new("command argv has no concrete word list contract")),
        };
        if let Some(source) = fresh_argv {
            self.verify_fresh_process_word_list(decoder, source, Arc::clone(&argv))?;
        }
        let mut selected: Option<&crate::modules::signature::ModuleFnSig> = None;
        for signature in crate::modules::signature::native_process_command_option_signatures() {
            self.budget.charge(1)?;
            if signature.params.len() != arguments.len() { continue; }
            let mut fits = true;
            let parameters = signature.params[..1].iter().enumerate().chain(
                signature.params[2..].iter().enumerate().map(|(position, parameter)| (position + 2, parameter)));
            for (position, parameter) in parameters {
                self.budget.charge(1)?;
                match arguments[position] {
                    Some(_) => {
                        if let Some(actual) = actual[position].as_ref() {
                            self.budget.charge_clone(actual)?;
                            if !actual.matches_expected(&parameter.ty) { fits = false; break; }
                        }
                    }
                    None if parameter.defaulted => {}
                    None => { fits = false; break; }
                }
            }
            if !fits { continue; }
            if selected.replace(signature).is_some() {
                return Err(IrVerifyError::new("command argv has ambiguous native option contracts"));
            }
        }
        let signature = selected.ok_or_else(|| IrVerifyError::new("command operands disagree with their native contract"))?;
        for (position, argument) in arguments.into_iter().enumerate() {
            let Some(argument) = argument else { continue };
            let ty = if position == 1 { Arc::clone(&argv) } else {
                match actual[position].as_ref() {
                    Some(actual) => Arc::clone(actual),
                    None => { self.budget.charge_clone(&signature.params[position].ty)?; Arc::new(signature.params[position].ty.clone()) },
                }
            };
            self.expression(decoder.owner, argument.raw() as usize, ty, false);
        }
        Ok(())
    }

    fn checked_fresh_list_method_receiver(&mut self, decoder: &FullDecoder<'a>, instruction: usize, expected: &Type) -> Result<Option<Arc<Type>>, IrVerifyError> {
        let Type::Set(item) = expected else { return Ok(None) };
        let schema::ExprView::Method { receiver, name, args, .. } = decoder.expression_view(instruction)? else { return Ok(None) };
        let receiver = receiver.raw() as usize;
        if !matches!(self.store.tags[receiver], FullTag::ExprList | FullTag::ExprListBuild)
            || self.declared_type(decoder, receiver)?.is_some()
        { return Ok(None); }
        self.budget.charge(1)?;
        let empty = matches!(decoder.expression_view(receiver)?, schema::ExprView::List(items) if items.len() == 0);
        if !empty {
            let Some(parent) = decoder.instruction_parent_of(instruction)? else { return Ok(None) };
            let schema::ExprView::CheckedValue { check: ValueCheckView::Ground(check), value, .. } = decoder.expression_view(parent)? else { return Ok(None) };
            if value.raw() as usize != instruction || check.schema.is_some() || self.contract_type(check.ty.id)?.as_ref() != expected { return Ok(None); }
        }
        self.budget.charge(args.len())?;
        let arguments = args.map(|argument| argument.map(|argument| argument.raw() as usize)).collect::<Result<Vec<_>, _>>()?;
        self.budget.charge_clone(item)?;
        let receiver_type = Arc::new(Type::List(item.clone()));
        let result = decoder.checked_method_return_type(&receiver_type, name, &arguments)?;
        if result.as_ref() != Some(expected) { return Ok(None); }
        Ok(Some(receiver_type))
    }

    fn ground_call_argument_ancestor(&mut self, owner: u32, instruction: usize) -> Result<Option<(usize, Arc<Type>)>, IrVerifyError> {
        let decoder = self.owner(owner)?;
        let mut child = instruction;
        let mut parent = decoder.instruction_parent_of(instruction)?;
        let mut remaining = decoder.instruction_range.len();
        while let Some(ancestor) = parent {
            if !decoder.instruction_range.contains(&ancestor) || remaining == 0 {
                return Err(IrVerifyError::new("ground call argument parent chain exceeds its owner range"));
            }
            remaining -= 1;
            self.budget.charge(1)?;
            if decoder.store.tags[ancestor].kind() != schema::InstructionKind::Expr { return Ok(None); }
            if matches!(decoder.store.tags[ancestor], FullTag::ExprCall | FullTag::ExprDirectPureCall) {
                let (function, arguments) = call_parts(&decoder, ancestor)?;
                self.budget.charge(arguments.len())?;
                if arguments.iter().any(|argument| matches!(argument, ProducerArgument::Single(value) | ProducerArgument::Splice(value) if *value == child))
                    && let FullCallableContract::Ground(signature) = self.store.functions[function].contract
                {
                    let return_type = self.store.semantic.signature_return_type(signature)?;
                    let expected = self.contract_type(return_type)?;
                    return Ok(Some((ancestor, expected)));
                }
            }
            child = ancestor;
            parent = decoder.instruction_parent_of(ancestor)?;
        }
        Ok(None)
    }

    fn checked_empty_set_module_call(&mut self, decoder: &FullDecoder<'a>, instruction: usize, expected: &Type) -> Result<bool, IrVerifyError> {
        if !matches!(expected.unvalidated(), Type::Set(_)) { return Ok(false); }
        let schema::ExprView::ModuleCall { op: super::super::RuntimeOp::SetEmpty, call_plan: None, args, .. } = decoder.expression_view(instruction)? else { return Ok(false) };
        let arguments = args.map(|argument| argument.map(|value| value.map(|value| value.raw() as usize))).collect::<Result<Vec<_>, _>>()?;
        if !arguments.is_empty() { return Ok(false); }
        let mut matched = false;
        for signature in crate::modules::signature::native_static_operation_signatures(super::super::RuntimeOp::SetEmpty) {
            self.budget.charge(1 + signature.params.len())?;
            if !signature.params.is_empty() { continue; }
            let Some(specialized) = self.native_signature(decoder, signature, &arguments, Some(expected))? else { continue };
            if !specialized.return_ty.matches_expected(expected) { continue; }
            if matched { return Err(IrVerifyError::new("empty set producer has ambiguous native contracts")); }
            matched = true;
        }
        Ok(matched)
    }

    fn receiver_pipeline_cannot_complete(&mut self, decoder: &FullDecoder<'a>, receiver: usize) -> Result<bool, IrVerifyError> {
        let schema::ExprView::Param(slot) = decoder.expression_view(receiver)? else { return Ok(false) };
        let ProducerBinding::ScopedDeclaration(declaration, _) = self.binding(decoder.owner, receiver, slot)? else { return Ok(false) };
        let value = match decoder.statement_view(declaration)? {
            schema::StmtView::Let { slot: declared, value, .. } if declared == slot => value.raw() as usize,
            _ => return Ok(false),
        };
        if self.store.tags[value] != FullTag::ExprPipeline { return Ok(false); }
        Ok(!super::super::indexed_expr_can_complete(decoder, value, 0)?)
    }

    fn verify_registered_method(&mut self, decoder: &FullDecoder<'a>, instruction: usize, receiver: usize, name: &str, arguments: &[usize], expected: &Type) -> Result<(), IrVerifyError> {
        let (actual_receiver, receiver_fact) = self.operand_context(decoder, receiver)?.into_parts();
        let receiver_type = match actual_receiver {
            Some(actual) => actual,
            None if self.receiver_pipeline_cannot_complete(decoder, receiver)? => return Ok(()),
            None => self.checked_fresh_list_method_receiver(decoder, instruction, expected)?.ok_or_else(|| IrVerifyError::new(format!("ground method has no receiver contract at owner {}, instruction {instruction}, receiver {receiver}, method {name}", decoder.owner)))?,
        };
        self.expression_with_fact(decoder.owner, receiver, Arc::clone(&receiver_type), false, receiver_fact);
        let receiver_type = if let Type::Optional(payload) = receiver_type.unvalidated()
            && self.optional_read_is_narrowed(decoder, receiver, &receiver_type)?
        {
            self.budget.charge_clone(payload)?;
            Arc::new((**payload).clone())
        } else { receiver_type };
        let signatures = FullDecoder::registered_method_signatures(receiver_type.as_ref(), name)?.ok_or_else(|| IrVerifyError::new("ground method has no registered parameter contract"))?;
        let mut selected = None;
        for signature in signatures {
            let first_parameter = if signature.op == super::super::RuntimeOp::ProcessHandleCancel
                && arguments.len() == 1 && signature.params.len() == 2
                && signature.params[0].defaulted && signature.params[0].ty == Type::Str
                && signature.params[1].defaulted && signature.params[1].ty == Type::Duration
                && self.need_context(decoder, arguments[0])?.is_some_and(|actual| matches!(actual.unvalidated(), Type::Duration))
            { 1 } else { 0 };
            if arguments.len() + first_parameter > signature.params.len()
                || signature.params[arguments.len() + first_parameter..].iter().any(|parameter| !parameter.defaulted)
            { continue; }
            let mut fits = true;
            for (&argument, parameter) in arguments.iter().zip(&signature.params[first_parameter..]) {
                if let Some(actual) = self.need_context(decoder, argument)? && !actual.matches_expected(&parameter.ty) { fits = false; break; }
            }
            if !fits { continue; }
            if selected.replace((signature, first_parameter)).is_some() { return Err(IrVerifyError::new("ground method parameter contract is ambiguous")); }
        }
        let (signature, first_parameter) = selected.ok_or_else(|| IrVerifyError::new("ground method arguments do not match a registered contract"))?;
        self.require_callable_effects(decoder, instruction, Some(signature.effects), signature.pure)?;
        let result = decoder.checked_method_return_type(receiver_type.as_ref(), name, arguments)?
            .ok_or_else(|| IrVerifyError::new("ground method has no resolved return contract"))?;
        if !result.matches_expected(expected)
            && !decoder.checked_module_get_relationship(instruction, &result, expected)?
        {
            return Err(IrVerifyError::new("ground method result disagrees with its selected contract"));
        }
        for (&argument, parameter) in arguments.iter().zip(&signature.params[first_parameter..]) {
            self.contextual_argument(decoder, argument, &parameter.ty)?;
            self.expression(decoder.owner, argument, Arc::new(parameter.ty.clone()), false);
        }
        Ok(())
    }

    fn verify_scalar_membership(&mut self, decoder: &FullDecoder<'a>, instruction: usize, slot: usize, needle: &Type) -> Result<(), IrVerifyError> {
        let container = self.slot_context_type(decoder, instruction, slot)?.ok_or_else(|| IrVerifyError::new("scalar membership has no container contract"))?;
        use crate::sema::types::MembershipCheck as M;
        match container.membership_needle_type(needle) {
            M::Expected(required) => {
                if !needle.matches_expected(required) { return Err(IrVerifyError::new("scalar membership needle disagrees with its container contract")); }
            }
            M::Compared(required) if needle.matches_expected(required) => {},
            _ => return Err(IrVerifyError::new("scalar membership operands have no checked domain")),
        }
        self.slot(decoder.owner, instruction, slot, container)
    }

    fn verify_scalar(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<(), IrVerifyError> {
        let tag = self.store.tags[instruction];
        let payload = super::super::FullPayload { store: self.store, cursor: super::super::FullCursor::new(self.store.payload(self.store.data[instruction].range())?) };
        match tag.kind() {
            schema::InstructionKind::Int => match schema::IntView::decode_from_decoder(decoder, tag, payload)? {
                schema::IntView::Slot(slot) => self.slot(decoder.owner, instruction, slot, Arc::new(Type::Int))?,
                schema::IntView::StrByteLenSlot { slot, .. } | schema::IntView::StrCountLinesSlot { slot, .. } | schema::IntView::StrByteAtSlot { slot, .. } => {
                    let ty = self.slot_context_type(decoder, instruction, slot)?.ok_or_else(|| IrVerifyError::new("scalar string producer has no slot contract"))?;
                    ensure_substitution_type(&ty)?;
                    if !matches!(ty.unvalidated(), Type::Str | Type::Bytes) { return Err(IrVerifyError::new("scalar string producer has a non-string slot")); }
                    self.slot(decoder.owner, instruction, slot, ty)?;
                }
                _ => {}
            },
            schema::InstructionKind::Bool => match schema::BoolView::decode_from_decoder(decoder, tag, payload)? {
                schema::BoolView::Slot(slot) => {
                    let ty = self.slot_context_type(decoder, instruction, slot)?.ok_or_else(|| IrVerifyError::new("scalar condition has no source contract"))?;
                    if !ConditionRole::Control.accepts(&ty) { return Err(IrVerifyError::new("scalar condition source is not truth-convertible")); }
                    self.slot(decoder.owner, instruction, slot, ty)?;
                },
                schema::BoolView::StrPredicateSlot { slot, needle, .. } | schema::BoolView::TrimStrPredicateSlot { slot, needle, .. } => {
                    let ty = self.slot_context_type(decoder, instruction, slot)?.ok_or_else(|| IrVerifyError::new("scalar string predicate has no slot contract"))?;
                    ensure_substitution_type(&ty)?;
                    if !matches!(ty.unvalidated(), Type::Str | Type::Bytes) { return Err(IrVerifyError::new("scalar string predicate has a non-string slot")); }
                    if ty.unvalidated() == &Type::Str && std::str::from_utf8(needle).is_err() { return Err(IrVerifyError::new("scalar string predicate has a non-UTF8 text needle")); }
                    self.slot(decoder.owner, instruction, slot, ty)?;
                }
                schema::BoolView::StrContainsSlot { slot, .. } => self.verify_scalar_membership(decoder, instruction, slot, &Type::Str)?,
                schema::BoolView::ContainsSlot { slot, needle, .. } => {
                    let container = self.slot_context_type(decoder, instruction, slot)?.ok_or_else(|| IrVerifyError::new("scalar membership has no container contract"))?;
                    needle.with_scalar_view(|amount| self.budget.charge(amount), |value| {
                        use crate::sema::types::MembershipCheck as M;
                        let needle_type = value.scalar_type().ok_or_else(|| IrVerifyError::new("scalar membership has a non-scalar literal needle"))?;
                        let accepts = match container.membership_needle_type(&needle_type) {
                            M::Expected(required) => crate::runtime::eval::type_view::value_matches_static_type(value, required),
                            M::Compared(required) => needle_type.matches_expected(required),
                            _ => false,
                        };
                        if accepts { Ok(()) } else { Err(IrVerifyError::new("scalar membership needle disagrees with its container contract")) }
                    })?;
                    self.slot(decoder.owner, instruction, slot, container)?;
                }
                schema::BoolView::TrimEmptySlot { slot, .. } => {
                    let ty = self.slot_context_type(decoder, instruction, slot)?.ok_or_else(|| IrVerifyError::new("scalar string predicate has no slot contract"))?;
                    ensure_substitution_type(&ty)?;
                    if !matches!(ty.unvalidated(), Type::Str | Type::Bytes) { return Err(IrVerifyError::new("scalar string predicate has a non-string slot")); }
                    self.slot(decoder.owner, instruction, slot, ty)?;
                }
                schema::BoolView::LiteralCompareSlot { slot, .. } => {
                    let ty = self.slot_context_type(decoder, instruction, slot)?.ok_or_else(|| IrVerifyError::new("scalar predicate has no slot contract"))?;
                    ensure_closed_type(&ty)?;
                    self.slot(decoder.owner, instruction, slot, ty)?;
                }
                _ => {}
            },
            _ => unreachable!("scalar producer instruction has a non-scalar tag"),
        }
        Ok(())
    }

    fn pipeline_stage(&mut self, decoder: &FullDecoder<'a>, pipeline: usize, position: usize) -> Result<super::super::StageView<'a>, IrVerifyError> {
        let raw = *self.index.owners[&decoder.owner].pipeline_stages.get(&pipeline).and_then(|stages| stages.get(position)).ok_or_else(|| IrVerifyError::new("pipeline producer stage is outside its verified owner index"))?;
        self.budget.charge(1)?;
        super::super::StageView::decode(super::super::decode::DecodeContext::from_decoder(decoder), raw)
    }

    fn pipeline_output_type(&mut self, decoder: &FullDecoder<'a>, pipeline: usize, end: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        self.budget.charge(1)?;
        let key = (decoder.owner, pipeline, end);
        if let Some(derived) = self.pipeline_outputs.get(&key) {
            return match derived {
                ProducerTypeDerivation::Pending => Err(IrVerifyError::new("pipeline prefix type depends on its own callback")),
                ProducerTypeDerivation::Resolved(ty) => Ok(ty.clone()),
                ProducerTypeDerivation::ResolvedBinding(..) => Err(IrVerifyError::new("pipeline prefix has a slot binding derivation")),
            };
        }
        let initial = (decoder.owner, pipeline, 0);
        if !self.pipeline_outputs.contains_key(&initial) {
            self.pipeline_outputs.insert(initial, ProducerTypeDerivation::Pending);
            let schema::ExprView::ListPipeline { input, .. } = decoder.expression_view(pipeline)? else { return Err(IrVerifyError::new("pipeline prefix has another expression kind")) };
            let output = self.need_context(decoder, input.raw() as usize)?;
            self.pipeline_outputs.insert(initial, ProducerTypeDerivation::Resolved(output));
        }
        let (&(_, _, start), state) = self.pipeline_outputs.range(initial..=key).next_back().ok_or_else(|| IrVerifyError::new("pipeline prefix has no initial source"))?;
        if matches!(state, ProducerTypeDerivation::Pending) { return Err(IrVerifyError::new("pipeline prefix type depends on its own callback")); }
        for position in start..end {
            self.budget.charge(1)?;
            let next = (decoder.owner, pipeline, position + 1);
            if let Some(derived) = self.pipeline_outputs.get(&next) {
                if matches!(derived, ProducerTypeDerivation::Pending) { return Err(IrVerifyError::new("pipeline prefix type depends on its own callback")); }
                continue;
            }
            let ProducerTypeDerivation::Resolved(source) = &self.pipeline_outputs[&(decoder.owner, pipeline, position)] else { return Err(IrVerifyError::new("pipeline prefix type depends on its own callback")) };
            let source = source.clone();
            self.pipeline_outputs.insert(next, ProducerTypeDerivation::Pending);
            let item = if let Some(source) = source { self.budget.charge_clone(&source)?; source.iteration_item_type() } else { None };
            let stage = self.pipeline_stage(decoder, pipeline, position)?;
            let output = if let super::super::StageView::Fold { acc_slot, initial, .. } = &stage {
                self.fold_accumulator_type(decoder, pipeline, position, *acc_slot, initial.raw() as usize)?
            } else {
                FullDecoder::checked_pipeline_stage_output_type(&stage, item, &mut |value| {
                    let Some(ty) = self.need_context(decoder, value)? else { return Ok(None) };
                    self.budget.charge_clone(&ty)?;
                    Ok(Some((*ty).clone()))
                })?.map(Arc::new)
            };
            self.pipeline_outputs.insert(next, ProducerTypeDerivation::Resolved(output));
        }
        let ProducerTypeDerivation::Resolved(output) = &self.pipeline_outputs[&key] else { return Err(IrVerifyError::new("pipeline prefix type depends on its own callback")) };
        Ok(output.clone())
    }

    fn pipeline_type(&mut self, decoder: &FullDecoder<'a>, pipeline: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        let len = self.index.owners[&decoder.owner].pipeline_stages[&pipeline].len();
        let Some(output) = self.pipeline_output_type(decoder, pipeline, len)? else { return Ok(None) };
        if let Type::Stream(item) = output.as_ref() {
            self.budget.charge_clone(item)?;
            Ok(Some(Arc::new(Type::List(item.clone()))))
        } else { Ok(Some(output)) }
    }

    fn fold_accumulator_type(&mut self, decoder: &FullDecoder<'a>, pipeline: usize, position: usize, slot: usize, initial: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        if let Some(actual) = self.need_context(decoder, initial)? { return Ok(Some(actual)); }
        let Some(id) = decoder.slot_type_fact(slot)? else { return Ok(None) };
        let ty = self.contract_type(id)?;
        self.budget.charge_clone(&ty)?;
        ensure_substitution_type(&ty)?;
        let anchor = self.index.owners[&decoder.owner].pipeline_groups.get(&(pipeline, position, false)).copied().ok_or_else(|| IrVerifyError::new("pipeline accumulator has no lexical callback region"))?;
        self.slot(decoder.owner, anchor, slot, Arc::clone(&ty))?;
        Ok(Some(ty))
    }

    fn pipeline_binding_type(&mut self, decoder: &FullDecoder<'a>, source: PipelineProducer) -> Result<Option<Arc<Type>>, IrVerifyError> {
        use super::super::PipelineStageSlotRole as Role;
        match source.role {
            Role::Item => {
                let Some(output) = self.pipeline_output_type(decoder, source.pipeline, source.stage)? else { return Ok(None) };
                self.budget.charge_clone(&output)?;
                Ok(output.iteration_item_type().map(Arc::new))
            }
            Role::Accumulator => {
                let super::super::StageView::Fold { acc_slot, initial, .. } = self.pipeline_stage(decoder, source.pipeline, source.stage)? else { return Err(IrVerifyError::new("pipeline accumulator has no fold stage")) };
                self.fold_accumulator_type(decoder, source.pipeline, source.stage, acc_slot, initial.raw() as usize)
            }
            Role::ReducedItem { value, flatten } => {
                let Some(output) = self.need_context(decoder, value)? else { return Ok(None) };
                if !flatten { return Ok(Some(output)); }
                self.budget.charge_clone(&output)?;
                Ok(output.iteration_item_type().map(Arc::new))
            }
        }
    }

    fn pipeline_prefix(&mut self, owner: u32, pipeline: usize, end: usize, expected: Arc<Type>) {
        let previous = self.prefixes.entry((owner, pipeline, end)).or_default();
        if previous.iter().any(|ty| **ty == *expected) { return; }
        previous.push(Arc::clone(&expected));
        self.pending.push(Obligation::PipelinePrefix(owner, pipeline, end, expected));
    }

    fn pipeline_callback(&mut self, owner: u32, pipeline: usize, position: usize, stage: &super::super::StageView<'_>, expected: Arc<Type>, reduced: bool) -> Result<(), IrVerifyError> {
        use super::super::StageView as S;
        let value = match stage {
            S::Where { predicate, .. } | S::Any { predicate, .. } | S::All { predicate, .. } => Some(predicate),
            S::Map { value, .. } | S::FlatMap { value, .. } | S::ParMap { value, .. }
            | S::WhereBlock { value, .. } | S::MapBlock { value, .. } | S::FlatMapBlock { value, .. }
            | S::AnyBlock { value, .. } | S::AllBlock { value, .. } | S::ParMapBlock { value, .. }
            | S::Fold { value, .. } | S::ReduceBy { value, .. } | S::ReduceByConfigured { value, .. } => Some(value),
            S::SortBy { key, .. } | S::UniqueBy { key, .. } | S::GroupBy { key, .. } | S::CountBy { key, .. } => Some(key),
            S::ParMapFlatMapReduceBy { value, reduce_value, .. } => Some(if reduced { reduce_value } else { value }),
            S::Tee { .. } | S::Each { .. } => None,
            _ => return Err(IrVerifyError::new("pipeline stage has no callback result")),
        };
        let decoder = self.owner(owner)?;
        if let Some(value) = value && super::super::indexed_stage_callback_can_complete(&decoder, stage)? { self.expression(owner, value.raw() as usize, Arc::clone(&expected), false); }
        if !self.index.owners[&owner].pipeline_groups.contains_key(&(pipeline, position, reduced)) { return Err(IrVerifyError::new("pipeline callback has no lexical region")); }
        Ok(())
    }

    fn pipeline_known_callback(&mut self, decoder: &FullDecoder<'a>, pipeline: usize, position: usize, stage: &super::super::StageView<'_>, value: usize) -> Result<(), IrVerifyError> {
        let ty = self.need_context(decoder, value)?.ok_or_else(|| IrVerifyError::new("pipeline callback has no derivable result contract"))?;
        self.pipeline_callback(decoder.owner, pipeline, position, stage, ty, false)
    }

    fn verify_pipeline_result(&mut self, owner: u32, pipeline: usize, expected: Arc<Type>) -> Result<(), IrVerifyError> {
        use super::super::StageView as S;
        let decoder = self.owner(owner)?;
        let len = self.index.owners[&owner].pipeline_stages[&pipeline].len();
        for position in 0..len {
            if let S::Range { start, end } = self.pipeline_stage(&decoder, pipeline, position)? {
                self.expression(owner, start.raw() as usize, Arc::new(Type::Int), false);
                self.expression(owner, end.raw() as usize, Arc::new(Type::Int), false);
            }
        }
        let position = len.checked_sub(1).ok_or_else(|| IrVerifyError::new("pipeline producer has no stages"))?;
        let last = self.pipeline_stage(&decoder, pipeline, position)?;
        let last = &last;
        match last {
            S::Fold { initial, .. } => {
                self.expression(owner, initial.raw() as usize, Arc::clone(&expected), false);
                self.pipeline_callback(owner, pipeline, position, last, expected, false)
            }
            S::Count => {
                if !Type::Int.matches_expected(&expected) { return Err(IrVerifyError::new("pipeline count result disagrees with its contract")); }
                Ok(())
            }
            S::Sum => {
                if !Type::Int.matches_expected(&expected) { return Err(IrVerifyError::new("pipeline sum result disagrees with its contract")); }
                self.pipeline_prefix(owner, pipeline, position, Arc::new(Type::Int));
                Ok(())
            }
            S::Any { .. } | S::AnyBlock { .. } | S::All { .. } | S::AllBlock { .. } => {
                if !Type::Bool.matches_expected(&expected) { return Err(IrVerifyError::new("pipeline predicate result disagrees with its contract")); }
                self.pipeline_callback(owner, pipeline, position, last, Arc::new(Type::Bool), false)
            }
            S::First | S::Last | S::Min | S::Max => {
                let Type::Result(item, error) = expected.unvalidated() else { return Err(IrVerifyError::new("pipeline selection has no result contract")) };
                if !Type::Error.matches_expected(error) { return Err(IrVerifyError::new("pipeline selection has an incompatible error contract")); }
                self.pipeline_prefix(owner, pipeline, position, Arc::new((**item).clone()));
                Ok(())
            }
            S::Each { .. } | S::TablePrint { .. } | S::TablePrintConfigured { .. } => {
                if !Type::Unit.matches_expected(&expected) { return Err(IrVerifyError::new("pipeline effect result disagrees with its unit contract")); }
                if matches!(last, S::Each { .. }) { self.pipeline_callback(owner, pipeline, position, last, Arc::new(Type::Unit), false)?; }
                Ok(())
            }
            S::CountBy { .. } => {
                let Type::Map(key, item) = expected.unvalidated() else { return Err(IrVerifyError::new("pipeline count-by has no map contract")) };
                if **key != Type::Str || !Type::Int.matches_expected(item) { return Err(IrVerifyError::new("pipeline count-by result disagrees with its contract")); }
                self.pipeline_callback(owner, pipeline, position, last, Arc::new(Type::Str), false)
            }
            S::ReduceBy { .. } | S::ReduceByConfigured { .. } | S::ParMapFlatMapReduceBy { .. } => {
                let Type::Map(key, item) = expected.unvalidated() else { return Err(IrVerifyError::new("pipeline reduction has no map contract")) };
                if **key != Type::Str { return Err(IrVerifyError::new("pipeline reduction has no string key contract")); }
                let row = Type::Record(Arc::new(BTreeMap::from([(Name::intern("key"), Type::Str), (Name::intern("value"), (**item).clone())])));
                self.pipeline_callback(owner, pipeline, position, last, Arc::new(row), matches!(last, S::ParMapFlatMapReduceBy { .. }))
            }
            _ => {
                let item = match expected.unvalidated() { Type::List(item) | Type::Stream(item) => Arc::new((**item).clone()), _ => return Err(IrVerifyError::new("pipeline producer has no collection output contract")) };
                self.pipeline_prefix(owner, pipeline, if matches!(last, S::Collect) { position } else { len }, item);
                Ok(())
            }
        }
    }

    fn verify_pipeline_prefix(&mut self, owner: u32, pipeline: usize, end: usize, expected: Arc<Type>) -> Result<(), IrVerifyError> {
        use super::super::StageView as S;
        let decoder = self.owner(owner)?;
        if end == 0 {
            let schema::ExprView::ListPipeline { input, .. } = decoder.expression_view(pipeline)? else { unreachable!("pipeline prefix has another expression") };
            let source = self.need_context(&decoder, input.raw() as usize)?.ok_or_else(|| IrVerifyError::new("pipeline source has no iterable contract"))?;
            let item = source.iteration_item_type().ok_or_else(|| IrVerifyError::new("pipeline source is not iterable"))?;
            if !item.matches_expected(&expected) { return Err(IrVerifyError::new("pipeline source item disagrees with its callback input")); }
            self.expression(owner, input.raw() as usize, source, false);
            return Ok(());
        }
        let position = end - 1;
        let stage = self.pipeline_stage(&decoder, pipeline, position)?;
        let stage = &stage;
        match stage {
            S::Map { .. } | S::MapBlock { .. } | S::ParMap { .. } | S::ParMapBlock { .. } => self.pipeline_callback(owner, pipeline, position, stage, expected, false),
            S::FlatMap { value, .. } | S::FlatMapBlock { value, .. } => {
                let source = self.need_context(&decoder, value.raw() as usize)?.ok_or_else(|| IrVerifyError::new("flat-map callback has no iterable result contract"))?;
                let item = source.iteration_item_type().ok_or_else(|| IrVerifyError::new("flat-map callback result is not iterable"))?;
                if !item.matches_expected(&expected) { return Err(IrVerifyError::new("flat-map callback item disagrees with its result contract")); }
                self.pipeline_callback(owner, pipeline, position, stage, source, false)
            }
            S::Where { .. } | S::WhereBlock { .. } => {
                self.pipeline_prefix(owner, pipeline, position, expected);
                self.pipeline_callback(owner, pipeline, position, stage, Arc::new(Type::Bool), false)
            }
            S::SortBy { key, .. } | S::UniqueBy { key, .. } => {
                self.pipeline_prefix(owner, pipeline, position, expected);
                self.pipeline_known_callback(&decoder, pipeline, position, stage, key.raw() as usize)
            }
            S::GroupBy { .. } => {
                let Type::Record(fields) = expected.unvalidated() else { return Err(IrVerifyError::new("pipeline grouping has no record contract")) };
                for (name, ty) in fields.iter() {
                    match name.as_str().as_str() {
                        "key" => self.pipeline_callback(owner, pipeline, position, stage, Arc::new(ty.clone()), false)?,
                        "items" => {
                            let Type::List(item) = ty.unvalidated() else { return Err(IrVerifyError::new("pipeline grouping items have no list contract")) };
                            self.pipeline_prefix(owner, pipeline, position, Arc::new((**item).clone()));
                        }
                        _ => return Err(IrVerifyError::new("pipeline grouping has an unknown result field")),
                    }
                }
                Ok(())
            }
            S::Enumerate => {
                let Type::Record(fields) = expected.unvalidated() else { return Err(IrVerifyError::new("pipeline enumeration has no record contract")) };
                for (name, ty) in fields.iter() {
                    match name.as_str().as_str() {
                        "index" if Type::Int.matches_expected(ty) => {}
                        "value" => self.pipeline_prefix(owner, pipeline, position, Arc::new(ty.clone())),
                        _ => return Err(IrVerifyError::new("pipeline enumeration field disagrees with its contract")),
                    }
                }
                Ok(())
            }
            S::Zip { other } => {
                let Type::Record(fields) = expected.unvalidated() else { return Err(IrVerifyError::new("pipeline zip has no record contract")) };
                for (name, ty) in fields.iter() {
                    match name.as_str().as_str() {
                        "left" => self.pipeline_prefix(owner, pipeline, position, Arc::new(ty.clone())),
                        "right" => {
                            let source = self.need_context(&decoder, other.raw() as usize)?.ok_or_else(|| IrVerifyError::new(format!("pipeline zip input {} {:?} has no contract", other.raw(), decoder.store.tags[other.raw() as usize])))?;
                            if !source.iteration_item_type().is_some_and(|item| item.matches_expected(ty)) { return Err(IrVerifyError::new("pipeline zip right input disagrees with its contract")); }
                            self.expression(owner, other.raw() as usize, source, false);
                        }
                        _ => return Err(IrVerifyError::new("pipeline zip has an unknown field")),
                    }
                }
                Ok(())
            }
            S::BatchCount { .. } | S::BatchMaxArgv { .. } | S::BatchMaxBytes { .. } | S::BatchLimits { .. } => {
                let Type::List(item) = expected.unvalidated() else { return Err(IrVerifyError::new("pipeline batch has no list item contract")) };
                self.pipeline_prefix(owner, pipeline, position, Arc::new((**item).clone()));
                Ok(())
            }
            S::Range { start, end } => {
                if !Type::Int.matches_expected(&expected) { return Err(IrVerifyError::new("pipeline range has an incompatible item contract")); }
                self.expression(owner, start.raw() as usize, Arc::new(Type::Int), false);
                self.expression(owner, end.raw() as usize, Arc::new(Type::Int), false);
                Ok(())
            }
            S::TextLines | S::BytesChunks { .. } | S::JsonLines => {
                let actual = match stage { S::TextLines => Type::Str, S::BytesChunks { .. } => Type::Bytes, _ => Type::Any };
                if !actual.matches_expected(&expected) { return Err(IrVerifyError::new("pipeline adapter item disagrees with its contract")); }
                Ok(())
            }
            S::Tee { .. } => {
                self.pipeline_prefix(owner, pipeline, position, expected);
                self.pipeline_callback(owner, pipeline, position, stage, Arc::new(Type::Unit), false)
            }
            S::Take(_) | S::Drop(_) | S::Repeat { .. } | S::Sort { .. } | S::Shuffle { .. } => {
                self.pipeline_prefix(owner, pipeline, position, expected);
                Ok(())
            }
            _ => Err(IrVerifyError::new("pipeline terminal stage occurs before a callback input")),
        }
    }

    fn iteration_item_type_at(&mut self, decoder: &FullDecoder<'a>, instruction: usize, source: &Type) -> Result<Option<Type>, IrVerifyError> {
        self.budget.charge_clone(source)?;
        if let Some(item) = source.iteration_item_type() { return Ok(Some(item)); }
        if source == &Type::Any {
            let Some(prefix) = &decoder.producer_prefix else { return Ok(None) };
            let Some(ty) = prefix.runtime_list_read_type(decoder, instruction)? else { return Ok(None) };
            self.budget.charge_clone(&ty)?;
            return Ok(ty.iteration_item_type());
        }
        let Type::Optional(payload) = source else { return Ok(None) };
        let Some(item) = payload.iteration_item_type() else { return Ok(None) };
        if self.optional_read_is_narrowed(decoder, instruction, source)? { return Ok(Some(item)); }
        let Some(target) = decoder.checked_binding_path(instruction, &mut |amount| self.budget.charge(amount))? else { return Ok(None) };
        let binding = self.binding(decoder.owner, instruction, target.0)?;
        if self.index.owners[&decoder.owner].optional_path_read_is_narrowed(decoder, instruction, &target, payload, binding, &mut |amount| self.budget.charge(amount))? {
            return Ok(Some(item));
        }
        Ok(None)
    }

    fn iteration_binding_type(&mut self, decoder: &FullDecoder<'a>, iter: usize, target: super::super::decode::CompTargetView<'_>, slot: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        let Some(source) = self.need_context(decoder, iter)? else { return Ok(None) };
        let item = self.iteration_item_type_at(decoder, iter, &source)?.ok_or_else(|| IrVerifyError::new("ground destructuring source has no item type"))?;
        self.budget.charge_clone(&item)?;
        let projected = comp_slot_type(target, slot, &item)?.ok_or_else(|| IrVerifyError::new("ground destructuring binding has no target projection"))?;
        self.expression(decoder.owner, iter, source, false);
        Ok(Some(Arc::new(projected)))
    }

    fn statement_binding_type(&mut self, decoder: &FullDecoder<'a>, declaration: usize, slot: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        if self.store.tags[declaration].kind() != schema::InstructionKind::Stmt { return Ok(None); }
        let (source, projected) = match decoder.statement_view(declaration)? {
            schema::StmtView::ForRecord { target, iter, .. } => return self.iteration_binding_type(decoder, iter.raw() as usize, target, slot),
            schema::StmtView::For { iter, .. } => {
                let Some(source) = self.need_context(decoder, iter.raw() as usize)? else { return Ok(None) };
                let projected = self.iteration_item_type_at(decoder, iter.raw() as usize, &source)?.ok_or_else(|| IrVerifyError::new("ground loop source has no item contract"))?;
                ((iter.raw() as usize, source), projected)
            }
            schema::StmtView::ForStrLines { text, .. } => {
                let Some(source) = self.need_context(decoder, text.raw() as usize)? else { return Ok(None) };
                if !matches!(source.unvalidated(), Type::Str | Type::Bytes) { return Err(IrVerifyError::new("ground line source is neither text nor bytes")); }
                let projected = FullDecoder::registered_method_return_type(source.unvalidated(), "lines")?.and_then(|result| result.iteration_item_type()).ok_or_else(|| IrVerifyError::new("ground line source has no registered item contract"))?;
                ((text.raw() as usize, source), projected)
            }
            schema::StmtView::LetRecord { source, target, .. } => {
                let binding = ProducerBinding::ScopedDeclaration(declaration, slot);
                let key = (decoder.owner, binding);
                self.budget.charge(1)?;
                if let Some(projected) = self.pattern_types.get(&key) {
                    return Ok(Some(Arc::clone(projected)));
                }
                let instruction = source.raw() as usize;
                let Some(source) = self.need_context(decoder, instruction)? else { return Ok(None) };
                self.budget.charge_clone(&source)?;
                let (projected, fields) = comp_slot_projection(target, slot, &source, &mut self.budget)?.ok_or_else(|| IrVerifyError::new("ground destructuring writer has no slot projection"))?;
                if let Type::Optional(payload) = &projected
                    && self.index.owners[&decoder.owner].binding_path_is_snapshot(decoder, binding, slot, &mut |amount| self.budget.charge(amount))?
                    && let Some((root, mut path)) = decoder.checked_binding_path(instruction, &mut |amount| self.budget.charge(amount))?
                {
                    self.budget.charge(fields.len())?;
                    path.extend(fields);
                    let root_binding = self.binding(decoder.owner, instruction, root)?;
                    if self.index.owners[&decoder.owner].optional_path_read_is_narrowed(decoder, instruction, &(root, path), payload, root_binding, &mut |amount| self.budget.charge(amount))? {
                        self.budget.charge_clone(payload)?;
                        let projected = Arc::new((**payload).clone());
                        self.expression(decoder.owner, instruction, source, false);
                        self.budget.charge(1)?;
                        self.pattern_types.insert(key, Arc::clone(&projected));
                        return Ok(Some(projected));
                    }
                }
                ((instruction, source), projected)
            }
            schema::StmtView::Guard { value, target, optional, else_param_slot, .. } => {
                let instruction = value.raw() as usize;
                let Some(source) = self.need_context(decoder, instruction)? else { return Ok(None) };
                self.budget.charge_clone(&source)?;
                let projected = if else_param_slot == Some(slot) {
                    match source.unvalidated() { Type::Result(_, error) if !optional => Some((**error).clone()), _ => None }
                } else {
                    let payload = match source.unvalidated() { Type::Result(ok, _) if !optional => Some(ok.as_ref()), Type::Optional(ok) if optional => Some(ok.as_ref()), _ => None };
                    match payload { Some(payload) => comp_slot_type(target, slot, payload)?, None => None }
                }.ok_or_else(|| IrVerifyError::new("ground guard has no slot transfer contract"))?;
                ((instruction, source), projected)
            }
            schema::StmtView::ScanLines { text_slot, line_slot, .. } => {
                if slot == line_slot { return Err(IrVerifyError::new("scan line temporary escapes its lexical iteration")); }
                let source = decoder.checked_slot_type(text_slot)?.ok_or_else(|| IrVerifyError::new("scan source has no string slot contract"))?;
                let source = self.ground_type(source)?;
                if !matches!(source.unvalidated(), Type::Str | Type::Bytes) { return Err(IrVerifyError::new("scan source slot is not text or bytes")); }
                self.slot(decoder.owner, declaration, text_slot, source)?;
                return Ok(Some(Arc::new(Type::Int)));
            }
            schema::StmtView::ScanBytes { config } => {
                let ty = if slot == config.block_depth_slot || slot == config.string_delim_slot { Type::Int } else { Type::Bool };
                self.slot(decoder.owner, declaration, config.line_slot, Arc::new(Type::Bytes))?;
                self.slot(decoder.owner, declaration, config.block_depth_slot, Arc::new(Type::Int))?;
                return Ok(Some(Arc::new(ty)));
            }
            _ => return Ok(None),
        };
        self.expression(decoder.owner, source.0, source.1, false);
        Ok(Some(Arc::new(projected)))
    }

    fn verify_comprehension_target(&mut self, target: super::super::decode::CompTargetView<'_>, item: &Type) -> Result<(), IrVerifyError> {
        FullDecoder::visit_comp_target_evidence(target, item,
            |source| if matches!(source.unvalidated(), Type::Record(_)) { Ok(()) } else { Err(IrVerifyError::new("ground comprehension record target has a non-record item")) },
            |source, name| Ok(match (*source).unvalidated() {
                Type::Record(fields) => fields.get(&name),
                _ => None,
            }),
            |_, _| Ok(()),
            |amount| self.budget.charge(amount))
    }

    fn comprehension_binding_type(&mut self, decoder: &FullDecoder<'a>, comprehension: usize, iter: usize, slot: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        let mut qualifiers = match decoder.expression_view(comprehension)? {
            schema::ExprView::ListComp { qualifiers, .. } | schema::ExprView::MapComp { qualifiers, .. } => qualifiers,
            _ => return Err(IrVerifyError::new("comprehension binding has no comprehension source")),
        };
        for qualifier in &mut qualifiers {
            self.budget.charge(1)?;
            if let super::super::decode::CompQualifierView::For { target, iter: candidate, .. } = qualifier? && candidate.raw() as usize == iter {
                return self.iteration_binding_type(decoder, iter, target, slot);
            }
        }
        Err(IrVerifyError::new("ground comprehension binder has no lexical source"))
    }

    fn verify_slot(&mut self, owner: u32, binding: ProducerBinding, slot: usize, expected: Arc<Type>) -> Result<(), IrVerifyError> {
        let decoder = self.owner(owner)?;
        if let Some(source) = self.index.owners[&owner].pipeline_sources.get(&binding).copied() {
            match source.role {
                super::super::PipelineStageSlotRole::Item => self.pipeline_prefix(owner, source.pipeline, source.stage, Arc::clone(&expected)),
                super::super::PipelineStageSlotRole::Accumulator => {
                    let stage = self.pipeline_stage(&decoder, source.pipeline, source.stage)?;
                    let super::super::StageView::Fold { initial, .. } = &stage else { return Err(IrVerifyError::new("pipeline accumulator has no fold stage")) };
                    self.expression(owner, initial.raw() as usize, Arc::clone(&expected), false);
                    self.pipeline_callback(owner, source.pipeline, source.stage, &stage, Arc::clone(&expected), false)?;
                }
                super::super::PipelineStageSlotRole::ReducedItem { value, flatten } => {
                    let ty = self.need_context(&decoder, value)?.ok_or_else(|| IrVerifyError::new("fused pipeline callback has no item contract"))?;
                    let item = if flatten { ty.iteration_item_type().ok_or_else(|| IrVerifyError::new("fused pipeline callback has no iterable result"))? } else { (*ty).clone() };
                    if !item.matches_expected(&expected) { return Err(IrVerifyError::new("fused pipeline callback disagrees with its reduced input")); }
                    self.expression(owner, value, ty, false);
                }
            }
            return self.verify_slot_writers(&decoder, binding, slot, expected, true);
        }
        if self.index.owners[&owner].pattern_sources.contains_key(&binding) {
            let actual = self.pattern_binding_type(&decoder, binding, slot)?.ok_or_else(|| IrVerifyError::new("ground expression pattern has no binding projection"))?;
            if !actual.matches_expected(&expected) { return Err(IrVerifyError::new("ground expression pattern binding disagrees with its producer contract")); }
            return self.verify_slot_writers(&decoder, binding, slot, expected, true);
        }
        if let Some(&(comprehension, iter)) = self.index.owners[&owner].comprehension_sources.get(&binding) {
            let projected = self.comprehension_binding_type(&decoder, comprehension, iter, slot)?.ok_or_else(|| IrVerifyError::new("ground comprehension has no iterable contract"))?;
            if !projected.matches_expected(&expected) { return Err(IrVerifyError::new("ground comprehension binding disagrees with its producer contract")); }
            return self.verify_slot_writers(&decoder, binding, slot, expected, true);
        }
        if matches!(binding, ProducerBinding::ScopedDeclaration(_, _)) {
            return self.verify_slot_writers(&decoder, binding, slot, expected, false);
        }
        if let Some(step) = super::super::driver_owner_index(owner) {
            if let Some(source) = self.index.owners[&owner].storage_bindings.get(&binding)
                && source.identity != IR_NONE
            { return self.binding_writer(source.identity, expected); }
            if let Some(source) = decoder.driver_local_let_source(slot, |amount| self.budget.charge(amount))? {
                let source_owner = driver_owner(source.step).map_err(|_| IrVerifyError::new("ground driver initializer owner is invalid"))?;
                if let Some(validation) = source.validation_type
                    && *self.ground_type(validation)? == *expected
                    && super::runtime_complete_value_check_type(&expected, &mut self.budget, 0)?
                {
                    return Ok(());
                }
                self.expression(source_owner, source.initializer, expected, false);
                return Ok(());
            }
            let driver = self.store.driver_steps[step];
            let binding = bounded(driver.slots, &self.store.driver_slots, "ground driver slot range is invalid")?.iter()
                .find(|binding| binding.slot as usize == slot).ok_or_else(|| IrVerifyError::new("ground driver slot has no lexical binding"))?;
            return self.binding_writer(binding.binding_identity, expected);
        }
        let function = self.store.functions[function_index(owner)?];
        let params = bounded(function.params, &self.store.params, "ground producer parameter range is invalid")?;
        if matches!(binding, ProducerBinding::HeaderParameter(_)) {
            let parameter = function.params.start as usize + slot;
            if let Ok(cold) = self.store.param_cold.binary_search_by_key(&(parameter as u32), |cold| cold.param) {
                let default = self.store.param_cold[cold].default;
                if default != IR_NONE {
                    let words = [default];
                    let value: super::super::LoweredValue = super::super::FullCodec::decode(&decoder, &mut super::super::FullCursor::new(&words))?;
                    decoder.verify_prepared_value_type(&value, &expected, &mut |amount| self.budget.charge(amount))?;
                }
            }
            let FullCallableContract::Ground(signature) = function.contract else {
                let ProducerBinding::HeaderParameter(parameter) = binding else { unreachable!() };
                let ty = self.scheme_parameter(owner, parameter)?.ok_or_else(|| IrVerifyError::new("symbolic parameter escaped its verified body proof"))?;
                if !ty.matches_expected(&expected) { return Err(IrVerifyError::new("scheme parameter disagrees with its ground producer contract")); }
                return Ok(());
            };
            let (_, parameter, _) = self.store.semantic.signature_param(signature, slot)?;
            if !self.contract_type(parameter)?.matches_expected(&expected) { return Err(IrVerifyError::new("ground producer parameter disagrees with its signature")); }
            for instruction in self.incoming.get(&function_index(owner)?).cloned().unwrap_or_default() {
                let caller = self.index.instruction_owner(instruction)?;
                let caller_decoder = self.owner(caller)?;
                let arguments = call_values(&caller_decoder, instruction)?;
                self.call_arguments(&caller_decoder, function_index(owner)?, &arguments)?;
            }
        }
        if let ProducerBinding::HeaderCapture(identity) = binding {
            return self.binding_writer(identity, Arc::clone(&expected));
        }
        let initialized = slot < params.len() || bounded(function.captures, &self.store.captures, "ground producer capture range is invalid")?.iter().any(|capture| binding == ProducerBinding::HeaderCapture(capture.binding_identity));
        self.verify_slot_writers(&decoder, binding, slot, expected, initialized)
    }

    fn verify_slot_writers(&mut self, decoder: &FullDecoder<'a>, binding: ProducerBinding, slot: usize, expected: Arc<Type>, initialized: bool) -> Result<(), IrVerifyError> {
        let owner = decoder.owner;
        let writers = self.index.owners[&owner].writers.get(&binding).cloned().unwrap_or_default();
        if writers.is_empty() && !initialized { return Err(IrVerifyError::new("ground producer slot has no initialized writer")); }
        for writer in writers {
            self.verify_slot_writer(decoder, slot, Arc::clone(&expected), writer)?;
        }
        Ok(())
    }

    fn reject_unchecked_unsigned_integer_writer(expected: &Type) -> Result<(), IrVerifyError> {
        if expected.has_unsigned_constraint() { return Err(IrVerifyError::new("integer writer bypasses an unsigned storage check")); }
        Ok(())
    }

    fn verify_slot_writer(&mut self, decoder: &FullDecoder<'a>, slot: usize, expected: Arc<Type>, writer: usize) -> Result<(), IrVerifyError> {
        let owner = decoder.owner;
        self.budget.charge(1)?;
        if let Some(projected) = self.statement_binding_type(decoder, writer, slot)? {
            if !projected.matches_expected(&expected) { return Err(IrVerifyError::new("ground binding writer disagrees with its slot contract")); }
            return Ok(());
        }
        match decoder.statement_view(writer)? {
                schema::StmtView::Let { value, .. } | schema::StmtView::DefaultParameter { value, .. } => self.expression(owner, value.raw() as usize, Arc::clone(&expected), false),
                schema::StmtView::Assign { value, check, .. } => {
                    let checked = check.map(|check| check.ty.materialize()).transpose()?;
                    if checked.as_ref() != Some(expected.as_ref()) { self.expression(owner, value.raw() as usize, Arc::clone(&expected), false); }
                }
                schema::StmtView::LetInt { .. } | schema::StmtView::AssignInt { .. } => {
                    Self::reject_unchecked_unsigned_integer_writer(&expected)?;
                    if !Type::Int.matches_expected(&expected) { return Err(IrVerifyError::new("integer writer disagrees with its ground slot contract")); }
                    self.verify_operands(decoder, writer)?;
                }
                schema::StmtView::LetBool { .. } | schema::StmtView::AssignBool { .. } => {
                    if !Type::Bool.matches_expected(&expected) { return Err(IrVerifyError::new("boolean writer disagrees with its ground slot contract")); }
                    self.verify_operands(decoder, writer)?;
                }
                schema::StmtView::AssignField { .. } | schema::StmtView::AssignFieldInt { .. } | schema::StmtView::AssignPath { .. } => {
                    self.verify_storage_statement(decoder, writer, slot, expected)?;
                }
                schema::StmtView::With { mut bindings, else_param_slot, .. } => {
                    let failure = else_param_slot == Some(slot);
                    if failure {
                        let actual = self.index.owners[&owner].with_errors.get(&writer).ok_or_else(|| IrVerifyError::new("ground With failure has no verified error contract"))?;
                        if !actual.matches_expected(&expected) { return Err(IrVerifyError::new("ground With failure binding disagrees with its producer contract")); }
                    }
                    let mut found = failure;
                    for binding in &mut bindings {
                        let (bound, value) = binding?;
                        if bound != slot && !failure { continue; }
                        let ty = self.need_context(decoder, value.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground With binding has no producer contract"))?;
                        let payload = match ty.unvalidated() { Type::Result(ok, _) => ok.as_ref(), other => other };
                        if !failure && !payload.matches_expected(&expected) { return Err(IrVerifyError::new("ground With writer disagrees with its slot contract")); }
                        self.expression(owner, value.raw() as usize, ty, false);
                        found = true;
                    }
                    if !found { return Err(IrVerifyError::new("ground With capture has no typed source writer")); }
                }
                schema::StmtView::Match { value, mut arms, .. } => {
                    let ty = self.need_context(decoder, value.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground match has no subject contract"))?;
                    let mut found = false;
                    for arm in &mut arms {
                        let (pattern, _, _) = arm?;
                        if let Some(projected) = decoder.checked_pattern_bound_type_with_budget(pattern.raw(), slot, Some(&ty), |amount| self.budget.charge(amount))? {
                            if !projected.matches_expected(&expected) { return Err(IrVerifyError::new("ground pattern writer disagrees with its slot contract")); }
                            found = true;
                        }
                    }
                    if !found { return Err(IrVerifyError::new("ground pattern writer has no authenticated slot projection")); }
                    self.expression(owner, value.raw() as usize, ty, false);
                }
                _ => return Err(IrVerifyError::new("ground producer writer has no authenticated transfer contract")),
        }
        Ok(())
    }

    fn storage_assignment_target(&mut self, root: &Type, path: &[super::super::CallableAssignmentPathStep]) -> Result<Arc<Type>, IrVerifyError> {
        let target = FullDecoder::assignment_target_from_type(root, path, |amount| self.budget.charge(amount))?
            .ok_or_else(|| IrVerifyError::new("storage writer path disagrees with its declaration"))?;
        self.budget.charge_clone(target)?;
        Ok(Arc::new(target.clone()))
    }

    fn storage_replacement(&mut self, decoder: &FullDecoder<'a>, value: usize, op: crate::syntax::node::AssignOp, expected: Arc<Type>) -> Result<(), IrVerifyError> {
        use crate::syntax::node::{AssignOp, BinaryOp};
        if op == AssignOp::Set || *expected == Type::Any {
            self.expression(decoder.owner, value, expected, false);
            return Ok(());
        }
        let actual = self.need_context(decoder, value)?.ok_or_else(|| IrVerifyError::new("compound storage writer has no operand contract"))?;
        let binary = match op { AssignOp::Set => unreachable!(), AssignOp::Add => BinaryOp::Add, AssignOp::Sub => BinaryOp::Sub, AssignOp::Mul => BinaryOp::Mul, AssignOp::Div => BinaryOp::Div, AssignOp::Rem => BinaryOp::Rem };
        let result = FullDecoder::checked_binary_result_type(binary, &expected, &actual).ok_or_else(|| IrVerifyError::new("compound storage writer has incompatible operands"))?;
        if !result.matches_expected(&expected) { return Err(IrVerifyError::new("compound storage writer changes its declaration type")); }
        self.expression(decoder.owner, value, actual, false);
        Ok(())
    }

    fn verify_storage_driver_assignment(&mut self, step: usize, binding: u32, expected: Arc<Type>) -> Result<(), IrVerifyError> {
        let decoder = self.owner(driver_owner(step).map_err(|_| IrVerifyError::new("storage assignment owner is invalid"))?)?;
        let row = self.store.driver_steps[step];
        let mut payload = super::super::FullCursor::new(self.store.payload(row.data.range())?);
        let _: Name = super::super::FullCodec::decode(&decoder, &mut payload)?;
        if payload.raw()? != binding { return Err(IrVerifyError::new("storage assignment identity disagrees with its census")); }
        let op = super::super::FullCodec::decode(&decoder, &mut payload)?;
        self.storage_replacement(&decoder, payload.raw()? as usize, op, expected)
    }

    fn verify_storage_statement(&mut self, decoder: &FullDecoder<'a>, instruction: usize, slot: usize, expected: Arc<Type>) -> Result<(), IrVerifyError> {
        match decoder.statement_view(instruction)? {
            schema::StmtView::Assign { value, op, .. } => self.storage_replacement(decoder, value.raw() as usize, op, expected),
            schema::StmtView::AssignInt { .. } => {
                Self::reject_unchecked_unsigned_integer_writer(&expected)?;
                if !Type::Int.matches_expected(&expected) { return Err(IrVerifyError::new("integer writer disagrees with its storage contract")); }
                self.verify_operands(decoder, instruction)
            }
            schema::StmtView::AssignBool { .. } => {
                if !Type::Bool.matches_expected(&expected) { return Err(IrVerifyError::new("boolean writer disagrees with its storage contract")); }
                self.verify_operands(decoder, instruction)
            },
            schema::StmtView::AssignField { field, value, op, .. } => {
                let target = self.storage_assignment_target(&expected, &[super::super::CallableAssignmentPathStep::Field(Name::intern(field))])?;
                self.storage_replacement(decoder, value.raw() as usize, op, target)
            }
            schema::StmtView::AssignFieldInt { field, .. } => {
                let target = self.storage_assignment_target(&expected, &[super::super::CallableAssignmentPathStep::Field(Name::intern(field))])?;
                Self::reject_unchecked_unsigned_integer_writer(&target)?;
                if !Type::Int.matches_expected(&target) { return Err(IrVerifyError::new("integer field writer disagrees with its storage contract")); }
                self.verify_operands(decoder, instruction)
            }
            schema::StmtView::AssignPath { mut path, value, op, .. } => {
                let mut target = Arc::clone(&expected);
                for step in &mut path {
                    self.budget.charge(1)?;
                    match step? {
                        super::super::decode::AssignStepView::Field(name) => target = self.storage_assignment_target(&target, &[super::super::CallableAssignmentPathStep::Field(name)])?,
                        super::super::decode::AssignStepView::Index(index) => {
                            let key = match FullDecoder::assignment_receiver_type(&target) {
                                Type::List(_) => Arc::new(Type::Int),
                                Type::Map(key, _) => { self.budget.charge_clone(key)?; Arc::new((**key).clone()) },
                                _ => return Err(IrVerifyError::new("storage writer index has no declared collection")),
                            };
                            self.expression(decoder.owner, index.raw() as usize, key, false);
                            target = self.storage_assignment_target(&target, &[super::super::CallableAssignmentPathStep::Index])?;
                        }
                    }
                }
                self.storage_replacement(decoder, value.raw() as usize, op, target)
            }
            _ => self.verify_slot_writer(decoder, slot, expected, instruction),
        }
    }

    fn binding_writer(&mut self, binding: u32, expected: Arc<Type>) -> Result<(), IrVerifyError> {
        self.storage_contract(binding, expected, StorageContractRelation::ReadCompatible)
    }

    fn storage_contract(&mut self, binding: u32, expected: Arc<Type>, relation: StorageContractRelation) -> Result<(), IrVerifyError> {
        self.budget.charge(1)?;
        let contracts = self.storage.entry(binding).or_default();
        self.budget.charge(contracts.len())?;
        if contracts.iter().any(|(previous, previous_relation)| previous == &expected && (*previous_relation == StorageContractRelation::ExactDeclaration || relation == StorageContractRelation::ReadCompatible)) { return Ok(()); }
        ensure_closed_type(&expected)?;
        contracts.push((Arc::clone(&expected), relation));
        self.pending.push(Obligation::Storage(binding, expected, relation));
        Ok(())
    }

    fn verify_storage(&mut self, binding: u32, expected: Arc<Type>, relation: StorageContractRelation) -> Result<(), IrVerifyError> {
        let identity = self.store.binding_identities.get(binding as usize).ok_or_else(|| IrVerifyError::new("ground capture identity is invalid"))?;
        if identity.flags & 1 != 0 {
            if self.store.string(identity.name)? == "args" && matches!(expected.as_ref(), Type::List(item) if matches!(item.as_ref(), Type::Str | Type::Bytes)) { return Ok(()); }
            return Err(IrVerifyError::new("ground capture has an invalid builtin binding"));
        }
        let (_, step, _) = self.index.binding_writers.get(binding as usize).and_then(|writer| *writer).ok_or_else(|| IrVerifyError::new("ground capture has no verified lexical writer"))?;
        let owner = driver_owner(step).map_err(|_| IrVerifyError::new("ground capture writer owner is invalid"))?;
        let decoder = self.owner(owner)?;
        let driver = self.store.driver_steps[step];
        if driver.tag == super::super::FullDriverTag::Use {
            if !matches!(expected.as_ref(), Type::Module(_) | Type::DynamicModule) {
                return Err(IrVerifyError::new("ground import producer has no module contract"));
            }
            let exports = self.index.module_exports.get(&owner).cloned().ok_or_else(|| IrVerifyError::new("ground import producer has no verified export census"))?;
            match TypeId::from_raw(driver.registered_module_type) {
                Some(type_id) => {
                    let actual = self.contract_type(type_id)?;
                    let Type::Module(module) = actual.as_ref() else { return Err(IrVerifyError::new("ground import registration has no module contract")) };
                    if *expected != Type::DynamicModule && !actual.matches_expected(&expected) {
                        return Err(IrVerifyError::new("ground import registration disagrees with its read contract"));
                    }
                    for (name, binding) in exports {
                        self.budget.charge(1)?;
                        let ty = module.exports.get(&name).and_then(crate::sema::types::ModuleExportType::field_value_type).ok_or_else(|| IrVerifyError::new("ground module export has no registered value contract"))?;
                        self.budget.charge_clone(&ty)?;
                        self.binding_writer(binding, Arc::new(ty))?;
                    }
                }
                None if *expected == Type::DynamicModule => {}
                None => return Err(IrVerifyError::new("ground import producer has no registered module contract")),
            }
        }
        let mut payload = super::super::FullCursor::new(self.store.payload(driver.data.range())?);
        let mut root_contract = Arc::clone(&expected);
        match driver.tag {
            super::super::FullDriverTag::Use => {}
            super::super::FullDriverTag::Let => {
                let _: Name = super::super::FullCodec::decode(&decoder, &mut payload)?;
                let _: Option<super::super::LoweredType> = super::super::FullCodec::decode(&decoder, &mut payload)?;
                let context = super::super::DecodeContext::from_decoder(&decoder);
                let validation = Option::<super::super::LoweredTypeCheck>::borrow(context, &mut payload)?;
                let annotation = Option::<Type>::borrow(context, &mut payload)?;
                let _: bool = super::super::FullCodec::decode(&decoder, &mut payload)?;
                let value = payload.raw()? as usize;
                root_contract = match annotation {
                    Some(annotation) => self.contract_type(annotation.id)?,
                    None => match self.need_context(&decoder, value)? {
                        Some(context) => context,
                        None if relation == StorageContractRelation::ReadCompatible => Arc::clone(&expected),
                        None => return Err(IrVerifyError::new("storage initializer has no monomorphic contract")),
                    },
                };
                ensure_closed_type(&root_contract)?;
                let validates = match validation {
                    Some(check) if check.schema.is_none() => *self.contract_type(check.ty.id)? == *root_contract
                        && super::runtime_complete_storage_check_type(&root_contract, &mut self.budget)? ,
                    _ => false,
                };
                if !validates { self.expression(owner, value, Arc::clone(&root_contract), false); }
            }
            super::super::FullDriverTag::LetRecord => {
                root_contract = self.driver_output_types(step)?.get(&binding).cloned().ok_or_else(|| IrVerifyError::new("storage destructuring has no slot projection"))?;
            }
            _ => return Err(IrVerifyError::new("ground capture has no lexical defining writer")),
        }
        match relation {
            StorageContractRelation::ReadCompatible if !root_contract.matches_expected(&expected) => return Err(IrVerifyError::new("storage declaration disagrees with its read contract")),
            StorageContractRelation::ExactDeclaration if *root_contract != *expected => return Err(IrVerifyError::new("mutable capture contract differs from its storage declaration")),
            _ => {}
        }
        for writer in self.index.storage_writers.get(&binding).cloned().unwrap_or_default() {
            self.budget.charge(1)?;
            match writer {
                StorageWriter::Statement { owner, instruction, slot, root_type } => {
                    let actual = self.contract_type(root_type)?;
                    if *actual != *root_contract { return Err(IrVerifyError::new("storage writer root disagrees with its declaration")); }
                    let decoder = self.owner(owner)?;
                    self.verify_storage_statement(&decoder, instruction, slot, Arc::clone(&root_contract))?;
                }
                StorageWriter::DriverAssign { step } => self.verify_storage_driver_assignment(step, binding, Arc::clone(&root_contract))?,
            }
        }
        Ok(())
    }
}

fn comp_slot_projection(target: super::super::decode::CompTargetView<'_>, slot: usize, ty: &Type,
    budget: &mut SchemeInstantiationBudget,
) -> Result<Option<(Type, Vec<Name>)>, IrVerifyError> {
    let budget = RefCell::new(budget);
    let mut selected = None;
    FullDecoder::visit_comp_target_evidence(target, (ty, Vec::new()),
        |(source, _)| if matches!(source.unvalidated(), Type::Record(_)) { Ok(()) } else { Err(IrVerifyError::new("ground record target has a non-record source")) },
        |(source, path), name| {
            let Type::Record(fields) = source.unvalidated() else { return Ok(None) };
            let Some(field) = fields.get(&name) else { return Ok(None) };
            budget.borrow_mut().charge(path.len() + 1)?;
            let mut path = path.clone();
            path.push(name);
            Ok(Some((field, path)))
        },
        |bound, (source, path)| {
            if bound == slot {
                budget.borrow_mut().charge_clone(source)?;
                budget.borrow_mut().charge(path.len())?;
                selected = Some(((*source).clone(), path.clone()));
            }
            Ok(())
        },
        |amount| budget.borrow_mut().charge(amount))?;
    Ok(selected)
}

fn comp_slot_type(target: super::super::decode::CompTargetView<'_>, slot: usize, ty: &Type) -> Result<Option<Type>, IrVerifyError> {
    let target = target.materialize()?;
    let Some(path) = FullDecoder::checked_target_field_path(&target, slot) else { return Ok(None) };
    let mut current = ty;
    for field in &path {
        let Type::Record(fields) = current.unvalidated() else { return Ok(None) };
        let Some(next) = fields.get(field) else { return Ok(None) };
        current = next;
    }
    Ok(Some(current.clone()))
}

fn function_index(owner: u32) -> Result<usize, IrVerifyError> {
    IrFunctionId::from_raw(owner).map(IrFunctionId::index).ok_or_else(|| IrVerifyError::new("ground producer function owner is invalid"))
}

fn function_owner(index: usize) -> Result<u32, IrVerifyError> {
    IrFunctionId::new(index).map(IrFunctionId::raw).map_err(|_| IrVerifyError::new("ground producer function index is invalid"))
}

#[cfg(test)]
mod declared_producer_fact_tests {
    use super::*;

    #[test]
    fn readonly_argv_requires_declared_permission_and_closed_word_items() {
        let parameter = Type::List(Box::new(Type::process_command_argv_item_type().clone()));
        let readonly = NativeParameterDomain::declared(&parameter);
        assert!(readonly.accepts(&parameter, &Type::List(Box::new(Type::Str))));
        assert!(readonly.accepts(&parameter, &Type::List(Box::new(Type::Path))));
        for item in [Type::Unknown, Type::Invalid, Type::Any, Type::Int] {
            assert!(!readonly.accepts(&parameter, &Type::List(Box::new(item))));
        }
        let invariant = NativeParameterDomain::declared(&Type::List(Box::new(Type::Any)));
        assert!(!invariant.accepts(&parameter, &Type::List(Box::new(Type::Str))));
    }

    #[test]
    fn facts_reject_owner_or_instruction_substitution() {
        let owner = function_owner(0).unwrap();
        let fact = || DeclaredProducerFact { owner, instruction: 7, ty: Arc::new(Type::Int), binding: None };
        assert_eq!(*fact().into_evidence(owner, 7).unwrap().0, Type::Int);
        for (actual_owner, actual_instruction) in [(function_owner(1).unwrap(), 7), (owner, 8)] {
            let error = fact().into_evidence(actual_owner, actual_instruction).unwrap_err();
            assert_eq!(error.message, "declared producer fact belongs to another expression");
        }
    }
}
