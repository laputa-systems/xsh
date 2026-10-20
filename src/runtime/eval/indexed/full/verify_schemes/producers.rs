use super::{
    BTreeMap, BTreeSet, CheckedArgumentType, FullCallableContract, FullDecoder, FullStore, FullTag,
    IR_NONE, IrFunctionId, IrVerifyError, Name, SchemeInstantiationBudget, Type, TypeId, bounded,
    checked_argument_type, checked_expression_type_id, decoder_for_driver_step,
    decoder_for_function, driver_owner, ensure_closed_type, ensure_substitution_type, schema,
    merge_list_item_types,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};
#[cfg(test)]
use std::cell::Cell;
use super::super::decode::BorrowCodec;

pub(in crate::runtime::eval::indexed::full) struct GroundProducerIndex {
    demand: crate::runtime::eval::ProducerDemand,
    result_constructors: Vec<usize>,
    owners: BTreeMap<u32, ProducerOwner>,
    starts: BTreeMap<usize, u32>,
    binding_writers: Vec<Option<(usize, usize, u32)>>,
    module_exports: BTreeMap<u32, Vec<(Name, u32)>>,
    function_signatures: Option<Arc<super::super::FunctionSignatureIndex>>,
    work: Option<Rc<RefCell<SchemeInstantiationBudget>>>,
}

struct ProducerOwner {
    prepared_constant_work: usize,
    work: Rc<RefCell<SchemeInstantiationBudget>>,
    range: std::ops::Range<usize>,
    children: Vec<u32>,
    siblings: Vec<u32>,
    writers: BTreeMap<ProducerBinding, Vec<usize>>,
    parents: Vec<Option<usize>>,
    statement_positions: BTreeMap<usize, (usize, usize)>,
    block_declarations: BTreeMap<(usize, usize), Vec<(usize, ProducerBinding)>>,
    #[cfg(test)]
    binding_work: Cell<(usize, usize, usize)>,
    #[cfg(test)]
    census_instruction_visits: usize,
    block_statements: BTreeMap<usize, Vec<usize>>,
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
    return_analysis: Rc<super::super::IndexedReturnAnalysis>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum CallableProducer {
    Expression(usize),
    PipelinePrefix(usize, usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum ProducerBinding {
    HeaderParameter(usize),
    HeaderCapture(u32),
    ScopedDeclaration(usize, usize),
    DriverBinding(usize),
}

impl ProducerOwner {
    fn header_returns(&self) -> impl Iterator<Item = &usize> {
        self.returns.iter().chain(self.callback_returns.values().flatten())
    }

    fn binding(&self, store: &FullStore, owner: u32, at: usize, slot: usize) -> Result<ProducerBinding, IrVerifyError> {
        self.visible_binding(store, owner, at, slot)?.ok_or_else(|| IrVerifyError::new("ground producer use has no visible lexical declaration"))
    }

    fn visible_binding(&self, store: &FullStore, owner: u32, mut at: usize, slot: usize) -> Result<Option<ProducerBinding>, IrVerifyError> {
        self.work.borrow_mut().charge(1)?;
        #[cfg(test)] { let (queries, parents, comparisons) = self.binding_work.get(); self.binding_work.set((queries + 1, parents, comparisons)); }
        loop {
            let mut root = at;
            loop {
                self.work.borrow_mut().charge(1)?;
                #[cfg(test)] { let (queries, parents, comparisons) = self.binding_work.get(); self.binding_work.set((queries, parents + 1, comparisons)); }
                if let Some(binding) = self.region_bindings.get(&(root, slot)) { return Ok(Some(*binding)); }
                if let Some((block, position)) = self.statement_positions.get(&root) {
                    if let Some(declarations) = self.block_declarations.get(&(*block, slot)) {
                        let (mut end, mut upper) = (0, declarations.len());
                        while end < upper {
                            self.work.borrow_mut().charge(1)?;
                            #[cfg(test)] { let (queries, parents, comparisons) = self.binding_work.get(); self.binding_work.set((queries, parents, comparisons + 1)); }
                            let middle = end + (upper - end) / 2;
                            if declarations[middle].0 < *position { end = middle + 1; } else { upper = middle; }
                        }
                        if let Some(previous) = end.checked_sub(1) { return Ok(Some(declarations[previous].1)); }
                    }
                    break;
                }
                match self.parents[root - self.range.start] {
                    Some(parent) => root = parent,
                    None => break,
                }
            }
            let Some(parent) = self.parents[root - self.range.start] else { break };
            if self.statement_positions.contains_key(&root)
                && self.declarations.get(&parent).is_some_and(|slots| slots.contains(&slot))
            {
                return Ok(Some(ProducerBinding::ScopedDeclaration(parent, slot)));
            }
            at = parent;
        }
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

impl GroundProducerIndex {
    pub(in crate::runtime::eval::indexed::full) fn new(store: &FullStore) -> Result<Self, IrVerifyError> {
        let mut result_constructors = Vec::new();
        for (index, fact) in store.checked_expr_types.iter().enumerate() {
            if !fact.is_contract() { continue; }
            let Some(&tag @ (FullTag::ExprOk | FullTag::ExprErr)) = store.tags.get(fact.instruction as usize) else { continue };
            let ty = store.semantic.to_type(fact.type_id)?;
            if fact.authenticates_result_constructor(tag, &ty)? { result_constructors.push(index); }
        }
        let demand = if !store.registered_calls.is_empty() || !result_constructors.is_empty() {
            crate::runtime::eval::ProducerDemand::Needed
        } else { crate::runtime::eval::ProducerDemand::Absent };
        Ok(Self { demand, result_constructors, owners: BTreeMap::new(), starts: BTreeMap::new(),
            binding_writers: Vec::new(), module_exports: BTreeMap::new(), function_signatures: None, work: None })
    }

    pub(in crate::runtime::eval::indexed::full) fn demand(&self) -> crate::runtime::eval::ProducerDemand {
        self.demand
    }

    pub(in crate::runtime::eval::indexed::full) fn record(&mut self, decoder: &FullDecoder<'_>) -> Result<(), IrVerifyError> {
        if self.demand == crate::runtime::eval::ProducerDemand::Absent { return Ok(()); }
        if self.function_signatures.is_none() { self.function_signatures.clone_from(&decoder.function_signature_index); }
        let work = Rc::clone(self.work.get_or_insert_with(|| Rc::new(RefCell::new(SchemeInstantiationBudget::new(decoder.store)))));
        let range = decoder.instruction_range.clone();
        let return_analysis = decoder.share_return_analysis()?;
        work.borrow_mut().charge(range.len() + return_analysis.uncaught_try_roots.len() + return_analysis.loop_transfers.len() + return_analysis.loop_values.as_ref().map_or(0, |values| values.len() + values.values().map(Vec::len).sum::<usize>()))?;
        let mut owner = ProducerOwner { prepared_constant_work: decoder.prepared_constant_work.get(), work: Rc::clone(&work), children: vec![IR_NONE; range.len()], siblings: vec![IR_NONE; range.len()], writers: BTreeMap::new(), parents: vec![None; range.len()], statement_positions: BTreeMap::new(), block_declarations: BTreeMap::new(), #[cfg(test)] binding_work: Cell::new((0, 0, 0)), #[cfg(test)] census_instruction_visits: 0, block_statements: BTreeMap::new(), declarations: BTreeMap::new(), following_declarations: BTreeMap::new(), region_bindings: BTreeMap::new(), pattern_sources: BTreeMap::new(), with_errors: decoder.checked_with_initializer_error_types()?, comprehension_sources: BTreeMap::new(), pipeline_sources: BTreeMap::new(), callback_returns: BTreeMap::new(), pipeline_groups: BTreeMap::new(), pipeline_stages: BTreeMap::new(), unreachable_roots: BTreeSet::new(), returns: Vec::new(), return_analysis, range: range.clone() };
        let mut roots = Vec::new();
                for instruction in range.clone() {
            #[cfg(test)] { owner.census_instruction_visits += 1; }
            work.borrow_mut().charge(1)?;
            owner.parents[instruction - range.start] = decoder.instruction_parent_of(instruction)?;
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
            for &block in blocks.states.borrow().keys() {
                work.borrow_mut().charge(1)?;
                if decoder.store.blocks[block].flags & super::super::BLOCK_STATEMENTS == 0 { continue; }
                let statements = decoder.store.payload(decoder.store.blocks[block].instructions)?.iter().skip(1).map(|word| *word as usize).collect::<Vec<_>>();
                let mut reachable = true;
                for (position, &statement) in statements.iter().enumerate() {
                    work.borrow_mut().charge(1)?;
                    owner.statement_positions.insert(statement, (block, position));
                    if reachable { reachable = !super::super::indexed_stmt_can_return_with_decoder(decoder.store, statement, Some(decoder))?; }
                    else { owner.unreachable_roots.insert(statement); }
                }
                owner.block_statements.insert(block, statements);
            }
        }
        for (&block, statements) in &owner.block_statements {
            for (position, statement) in statements.iter().enumerate() {
                work.borrow_mut().charge(1)?;
                for &slot in owner.following_declarations.get(statement).into_iter().flatten() {
                    owner.block_declarations.entry((block, slot)).or_default().push((position, ProducerBinding::ScopedDeclaration(*statement, slot)));
                }
            }
        }
        let mut callback_anchors = BTreeMap::new();
        for instruction in range.clone() {
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
        if super::super::driver_owner_index(decoder.owner).is_none() {
            for (&binding, source) in &owner.pipeline_sources {
                let ProducerBinding::ScopedDeclaration(_, slot) = binding else { unreachable!("pipeline producer has a header binding") };
                if owner.visible_binding(decoder.store, decoder.owner, source.pipeline, slot)?.is_some() {
                    return Err(IrVerifyError::new("pipeline producer declaration overwrites an active lexical slot"));
                }
            }
            for (&binding, source) in &owner.pattern_sources {
                let ProducerBinding::ScopedDeclaration(_, slot) = binding else { unreachable!("pattern producer has a header binding") };
                if let Some(active) = owner.visible_binding(decoder.store, decoder.owner, source.source, slot)? {
                    return Err(IrVerifyError::new(format!("pattern producer {binding:?} overwrites active lexical slot {active:?}")));
                }
            }
            for (&binding, &(_, iter)) in &owner.comprehension_sources {
                let ProducerBinding::ScopedDeclaration(_, slot) = binding else { unreachable!("comprehension producer has a header binding") };
                if owner.visible_binding(decoder.store, decoder.owner, iter, slot)?.is_some() {
                    return Err(IrVerifyError::new("comprehension producer declaration overwrites an active lexical slot"));
                }
            }
            for (&declaration, slots) in &owner.declarations {
                for &slot in slots {
                    if owner.visible_binding(decoder.store, decoder.owner, declaration, slot)?.is_some() {
                        return Err(IrVerifyError::new("scoped producer declaration overwrites an active lexical slot"));
                    }
                }
            }
        }
        let mut pending = roots.into_iter().map(|instruction| (instruction, None, true)).collect::<Vec<_>>();
        while let Some((instruction, callback, reachable)) = pending.pop() {
            #[cfg(test)] { owner.census_instruction_visits += 1; }
            work.borrow_mut().charge(1)?;
            if decoder.store.tags[instruction].kind() == schema::InstructionKind::Stmt {
                for slot in decoder.statement_written_slots(instruction)?.0 {
                    let binding = if owner.declarations.get(&instruction).is_some_and(|slots| slots.contains(&slot)) {
                        ProducerBinding::ScopedDeclaration(instruction, slot)
                    } else {
                        owner.binding(decoder.store, decoder.owner, instruction, slot)?
                    };
                    owner.writers.entry(binding).or_default().push(instruction);
                }
            }
            let reachable = reachable && !owner.unreachable_roots.contains(&instruction);
            let callback = callback_anchors.get(&instruction).copied().or(callback);
            if reachable && decoder.store.tags[instruction] == FullTag::StmtReturn {
                match callback {
                    Some(anchor) => owner.callback_returns.entry(anchor).or_default().push(instruction),
                    None => owner.returns.push(instruction),
                }
            }
            let mut child = owner.children[instruction - range.start];
            while child != IR_NONE {
                pending.push((child as usize, callback, reachable));
                child = owner.siblings[child as usize - range.start];
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
            + self.result_constructors.capacity() * size_of::<usize>()
            + self.work.as_ref().map_or(0, |_| size_of::<RefCell<SchemeInstantiationBudget>>() + 2 * size_of::<usize>())
            + self.starts.len() * size_of::<(usize, u32)>()
            + self.binding_writers.capacity() * size_of::<Option<(usize, usize, u32)>>()
            + self.module_exports.len() * size_of::<(u32, Vec<(Name, u32)>)>()
            + self.module_exports.values().map(|values| values.capacity() * size_of::<(Name, u32)>()).sum::<usize>()
            + self.owners.values().map(|owner| {
                size_of::<(u32, ProducerOwner)>()
                    + owner.children.capacity() * size_of::<u32>() + owner.siblings.capacity() * size_of::<u32>()
                    + owner.parents.capacity() * size_of::<Option<usize>>() + owner.returns.capacity() * size_of::<usize>()
                    + owner.statement_positions.len() * size_of::<(usize, (usize, usize))>()
                    + owner.block_statements.len() * size_of::<(usize, Vec<usize>)>()
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
                    + owner.pattern_sources.len() * size_of::<(ProducerBinding, PatternProducer)>()
                    + owner.pattern_sources.values().map(|source| source.preceding.capacity() * size_of::<(u32, bool)>()).sum::<usize>()
                    + owner.comprehension_sources.len() * size_of::<(ProducerBinding, (usize, usize))>()
                    + owner.with_errors.len() * size_of::<(usize, Type)>()
                    + owner.with_errors.values().map(Type::retained_bytes).sum::<usize>()
                    + owner.pipeline_sources.len() * size_of::<(ProducerBinding, PipelineProducer)>()
                    + owner.callback_returns.len() * size_of::<(usize, Vec<usize>)>()
                    + owner.callback_returns.values().map(|values| values.capacity() * size_of::<usize>()).sum::<usize>()
                    + owner.pipeline_groups.len() * size_of::<((usize, usize, bool), usize)>()
                    + size_of::<super::super::IndexedReturnAnalysis>()
                    + 2 * size_of::<usize>()
                    + owner.return_analysis.uncaught_try_roots.capacity() * size_of::<usize>()
                    + owner.return_analysis.loop_transfers.capacity() * size_of::<(usize, u8)>()
                    + owner.return_analysis.loop_values.as_ref().map_or(0, |values| values.capacity() * size_of::<(usize, Vec<usize>)>() + values.values().map(|values| values.capacity() * size_of::<usize>()).sum::<usize>())
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

#[derive(Clone)]
enum Obligation {
    Function(usize),
    Expression(u32, usize, Arc<Type>, bool),
    Slot(u32, ProducerBinding, usize, Arc<Type>),
    PipelinePrefix(u32, usize, usize, Arc<Type>),
}

pub(in crate::runtime::eval::indexed::full) enum ProducerContexts<'a> {
    Pending(&'a [(u32, super::super::ExecutableOwner, Arc<Type>)]),
    Published,
}

struct ProducerProof<'a> {
    store: &'a FullStore,
    contexts: ProducerContexts<'a>,
    used_contexts: BTreeMap<u32, (super::super::ExecutableOwner, Arc<Type>)>,
    index: &'a GroundProducerIndex,
    pending: Vec<Obligation>,
    functions: BTreeSet<usize>,
    expressions: BTreeMap<(u32, usize), Vec<(Arc<Type>, bool)>>,
    slots: BTreeMap<(u32, ProducerBinding), Vec<Arc<Type>>>,
    prefixes: BTreeMap<(u32, usize, usize), Vec<Arc<Type>>>,
    decoders: BTreeMap<u32, Rc<FullDecoder<'a>>>,
    types: BTreeMap<TypeId, Arc<Type>>,
    declared_types: BTreeMap<(u32, usize), ProducerTypeDerivation>,
    incoming: BTreeMap<usize, Vec<usize>>,
    budget: SchemeInstantiationBudget,
}

pub(super) fn verify<'a>(store: &'a FullStore, index: &'a GroundProducerIndex, contexts: ProducerContexts<'a>) -> Result<Vec<(u32, super::super::ExecutableOwner, Arc<Type>)>, IrVerifyError> {
    if index.demand == crate::runtime::eval::ProducerDemand::Absent
        && store.checked_expr_types.iter().any(|fact| !fact.is_contract())
    {
        return Err(IrVerifyError::new("producer context has no selected operation"));
    }
    if index.demand == crate::runtime::eval::ProducerDemand::Absent { return Ok(Vec::new()); }
    let mut proof = ProducerProof { store, index, contexts, used_contexts: BTreeMap::new(), pending: Vec::new(), functions: BTreeSet::new(), expressions: BTreeMap::new(), slots: BTreeMap::new(), prefixes: BTreeMap::new(), decoders: BTreeMap::new(), types: BTreeMap::new(), declared_types: BTreeMap::new(), incoming: BTreeMap::new(), budget: SchemeInstantiationBudget::for_prepared_constants(store, index.owners.values().fold(0usize, |work, owner| work.saturating_add(owner.prepared_constant_work))) };
    for call in &store.checked_call_targets { proof.incoming.entry(call.function.index()).or_default().push(call.instruction as usize); }
    for &fact_index in &index.result_constructors {
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
            proof.slot(decoder.owner, decoder.instruction_range.start, (capture.slot_and_flags & !(1 << 31)) as usize, ty)?;
        }
        for instruction in decoder.instruction_range.clone() {
            match store.tags[instruction] {
                FullTag::ExprCall | FullTag::ExprDirectPureCall => {
                    let (function, arguments) = call_parts(&decoder, instruction)?;
                    if matches!(store.functions[function].contract, FullCallableContract::Ground(_)) {
                        proof.function(function);
                        proof.call_arguments(&decoder, function, &arguments)?;
                    }
                }
                _ => {}
            }
        }
    }
    for instantiation in &store.call_instantiations {
        let owner = index.instruction_owner(instantiation.instruction as usize)?;
        if super::super::driver_owner_index(owner).is_none() && matches!(store.functions[function_index(owner)?].contract, FullCallableContract::Scheme(_)) { continue; }
        let decoder = proof.owner(owner)?;
        let (_, arguments) = call_parts(&decoder, instantiation.instruction as usize)?;
        for argument in arguments {
            let (ProducerArgument::Single(argument) | ProducerArgument::Splice(argument)) = argument else { continue };
            let ty = proof.need_context(&decoder, argument)?.ok_or_else(|| IrVerifyError::new("generic call argument has no ground producer contract"))?;
            proof.expression(owner, argument, ty, false);
        }
    }
    proof.finish()?;
    if let Some(work) = &index.work {
        let initial = SchemeInstantiationBudget::new(store).remaining;
        proof.budget.charge(initial - work.borrow().remaining)?;
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

enum ProducerTypeDerivation {
    Pending,
    Resolved(Option<Arc<Type>>),
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
        schema::ExprView::Call { args, .. } | schema::ExprView::DirectPureCall { args, .. } | schema::ExprView::SelfCall { args, .. } | schema::ExprView::TypedCall { args, .. } => args,
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

impl<'a> ProducerProof<'a> {
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
        decoder.producer_demand = self.index.demand;
        decoder.function_signature_index.clone_from(&self.index.function_signatures);
        *decoder.return_analysis.borrow_mut() = Some(super::super::ReturnAnalysisSource::Shared(Rc::clone(&self.index.owners[&owner].return_analysis)));
        let decoder = Rc::new(decoder);
        self.decoders.insert(owner, Rc::clone(&decoder));
        Ok(decoder)
    }

    fn function(&mut self, function: usize) {
        if self.functions.insert(function) { self.pending.push(Obligation::Function(function)); }
    }

    fn expression(&mut self, owner: u32, instruction: usize, expected: Arc<Type>, returned: bool) {
        let previous = self.expressions.entry((owner, instruction)).or_default();
        if previous.iter().any(|(ty, transport)| **ty == *expected && *transport == returned) { return; }
        previous.push((Arc::clone(&expected), returned));
        self.pending.push(Obligation::Expression(owner, instruction, expected, returned));
    }

    fn slot(&mut self, owner: u32, at: usize, slot: usize, expected: Arc<Type>) -> Result<(), IrVerifyError> {
        let binding = self.index.owners[&owner].binding(self.store, owner, at, slot)?;
        let previous = self.slots.entry((owner, binding)).or_default();
        if previous.iter().any(|ty| **ty == *expected) { return Ok(()); }
        previous.push(Arc::clone(&expected));
        self.pending.push(Obligation::Slot(owner, binding, slot, expected));
        Ok(())
    }

    fn declared_type(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        self.budget.charge(1)?;
        let key = (decoder.owner, instruction);
        if let Some(derivation) = self.declared_types.get(&key) {
            return match derivation {
                ProducerTypeDerivation::Pending => Err(IrVerifyError::new("ground producer type derivation is cyclic")),
                ProducerTypeDerivation::Resolved(ty) => Ok(ty.clone()),
            };
        }
        self.declared_types.insert(key, ProducerTypeDerivation::Pending);
        let ty = self.derive_declared_type(decoder, instruction)?;
        self.declared_types.insert(key, ProducerTypeDerivation::Resolved(ty.clone()));
        Ok(ty)
    }

    fn derive_declared_type(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        if let Some(id) = checked_expression_type_id(self.store, instruction as u32) { return self.contract_type(id).map(Some); }
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
            schema::ExprView::ValueBlock { mut body, .. } => {
                return match decoder.derive_checked_block_tail_expression(&mut body)? {
                    Some(tail) => self.need_context(decoder, tail),
                    None => Ok(None),
                };
            }
            schema::ExprView::IfExpr { branches, else_value, .. } => {
                return FullDecoder::checked_branch_result_type_with(
                    branches.map(|branch| branch.map(|(_, value)| value.raw() as usize))
                        .chain(std::iter::once(Ok(else_value.raw() as usize)))
                        .map(|instruction| instruction.and_then(|instruction| decoder.checked_branch_value_instruction(instruction)))
                        .filter_map(Result::transpose),
                    |instruction| self.need_context(decoder, instruction),
                );
            }
            schema::ExprView::PatternIf { branches, else_value, .. } => {
                return FullDecoder::checked_branch_result_type_with(
                    branches.map(|branch| branch.map(|(_, value, _)| value.raw() as usize))
                        .chain(std::iter::once(Ok(else_value.raw() as usize)))
                        .map(|instruction| instruction.and_then(|instruction| decoder.checked_branch_value_instruction(instruction)))
                        .filter_map(Result::transpose),
                    |instruction| self.need_context(decoder, instruction),
                );
            }
            schema::ExprView::MatchExpr { mut arms, .. } => {
                let mut output: Option<Arc<Type>> = None;
                for arm in &mut arms {
                    let Some(ty) = self.need_context(decoder, arm?.2.raw() as usize)? else { return Ok(None) };
                    match &output {
                        Some(previous) if !ty.matches_expected(previous) => {
                            if previous.matches_expected(&ty) { output = Some(ty); } else { return Ok(None); }
                        }
                        None => output = Some(ty),
                        _ => {}
                    }
                }
                return Ok(output);
            }
            schema::ExprView::StrByteLen { .. } => Some(Type::Int),
            schema::ExprView::StrByteAt { .. } => FullDecoder::registered_method_return_type(&Type::Str, "byte_at")?,
            schema::ExprView::StrPredicate { .. } => Some(Type::Bool),
            schema::ExprView::List(mut values) => {
                let mut items = Vec::new();
                for value in &mut values {
                    let Some(item) = self.need_context(decoder, value?.raw() as usize)? else { return Ok(None) };
                    items.push((*item).clone());
                }
                return merge_list_item_types(&items, &mut self.budget).map(|ty| ty.map(Arc::new));
            }
            schema::ExprView::Require { check, .. } => Some(Type::Result(Box::new(check.ty.materialize()?), Box::new(Type::Error))),
            schema::ExprView::CheckedValue { check, .. } => Some(check.ty.materialize()?),
            schema::ExprView::Tag { type_name, .. } => Some(Type::Tag(type_name)),
            schema::ExprView::ComparisonChain { .. } => Some(Type::Bool),
            schema::ExprView::Binary { op: super::super::BinaryOp::Eq | super::super::BinaryOp::Ne | super::super::BinaryOp::Lt | super::super::BinaryOp::Le | super::super::BinaryOp::Gt | super::super::BinaryOp::Ge | super::super::BinaryOp::And | super::super::BinaryOp::Or | super::super::BinaryOp::In | super::super::BinaryOp::NotIn, .. } => Some(Type::Bool),
            schema::ExprView::Binary { op, left, right, .. } => {
                let Some(left) = self.need_context(decoder, left.raw() as usize)? else { return Ok(None) };
                let Some(right) = self.need_context(decoder, right.raw() as usize)? else { return Ok(None) };
                FullDecoder::checked_binary_result_type(op, left.unvalidated(), right.unvalidated())
            }
            schema::ExprView::ListPipeline { .. } => return self.pipeline_type(decoder, instruction),
            schema::ExprView::Param(slot) => {
                let binding = self.index.owners[&decoder.owner].binding(self.store, decoder.owner, instruction, slot)?;
                if let Some(source) = self.index.owners[&decoder.owner].pattern_sources.get(&binding) {
                    let (subject, pattern, preceding) = (source.source, source.pattern, source.preceding.clone());
                    let Some(subject) = self.need_context(decoder, subject)? else { return Ok(None) };
                    let mut subject = Some((*subject).clone());
                    for (pattern, guarded) in preceding { subject = decoder.subject_after_arm(pattern, guarded, subject)?; }
                    return decoder.checked_pattern_bound_type_with_budget(pattern, slot, subject.as_ref(), |amount| self.budget.charge(amount)).map(|ty| ty.map(Arc::new));
                }
                if self.index.owners[&decoder.owner].comprehension_sources.contains_key(&binding) || self.index.owners[&decoder.owner].pipeline_sources.contains_key(&binding) { return Ok(None); }
                if let ProducerBinding::ScopedDeclaration(declaration, _) = binding {
                    return match decoder.statement_view(declaration)? {
                        schema::StmtView::Let { annotation: Some(annotation), .. } => self.contract_type(annotation.id).map(Some),
                        schema::StmtView::Let { value, .. } => self.declared_type(decoder, value.raw() as usize),
                        schema::StmtView::LetInt { .. } => Ok(Some(Arc::new(Type::Int))),
                        schema::StmtView::LetBool { .. } => Ok(Some(Arc::new(Type::Bool))),
                        schema::StmtView::For { iter, .. } => {
                            let Some(source) = self.need_context(decoder, iter.raw() as usize)? else { return Ok(None) };
                            self.budget.charge_clone(&source)?;
                            Ok(source.iteration_item_type().map(Arc::new))
                        }
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
                if let Some(id) = decoder.checked_slot_type(slot)? { return self.contract_type(id).map(Some); }
                return Ok(None);
            }
            schema::ExprView::Call { function, .. } | schema::ExprView::DirectPureCall { function, .. } => return self.contract_type(decoder.function_return_type_at(instruction, function.id.index())?).map(Some),
            schema::ExprView::SelfCall { .. } if super::super::driver_owner_index(decoder.owner).is_none() => return self.contract_type(decoder.function_return_type_at(instruction, function_index(decoder.owner)?)?).map(Some),
            schema::ExprView::ModuleCall { op, call_plan, args, .. } => {
                let arguments = args.map(|argument| argument.map(|value| value.map(|value| value.raw() as usize))).collect::<Result<Vec<_>, _>>()?;
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
                if let Some(base) = self.need_context(decoder, base.raw() as usize)? {
                    match base.unvalidated() {
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
        if let Some(ty) = self.declared_type(decoder, instruction)? { return Ok(Some(ty)); }
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

    fn binding_initializer(&mut self, binding: u32) -> Result<(u32, usize), IrVerifyError> {
        let (_, step, _) = self.index.binding_writers.get(binding as usize).and_then(|writer| *writer).ok_or_else(|| IrVerifyError::new("callable capture has no lexical initializer"))?;
        let owner = driver_owner(step).map_err(|_| IrVerifyError::new("callable capture initializer owner is invalid"))?;
        let decoder = self.owner(owner)?;
        let row = self.store.driver_steps[step];
        if row.tag != super::super::FullDriverTag::Let { return Err(IrVerifyError::new("callable capture initializer is not a value declaration")); }
        let mut cursor = super::super::FullCursor::new(self.store.payload(row.data.range())?);
        let _: Name = super::super::FullCodec::decode(&decoder, &mut cursor)?;
        let _: Option<super::super::LoweredType> = super::super::FullCodec::decode(&decoder, &mut cursor)?;
        let _: Option<super::super::LoweredTypeCheck> = super::super::FullCodec::decode(&decoder, &mut cursor)?;
        let _: Option<Type> = super::super::FullCodec::decode(&decoder, &mut cursor)?;
        let mutable: bool = super::super::FullCodec::decode(&decoder, &mut cursor)?;
        if mutable { return Err(IrVerifyError::new("callable capture has mutable lexical storage")); }
        Ok((owner, cursor.raw()? as usize))
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

    fn function_targets(&mut self, owner: u32, instruction: usize) -> Result<BTreeSet<usize>, IrVerifyError> {
        let mut visited = BTreeSet::new();
        let mut pending = vec![(owner, CallableProducer::Expression(instruction), Vec::<FunctionProjection>::new())];
        let mut targets = BTreeSet::new();
        let mut parameter_contracts = BTreeSet::new();
        while let Some((owner, source, mut path)) = pending.pop() {
            self.budget.charge(1 + path.len())?;
            if !visited.insert((owner, source, path.clone())) { continue; }
            let decoder = self.owner(owner)?;
            let instruction = match source {
                CallableProducer::Expression(instruction) => instruction,
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
            match decoder.expression_view(instruction)? {
                schema::ExprView::FunctionRef { function, pure } if path.is_empty() => {
                    let (target, _, selected_pure) = decoder.function_reference_target(function)?;
                    if !pure || !selected_pure { return Err(IrVerifyError::new("ground callable producer is not pure")); }
                    targets.insert(target.index());
                }
                schema::ExprView::Param(slot) => {
                    let binding = self.index.owners[&owner].binding(self.store, owner, instruction, slot)?;
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
                                    schema::StmtView::Guard { target, value, optional, .. } => {
                                        let fields = FullDecoder::checked_target_field_path(&target.materialize()?, slot).ok_or_else(|| IrVerifyError::new("callable guard binder has no field projection"))?;
                                        let mut path = path.clone();
                                        path.extend(fields.into_iter().rev().map(FunctionProjection::Field));
                                        if !optional { path.push(FunctionProjection::ResultPayload(true)); }
                                        pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                                    }
                                    schema::StmtView::With { bindings, .. } => {
                                        let mut found = false;
                                        for binding in bindings {
                                            let (bound, value) = binding?;
                                            if bound != slot { continue; }
                                            found = true;
                                            let mut path = path.clone();
                                            if self.need_context(&decoder, value.raw() as usize)?.is_some_and(|ty| ty.is_result()) { path.push(FunctionProjection::ResultPayload(true)); }
                                            pending.push((owner, CallableProducer::Expression(value.raw() as usize), path));
                                        }
                                        if !found { return Err(IrVerifyError::new("callable with binder has no success producer")); }
                                    }
                                    _ => return Err(IrVerifyError::new("ground callable lexical writer is not a value transfer")),
                                }
                            }
                            if !self.index.owners[&owner].writers.contains_key(&binding) && !self.index.owners[&owner].pattern_sources.contains_key(&binding) { return Err(IrVerifyError::new(format!("ground callable declaration {declaration} has no lexical writer"))); }
                        }
                        ProducerBinding::HeaderParameter(parameter) => {
                            let function = function_index(owner)?;
                            let FullCallableContract::Ground(signature) = self.store.functions[function].contract else {
                                return Err(IrVerifyError::new("callable parameter source has no ground signature"));
                            };
                            let (_, parameter_type, _) = self.store.semantic.signature_param(signature, parameter)?;
                            if self.contract_type(parameter_type)?.contains_typed_callable() {
                                parameter_contracts.insert((owner, parameter));
                            }

                            for caller in self.incoming.get(&function).cloned().unwrap_or_default() {
                                let caller_owner = self.index.instruction_owner(caller)?;
                                let caller_decoder = self.owner(caller_owner)?;
                                let values = call_values(&caller_decoder, caller)?;
                                let FullCallableContract::Ground(signature) = self.store.functions[function].contract else { return Err(IrVerifyError::new("callable parameter source has no ground signature")) };
                                let (_, _, flags) = self.store.semantic.signature_param(signature, parameter)?;
                                if flags & 2 != 0 {
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
                            let (source_owner, value) = self.binding_initializer(binding)?;
                            pending.push((source_owner, CallableProducer::Expression(value), path));
                        }
                        ProducerBinding::DriverBinding(slot) => {
                            if let Some(source) = decoder.driver_local_let_source(slot, |amount| self.budget.charge(amount))? {
                                pending.push((driver_owner(source.step).map_err(|_| IrVerifyError::new("callable driver initializer owner is invalid"))?, CallableProducer::Expression(source.initializer), path));
                            } else {
                                let step = super::super::driver_owner_index(owner).ok_or_else(|| IrVerifyError::new("callable driver owner is invalid"))?;
                                let slots = bounded(self.store.driver_steps[step].slots, &self.store.driver_slots, "callable driver binding range is invalid")?;
                                let binding = slots.iter().find(|binding| binding.slot as usize == slot).ok_or_else(|| IrVerifyError::new("callable driver slot has no lexical identity"))?;
                                let (source_owner, value) = self.binding_initializer(binding.binding_identity)?;
                                pending.push((source_owner, CallableProducer::Expression(value), path));
                            }
                        }
                    }
                }
                schema::ExprView::Require { value, .. } | schema::ExprView::CheckedValue { value, .. } | schema::ExprView::Try { value, .. } => pending.push((owner, CallableProducer::Expression(value.raw() as usize), path)),
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
                    if path.last() == Some(&FunctionProjection::ResultPayload(true)) { path.pop(); }
                    match crate::modules::api_spec().method_op(domain, name) {
                        Some(super::super::RuntimeOp::MapGet | super::super::RuntimeOp::ListGet) => path.push(FunctionProjection::Element),
                        Some(super::super::RuntimeOp::RecordGet) => {
                            let argument = args.into_iter().next().ok_or_else(|| IrVerifyError::new("callable record get has no field argument"))??;
                            let field = decoder.checked_literal_string(argument.raw() as usize)?.ok_or_else(|| IrVerifyError::new("callable record get has no literal field"))?;
                            path.push(FunctionProjection::Field(Name::intern(&field)));
                        }
                        _ => return Err(IrVerifyError::new("callable registered method has no value projection")),
                    }
                    pending.push((owner, CallableProducer::Expression(receiver.raw() as usize), path));
                }
                schema::ExprView::Index { base, .. } | schema::ExprView::IndexFromEnd { base, .. } => {
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
        if targets.is_empty() && parameter_contracts.is_empty() { return Err(IrVerifyError::new("ground callable has no executable function target")); }
        Ok(targets)
    }

    fn call_arguments(&mut self, decoder: &FullDecoder<'a>, function: usize, arguments: &[ProducerArgument]) -> Result<(), IrVerifyError> {
        if super::super::driver_owner_index(decoder.owner).is_none() && matches!(self.store.functions[function_index(decoder.owner)?].contract, FullCallableContract::Scheme(_)) { return Ok(()); }
        let FullCallableContract::Ground(signature) = self.store.functions[function].contract else { return Ok(()) };
        let count = self.store.semantic.signature_param_count(signature)?;
        let mut position = 0;
        for &argument in arguments {
            if position >= count { return Err(IrVerifyError::new("ground producer call has excess arguments")); }
            let (_, expected, flags) = self.store.semantic.signature_param(signature, position)?;
            let expected = self.contract_type(expected)?;
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
                    self.expression(decoder.owner, instruction, expected, false);
                }
                ProducerArgument::Splice(instruction) => {
                    if !rest { return Err(IrVerifyError::new("ground producer splice has no rest parameter")); }
                    self.expression(decoder.owner, instruction, expected, false);
                }
            }
            if !rest { position += 1; }
        }
        for index in position..count {
            let (_, _, flags) = self.store.semantic.signature_param(signature, index)?;
            if flags & 3 == 0 { return Err(IrVerifyError::new("ground producer call omits a required argument")); }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), IrVerifyError> {
        while let Some(obligation) = self.pending.pop() {
            self.budget.charge(1)?;
            match obligation {
                Obligation::Function(index) => {
                    let owner = function_owner(index)?;
                    let decoder = self.owner(owner)?;
                    let expected = self.contract_type(decoder.function_return_type(index)?)?;
                    for returned in self.index.owners[&owner].header_returns().copied().collect::<Vec<_>>() {
                        let schema::StmtView::Return { value } = decoder.statement_view(returned)? else { unreachable!("ground producer return index contains a non-return") };
                        self.expression(owner, value.raw() as usize, Arc::clone(&expected), true);
                    }
                }
                Obligation::Expression(owner, instruction, expected, returned) => self.verify_expression(owner, instruction, expected, returned)?,
                Obligation::Slot(owner, binding, slot, expected) => self.verify_slot(owner, binding, slot, expected)?,
                Obligation::PipelinePrefix(owner, pipeline, end, expected) => self.verify_pipeline_prefix(owner, pipeline, end, expected)?,
            }
        }
        Ok(())
    }

    fn verify_expression(&mut self, owner: u32, instruction: usize, expected: Arc<Type>, returned: bool) -> Result<(), IrVerifyError> {
        let decoder = self.owner(owner)?;
        self.budget.charge(1)?;
        if *expected == Type::Any { return Ok(()); }
        let actual = self.declared_type(&decoder, instruction)?;
        if let Some(actual) = &actual {
            let fits = if returned { crate::sema::types::tail_type_matches_expected(&expected, actual) } else { actual.matches_expected(&expected) };
            if !fits {
                return Err(IrVerifyError::new("ground producer value disagrees with its checked contract"));
            }
        }
        let preserves_result = matches!(decoder.expression_view(instruction)?, schema::ExprView::Ok(_) | schema::ExprView::Err { .. } | schema::ExprView::IfExpr { .. } | schema::ExprView::PatternIf { .. } | schema::ExprView::MatchExpr { .. } | schema::ExprView::StrMatchExpr { .. } | schema::ExprView::TagMatchExpr { .. } | schema::ExprView::ResultFallback { .. } | schema::ExprView::ValueBlock { .. });
        let expected = if returned && !preserves_result && !actual.as_ref().is_some_and(|actual| actual.is_result()) {
            match expected.as_ref() { Type::Result(ok, _) => Arc::new((**ok).clone()), _ => expected }
        } else { expected };
        match decoder.expression_view(instruction)? {
            schema::ExprView::Param(slot) => self.slot(owner, instruction, slot, actual.unwrap_or(expected)),
            schema::ExprView::Call { .. } | schema::ExprView::DirectPureCall { .. } | schema::ExprView::SelfCall { .. } => {
                let (function, arguments) = call_parts(&decoder, instruction)?;
                match self.store.functions[function].contract {
                    FullCallableContract::Ground(_) => { self.function(function); self.call_arguments(&decoder, function, &arguments)?; }
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
            schema::ExprView::ModuleCall { op, call_plan, args, .. } => {
                let arguments = args.map(|argument| argument.map(|value| value.map(|value| value.raw() as usize))).collect::<Result<Vec<_>, _>>()?;
                let (result, parameters) = if let Some(bridge) = crate::stdlib::bridge_contract(op) {
                    (*bridge.sig.return_ty, bridge.sig.params.into_iter().map(|parameter| (parameter.ty, parameter.defaulted)).collect::<Vec<_>>())
                } else if let Some(crate::runtime::eval::ModuleCallPlan::CommandArguments(plan)) = call_plan.map(AsRef::as_ref) {
                    (plan.signature.return_ty.clone(), plan.signature.params.iter().enumerate().map(|(slot, parameter)| {
                        let ty = if plan.word_conversions.iter().any(|conversion| conversion.slot == slot) {
                            Type::Any
                        } else { parameter.ty.clone() };
                        (ty, parameter.defaulted)
                    }).collect::<Vec<_>>())
                } else {
                    let mut selected: Option<&crate::modules::signature::ModuleFnSig> = None;
                    for &signature in FullDecoder::native_static_operation_signatures(op) {
                        self.budget.charge(1 + signature.params.len())?;
                        if arguments.len() > signature.params.len() { continue; }
                        let mut fits = true;
                        for (position, parameter) in signature.params.iter().enumerate() {
                            match arguments.get(position).copied().flatten() {
                                Some(argument) if parameter.ty != Type::Any => {
                                    if self.need_context(&decoder, argument)?.is_some_and(|actual| !actual.matches_expected(&parameter.ty)) {
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
                        if let Some(previous) = selected {
                            if previous.return_ty != signature.return_ty || previous.pure != signature.pure
                                || previous.effects != signature.effects || previous.params.len() != signature.params.len()
                                || previous.params.iter().zip(&signature.params).any(|(left, right)| left.ty != right.ty || left.defaulted != right.defaulted)
                            {
                                return Err(IrVerifyError::new("ground native operation has ambiguous parameter contracts"));
                            }
                        } else { selected = Some(signature); }
                    }
                    let signature = selected.ok_or_else(|| IrVerifyError::new("ground native operation arguments have no matching contract"))?;
                    let result = match call_plan.map(AsRef::as_ref) {
                        Some(crate::runtime::eval::ModuleCallPlan::SignatureCli(descriptor)) => descriptor.return_type(op == super::super::RuntimeOp::CliParseFull),
                        _ => signature.return_ty.clone(),
                    };
                    (result, signature.params.iter().map(|parameter| (parameter.ty.clone(), parameter.defaulted)).collect::<Vec<_>>())
                };
                if !result.matches_expected(&expected) { return Err(IrVerifyError::new("ground native result disagrees with its operation contract")); }
                if arguments.len() > parameters.len() { return Err(IrVerifyError::new("ground native operation has excess arguments")); }
                for (position, (parameter, defaulted)) in parameters.into_iter().enumerate() {
                    match arguments.get(position).copied().flatten() {
                        Some(argument) => {
                            let expected = if parameter == Type::Any { Arc::new(parameter) }
                            else if let Some(actual) = self.need_context(&decoder, argument)? {
                                if !actual.matches_expected(&parameter) { return Err(IrVerifyError::new("ground native argument disagrees with its operation contract")); }
                                actual
                            } else { Arc::new(parameter) };
                            self.expression(owner, argument, expected, false);
                        }
                        None if defaulted => {}
                        None => return Err(IrVerifyError::new("ground native operation omits a required argument")),
                    }
                }
                Ok(())
            }
            schema::ExprView::TypedCall { callee, signature, args, pure, .. } => {
                if !pure { return Err(IrVerifyError::new("ground producer typed call is not pure")); }
                let targets = self.function_targets(owner, callee.raw() as usize)?;
                let contract = self.contract_type(signature.id)?;
                let Type::Callable(callable) = contract.as_ref() else { return Err(IrVerifyError::new("typed producer call has no callable contract")) };
                self.expression(owner, callee.raw() as usize, Arc::clone(&contract), false);
                let mut arguments = Vec::new();
                for argument in args {
                    let super::super::decode::CallArgView::Single(value) = argument? else { return Err(IrVerifyError::new("verified typed call has a non-single argument")) };
                    arguments.push(ProducerArgument::Single(value.raw() as usize));
                }
                if arguments.len() != callable.sig.params.len() { return Err(IrVerifyError::new("typed producer call arguments do not match its contract")); }
                for (argument, parameter) in arguments.iter().zip(&callable.sig.params) {
                    let ProducerArgument::Single(value) = *argument else { unreachable!("typed arguments were decoded as single values") };
                    self.budget.charge_clone(&parameter.ty)?;
                    self.expression(owner, value, Arc::new(parameter.ty.clone()), false);
                }
                for target in targets {
                    let incoming = self.incoming.entry(target).or_default();
                    if !incoming.contains(&instruction) { incoming.push(instruction); }
                    self.function(target);
                    self.call_arguments(&decoder, target, &arguments)?;
                }
                Ok(())
            }
            schema::ExprView::Method { receiver, name, args, .. } => {
                let receiver_instruction = receiver.raw() as usize;
                let receiver_type = self.need_context(&decoder, receiver_instruction)?.ok_or_else(|| IrVerifyError::new("ground method has no receiver contract"))?;
                self.expression(owner, receiver_instruction, Arc::clone(&receiver_type), false);
                let arguments = args.collect::<Result<Vec<_>, _>>()?.into_iter().map(|argument| argument.raw() as usize).collect::<Vec<_>>();
                let signatures = FullDecoder::registered_method_signatures(&receiver_type, name)?.ok_or_else(|| IrVerifyError::new("ground method has no registered parameter contract"))?;
                let mut selected = None;
                for signature in signatures {
                    if arguments.len() > signature.params.len() || signature.params[arguments.len()..].iter().any(|parameter| !parameter.defaulted) { continue; }
                    let mut fits = true;
                    for (&argument, parameter) in arguments.iter().zip(&signature.params) {
                        if let Some(actual) = self.need_context(&decoder, argument)? && !actual.matches_expected(&parameter.ty) { fits = false; break; }
                    }
                    if !fits { continue; }
                    if selected.replace(signature).is_some() { return Err(IrVerifyError::new("ground method parameter contract is ambiguous")); }
                }
                let signature = selected.ok_or_else(|| IrVerifyError::new("ground method arguments do not match a registered contract"))?;
                let result = decoder.checked_method_return_type(&receiver_type, name, &arguments)?
                    .ok_or_else(|| IrVerifyError::new("ground method has no resolved return contract"))?;
                if !result.matches_expected(&expected) {
                    return Err(IrVerifyError::new("ground method result disagrees with its selected contract"));
                }
                for (&argument, parameter) in arguments.iter().zip(&signature.params) { self.expression(owner, argument, Arc::new(parameter.ty.clone()), false); }
                Ok(())
            }
            schema::ExprView::Field { base, .. } => {
                let ty = self.need_context(&decoder, base.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground field producer has no receiver contract"))?;
                self.expression(owner, base.raw() as usize, ty, false);
                Ok(())
            }
            schema::ExprView::Record(mut fields) => {
                let Type::Record(expected_fields) = expected.unvalidated() else { return Err(IrVerifyError::new("ground record producer has no record contract")) };
                let writers = decoder.checked_record_field_writers(&mut fields, |value| self.need_context(&decoder, value)?.ok_or_else(|| IrVerifyError::new("ground record spread has no producer contract")))?;
                for (name, expected) in expected_fields.iter() {
                    match writers.get(name) {
                        Some(super::super::CheckedRecordFieldWriter::Value(value)) => self.expression(owner, *value, Arc::new(expected.clone()), false),
                        Some(super::super::CheckedRecordFieldWriter::Spread { instruction, fields }) => {
                            let Type::Record(source) = fields.unvalidated() else { unreachable!("record spread writer has a non-record field contract") };
                            if !decoder.checked_type_fits(&source[name], expected) { return Err(IrVerifyError::new("ground record last spread writer disagrees with its result field contract")); }
                            self.expression(owner, *instruction, Arc::clone(fields), false);
                        }
                        None => return Err(IrVerifyError::new("ground record producer omits a required field")),
                    }
                }
                Ok(())
            }
            schema::ExprView::Loop { .. } => {
                for value in self.index.owners[&owner].return_analysis.loop_values.as_ref().and_then(|values| values.get(&instruction)).into_iter().flatten().copied().collect::<Vec<_>>() { self.expression(owner, value, Arc::clone(&expected), returned); }
                Ok(())
            }
            schema::ExprView::ListPipeline { .. } => self.verify_pipeline_result(owner, instruction, expected),
            schema::ExprView::ListComp { value, mut qualifiers, .. } => {
                let Type::List(item) = expected.unvalidated() else { return Err(IrVerifyError::new("ground list comprehension has no list contract")) };
                for qualifier in &mut qualifiers {
                    match qualifier? {
                        super::super::decode::CompQualifierView::For { iter, .. } => {
                            let source = self.need_context(&decoder, iter.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground comprehension has no iterable input contract"))?;
                            self.expression(owner, iter.raw() as usize, source, false);
                        }
                        super::super::decode::CompQualifierView::If { condition, .. } => self.expression(owner, condition.raw() as usize, Arc::new(Type::Bool), false),
                    }
                }
                self.expression(owner, value.raw() as usize, Arc::new((**item).clone()), false);
                Ok(())
            }
            schema::ExprView::MapComp { key, value, mut qualifiers, .. } => {
                let Type::Map(key_type, value_type) = expected.unvalidated() else { return Err(IrVerifyError::new("ground map comprehension has no map contract")) };
                for qualifier in &mut qualifiers {
                    match qualifier? {
                        super::super::decode::CompQualifierView::For { iter, .. } => {
                            let source = self.need_context(&decoder, iter.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground comprehension has no iterable input contract"))?;
                            self.expression(owner, iter.raw() as usize, source, false);
                        }
                        super::super::decode::CompQualifierView::If { condition, .. } => self.expression(owner, condition.raw() as usize, Arc::new(Type::Bool), false),
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
                let Type::List(item) = expected.unvalidated() else { return Err(IrVerifyError::new("ground list build has no list contract")) };
                for entry in &mut entries {
                    let (splice, value, _) = entry?;
                    let ty = if splice { Arc::clone(&expected) } else { Arc::new((**item).clone()) };
                    self.expression(owner, value.raw() as usize, ty, false);
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
                let Type::List(item) = expected.unvalidated() else { return Err(IrVerifyError::new("ground list producer has no item contract")) };
                for value in &mut values { self.expression(owner, value?.raw() as usize, Arc::new((**item).clone()), false); }
                Ok(())
            }
            schema::ExprView::ValueBlock { mut body, .. } => {
                if decoder.checked_branch_value_instruction(instruction)?.is_none() {
                    self.function(function_index(owner)?);
                    return Ok(());
                }
                let value = decoder.checked_block_tail_expression(&mut body)?;
                self.expression(owner, value, expected, returned);
                Ok(())
            }
            schema::ExprView::IfExpr { mut branches, else_value, .. } => {
                for branch in &mut branches {
                    let (condition, value) = branch?;
                    self.expression(owner, condition.raw() as usize, Arc::new(Type::Bool), false);
                    self.expression(owner, value.raw() as usize, Arc::clone(&expected), returned);
                }
                self.expression(owner, else_value.raw() as usize, expected, returned);
                Ok(())
            }
            schema::ExprView::PatternIf { mut branches, else_value, .. } => {
                for branch in &mut branches {
                    let (condition, value, _) = branch?;
                    self.expression(owner, condition.raw() as usize, Arc::new(Type::Bool), false);
                    self.expression(owner, value.raw() as usize, Arc::clone(&expected), returned);
                }
                self.expression(owner, else_value.raw() as usize, expected, returned);
                Ok(())
            }
            schema::ExprView::MatchExpr { value, mut arms, .. } => {
                let subject = self.need_context(&decoder, value.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground match expression has no subject contract"))?;
                self.expression(owner, value.raw() as usize, subject, false);
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
                let key = super::super::CheckedPatternVariantKey { kind: super::super::CheckedPatternVariantKind::Error, owner: Name::intern(family_identity), variant: Name::intern(variant) };
                let layout = self.store.declared_pattern_variants.get(&key).ok_or_else(|| IrVerifyError::new("ground error producer has no declared payload layout"))?;
                let mut expected_fields = layout.fields.iter().map(|field| field.name.map(|name| (name, field.type_id)).ok_or_else(|| IrVerifyError::new("ground error payload declaration has an unnamed field"))).collect::<Result<BTreeMap<_, _>, _>>()?;
                for field in &mut fields {
                    let (name, value) = field?;
                    let ty = expected_fields.remove(&Name::intern(name)).ok_or_else(|| IrVerifyError::new("ground error producer repeats or adds a payload field"))?;
                    let ty = self.contract_type(ty)?;
                    self.expression(owner, value.raw() as usize, ty, false);
                }
                if !expected_fields.is_empty() { return Err(IrVerifyError::new("ground error producer omits a declared payload field")); }
                Ok(())
            }
            schema::ExprView::PreparedConstant(value) => decoder.verify_prepared_constant_type(value, &expected, &mut |amount| self.budget.charge(amount)),
            schema::ExprView::CheckedValue { check, value, .. } => {
                let checked = self.contract_type(check.ty.id)?;
                if !checked.matches_expected(&expected) { return Err(IrVerifyError::new("ground checked producer disagrees with its contract")); }
                if check.schema.is_some() || !super::runtime_complete_storage_check_type(&checked, &mut self.budget)? {
                    self.expression(owner, value.raw() as usize, checked, false);
                }
                Ok(())
            }
            schema::ExprView::Require { check, .. } => {
                let checked = check.ty.materialize()?;
                if !checked.matches_expected(&expected) && !matches!(expected.as_ref(), Type::Result(ok, _) if checked.matches_expected(ok)) { return Err(IrVerifyError::new("ground checked producer disagrees with its contract")); }
                Ok(())
            }
            _ if actual.is_some() => self.verify_operands(&decoder, instruction),
            _ => Err(IrVerifyError::new("ground producer form has no executable type proof")),
        }
    }

    fn verify_operands(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<(), IrVerifyError> {
        let owner = &self.index.owners[&decoder.owner];
        let mut pending = Vec::new();
        let mut child = owner.children[instruction - owner.range.start];
        while child != IR_NONE { pending.push(child as usize); child = owner.siblings[child as usize - owner.range.start]; }
        while let Some(instruction) = pending.pop() {
            self.budget.charge(1)?;
            match self.store.tags[instruction].kind() {
                schema::InstructionKind::Expr => {
                    let ty = self.need_context(decoder, instruction)?.ok_or_else(|| IrVerifyError::new("ground producer operand has no checked contract"))?;
                    self.expression(decoder.owner, instruction, ty, false);
                }
                schema::InstructionKind::Int | schema::InstructionKind::Bool => {
                    self.verify_scalar(decoder, instruction)?;
                    let mut child = owner.children[instruction - owner.range.start];
                    while child != IR_NONE { pending.push(child as usize); child = owner.siblings[child as usize - owner.range.start]; }
                }
                _ => return Err(IrVerifyError::new("ground producer operand has no value transfer contract")),
            }
        }
        Ok(())
    }

    fn verify_scalar(&mut self, decoder: &FullDecoder<'a>, instruction: usize) -> Result<(), IrVerifyError> {
        let tag = self.store.tags[instruction];
        let payload = super::super::FullPayload { store: self.store, cursor: super::super::FullCursor::new(self.store.payload(self.store.data[instruction].range())?) };
        match tag.kind() {
            schema::InstructionKind::Int => match schema::IntView::decode_from_decoder(decoder, tag, payload)? {
                schema::IntView::Slot(slot) => self.slot(decoder.owner, instruction, slot, Arc::new(Type::Int))?,
                schema::IntView::StrByteLenSlot { slot, .. } | schema::IntView::StrCountLinesSlot { slot, .. } | schema::IntView::StrByteAtSlot { slot, .. } => {
                    let ty = decoder.checked_slot_type(slot)?.ok_or_else(|| IrVerifyError::new("scalar string producer has no slot contract"))?;
                    let ty = self.ground_type(ty)?;
                    if !matches!(ty.unvalidated(), Type::Str | Type::Bytes) { return Err(IrVerifyError::new("scalar string producer has a non-string slot")); }
                    self.slot(decoder.owner, instruction, slot, ty)?;
                }
                _ => {}
            },
            schema::InstructionKind::Bool => match schema::BoolView::decode_from_decoder(decoder, tag, payload)? {
                schema::BoolView::Slot(slot) => self.slot(decoder.owner, instruction, slot, Arc::new(Type::Bool))?,
                schema::BoolView::StrPredicateSlot { slot, .. } | schema::BoolView::StrContainsSlot { slot, .. } | schema::BoolView::TrimEmptySlot { slot, .. } | schema::BoolView::TrimStrPredicateSlot { slot, .. } => {
                    let ty = decoder.checked_slot_type(slot)?.ok_or_else(|| IrVerifyError::new("scalar string predicate has no slot contract"))?;
                    let ty = self.ground_type(ty)?;
                    if !matches!(ty.unvalidated(), Type::Str | Type::Bytes) { return Err(IrVerifyError::new("scalar string predicate has a non-string slot")); }
                    self.slot(decoder.owner, instruction, slot, ty)?;
                }
                schema::BoolView::ContainsSlot { slot, .. } | schema::BoolView::LiteralCompareSlot { slot, .. } => {
                    let ty = decoder.checked_slot_type(slot)?.ok_or_else(|| IrVerifyError::new("scalar predicate has no slot contract"))?;
                    let ty = self.ground_type(ty)?;
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

    fn pipeline_type(&mut self, decoder: &FullDecoder<'a>, pipeline: usize) -> Result<Option<Arc<Type>>, IrVerifyError> {
        let schema::ExprView::ListPipeline { input, .. } = decoder.expression_view(pipeline)? else { unreachable!("pipeline type has another expression kind") };
        let mut item = self.need_context(decoder, input.raw() as usize)?.and_then(|ty| ty.iteration_item_type());
        let mut output = None;
        let len = self.index.owners[&decoder.owner].pipeline_stages[&pipeline].len();
        for position in 0..len {
            let stage = self.pipeline_stage(decoder, pipeline, position)?;
            output = FullDecoder::checked_pipeline_stage_output_type(&stage, item, &mut |value| self.need_context(decoder, value).map(|ty| ty.map(|ty| (*ty).clone())))?;
            item = output.as_ref().and_then(Type::iteration_item_type);
        }
        Ok(output.map(|ty| Arc::new(match ty { Type::Stream(item) => Type::List(item), other => other })))
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
        if let Some(source) = self.index.owners[&owner].pattern_sources.get(&binding) {
            let source_instruction = source.source;
            let pattern = source.pattern;
            let preceding = source.preceding.clone();
            let ty = self.need_context(&decoder, source_instruction)?.ok_or_else(|| IrVerifyError::new("ground expression pattern has no subject contract"))?;
            let mut narrowed = Some((*ty).clone());
            for (pattern, guarded) in preceding { narrowed = decoder.subject_after_arm(pattern, guarded, narrowed)?; }
            let actual = decoder.checked_pattern_bound_type_with_budget(pattern, slot, narrowed.as_ref(), |amount| self.budget.charge(amount))?.ok_or_else(|| IrVerifyError::new("ground expression pattern has no binding projection"))?;
            if !actual.matches_expected(&expected) { return Err(IrVerifyError::new("ground expression pattern binding disagrees with its producer contract")); }
            self.expression(owner, source_instruction, ty, false);
            return self.verify_slot_writers(&decoder, binding, slot, expected, true);
        }
        if let Some(&(comprehension, iter)) = self.index.owners[&owner].comprehension_sources.get(&binding) {
            let source = self.need_context(&decoder, iter)?.ok_or_else(|| IrVerifyError::new("ground comprehension has no iterable contract"))?;
            let item = source.iteration_item_type().ok_or_else(|| IrVerifyError::new("ground comprehension source has no item type"))?;
            let mut qualifiers = match decoder.expression_view(comprehension)? {
                schema::ExprView::ListComp { qualifiers, .. } | schema::ExprView::MapComp { qualifiers, .. } => qualifiers,
                _ => unreachable!("comprehension binder has a non-comprehension source"),
            };
            let mut found = false;
            for qualifier in &mut qualifiers {
                if let super::super::decode::CompQualifierView::For { target, iter: candidate, .. } = qualifier? && candidate.raw() as usize == iter {
                    let projected = comp_slot_type(target, slot, &item)?.ok_or_else(|| IrVerifyError::new("ground comprehension binding has no target projection"))?;
                    if !projected.matches_expected(&expected) { return Err(IrVerifyError::new("ground comprehension binding disagrees with its producer contract")); }
                    found = true;
                }
            }
            if !found { return Err(IrVerifyError::new("ground comprehension binder has no lexical source")); }
            self.expression(owner, iter, source, false);
            return self.verify_slot_writers(&decoder, binding, slot, expected, true);
        }
        if let Some(step) = super::super::driver_owner_index(owner) {
            if let Some(source) = decoder.driver_local_let_source(slot, |amount| self.budget.charge(amount))? {
                let source_owner = driver_owner(source.step).map_err(|_| IrVerifyError::new("ground driver initializer owner is invalid"))?;
                if let Some(validation) = source.validation_type
                    && *self.ground_type(validation)? == *expected
                {
                    return Ok(());
                }
                self.expression(source_owner, source.initializer, expected, false);
                return Ok(());
            }
            let driver = self.store.driver_steps[step];
            let binding = bounded(driver.slots, &self.store.driver_slots, "ground driver slot range is invalid")?.iter().find(|binding| binding.slot as usize == slot).ok_or_else(|| IrVerifyError::new("ground driver slot has no lexical binding"))?;
            if binding.flags & super::super::DRIVER_SLOT_MUTABLE != 0 { return Err(IrVerifyError::new("ground producer cannot authenticate mutable global state")); }
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
            let FullCallableContract::Ground(signature) = function.contract else { return Err(IrVerifyError::new("symbolic parameter escaped its verified body proof")) };
            let (_, parameter, _) = self.store.semantic.signature_param(signature, slot)?;
            if !self.contract_type(parameter)?.matches_expected(&expected) { return Err(IrVerifyError::new("ground producer parameter disagrees with its signature")); }
            for instruction in self.incoming.get(&function_index(owner)?).cloned().unwrap_or_default() {
                let caller = self.index.instruction_owner(instruction)?;
                let caller_decoder = self.owner(caller)?;
                let arguments = call_values(&caller_decoder, instruction)?;
                self.call_arguments(&caller_decoder, function_index(owner)?, &arguments)?;
            }
        }
        if let Some(capture) = bounded(function.captures, &self.store.captures, "ground producer capture range is invalid")?.iter().find(|capture| binding == ProducerBinding::HeaderCapture(capture.binding_identity)) {
            if capture.slot_and_flags & (1 << 31) != 0 { return Err(IrVerifyError::new("ground producer reads mutable captured state")); }
            self.binding_writer(capture.binding_identity, Arc::clone(&expected))?;
        }
        let initialized = slot < params.len() || bounded(function.captures, &self.store.captures, "ground producer capture range is invalid")?.iter().any(|capture| binding == ProducerBinding::HeaderCapture(capture.binding_identity));
        self.verify_slot_writers(&decoder, binding, slot, expected, initialized)
    }

    fn verify_slot_writers(&mut self, decoder: &FullDecoder<'a>, binding: ProducerBinding, slot: usize, expected: Arc<Type>, initialized: bool) -> Result<(), IrVerifyError> {
        let owner = decoder.owner;
        let writers = self.index.owners[&owner].writers.get(&binding).cloned().unwrap_or_default();
        if writers.is_empty() && !initialized { return Err(IrVerifyError::new("ground producer slot has no initialized writer")); }
        for writer in writers {
            self.budget.charge(1)?;
            match decoder.statement_view(writer)? {
                schema::StmtView::Let { value, .. } | schema::StmtView::DefaultParameter { value, .. } => self.expression(owner, value.raw() as usize, Arc::clone(&expected), false),
                schema::StmtView::Assign { value, check, .. } => {
                    let checked = check.map(|check| check.ty.materialize()).transpose()?;
                    if checked.as_ref() != Some(expected.as_ref()) { self.expression(owner, value.raw() as usize, Arc::clone(&expected), false); }
                }
                schema::StmtView::LetInt { .. } | schema::StmtView::AssignInt { .. } => {
                    if !Type::Int.matches_expected(&expected) { return Err(IrVerifyError::new("integer writer disagrees with its ground slot contract")); }
                    self.verify_operands(decoder, writer)?;
                }
                schema::StmtView::LetBool { .. } | schema::StmtView::AssignBool { .. } => {
                    if !Type::Bool.matches_expected(&expected) { return Err(IrVerifyError::new("boolean writer disagrees with its ground slot contract")); }
                    self.verify_operands(decoder, writer)?;
                }
                schema::StmtView::AssignField { field, value, .. } => {
                    let path = [super::super::CallableAssignmentPathStep::Field(Name::intern(field))];
                    let ty = decoder.callable_assignment_target_type(slot, &path)?.ok_or_else(|| IrVerifyError::new("ground field writer has no checked target type"))?;
                    self.expression(owner, value.raw() as usize, Arc::new(ty), false);
                }
                schema::StmtView::AssignFieldInt { field, .. } => {
                    let path = [super::super::CallableAssignmentPathStep::Field(Name::intern(field))];
                    let ty = decoder.callable_assignment_target_type(slot, &path)?.ok_or_else(|| IrVerifyError::new("ground integer field writer has no target type"))?;
                    if !Type::Int.matches_expected(&ty) { return Err(IrVerifyError::new("integer field writer disagrees with its ground target")); }
                    self.verify_operands(decoder, writer)?;
                }
                schema::StmtView::AssignPath { mut path, value, .. } => {
                    let mut checked = Vec::new();
                    for step in &mut path {
                        checked.push(match step? {
                            super::super::decode::AssignStepView::Field(name) => super::super::CallableAssignmentPathStep::Field(name),
                            super::super::decode::AssignStepView::Index(value) => {
                                let ty = self.need_context(decoder, value.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground assignment index has no type"))?;
                                self.expression(owner, value.raw() as usize, ty, false);
                                super::super::CallableAssignmentPathStep::Index
                            }
                        });
                    }
                    let ty = decoder.callable_assignment_target_type(slot, &checked)?.ok_or_else(|| IrVerifyError::new("ground path writer has no checked target type"))?;
                    self.expression(owner, value.raw() as usize, Arc::new(ty), false);
                }
                schema::StmtView::ScanLines { text_slot, line_slot, .. } => {
                    if slot == line_slot { return Err(IrVerifyError::new("scan line temporary escapes its lexical iteration")); }
                    if !Type::Int.matches_expected(&expected) { return Err(IrVerifyError::new("scan counter writer disagrees with its integer contract")); }
                    let source = decoder.checked_slot_type(text_slot)?.ok_or_else(|| IrVerifyError::new("scan source has no string slot contract"))?;
                    let source = self.ground_type(source)?;
                    if !matches!(source.unvalidated(), Type::Str | Type::Bytes) { return Err(IrVerifyError::new("scan source slot is not text or bytes")); }
                    self.slot(owner, writer, text_slot, source)?;
                }
                schema::StmtView::ScanBytes { config } => {
                    let ty = if slot == config.block_depth_slot || slot == config.string_delim_slot { Type::Int } else { Type::Bool };
                    if !ty.matches_expected(&expected) { return Err(IrVerifyError::new("byte scan writer disagrees with its scalar contract")); }
                    self.slot(owner, writer, config.line_slot, Arc::new(Type::Bytes))?;
                    self.slot(owner, writer, config.block_depth_slot, Arc::new(Type::Int))?;
                }
                schema::StmtView::ForRecord { target, iter, .. } => {
                    let ty = self.need_context(decoder, iter.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground destructuring loop has no iterable contract"))?;
                    let item = ty.iteration_item_type().ok_or_else(|| IrVerifyError::new("ground destructuring loop has no item contract"))?;
                    let projected = comp_slot_type(target, slot, &item)?.ok_or_else(|| IrVerifyError::new("ground destructuring loop has no slot projection"))?;
                    if !projected.matches_expected(&expected) { return Err(IrVerifyError::new("ground destructuring loop disagrees with its slot contract")); }
                    self.expression(owner, iter.raw() as usize, ty, false);
                }
                schema::StmtView::For { iter, .. } => {
                    let ty = self.need_context(decoder, iter.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground loop producer has no iterable contract"))?;
                    let item = ty.iteration_item_type().ok_or_else(|| IrVerifyError::new("ground loop writer has no item contract"))?;
                    if !item.matches_expected(&expected) { return Err(IrVerifyError::new("ground loop writer disagrees with its slot contract")); }
                    self.expression(owner, iter.raw() as usize, ty, false);
                }
                schema::StmtView::ForStrLines { text, .. } => {
                    let ty = self.need_context(decoder, text.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground line writer has no string contract"))?;
                    if !ty.matches_expected(&expected) { return Err(IrVerifyError::new("ground line writer disagrees with its slot contract")); }
                    self.expression(owner, text.raw() as usize, ty, false);
                }
                schema::StmtView::LetRecord { source, target, .. } => {
                    let ty = self.need_context(decoder, source.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground destructuring writer has no source contract"))?;
                    let projected = comp_slot_type(target, slot, &ty)?.ok_or_else(|| IrVerifyError::new("ground destructuring writer has no slot projection"))?;
                    if !projected.matches_expected(&expected) { return Err(IrVerifyError::new("ground destructuring writer disagrees with its slot contract")); }
                    self.expression(owner, source.raw() as usize, ty, false);
                }
                schema::StmtView::Guard { value, target, optional, else_param_slot, .. } => {
                    let ty = self.need_context(decoder, value.raw() as usize)?.ok_or_else(|| IrVerifyError::new("ground guard has no subject contract"))?;
                    let projected = if else_param_slot == Some(slot) {
                        match ty.unvalidated() { Type::Result(_, error) if !optional => Some((**error).clone()), _ => None }
                    } else {
                        let payload = match ty.unvalidated() { Type::Result(ok, _) if !optional => Some(ok.as_ref()), Type::Optional(ok) if optional => Some(ok.as_ref()), _ => None };
                        match payload { Some(payload) => comp_slot_type(target, slot, payload)?, None => None }
                    }.ok_or_else(|| IrVerifyError::new("ground guard has no slot transfer contract"))?;
                    if !projected.matches_expected(&expected) { return Err(IrVerifyError::new("ground guard writer disagrees with its slot contract")); }
                    self.expression(owner, value.raw() as usize, ty, false);
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
        }
        Ok(())
    }

    fn binding_writer(&mut self, binding: u32, expected: Arc<Type>) -> Result<(), IrVerifyError> {
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
            let Type::Module(module) = expected.as_ref() else { return Err(IrVerifyError::new("ground import producer has no module contract")) };
            for (name, binding) in self.index.module_exports.get(&owner).cloned().unwrap_or_default() {
                let ty = module.exports.get(&name).and_then(crate::sema::types::ModuleExportType::field_value_type).ok_or_else(|| IrVerifyError::new("ground module export has no registered value contract"))?;
                self.binding_writer(binding, Arc::new(ty))?;
            }
            return Ok(());
        }
        let mut payload = super::super::FullCursor::new(self.store.payload(driver.data.range())?);
        match driver.tag {
            super::super::FullDriverTag::Let => {
                let _: Name = super::super::FullCodec::decode(&decoder, &mut payload)?;
                let _: Option<super::super::LoweredType> = super::super::FullCodec::decode(&decoder, &mut payload)?;
                let validation: Option<super::super::LoweredTypeCheck> = super::super::FullCodec::decode(&decoder, &mut payload)?;
                let _: Option<Type> = super::super::FullCodec::decode(&decoder, &mut payload)?;
                let mutable: bool = super::super::FullCodec::decode(&decoder, &mut payload)?;
                if mutable { return Err(IrVerifyError::new("ground capture writer is mutable")); }
                let value = payload.raw()? as usize;
                if validation.is_none_or(|check| check.ty != *expected) { self.expression(owner, value, expected, false); }
            }
            super::super::FullDriverTag::LetRecord => {
                let value = payload.raw()? as usize;
                let fields: Vec<(Name, usize)> = super::super::FullCodec::decode(&decoder, &mut payload)?;
                let target = super::super::LoweredCompTarget::borrow(
                    super::super::DecodeContext::from_decoder(&decoder), &mut payload,
                )?;
                let mutable: bool = super::super::FullCodec::decode(&decoder, &mut payload)?;
                if mutable { return Err(IrVerifyError::new("ground captured destructuring writer is mutable")); }
                if matches!(target, super::super::decode::CompTargetView::Discard) {
                    let target_name = self.store.string(identity.name)?;
                    let slot = fields.iter().find(|(name, _)| name.as_str() == target_name).map(|(_, slot)| *slot).ok_or_else(|| IrVerifyError::new("ground captured guard has no declared slot"))?;
                    let (target, operand, optional) = decoder.driver_guard_source(value)?;
                    let (producer, ty, success) = match (optional, decoder.expression_view(operand)?) {
                        (false, schema::ExprView::Ok(inner)) => {
                            let producer = inner.raw() as usize;
                            let ty = self.need_context(&decoder, producer)?.ok_or_else(|| IrVerifyError::new("ground captured guard has no success producer type"))?;
                            (producer, Arc::clone(&ty), ty)
                        }
                        _ => {
                            let ty = self.need_context(&decoder, operand)?.ok_or_else(|| IrVerifyError::new("ground captured guard has no subject producer type"))?;
                            let success = match ty.unvalidated() {
                                Type::Result(ok, _) if !optional => Arc::new((**ok).clone()),
                                Type::Optional(ok) if optional => Arc::new((**ok).clone()),
                                _ => return Err(IrVerifyError::new("ground captured guard has no success contract")),
                            };
                            (operand, ty, success)
                        }
                    };
                    let projected = comp_slot_type(target, slot, &success)?.ok_or_else(|| IrVerifyError::new("ground captured guard has no slot projection"))?;
                    if !projected.matches_expected(&expected) { return Err(IrVerifyError::new("ground captured guard disagrees with its slot contract")); }
                    self.expression(owner, producer, ty, false);
                    return Ok(());
                }
                let ty = self.need_context(&decoder, value)?.ok_or_else(|| IrVerifyError::new("ground captured destructuring has no producer type"))?;
                self.expression(owner, value, ty, false);
            }
            _ => return Err(IrVerifyError::new("ground capture has no immutable lexical writer")),
        }
        Ok(())
    }
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
