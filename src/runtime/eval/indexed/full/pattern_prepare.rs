use super::*;
use super::super::pattern::*;
use super::super::generic::{PatternSourceId, PatternCaptureId};
use crate::sema::check::{PatternIdentity, PatternCaptureIdentity, SolvedPatternShape, SolvedPatternDecision, SolvedTypes, NominalMemberKind};

fn pattern_problem() -> IrBuildError { IrBuildError::format("checked_pattern_evidence", None, 0, 0) }
fn pattern_invalid() -> IrVerifyError { IrVerifyError::new("encoded pattern disagrees with its original checked relationship") }

fn pattern_block(store: &FullStore, raw: u32) -> Result<&[u32], IrVerifyError> {
    let block = IrBlockId::from_raw(raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(pattern_invalid)?;
    store.payload(block.instructions)
}

fn match_arms(store: &FullStore, matcher: u32) -> Result<Vec<(u32, Option<u32>, PatternArmBody)>, IrVerifyError> {
    let tag = *store.tags.get(matcher as usize).ok_or_else(pattern_invalid)?;
    if !matches!(tag, FullTag::ExprMatch | FullTag::StmtMatch) { return Err(pattern_invalid()); }
    let words = store.payload(store.data[matcher as usize].range())?;
    let mut cursor = FullCursor::new(pattern_block(store, *words.get(1).ok_or_else(pattern_invalid)?)?);
    let count = cursor.raw()?;
    let mut result = Vec::new();
    for _ in 0..count {
        let pattern = cursor.raw()?;
        let guard = match cursor.raw()? { 0 => None, 1 => Some(cursor.raw()?), _ => return Err(pattern_invalid()) };
        let raw = cursor.raw()?;
        let body = if tag == FullTag::ExprMatch { PatternArmBody::Expression(raw) } else { PatternArmBody::Statements(IrBlockId::from_raw(raw).ok_or_else(pattern_invalid)?) };
        result.push((pattern, guard, body));
    }
    cursor.finish()?;
    Ok(result)
}

fn conditional_branch(store: &FullStore, control: u32, branch: u32) -> Result<(u32, PatternArmBody, Vec<u32>), IrVerifyError> {
    let tag = *store.tags.get(control as usize).ok_or_else(pattern_invalid)?;
    let words = store.payload(store.data[control as usize].range())?;
    if tag == FullTag::StmtPatternWhile {
        if branch != 0 { return Err(pattern_invalid()); }
        let condition = *words.first().ok_or_else(pattern_invalid)?;
        let body = IrBlockId::from_raw(*words.get(1).ok_or_else(pattern_invalid)?).ok_or_else(pattern_invalid)?;
        let captures = pattern_block(store, *words.get(2).ok_or_else(pattern_invalid)?)?;
        if captures.first().copied().map(|count| count as usize) != Some(captures.len() - 1) { return Err(pattern_invalid()); }
        return Ok((condition, PatternArmBody::Statements(body), captures[1..].to_vec()));
    }
    if !matches!(tag, FullTag::ExprPatternIf | FullTag::StmtPatternIf) { return Err(pattern_invalid()); }
    let branches = pattern_block(store, *words.first().ok_or_else(pattern_invalid)?)?;
    let mut input = FullCursor::new(branches);
    let count = input.raw()?;
    if branch >= count { return Err(pattern_invalid()); }
    let mut selected = None;
    for index in 0..count {
        let condition = input.raw()?;
        let raw_body = input.raw()?;
        let raw_captures = input.raw()?;
        if index == branch {
            let body = if tag == FullTag::ExprPatternIf { PatternArmBody::Expression(raw_body) }
                else { PatternArmBody::Statements(IrBlockId::from_raw(raw_body).ok_or_else(pattern_invalid)?) };
            let captures = pattern_block(store, raw_captures)?;
            if captures.first().copied().map(|count| count as usize) != Some(captures.len() - 1) { return Err(pattern_invalid()); }
            selected = Some((condition, body, captures[1..].to_vec()));
        }
    }
    input.finish()?;
    selected.ok_or_else(pattern_invalid)
}

impl FullBuilder {
    pub(super) fn stage_pattern_admission_control(
        &mut self, row: super::super::super::BuildPatternControlRow, instruction: u32, scratch: &BuildScratch,
    ) -> Result<(), IrBuildError> {
        use super::super::super::{BuildPatternAdmissionBody as Body, BuildPatternControlRow as Control};
        let Some(originals) = self.active_pattern_admissions.remove(&row) else { return Ok(()); };
        let raw = self.current_owner.ok_or_else(pattern_problem)?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(pattern_problem)?) };
        for (matcher, original) in originals {
            let (actual_matcher, actual_body, actual_captures) = match row {
                Control::Expression(row) => {
                    let BuildExprRow::PatternIf { branches, .. } = scratch.expressions.get(row.index()).ok_or_else(pattern_problem)? else { return Err(pattern_problem()); };
                    let (condition, body, captures) = branches.get(original.branch as usize).ok_or_else(pattern_problem)?;
                    (*condition, Body::Expression(*body), captures)
                }
                Control::Statement(row) => {
                    match scratch.statements.get(row.index()).ok_or_else(pattern_problem)? {
                        BuildStmtRow::PatternIf { branches, .. } => {
                            let (condition, body, captures) = branches.get(original.branch as usize).ok_or_else(pattern_problem)?;
                            (*condition, Body::Statements(body.clone().into_boxed_slice()), captures)
                        }
                        BuildStmtRow::PatternWhile { condition, body, captures, .. } if original.branch == 0 =>
                            (*condition, Body::Statements(body.clone().into_boxed_slice()), captures),
                        _ => return Err(pattern_problem()),
                    }
                }
            };
            if original.control != row || matcher != actual_matcher || original.body != actual_body { return Err(pattern_problem()); }
            let matcher = *self.active_encoded_expressions.get(&matcher).ok_or_else(pattern_problem)?;
            let (condition, body, captures) = conditional_branch(&self.store, instruction, original.branch).map_err(|_| pattern_problem())?;
            if condition != matcher || captures.iter().copied().map(usize::try_from).collect::<Result<Vec<_>, _>>().map_err(|_| pattern_problem())? != *actual_captures { return Err(pattern_problem()); }
            match (&original.body, body) {
                (Body::Expression(original), PatternArmBody::Expression(actual)) if self.active_encoded_expressions.get(original) == Some(&actual) => {},
                (Body::Statements(originals), PatternArmBody::Statements(actual)) => {
                    let actual = self.store.payload(self.store.blocks.get(actual.index()).ok_or_else(pattern_problem)?.instructions).map_err(|_| pattern_problem())?;
                    let expected = originals.iter().map(|statement| self.active_pattern_statements.get(statement).copied().ok_or_else(pattern_problem)).collect::<Result<Vec<_>, _>>()?;
                    if actual.first().copied().map(|count| count as usize) != Some(expected.len()) || actual[1..] != expected { return Err(pattern_problem()); }
                }
                _ => return Err(pattern_problem()),
            }
            let result = original.result.as_ref().map(|result| self.prepare_pattern_result(instruction, owner, result)).transpose()?;
            self.generic_pattern_admissions.push((matcher, PreparedPatternAdmission::Conditional {
                control: instruction, control_origin: original.control_origin, condition_origin: original.condition_origin,
                branch: original.branch, body,
            }, owner, result));
        }
        Ok(())
    }

    fn prepare_pattern_result(&mut self, control: u32, owner: InstructionOwner, original: &super::super::super::BuildPatternConditionalResult) -> Result<Box<PreparedPatternConditionalResult>, IrBuildError> {
        let solved = self.solved.clone().ok_or_else(pattern_problem)?;
        let caller = original.source.caller;
        let scope = caller.and_then(|caller| self.generic_declarations.get(&caller).copied());
        let result_source = self.prepare_pattern_result_source(&solved, caller, scope, &original.source)?;
        let mut bodies = Vec::new();
        for original in original.branches.iter().chain(std::iter::once(&original.fallback)) {
            let expected = self.prepare_pattern_result_source(&solved, caller, scope, &original.source)?;
            let condition = original.condition.as_ref().map(|(value, source)| {
                let expected = self.prepare_pattern_result_source(&solved, caller, scope, source)?;
                Ok::<_, IrBuildError>((*self.active_encoded_expressions.get(value).ok_or_else(pattern_problem)?, expected))
            }).transpose()?;
            let terminal = original.terminal.as_ref().map(|(statement, value, original, identity)| {
                let instruction = *self.active_encoded_expressions.get(value).ok_or_else(pattern_problem)?;
                let expected = match original {
                    super::super::super::BuildPatternResultTerminalSource::Expression(original) =>
                        PreparedPatternResultTerminalSource::Expression(self.prepare_pattern_result_source(&solved, caller, scope, original)?),
                    super::super::super::BuildPatternResultTerminalSource::PatternCapture(capture) => {
                        let pattern = solved.checked_pattern(capture.pattern).map_err(|_| pattern_problem())?;
                        let original = pattern.captures.iter().find(|original| original.identity == *capture).ok_or_else(pattern_problem)?;
                        if pattern.caller != caller || !self.generic_pattern_statement_use_rows.iter().any(|&(row, statement, actual, actual_owner)|
                            (row, statement, actual, actual_owner) == (instruction, *identity, *capture, owner)) { return Err(pattern_problem()); }
                        let original_scope = solved.checked_pattern_scope(capture.pattern).map_err(|_| pattern_problem())?;
                        solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: original.ty, scope: original_scope }).map_err(|_| pattern_problem())?;
                        PreparedPatternResultTerminalSource::PatternCapture { identity: *capture, expected: self.call_reference(&solved, scope, original.ty, false)? }
                    }
                };
                Ok::<_, IrBuildError>((*self.active_pattern_statements.get(statement).ok_or_else(pattern_problem)?,
                    instruction, expected, *identity))
            }).transpose()?;
            bodies.push(PreparedPatternResultBody { instruction: *self.active_encoded_expressions.get(&original.instruction).ok_or_else(pattern_problem)?, source: expected, condition, terminal });
        }
        let fallback = bodies.pop().ok_or_else(pattern_problem)?;
        Ok(Box::new(PreparedPatternConditionalResult { control, owner, scope, source: result_source, branches: bodies.into_boxed_slice(), fallback }))
    }

    fn prepare_pattern_result_source(&mut self, solved: &SolvedTypes, caller: Option<crate::sema::check::DeclarationIdentity>, scope: Option<SchemeScopeId>, original: &super::super::super::BuildPatternResultSource) -> Result<PreparedPatternResultSource, IrBuildError> {
        if original.caller != caller || solved.expression_owners.get(&original.origin).copied() != caller
            || solved.expressions.get(&original.origin) != Some(&original.ty)
            || solved.expression_scope(original.origin, caller).map_err(|_| pattern_problem())? != original.scope { return Err(pattern_problem()); }
        solved.graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: original.ty, scope: original.scope }).map_err(|_| pattern_problem())?;
        Ok(PreparedPatternResultSource { origin: original.origin, expected: self.call_reference(solved, scope, original.ty, false)? })
    }
    fn pattern_source(&mut self, solved: &SolvedTypes, identity: PatternIdentity, cache: &mut BTreeMap<PatternIdentity, PatternSourceId>, depth: usize) -> Result<PatternSourceId, IrBuildError> {
        if let Some(&id) = cache.get(&identity) { return Ok(id); }
        if depth >= 512 { return Err(pattern_problem()); }
        let original = solved.checked_pattern(identity).map_err(|_| pattern_problem())?;
        let lexical = original.caller.map(|caller| solved.declarations.get(&caller).map(|declaration| declaration.scheme).ok_or_else(pattern_problem)).transpose()?;
        if solved.checked_pattern_scope(identity).map_err(|_| pattern_problem())? != lexical { return Err(pattern_problem()); }
        let scope = original.caller.and_then(|caller| self.generic_declarations.get(&caller).copied());
        let shape = match &original.shape {
            SolvedPatternShape::Wildcard => PreparedPatternShape::Wildcard,
            SolvedPatternShape::Binding => PreparedPatternShape::Binding,
            SolvedPatternShape::Literal { value: Some(value), .. } => PreparedPatternShape::Literal(value.clone()),
            SolvedPatternShape::Group => PreparedPatternShape::Group,
            SolvedPatternShape::Alias { name } => PreparedPatternShape::Alias { name: *name },
            SolvedPatternShape::List { elements, has_rest } => PreparedPatternShape::List { elements: *elements, has_rest: *has_rest },
            SolvedPatternShape::Record { fields } => PreparedPatternShape::Record { fields: fields.clone().into_boxed_slice() },
            SolvedPatternShape::Alternation => PreparedPatternShape::Alternation,
            SolvedPatternShape::Type => PreparedPatternShape::Type,
            SolvedPatternShape::Facet => PreparedPatternShape::Facet,
            SolvedPatternShape::Constructor => PreparedPatternShape::Constructor,
            SolvedPatternShape::TestName => PreparedPatternShape::TestName,
            SolvedPatternShape::Tuple => {
                let SolvedPatternDecision::TagFields { fields } = &original.decision else { return Err(pattern_problem()); };
                PreparedPatternShape::Tuple { fields: fields.iter().map(|&ty| self.call_reference(solved, scope, ty, false)).collect::<Result<Vec<_>, _>>()?.into_boxed_slice() }
            }
            SolvedPatternShape::ErrorVariant { fields } => PreparedPatternShape::ErrorVariant { fields: fields.clone().into_boxed_slice() },
            _ => return Err(pattern_problem()),
        };
        let decision = match original.decision {
            SolvedPatternDecision::Structural => PreparedPatternDecision::Structural,
            SolvedPatternDecision::Alternation => PreparedPatternDecision::Alternation,
            SolvedPatternDecision::Binding => PreparedPatternDecision::Binding,
            SolvedPatternDecision::Type => PreparedPatternDecision::Type,
            SolvedPatternDecision::Facet { facet } => PreparedPatternDecision::Facet { facet },
            SolvedPatternDecision::Result { success, payload } => PreparedPatternDecision::Result { success, payload: payload.map(|ty| self.call_reference(solved, scope, ty, false)).transpose()? },
            SolvedPatternDecision::TagConstructor { identity, type_name, constructor, ref fields } => {
                let actual = fields.iter().map(|&ty| self.call_reference(solved, scope, ty, false)).collect::<Result<Vec<_>, _>>()?;
                let member = self.generic_evidence_mut().pattern_nominal(identity).map_err(|_| pattern_problem())?;
                if member.kind != PreparedPatternNominalKind::Tag || member.family != type_name || member.member != constructor
                    || actual != member.fields.iter().map(|(_, ty)| *ty).collect::<Vec<_>>() { return Err(pattern_problem()); }
                PreparedPatternDecision::TagConstructor { identity, family: type_name, member: constructor }
            }
            SolvedPatternDecision::TagFields { .. } => PreparedPatternDecision::TagFields,
            SolvedPatternDecision::ErrorVariant { identity, family, variant, ref fields } => {
                let actual = fields.iter().map(|&(name, ty)| self.call_reference(solved, scope, ty, false).map(|ty| (Some(name), ty))).collect::<Result<Vec<_>, _>>()?;
                let member = self.generic_evidence_mut().pattern_nominal(identity).map_err(|_| pattern_problem())?;
                if member.kind != PreparedPatternNominalKind::Error || member.family != family || member.member != variant
                    || actual.iter().any(|field| !member.fields.contains(field)) { return Err(pattern_problem()); }
                PreparedPatternDecision::ErrorVariant { identity, family, member: variant }
            }
            _ => return Err(pattern_problem()),
        };
        let input = self.call_reference(solved, scope, original.input, false)?;
        let tested = original.tested.map(|ty| self.call_reference(solved, scope, ty, false)).transpose()?;
        let input_nominal = solved.checked_pattern_nominal(identity, crate::sema::check::PatternTypePosition::Input).map_err(|_| pattern_problem())?;
        let tested_nominal = solved.checked_pattern_nominal(identity, crate::sema::check::PatternTypePosition::Tested).map_err(|_| pattern_problem())?;
        let mut children = Vec::new();
        for &child in &original.children { children.push(self.pattern_source(solved, child, cache, depth + 1)?); }
        let mut captures = Vec::new();
        for capture in &original.captures {
            captures.push(PatternSourceCapture {
                identity: capture.identity,
                expected: self.call_reference(solved, scope, capture.ty, false)?,
                branches: capture.branches.clone().into_boxed_slice(),
                slot: *self.pattern_capture_slots.get(&capture.identity).ok_or_else(|| pattern_problem())?,
            });
        }
        let id = self.generic_evidence_mut().add_pattern_source(PreparedPatternSource {
            origin: identity, caller: original.caller, scope, input, tested, input_nominal, tested_nominal, shape, decision,
            children: children.into_boxed_slice(), captures: captures.into_boxed_slice(),
        }).map_err(|_| pattern_problem())?;
        cache.insert(identity, id);
        Ok(id)
    }

    fn original_visible_captures(solved: &SolvedTypes, identity: PatternIdentity) -> Result<Vec<crate::sema::check::SolvedPatternCapture>, IrBuildError> {
        let mut captures = Vec::new();
        let mut pending = vec![(identity, 0usize)];
        let mut seen = std::collections::BTreeSet::new();
        while let Some((identity, depth)) = pending.pop() {
            if depth >= 512 || !seen.insert(identity) { return Err(pattern_problem()); }
            let original = solved.checked_pattern(identity).map_err(|_| pattern_problem())?;
            captures.extend(original.captures.iter().cloned());
            if !matches!(original.shape, SolvedPatternShape::Alternation) { pending.extend(original.children.iter().rev().map(|&child| (child, depth + 1))); }
        }
        Ok(captures)
    }

    pub(super) fn prepare_pattern_evidence(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.as_ref().cloned() else { return Ok(()); };
        let mut admissions = BTreeMap::new();
        for (matcher, admission, owner, result) in self.generic_pattern_admissions.clone() {
            let PreparedPatternAdmission::Conditional { control, control_origin, condition_origin, .. } = admission else { return Err(pattern_problem()); };
            self.generic_evidence_mut().register_instruction_origin(control, control_origin, owner).map_err(|_| pattern_problem())?;
            self.generic_evidence_mut().register_instruction_origin(matcher, super::super::generic::OperationSourceOrigin::Expression(condition_origin), owner).map_err(|_| pattern_problem())?;
            if let Some(result) = result.as_ref() {
                for body in result.branches.iter().chain(std::iter::once(&result.fallback)) {
                    self.generic_evidence_mut().register_instruction_origin(body.instruction, super::super::generic::OperationSourceOrigin::Expression(body.source.origin), owner).map_err(|_| pattern_problem())?;
                    if let Some((statement, value, source, identity)) = &body.terminal {
                        self.generic_evidence_mut().register_instruction_origin(*statement, super::super::generic::OperationSourceOrigin::Statement(*identity), owner).map_err(|_| pattern_problem())?;
                        let origin = match source {
                            PreparedPatternResultTerminalSource::Expression(source) => super::super::generic::OperationSourceOrigin::Expression(source.origin),
                            PreparedPatternResultTerminalSource::PatternCapture { .. } => super::super::generic::OperationSourceOrigin::Statement(*identity),
                        };
                        self.generic_evidence_mut().register_instruction_origin(*value, origin, owner).map_err(|_| pattern_problem())?;
                    }
                    if let Some((condition, source)) = &body.condition {
                        self.generic_evidence_mut().register_instruction_origin(*condition, super::super::generic::OperationSourceOrigin::Expression(source.origin), owner).map_err(|_| pattern_problem())?;
                    }
                }
            }
            if admissions.insert(matcher, (admission, owner, result)).is_some() { return Err(pattern_problem()); }
        }
        let retry_patterns = self.generic_evidence_mut().retry_selection_origins();
        let mut needed = std::collections::BTreeSet::new();
        let mut visited = std::collections::BTreeSet::new();
        let mut pending: Vec<_> = self.generic_pattern_rows.iter().map(|&(_, identity, _)| (identity, 0usize)).collect();
        pending.extend(retry_patterns.iter().copied().map(|identity| (identity, 0)));
        while let Some((identity, depth)) = pending.pop() {
            if depth >= 512 { return Err(pattern_problem()); }
            if !visited.insert(identity) { continue; }
            let original = solved.checked_pattern(identity).map_err(|_| pattern_problem())?;
            if let SolvedPatternDecision::TagConstructor { identity, .. } | SolvedPatternDecision::ErrorVariant { identity, .. } = original.decision { needed.insert(identity); }
            pending.extend(original.children.iter().map(|&child| (child, depth + 1)));
        }
        for identity in needed {
            let original = solved.checked_nominal_member(identity).map_err(|_| pattern_problem())?;
            if original.scope.is_some() { return Err(pattern_problem()); }
            let tested = self.call_reference(&solved, None, original.tested, false)?;
            let fields = original.fields.iter().map(|&(name, ty)| self.call_reference(&solved, None, ty, false).map(|ty| (name, ty))).collect::<Result<Vec<_>, _>>()?;
            self.generic_evidence_mut().register_pattern_nominal(PreparedPatternNominalMember {
                identity, kind: match original.kind { NominalMemberKind::Tag => PreparedPatternNominalKind::Tag, NominalMemberKind::Error => PreparedPatternNominalKind::Error },
                family: original.family, member: original.member, tested, fields: fields.into_boxed_slice(), facets: original.facets.clone().into_boxed_slice(),
            }).map_err(|_| pattern_problem())?;
        }
        let patterns: BTreeMap<_, _> = self.generic_pattern_rows.iter().map(|&(row, identity, owner)| (row, (identity, owner))).collect();
        let expressions: BTreeMap<_, _> = self.generic_expression_rows.iter().map(|&(row, identity, owner)| (row, (identity, owner))).collect();
        let mut sources = BTreeMap::new();
        for identity in retry_patterns {
            self.pattern_source(&solved, identity, &mut sources, 0)?;
        }
        let mut captures: BTreeMap<PatternCaptureIdentity, (PatternCaptureId, InstructionOwner)> = BTreeMap::new();
        for matcher in 0..self.store.tags.len() {
            if !matches!(self.store.tags[matcher], FullTag::ExprMatch | FullTag::StmtMatch) { continue; }
            let arms = match_arms(&self.store, matcher as u32).map_err(|_| pattern_problem())?;
            let subject = self.store.payload(self.store.data[matcher].range()).map_err(|_| pattern_problem())?[0];
            for (arm, (pattern, guard, body)) in arms.into_iter().enumerate() {
                let Some(&(identity, owner)) = patterns.get(&pattern) else { continue; };
                let source = self.pattern_source(&solved, identity, &mut sources, 0)?;
                let (subject_source, subject_wrappers) = self.argument_initializer_lineage(subject, owner)?;
                let &(subject_origin, subject_owner) = expressions.get(&subject_source).ok_or_else(|| pattern_problem())?;
                if owner != subject_owner { return Err(pattern_problem()); }
                let original = solved.checked_pattern(identity).map_err(|_| pattern_problem())?;
                let scope = original.caller.and_then(|caller| self.generic_declarations.get(&caller).copied());
                let actual = *solved.expressions.get(&subject_origin).ok_or_else(|| pattern_problem())?;
                if self.call_reference(&solved, scope, actual, false)? != self.call_reference(&solved, scope, original.input, false)? { return Err(pattern_problem()); }
                let application = self.generic_evidence_mut().add_pattern_application(PreparedPatternApplication {
                    source, owner, matcher: matcher as u32, subject, subject_source, subject_wrappers, subject_origin, pattern, arm: arm as u32, guard, body,
                    admission: match admissions.get(&(matcher as u32)) { Some((admission, admission_owner, _)) if *admission_owner == owner && arm == 0 => *admission,
                        Some(_) => return Err(pattern_problem()), None => PreparedPatternAdmission::MatchArm },
                    result: admissions.get(&(matcher as u32)).and_then(|(_, _, result)| result.clone()),
                }).map_err(|_| pattern_problem())?;
                for capture in Self::original_visible_captures(&solved, identity)? {
                    let expected = self.call_reference(&solved, scope, capture.ty, false)?;
                    let slot = *self.pattern_capture_slots.get(&capture.identity).ok_or_else(|| pattern_problem())?;
                    let id = self.generic_evidence_mut().add_pattern_capture(PreparedPatternCapture { application, identity: capture.identity, expected, slot }).map_err(|_| pattern_problem())?;
                    if captures.insert(capture.identity, (id, owner)).is_some() { return Err(pattern_problem()); }
                }
            }
        }
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            let Some(&identity) = self.pattern_use_origins.get(&origin) else { continue; };
            let &(capture, capture_owner) = captures.get(&identity).ok_or_else(|| pattern_problem())?;
            if owner != capture_owner { return Err(pattern_problem()); }
            let slot = self.pattern_capture_slots[&identity];
            self.generic_evidence_mut().add_pattern_use(PreparedPatternUse { capture, origin: SourceUseIdentity::Expression(origin), owner, instruction, slot });
        }
        for (instruction, origin, identity, owner) in self.generic_pattern_statement_use_rows.clone() {
            let &(capture, capture_owner) = captures.get(&identity).ok_or_else(|| pattern_problem())?;
            if owner != capture_owner { return Err(pattern_problem()); }
            let slot = self.pattern_capture_slots[&identity];
            self.generic_evidence_mut().add_pattern_use(PreparedPatternUse { capture, origin: SourceUseIdentity::Statement(origin), owner, instruction, slot });
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_pattern_conditional_result(
        store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner,
        expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>,
    ) -> Result<(), IrVerifyError> {
        let result = generic.pattern_conditional_result_at(instruction)?.ok_or_else(|| IrVerifyError::new("conditional value lacks its original result roots"))?;
        if result.owner != owner || store.tags.get(instruction as usize) != Some(&FullTag::ExprPatternIf) { return Err(pattern_invalid()); }
        let original_type = |reference| -> Result<Type, IrVerifyError> {
            if let TypeRef::Ground(ty) = reference { return store.semantic.to_type(ty); }
            let scope = result.scope.ok_or_else(pattern_invalid)?;
            let frame = generic.instance(instance.ok_or_else(|| IrVerifyError::new("conditional result requires its original declaration instance"))?)?;
            if frame.scope != scope { return Err(pattern_invalid()); }
            generic.expand(&store.semantic, reference, &frame.substitutions, &mut FxHashMap::default())
        };
        if original_type(result.source.expected)? != *expected { return Err(pattern_invalid()); }
        let words = store.payload(store.data[instruction as usize].range())?;
        let branches = pattern_block(store, *words.first().ok_or_else(pattern_invalid)?)?;
        if branches.first().copied().map(|count| count as usize) != Some(result.branches.len())
            || branches.len() != 1 + result.branches.len() * 3
            || words.get(1) != Some(&result.fallback.instruction) { return Err(pattern_invalid()); }
        for (ordinal, body) in result.branches.iter().enumerate() {
            let (condition, condition_source) = body.condition.as_ref().ok_or_else(pattern_invalid)?;
            if branches[1 + ordinal * 3] != *condition || branches[2 + ordinal * 3] != body.instruction { return Err(pattern_invalid()); }
            if store.tags.get(*condition as usize) == Some(&FullTag::ExprMatch) {
                let arms = match_arms(store, *condition)?;
                if arms.len() != 2 { return Err(pattern_invalid()); }
                for (_, guard, body) in arms {
                    let PatternArmBody::Expression(body) = body else { return Err(pattern_invalid()); };
                    if guard.is_some() || store.tags.get(body as usize) != Some(&FullTag::ExprBool)
                        || !matches!(store.payload(store.data[body as usize].range())?, [0] | [1]) { return Err(pattern_invalid()); }
                }
            } else {
                Self::verify_pattern_result_value(store, generic, *condition, owner, &original_type(condition_source.expected)?, instance, active)?;
            }
        }
        for body in result.branches.iter().chain(std::iter::once(&result.fallback)) {
            let body_type = original_type(body.source.expected)?;
            if let Some((statement, value, source, identity)) = &body.terminal {
                if store.tags.get(body.instruction as usize) != Some(&FullTag::ExprValueBlock) { return Err(pattern_invalid()); }
                let body_words = store.payload(store.data[body.instruction as usize].range())?;
                let statements = pattern_block(store, *body_words.first().ok_or_else(pattern_invalid)?)?;
                if statements.first().copied().map(|count| count as usize) != Some(statements.len() - 1)
                    || statements.last() != Some(statement) || store.tags.get(*statement as usize) != Some(&FullTag::StmtValue)
                    || store.payload(store.data[*statement as usize].range())? != [*value] { return Err(pattern_invalid()); }
                let expected = match source {
                    PreparedPatternResultTerminalSource::Expression(source) => source.expected,
                    PreparedPatternResultTerminalSource::PatternCapture { identity: expected_identity, expected } => {
                        let original = generic.pattern_use(*value).ok_or_else(pattern_invalid)?;
                        let capture = generic.pattern_capture(original.capture)?;
                        if original.origin != SourceUseIdentity::Statement(*identity) || original.owner != owner
                            || capture.identity != *expected_identity || capture.expected != *expected { return Err(pattern_invalid()); }
                        *expected
                    }
                };
                Self::verify_pattern_result_value(store, generic, *value, owner, &original_type(expected)?, instance, active)?;
            } else {
                Self::verify_pattern_result_value(store, generic, body.instruction, owner, &body_type, instance, active)?;
            }
        }
        Ok(())
    }

    fn verify_pattern_result_value(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        if let Some(operation) = generic.operation_at(instruction)? {
            let source = generic.operation_source(operation.source)?;
            let TypeRef::Ground(result) = operation.result else { return Err(pattern_invalid()); };
            if source.owner != owner || store.semantic.to_type(result)? != *expected { return Err(pattern_invalid()); }
            return Ok(());
        }
        Self::verify_generic_source(store, generic, instruction, owner, expected, instance, active)
    }

    pub(super) fn verify_pattern_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        Self::verify_pattern_operand_instantiated(store, generic, instruction, owner, expected, None)
    }

    fn pattern_capture_operand<'a>(store: &FullStore, generic: &'a GenericEvidenceStore, instruction: u32, owner: InstructionOwner) -> Result<Option<(&'a PreparedPatternCapture, Option<SchemeScopeId>)>, IrVerifyError> {
        let Some(use_) = generic.pattern_use(instruction) else { return Ok(None); };
        let capture = generic.pattern_capture(use_.capture)?;
        if use_.owner != owner || !matches!(store.tags.get(instruction as usize), Some(FullTag::ExprParam | FullTag::IntSlot | FullTag::BoolSlot))
            || store.payload(store.data[instruction as usize].range())?.first() != Some(&capture.slot)
            || capture.slot != use_.slot { return Err(pattern_invalid()); }
        let application = generic.pattern_application(capture.application)?;
        Ok(Some((capture, generic.pattern_source(application.source)?.scope)))
    }

    pub(super) fn verify_pattern_operand_instantiated(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>) -> Result<bool, IrVerifyError> {
        let Some((capture, scope)) = Self::pattern_capture_operand(store, generic, instruction, owner)? else { return Ok(false); };
        let actual = if let Some(scope) = scope {
            let instance = generic.instance(instance.ok_or_else(pattern_invalid)?)?;
            if instance.scope != scope || owner != InstructionOwner::Function(generic.scope(scope)?.owner) { return Err(pattern_invalid()); }
            generic.expand(&store.semantic, capture.expected, &instance.substitutions, &mut FxHashMap::default())?
        } else { pattern_ground_type(&store.semantic, capture.expected)? };
        if actual != *expected { return Err(pattern_invalid()); }
        Ok(true)
    }

    pub(super) fn verify_pattern_symbolic_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, scope: SchemeScopeId, expected: TypeRef) -> Result<bool, IrVerifyError> {
        let owner = InstructionOwner::Function(generic.scope(scope)?.owner);
        let Some((capture, original_scope)) = Self::pattern_capture_operand(store, generic, instruction, owner)? else { return Ok(false); };
        if original_scope != Some(scope) || !generic.references_equal(&store.semantic, scope, capture.expected, expected)? { return Err(pattern_invalid()); }
        Ok(true)
    }

    fn verify_pattern_slot_leaf(generic: &GenericEvidenceStore, source: PatternSourceId, slot: Option<u32>) -> Result<(), IrVerifyError> {
        let source = generic.pattern_source(source)?;
        match (&source.shape, source.captures.as_ref(), slot) {
            (PreparedPatternShape::Wildcard, [], None) => Ok(()),
            (PreparedPatternShape::Binding, [capture], Some(slot)) if capture.slot == slot => Ok(()),
            _ => Err(pattern_invalid()),
        }
    }

    pub(super) fn verify_pattern_shape(store: &FullStore, generic: &GenericEvidenceStore, source: PatternSourceId, row: u32, depth: usize) -> Result<(), IrVerifyError> {
        if depth >= 512 { return Err(pattern_invalid()); }
        let source = generic.pattern_source(source)?;
        if matches!(source.shape, PreparedPatternShape::Group) { return Self::verify_pattern_shape(store, generic, *source.children.first().ok_or_else(pattern_invalid)?, row, depth + 1); }
        if let PreparedPatternDecision::Facet { facet } = source.decision {
            if store.patterns.get(row as usize) != Some(&FullPatternTag::Facet) { return Err(pattern_invalid()); }
            let mut input = FullCursor::new(store.payload(store.pattern_data[row as usize].range())?);
            let decoder = FullDecoder { store, owner: 0, instruction_range: 0..store.tags.len(), instruction_states: None, block_states: None, slot_count: u32::MAX,
                pattern_ceiling: Cell::new(usize::MAX), pattern_tree: None, verified: true };
            if Name::decode(&decoder, &mut input)? != facet || bool::decode(&decoder, &mut input)? { return Err(pattern_invalid()); }
            input.finish()?;
            return Ok(());
        }
        if let PreparedPatternDecision::TagConstructor { identity, family, member } | PreparedPatternDecision::ErrorVariant { identity, family, member } = source.decision {
            let original = generic.pattern_nominal(identity)?;
            if original.family != family || original.member != member { return Err(pattern_invalid()); }
            let tag = *store.patterns.get(row as usize).ok_or_else(pattern_invalid)?;
            let mut input = FullCursor::new(store.payload(store.pattern_data[row as usize].range())?);
            let decoder = FullDecoder { store, owner: 0, instruction_range: 0..store.tags.len(), instruction_states: None, block_states: None, slot_count: u32::MAX,
                pattern_ceiling: Cell::new(usize::MAX), pattern_tree: None, verified: true };
            if Name::decode(&decoder, &mut input)? != family || Name::decode(&decoder, &mut input)? != member { return Err(pattern_invalid()); }
            if original.kind == PreparedPatternNominalKind::Tag {
                let fields = match source.children.as_ref() {
                    [] => Vec::new(),
                    [child] => {
                        let child_source = generic.pattern_source(*child)?;
                        if matches!(child_source.shape, PreparedPatternShape::Tuple { .. }) { child_source.children.to_vec() } else { vec![*child] }
                    }
                    _ => return Err(pattern_invalid()),
                };
                if fields.len() != original.fields.len() { return Err(pattern_invalid()); }
                match tag {
                    FullPatternTag::Tag => {
                        let slots = BuildPatternIdSlots::decode(&decoder, &mut input)?;
                        if slots.len() != fields.len() { return Err(pattern_invalid()); }
                        for (&field, slot) in fields.iter().zip(slots) { Self::verify_pattern_slot_leaf(generic, field, slot.map(|slot| slot as u32))?; }
                    }
                    FullPatternTag::TagTest => {
                        let mut children = FullCursor::new(pattern_block(store, input.raw()?)?);
                        if children.raw()? as usize != fields.len() { return Err(pattern_invalid()); }
                        for field in fields { Self::verify_pattern_shape(store, generic, field, children.raw()?, depth + 1)?; }
                        children.finish()?;
                    }
                    _ => return Err(pattern_invalid()),
                }
            } else {
                if tag != FullPatternTag::ErrorTest { return Err(pattern_invalid()); }
                let names = match &source.shape { PreparedPatternShape::ErrorVariant { fields } => fields.as_ref(), PreparedPatternShape::TestName => &[], _ => return Err(pattern_invalid()) };
                let mut children = FullCursor::new(pattern_block(store, input.raw()?)?);
                if children.raw()? as usize != names.len() || names.len() != source.children.len() { return Err(pattern_invalid()); }
                for (&name, &child) in names.iter().zip(source.children.iter()) {
                    if Name::decode(&decoder, &mut children)? != name { return Err(pattern_invalid()); }
                    Self::verify_pattern_shape(store, generic, child, children.raw()?, depth + 1)?;
                }
                children.finish()?;
            }
            input.finish()?;
            return Ok(());
        }
        if let PreparedPatternDecision::Result { success, .. } = source.decision {
            let tag = *store.patterns.get(row as usize).ok_or_else(pattern_invalid)?;
            let mut words = FullCursor::new(store.payload(store.pattern_data[row as usize].range())?);
            match tag {
                FullPatternTag::ResultTest => {
                    if words.raw()? != u32::from(success) || source.children.len() != 1 { return Err(pattern_invalid()); }
                    Self::verify_pattern_shape(store, generic, source.children[0], words.raw()?, depth + 1)?;
                }
                FullPatternTag::ResultOk | FullPatternTag::ResultErr => {
                    if (tag == FullPatternTag::ResultOk) != success { return Err(pattern_invalid()); }
                    let slot = match words.raw()? { 0 => None, 1 => Some(words.raw()?), _ => return Err(pattern_invalid()) };
                    let unit_only = words.raw()?;
                    if unit_only != u32::from(source.children.is_empty()) { return Err(pattern_invalid()); }
                    match source.children.as_ref() {
                        [] if slot.is_none() => {},
                        [child] => Self::verify_pattern_slot_leaf(generic, *child, slot)?,
                        _ => return Err(pattern_invalid()),
                    }
                }
                _ => return Err(pattern_invalid()),
            }
            words.finish()?;
            return Ok(());
        }
        let tag = *store.patterns.get(row as usize).ok_or_else(pattern_invalid)?;
        let words = store.payload(store.pattern_data[row as usize].range())?;
        let children = |raw| -> Result<Vec<u32>, IrVerifyError> {
            let words = pattern_block(store, raw)?;
            if words.first().copied().map(|count| count as usize) != Some(words.len() - 1) { return Err(pattern_invalid()); }
            Ok(words[1..].to_vec())
        };
        let decoder = FullDecoder { store, owner: 0, instruction_range: 0..store.tags.len(), instruction_states: None, block_states: None, slot_count: u32::MAX,
            pattern_ceiling: Cell::new(usize::MAX), pattern_tree: None, verified: true };
        let mut input = FullCursor::new(words);
        let actual_children = match (&source.shape, tag) {
            (PreparedPatternShape::Wildcard, FullPatternTag::Wildcard) => Vec::new(),
            (PreparedPatternShape::Binding, FullPatternTag::Bind) => {
                if input.raw()? != source.captures.first().ok_or_else(pattern_invalid)?.slot { return Err(pattern_invalid()); } Vec::new()
            }
            (PreparedPatternShape::Alias { .. }, FullPatternTag::Alias) => {
                let child = input.raw()?;
                if input.raw()? != source.captures.first().ok_or_else(pattern_invalid)?.slot { return Err(pattern_invalid()); } vec![child]
            }
            (PreparedPatternShape::Alternation, FullPatternTag::Alternation) => children(input.raw()?)?,
            (PreparedPatternShape::List { elements, has_rest }, FullPatternTag::List) => {
                let mut children = children(input.raw()?)?;
                if children.len() != *elements as usize || input.raw()? != u32::from(*has_rest) { return Err(pattern_invalid()); }
                if *has_rest { children.push(input.raw()?); } children
            }
            (PreparedPatternShape::Record { fields }, FullPatternTag::RecordTest) => {
                let mut fields_cursor = FullCursor::new(pattern_block(store, input.raw()?)?);
                if fields_cursor.raw()? as usize != fields.len() { return Err(pattern_invalid()); }
                let mut children = Vec::new();
                for field in fields { if Name::decode(&decoder, &mut fields_cursor)? != *field { return Err(pattern_invalid()); } children.push(fields_cursor.raw()?); }
                fields_cursor.finish()?; children
            }
            (PreparedPatternShape::Type, FullPatternTag::Type) => {
                let ty = Type::decode(&decoder, &mut input)?;
                if Some(ty) != source.tested.map(|ty| pattern_ground_type(&store.semantic, ty)).transpose()? { return Err(pattern_invalid()); }
                let slot = Option::<usize>::decode(&decoder, &mut input)?;
                if slot != source.captures.first().map(|capture| capture.slot as usize) { return Err(pattern_invalid()); } Vec::new()
            }
            (PreparedPatternShape::Literal(expected), FullPatternTag::Literal) => {
                let actual = LoweredValue::decode(&decoder, &mut input)?;
                use crate::sema::constants::LiteralConstant as L;
                let equal = match (expected, actual) {
                    (L::Null, LoweredValue::Null) => true,
                    (L::Int(a), LoweredValue::Int(b)) => *a == b,
                    (L::Bool(a), LoweredValue::Bool(b)) => *a == b,
                    (L::Float(a), LoweredValue::Float(b)) => *a == b.0.to_bits(),
                    (L::Duration(a), LoweredValue::Duration(b)) => *a == b.millis,
                    (L::Str(a), LoweredValue::Str(b)) => *a == b,
                    (L::Bytes(a), LoweredValue::Bytes(b)) => *a == b,
                    _ => false,
                };
                if !equal { return Err(pattern_invalid()); } Vec::new()
            }
            _ => return Err(pattern_invalid()),
        };
        input.finish()?;
        if actual_children.len() != source.children.len() { return Err(pattern_invalid()); }
        for (&source, actual) in source.children.iter().zip(actual_children) { Self::verify_pattern_shape(store, generic, source, actual, depth + 1)?; }
        Ok(())
    }

    pub(super) fn verify_pattern_evidence(store: &FullStore, tree: &PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_ref() else { return Ok(()); };
        let mut applications = BTreeMap::new();
        for (id, application) in generic.pattern_applications() {
            let words = store.payload(store.data.get(application.matcher as usize).ok_or_else(pattern_invalid)?.range())?;
            let arms = match_arms(store, application.matcher)?;
            if words.first() != Some(&application.subject) || arms.get(application.arm as usize) != Some(&(application.pattern, application.guard, application.body))
                || generic.registered_pattern_origin(application.pattern) != Some((generic.pattern_source(application.source)?.origin, application.owner)) { return Err(pattern_invalid()); }
            Self::verify_argument_initializer_lineage(store, generic, application.subject, application.subject_source,
                &application.subject_wrappers, application.owner)?;
            Self::verify_pattern_shape(store, generic, application.source, application.pattern, 0)?;
            if let PreparedPatternAdmission::Conditional { control, branch, body, .. } = application.admission {
                let (condition, actual_body, slots) = conditional_branch(store, control, branch)?;
                if condition != application.matcher || body != actual_body || application.arm != 0 || application.guard.is_some()
                    || arms.len() != 2 || !tree.is_descendant(control, application.matcher)? { return Err(pattern_invalid()); }
                let bool_body = |body, expected| -> Result<bool, IrVerifyError> {
                    let PatternArmBody::Expression(body) = body else { return Ok(false); };
                    Ok(store.tags.get(body as usize) == Some(&FullTag::ExprBool)
                        && store.payload(store.data[body as usize].range())? == [u32::from(expected)])
                };
                if !bool_body(application.body, true)? || arms[1].1.is_some() || !bool_body(arms[1].2, false)?
                    || store.patterns.get(arms[1].0 as usize) != Some(&FullPatternTag::Wildcard)
                    || generic.registered_pattern_origin(arms[1].0).is_some() { return Err(pattern_invalid()); }
                let expected = visible_pattern_captures(generic, application.source)?.into_iter().map(|capture| capture.slot).collect::<BTreeSet<_>>();
                if slots.iter().copied().collect::<BTreeSet<_>>() != expected || slots.len() != expected.len() { return Err(pattern_invalid()); }
            }
            let source = generic.pattern_source(application.source)?;
            if let Some(scope) = source.scope { Self::verify_generic_symbolic_source(store, generic, application.subject, scope, source.input, &mut Vec::new())?; }
            else { Self::verify_generic_source(store, generic, application.subject, application.owner, &pattern_ground_type(&store.semantic, source.input)?, None, &mut Vec::new())?; }
            applications.insert((application.matcher, application.arm), id);
        }
        for matcher in 0..store.tags.len() {
            if !matches!(store.tags[matcher], FullTag::ExprMatch | FullTag::StmtMatch) { continue; }
            for (arm, (pattern, _, _)) in match_arms(store, matcher as u32)?.into_iter().enumerate() {
                if generic.registered_pattern_origin(pattern).is_some() && !applications.contains_key(&(matcher as u32, arm as u32)) { return Err(pattern_invalid()); }
            }
        }
        for use_ in generic.pattern_uses() {
            let capture = generic.pattern_capture(use_.capture)?;
            let application = generic.pattern_application(capture.application)?;
            let mut visible = application.guard.map(|guard| tree.is_descendant(guard, use_.instruction)).transpose()?.unwrap_or(false);
            let body = match application.admission { PreparedPatternAdmission::MatchArm => application.body,
                PreparedPatternAdmission::Conditional { body, .. } => body };
            match body {
                PatternArmBody::Expression(body) => visible |= tree.is_descendant(body, use_.instruction)?,
                PatternArmBody::Statements(body) => {
                    let statements = store.payload(store.blocks.get(body.index()).ok_or_else(pattern_invalid)?.instructions)?;
                    for &statement in statements.iter().skip(1) { visible |= tree.is_descendant(statement, use_.instruction)?; }
                }
            }
            let source = generic.pattern_source(application.source)?;
            let valid = if let Some(scope) = source.scope { Self::verify_pattern_symbolic_operand(store, generic, use_.instruction, scope, capture.expected)? }
                else { Self::verify_pattern_operand(store, generic, use_.instruction, use_.owner, &pattern_ground_type(&store.semantic, capture.expected)?)? };
            if !visible || !valid { return Err(pattern_invalid()); }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::Evaluator;
    use crate::sema::check::Checker;

    fn prepared_pattern_program(source: &str) -> FullProgram {
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "pattern-proof-mutants.xsh", crate::loader::entry_source_from_text("pattern-proof-mutants.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let weak = Arc::downgrade(&checked.solved);
        let counters = checked.solved.graph.counters().clone();
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
        assert_eq!(checked.solved.graph.counters(), &counters);
        drop(checked); drop(parsed);
        assert!(weak.upgrade().is_none());
        evaluator.indexed_program.as_ref().unwrap().as_ref().clone()
    }

    #[test]
    fn retry_selection_roots_remain_reachable_without_match_applications_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared_pattern_program("error RetryError = Busy(message: Str) | Fatal(message: Str)\nproc attempt() -> Result[Int, RetryError] { 7 }\nproc selected() [time] -> Result[Int, RetryError] { retry [0ms] on (RetryError.Busy) { attempt()? } }\n");
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            assert_eq!(generic.pattern_applications().count(), 0);
            let (_, capture) = generic.try_capture_sources().find(|(_, source)| source.retry.is_some()).unwrap();
            let (origin, _, pattern) = capture.retry.as_ref().unwrap().selection.unwrap();
            let (id, source) = generic.pattern_sources().find(|(_, source)| source.origin == origin).unwrap();
            assert_eq!(generic.registered_pattern_origin(pattern), Some((origin, capture.owner)));
            assert_eq!(super::super::super::pattern::pattern_ground_type(&program.store.semantic, source.input).unwrap(), Type::ErrorFamily(Name::intern("RetryError")));
            assert!(FullVerifier::verify(&program).is_ok());
            let mut removed = program.clone();
            removed.store.generic.as_mut().unwrap().test_remove_pattern_evidence();
            assert!(FullVerifier::verify(&removed).is_err(), "the original retry selection still requires its checked pattern tree");
            let mut changed = program.clone();
            changed.store.generic.as_mut().unwrap().test_pattern_source_mut(id).unwrap().origin.namespace = Some(Name::intern("outside"));
            assert!(FullVerifier::verify(&changed).is_err(), "a same-shaped selection from another lexical owner is foreign");
        });
    }

    #[test]
    fn cold_patterns_reject_same_type_branch_and_identifier_slot_swaps() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared_pattern_program(include_str!("../../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh"));
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let application = generic.pattern_applications().find(|(_, application)| {
                let source = generic.pattern_source(application.source).unwrap();
                matches!(source.shape, PreparedPatternShape::Alternation)
                    && source.captures.iter().any(|capture| capture.identity.name.as_str().as_str() == "left")
            }).unwrap().1;
            let alt_words = program.store.payload(program.store.pattern_data[application.pattern as usize].range()).unwrap();
            let second_branch = pattern_block(&program.store, alt_words[0]).unwrap()[2];
            let list_words = program.store.payload(program.store.pattern_data[second_branch as usize].range()).unwrap();
            let bindings = pattern_block(&program.store, list_words[0]).unwrap();
            let first = program.store.pattern_data[bindings[1] as usize].range().bounds(program.store.extra.len()).unwrap().start;
            let second = program.store.pattern_data[bindings[2] as usize].range().bounds(program.store.extra.len()).unwrap().start;
            let mut swapped_branch = program.clone();
            swapped_branch.store.extra.swap(first, second);
            assert!(FullVerifier::verify(&swapped_branch).is_err(), "equal Int types and identical slot sets cannot exchange original branch bindings");

            let left = generic.pattern_uses().iter().find(|use_| generic.pattern_capture(use_.capture).unwrap().identity.name.as_str().as_str() == "left").unwrap();
            let right_slot = generic.pattern_captures().find(|(_, capture)| capture.identity.name.as_str().as_str() == "right").unwrap().1.slot;
            let words = program.store.data[left.instruction as usize].range().bounds(program.store.extra.len()).unwrap();
            let mut swapped_use = program.clone();
            swapped_use.store.extra[words.start] = right_slot;
            assert!(FullVerifier::verify(&swapped_use).is_err(), "an original left read cannot silently become a same-typed right read");
        });
    }

    #[test]
    fn cold_pattern_reads_reject_coupled_same_arm_capture_and_instruction_swaps() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared_pattern_program(include_str!("../../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh"));
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let left = generic.pattern_uses().iter().find(|use_| generic.pattern_capture(use_.capture).unwrap().identity.name.as_str().as_str() == "left").unwrap();
            let left_capture = generic.pattern_capture(left.capture).unwrap();
            let (right_id, right) = generic.pattern_captures().find(|(_, capture)| capture.application == left_capture.application && capture.identity.name.as_str().as_str() == "right").unwrap();
            assert_eq!(right.expected, left_capture.expected);
            let mut changed = program.clone();
            let range = changed.store.data[left.instruction as usize].range().bounds(changed.store.extra.len()).unwrap();
            changed.store.extra[range.start] = right.slot;
            let use_ = changed.store.generic.as_mut().unwrap().test_pattern_use_mut(left.instruction).unwrap();
            use_.capture = right_id;
            use_.slot = right.slot;
            assert!(FullVerifier::verify(&changed).is_err(), "the original left identifier cannot become right by changing both its use receipt and same-arm instruction");
        });
    }

    #[test]
    fn cold_patterns_reject_coupled_same_typed_subject_and_application_swaps() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(left: List[Int], right: List[Int]) -> Int { let ignored = right; match left { [number] => number, _ => 0 } }\n";
            let program = prepared_pattern_program(source);
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let (id, application) = generic.pattern_applications().find(|(_, application)| matches!(generic.pattern_source(application.source).unwrap().shape, PreparedPatternShape::List { .. })).unwrap();
            let InstructionOwner::Function(owner) = application.owner else { panic!("the fixture has a declaration-owned matcher"); };
            let (replacement, origin) = program.store.function_instruction_range(owner.index()).unwrap().find_map(|instruction| {
                if program.store.tags[instruction] != FullTag::ExprParam { return None; }
                let words = program.store.payload(program.store.data[instruction].range()).unwrap();
                if words[0] != 1 { return None; }
                let (super::super::super::generic::OperationSourceOrigin::Expression(origin), actual_owner) = generic.registered_instruction_origin(instruction as u32, false)? else { return None; };
                (actual_owner == application.owner).then_some((instruction as u32, origin))
            }).unwrap();
            assert_ne!(replacement, application.subject);
            let mut changed = program.clone();
            let range = changed.store.data[application.matcher as usize].range().bounds(changed.store.extra.len()).unwrap();
            changed.store.extra[range.start] = replacement;
            let other_parent = program.store.function_instruction_range(owner.index()).unwrap().find(|&instruction| {
                program.store.tags[instruction] == FullTag::StmtLet
                    && program.store.payload(program.store.data[instruction].range()).unwrap()[1] == replacement
            }).unwrap();
            let other_range = changed.store.data[other_parent].range().bounds(changed.store.extra.len()).unwrap();
            changed.store.extra[other_range.start + 1] = application.subject;
            let changed_application = changed.store.generic.as_mut().unwrap().test_pattern_application_mut(id).unwrap();
            changed_application.subject = replacement;
            changed_application.subject_origin = origin;
            let verified = FullVerifier::verify(&changed);
            assert!(verified.is_err(), "the original matcher cannot select the independent right parameter by changing both its application and encoded subject");
        });
    }

    #[test]
    fn cold_patterns_reject_coupled_capture_types_removed_proofs_and_wrong_arms() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared_pattern_program(include_str!("../../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh"));
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let (capture_id, capture) = generic.pattern_captures().find(|(_, capture)| capture.identity.name.as_str().as_str() == "left").unwrap();
            let (source_id, _) = generic.pattern_sources().find(|(_, source)| source.origin == capture.identity.pattern).unwrap();
            let mut changed = program.clone();
            let ty = SemanticPoolBuilder::default().intern_type(&mut changed.store.semantic, &Type::Str).unwrap();
            let changed_generic = changed.store.generic.as_mut().unwrap();
            changed_generic.test_pattern_capture_mut(capture_id).unwrap().expected = TypeRef::Ground(ty);
            let leaf = changed_generic.test_pattern_source_mut(source_id).unwrap();
            leaf.input = TypeRef::Ground(ty); leaf.captures[0].expected = TypeRef::Ground(ty);
            assert!(FullVerifier::verify(&changed).is_err(), "changing both capture receipts cannot alter the saved List[Int] parent relationship");

            let mut removed = program.clone();
            removed.store.generic.as_mut().unwrap().test_remove_pattern_evidence();
            assert!(FullVerifier::verify(&removed).is_err(), "original pattern origins require evidence even when all derived tables are removed");

            let mut wrong_arm = program.clone();
            let (application_id, _) = generic.pattern_applications().next().unwrap();
            wrong_arm.store.generic.as_mut().unwrap().test_pattern_application_mut(application_id).unwrap().arm += 1;
            assert!(FullVerifier::verify(&wrong_arm).is_err(), "a capture application cannot claim another decoded arm");
        });
    }

    #[test]
    fn bare_tail_capture_reads_retain_statement_identity_and_arm_visibility() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(values: List[Int]) -> Int { match values { [left] if left > 0 => left, [right] => right, _ => 0 } }\n";
            let program = prepared_pattern_program(source);
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let left = generic.pattern_uses().iter().find(|use_| {
                matches!(use_.origin, SourceUseIdentity::Statement(_)) && generic.pattern_capture(use_.capture).unwrap().identity.name.as_str().as_str() == "left"
            }).unwrap();
            let (right_id, right) = generic.pattern_captures().find(|(_, capture)| capture.identity.name.as_str().as_str() == "right").unwrap();
            let mut changed = program.clone();
            let words = changed.store.data[left.instruction as usize].range().bounds(changed.store.extra.len()).unwrap();
            changed.store.extra[words.start] = right.slot;
            let use_ = changed.store.generic.as_mut().unwrap().test_pattern_use_mut(left.instruction).unwrap();
            use_.capture = right_id; use_.slot = right.slot;
            assert!(FullVerifier::verify(&changed).is_err(), "a same-typed capture from a different lexical arm is not visible in the original statement");
        });
    }

    #[test]
    fn pattern_handles_reject_foreign_roots_and_rewound_serials() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared_pattern_program(include_str!("../../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh"));
            let _symbols = program.symbol_owner().enter();
            let payload = program.store.generic.as_ref().unwrap().pattern_sources().next().unwrap().1.clone();
            let mut builder = GenericEvidenceBuilder::default();
            let checkpoint = builder.checkpoint();
            let retired = builder.add_pattern_source(payload.clone()).unwrap();
            assert!(builder.pattern_source(retired).is_ok());
            builder.rewind(checkpoint).unwrap();
            let replacement = builder.add_pattern_source(payload.clone()).unwrap();
            assert_ne!(retired, replacement, "rewind must never recycle proof serials");
            assert!(builder.pattern_source(retired).is_err());
            assert!(builder.pattern_source(replacement).is_ok());
            let mut outsider = GenericEvidenceBuilder::default();
            let foreign = outsider.add_pattern_source(payload).unwrap();
            assert!(builder.pattern_source(foreign).is_err());
            let mut changed = program.clone();
            let application = changed.store.generic.as_ref().unwrap().pattern_applications().next().unwrap().0;
            changed.store.generic.as_mut().unwrap().test_pattern_application_mut(application).unwrap().source = foreign;
            assert!(FullVerifier::verify(&changed).is_err(), "same-shaped source evidence belongs to one immutable program");
        });
    }

    fn assert_pattern_runtime_after_frontend_drop(source: &str, expected: &[u8]) {
        for recursive in [false, true] {
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                "pattern-payloads.xsh", crate::loader::entry_source_from_text("pattern-payloads.xsh", source.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let source_id = SourceMap::files(&sources).first().unwrap().id();
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let weak = Arc::downgrade(&checked.solved);
            let counters = checked.solved.graph.counters().clone();
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
            let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
            assert_eq!(checked.solved.graph.counters(), &counters);
            drop(checked); drop(parsed);
            assert!(weak.upgrade().is_none());
            let symbols = evaluator.indexed_program.as_ref().unwrap().symbol_owner().clone();
            let execute = || symbols.with_current(|| {
                assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("prepared program remains installed"))
            });
            let output = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) } else { execute() };
            assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
            assert_eq!(output.stdout, expected, "recursive={recursive}: {:?}", output.diagnostics);
            assert!(output.stderr.is_empty()); assert!(output.diagnostics.is_empty());
        }
    }

    #[test]
    fn ground_result_payload_captures_keep_success_and_error_types_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(value: Result[Int, Int]) -> Int { match value { Ok(number) => number + 1, Err(problem) => problem + 2 } }\npure nested(value: Result[List[Int], Int]) -> Int { match value { Ok([number, ..rest]) => number + rest.len(), Err(problem) => problem, _ => 0 } }\nprint ${selected(Ok(7))}\nprint ${selected(Err(9))}\nprint ${nested(Ok([5, 6, 7]))}\n";
            assert_pattern_runtime_after_frontend_drop(source, b"8\n11\n7\n");
        });
    }

    #[test]
    fn declared_dynamic_list_patterns_preserve_narrowed_capture_and_rest_types_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(value: Any) -> Int { match value { [number is Int, ..tail] => number + tail.len(), _ => 0 } }\nprint ${selected([7, 8, 9])}\nprint ${selected(\"quiet\")}\n";
            assert_pattern_runtime_after_frontend_drop(source, b"9\n0\n");
        });
    }

    #[test]
    fn ground_driver_captures_keep_their_actual_step_owner_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "let answer = match [7] { [number] => number + 1, _ => 0 }\nprint $answer\n";
            assert_pattern_runtime_after_frontend_drop(source, b"8\n");
        });
    }

    #[test]
    fn scoped_pattern_list_proofs_reject_other_rigids_and_foreign_callers() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(left, right) { let empty: List[Int] = []; let values = [left, left]; match values { [first, second, ..rest] => first, _ => left } }\npure unrelated(value) { match value { original => original } }\nprint ${selected(7, \"word\")}\nprint ${selected(\"word\", 9)}\n";
            let program = prepared_pattern_program(source);
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let original = generic.pattern_sources().find(|(_, source)| matches!(source.shape, PreparedPatternShape::List { elements: 2, .. })).unwrap().1;
            let scope = original.scope.unwrap();
            let owner = generic.scope(scope).unwrap().owner;
            let lists = program.store.function_instruction_range(owner.index()).unwrap().filter(|&instruction| program.store.tags[instruction] == FullTag::ExprList).collect::<Vec<_>>();
            let list = *lists.iter().find(|&&instruction| {
                let words = program.store.payload(program.store.data[instruction].range()).unwrap();
                pattern_block(&program.store, words[0]).unwrap()[0] == 2
            }).unwrap();
            assert!(FullVerifier::verify_generic_symbolic_source(&program.store, generic, list as u32, scope, original.input, &mut Vec::new()).is_ok());
            let words = program.store.payload(program.store.data[list].range()).unwrap();
            let element = pattern_block(&program.store, words[0]).unwrap()[1];
            assert_eq!(program.store.tags[element as usize], FullTag::ExprParam);
            let mut changed = program.clone();
            let range = changed.store.data[element as usize].range().bounds(changed.store.extra.len()).unwrap();
            assert_eq!(changed.store.extra[range.start], 0);
            changed.store.extra[range.start] = 1;
            assert!(FullVerifier::verify(&changed).is_err(), "equal physical Generic storage cannot replace the original independent left binder with right");
            let foreign = generic.scopes().find(|(id, _)| *id != scope).unwrap().0;
            assert!(FullVerifier::verify_generic_symbolic_source(&program.store, generic, list as u32, foreign, original.input, &mut Vec::new()).is_err(), "a same-shaped rigid cannot authorize another declaration's list instruction");
            assert_pattern_runtime_after_frontend_drop(source, b"7\nword\n");
        });
    }

    #[test]
    fn scoped_pattern_list_captures_preserve_item_and_rest_binders_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure first(value) { let values = [value]; match values { [original, ..rest] => original, _ => value } }\npure forwarded(value) { first(value) }\nprint ${forwarded(7)}\nprint ${forwarded(\"word\")}\n";
            assert_pattern_runtime_after_frontend_drop(source, b"7\nword\n");
        });
    }

    #[test]
    fn scoped_pattern_result_captures_preserve_original_success_binder_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure preserved(value) { match Ok(value) { Ok(original) => original, Err(_) => value } }\nprint ${preserved(7)}\nprint ${preserved(\"word\")}\n";
            assert_pattern_runtime_after_frontend_drop(source, b"7\nword\n");
        });
    }

    #[test]
    fn scoped_pattern_captures_keep_original_caller_binders_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(value) { match value { original => original } }\npure forwarded(value) { selected(value) }\nprint ${forwarded(7)}\nprint ${forwarded(\"word\")}\nprint ${forwarded(\"next\")}\nprint ${forwarded(9)}\n";
            assert_pattern_runtime_after_frontend_drop(source, b"7\nword\nnext\n9\n");
        });
    }

    #[test]
    fn fs_root_result_patterns_keep_original_subject_receipts_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = include_str!("../../../../../tests/fixtures/frontend-indexed/fs-root-methods.xsh");
            let program = prepared_pattern_program(source);
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let (source_id, original) = generic.pattern_sources().find(|(_, source)| matches!(source.decision,
                PreparedPatternDecision::Result { success: false, .. })).unwrap();
            assert!(original.caller.is_some());
            let (application_id, application) = generic.pattern_applications().find(|(_, application)| application.source == source_id).unwrap();
            assert_ne!(application.subject, application.subject_source);
            assert!(!application.subject_wrappers.is_empty());
            assert_eq!(generic.registered_instruction_origin(application.subject_source, false),
                Some((crate::runtime::eval::indexed::generic::OperationSourceOrigin::Expression(application.subject_origin), application.owner)));
            assert!(generic.registered_instruction_origin(application.subject, false).is_none(), "saving a receiver does not create another authored result expression");
            assert!(FullVerifier::verify(&program).is_ok());
            let mut removed = program.clone();
            removed.store.generic.as_mut().unwrap().test_remove_pattern_evidence();
            assert!(FullVerifier::verify(&removed).is_err(), "the FsRoot result operation cannot supply a missing independent pattern receipt");
            let mut foreign = program.clone();
            foreign.store.generic.as_mut().unwrap().test_pattern_source_mut(source_id).unwrap().origin.namespace = Some(Name::intern("unrelated"));
            assert!(FullVerifier::verify(&foreign).is_err(), "another namespace cannot authorize the FsRoot result pattern with the same local pattern number");
            let mut forged_source = program.clone();
            let changed = forged_source.store.generic.as_mut().unwrap().test_pattern_application_mut(application_id).unwrap();
            changed.subject_source = changed.subject;
            changed.subject_wrappers = Box::new([]);
            assert!(FullVerifier::verify(&forged_source).is_err(), "removing receiver wrappers cannot turn the saved binding into the original authored result source");
        });
    }

    #[test]
    fn scoped_pattern_capture_reads_reject_another_declarations_equal_storage() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(value) { match value { original => original } }\npure unrelated(value) { match value { original => original } }\nprint ${selected(7)}\nprint ${selected(\"word\")}\n";
            let program = prepared_pattern_program(source);
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let original = generic.pattern_uses().first().unwrap();
            let capture = generic.pattern_capture(original.capture).unwrap();
            let application = generic.pattern_application(capture.application).unwrap();
            let scope = generic.pattern_source(application.source).unwrap().scope.unwrap();
            let foreign = generic.scopes().find(|(id, _)| *id != scope).unwrap().0;
            assert!(FullVerifier::verify_pattern_symbolic_operand(&program.store, generic, original.instruction, scope, capture.expected).unwrap());
            assert!(FullVerifier::verify_pattern_symbolic_operand(&program.store, generic, original.instruction, foreign, capture.expected).is_err(), "a generic capture read belongs to its original lexical declaration even when another declaration allocates identical storage");
            assert_pattern_runtime_after_frontend_drop(source, b"7\nword\n");
        });
    }

    #[test]
    fn conditional_bare_capture_tails_keep_original_statement_authority_for_typed_calls() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure identity(value) { value }\npure selected(value: Result[Str, Str]) -> Str { identity(if let Ok(original) = value { original } else { \"missing\" }) }\nprint ${selected(Ok(\"word\"))}\nprint ${selected(Err(\"quiet\"))}\n";
            let program = prepared_pattern_program(source);
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let result = generic.pattern_applications().find_map(|(_, application)| application.result.as_ref()).unwrap();
            let (statement, value, original, identity) = result.branches[0].terminal.as_ref().unwrap();
            let PreparedPatternResultTerminalSource::PatternCapture { identity: capture, .. } = original else { panic!("a bare capture tail retains capture authority instead of an invented expression origin"); };
            let use_ = generic.pattern_use(*value).unwrap();
            assert_eq!(use_.origin, SourceUseIdentity::Statement(*identity));
            assert_eq!(generic.pattern_capture(use_.capture).unwrap().identity, *capture);
            assert_eq!(generic.registered_instruction_origin(*value, false), Some((crate::runtime::eval::indexed::generic::OperationSourceOrigin::Statement(*identity), result.owner)));
            let (_, fallback, _, _) = result.fallback.terminal.as_ref().unwrap();
            let mut changed = program.clone();
            let words = changed.store.data[*statement as usize].range().bounds(changed.store.extra.len()).unwrap();
            changed.store.extra[words.start] = *fallback;
            assert!(FullVerifier::verify(&changed).is_err(), "another Str-valued tail cannot replace the original successful capture statement");
            assert_pattern_runtime_after_frontend_drop(source, b"word\nmissing\n");
        });
    }

    #[test]
    fn ground_builtin_facet_patterns_keep_nominal_alias_capture_types_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "error Failure = Missing(code: Int) : NotFound | Broken(code: Int) : InvalidData\npure answer(value: Failure) -> Int { 1 }\npure selected(value: Failure) -> Int { match value { (is NotFound) as original => answer(original), _ => 2 } }\nprint ${selected(Failure.Missing(code: 7))}\nprint ${selected(Failure.Broken(code: 9))}\n";
            let program = prepared_pattern_program(source);
            let _symbols = program.symbol_owner().enter();
            let row = program.store.patterns.iter().position(|tag| *tag == FullPatternTag::Facet).unwrap();
            let range = program.store.pattern_data[row].range().bounds(program.store.extra.len()).unwrap();
            let mut changed = program.clone();
            changed.store.extra[range.start] = Name::intern("InvalidData").symbol().raw();
            assert!(FullVerifier::verify(&changed).is_err(), "the encoded predicate must preserve the original registered facet");
            let mut wrapped = program.clone();
            wrapped.store.extra[range.start + 1] = 1;
            assert!(FullVerifier::verify(&wrapped).is_err(), "a plain error facet cannot become a Result error projection");
            assert_pattern_runtime_after_frontend_drop(source, b"1\n2\n");
        });
    }

    #[test]
    fn ground_conditional_patterns_keep_captures_inside_the_admitted_branch_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(value: Result[Int, Int]) -> Int { if let Ok(number) = value { number + 1 } else { 0 } }\npure expression_selected(value: Result[Int, Int]) -> Int { let answer = if let Ok(number) = value { number + 1 } else { 0 }; answer }\nprint ${selected(Ok(7))}\nprint ${selected(Err(9))}\nprint ${expression_selected(Ok(7))}\nprint ${expression_selected(Err(9))}\n";
            let program = prepared_pattern_program(source);
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let (id, application) = generic.pattern_applications().find(|(_, application)| {
                matches!(application.admission, PreparedPatternAdmission::Conditional { control, .. } if program.store.tags[control as usize] == FullTag::ExprPatternIf)
            }).unwrap();
            let PreparedPatternAdmission::Conditional { control, branch, body, .. } = application.admission else { unreachable!() };
            assert_eq!(branch, 0);
            let PatternArmBody::Expression(yes) = application.body else { unreachable!() };
            let mut false_admission = program.clone();
            let range = false_admission.store.data[yes as usize].range().bounds(false_admission.store.extra.len()).unwrap();
            false_admission.store.extra[range.start] = 0;
            assert!(FullVerifier::verify(&false_admission).is_err(), "the capture admission protocol must report true for its matching arm");
            let control_words = program.store.payload(program.store.data[control as usize].range()).unwrap();
            let branches = pattern_block(&program.store, control_words[0]).unwrap();
            let capture_block = program.store.blocks[IrBlockId::from_raw(branches[3]).unwrap().index()].instructions;
            let mut wrong_capture = program.clone();
            let range = capture_block.bounds(wrong_capture.store.extra.len()).unwrap();
            assert_eq!(wrong_capture.store.extra[range.start], 1);
            wrong_capture.store.extra[range.start + 1] = 0;
            assert!(FullVerifier::verify(&wrong_capture).is_err(), "the admitted branch cannot hydrate the original parameter slot instead of its checked capture");
            let mut wrong_body = program.clone();
            let original = wrong_body.store.generic.as_mut().unwrap().test_pattern_application_mut(id).unwrap();
            let PreparedPatternAdmission::Conditional { body: expected_body, .. } = &mut original.admission else { unreachable!() };
            assert_ne!(body, PatternArmBody::Expression(control_words[1]));
            *expected_body = PatternArmBody::Expression(control_words[1]);
            assert!(FullVerifier::verify(&wrong_body).is_err(), "a saved true-branch capture cannot acquire the else body's lexical visibility");
            let (result_id, result_application) = generic.pattern_applications().find(|(_, application)| application.result.is_some()).unwrap();
            let result = result_application.result.as_ref().unwrap();
            let (statement, value, _, _) = result.branches[0].terminal.as_ref().unwrap();
            let (_, fallback, _, _) = result.fallback.terminal.as_ref().unwrap();
            assert_ne!(value, fallback);
            let mut wrong_terminal = program.clone();
            let range = wrong_terminal.store.data[*statement as usize].range().bounds(wrong_terminal.store.extra.len()).unwrap();
            wrong_terminal.store.extra[range.start] = *fallback;
            assert!(FullVerifier::verify(&wrong_terminal).is_err(), "the same Int type cannot replace an original branch's value expression");
            let mut coupled_terminal = wrong_terminal.clone();
            let application = coupled_terminal.store.generic.as_mut().unwrap().test_pattern_application_mut(result_id).unwrap();
            application.result.as_mut().unwrap().branches[0].terminal.as_mut().unwrap().1 = *fallback;
            assert!(FullVerifier::verify(&coupled_terminal).is_err(), "an encoded branch and rewritten expected terminal cannot authorize each other");
            assert_pattern_runtime_after_frontend_drop(source, b"8\n0\n8\n0\n");
        });
    }

    #[test]
    fn ground_pattern_while_captures_keep_the_original_loop_body_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(value: Result[Int, Int]) -> Int { var answer = 0; while let Ok(number) = value { answer = number + 1; break }; answer }\nprint ${selected(Ok(7))}\nprint ${selected(Err(9))}\n";
            assert_pattern_runtime_after_frontend_drop(source, b"8\n0\n");
        });
    }

    #[test]
    fn ground_conditional_result_roots_preserve_nullable_branch_widening_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(value: Result[Int, Int]) -> Int? { let answer = if let Ok(number) = value { number + 1 } else { null }; answer }\nprint ${selected(Ok(7)) == 8}\nprint ${selected(Err(9)) == null}\n";
            let program = prepared_pattern_program(source);
            let generic = program.store.generic.as_ref().unwrap();
            let result = generic.pattern_applications().find_map(|(_, application)| application.result.as_ref()).unwrap();
            let TypeRef::Ground(ty) = result.source.expected else { panic!("the original result is a closed nullable Int"); };
            assert_eq!(program.store.semantic.to_type(ty).unwrap(), Type::Optional(Box::new(Type::Int)));
            let TypeRef::Ground(ty) = result.branches[0].source.expected else { unreachable!() };
            assert_eq!(program.store.semantic.to_type(ty).unwrap(), Type::Int);
            let TypeRef::Ground(ty) = result.fallback.source.expected else { unreachable!() };
            assert_eq!(program.store.semantic.to_type(ty).unwrap(), Type::Null);
            assert_pattern_runtime_after_frontend_drop(source, b"true\ntrue\n");
        });
    }

    #[test]
    fn ground_nominal_tag_payloads_keep_canonical_members_and_field_order_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "enum Token { Number(Int), Pair(Int, Int), Empty }\npure selected(value: Token) -> Int { match value { Number(number) => number + 1, Pair(left, right) => left * 10 + right, Empty => 0 } }\nprint ${selected(Number(7))}\nprint ${selected(Pair(3, 9))}\n";
            assert_pattern_runtime_after_frontend_drop(source, b"8\n39\n");
        });
    }

    #[test]
    fn ground_nominal_error_fields_keep_original_member_identity_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let source = "error Failure = Raised(code: Int) | Other(code: Int)\npure selected(value: Result[Int, Failure]) -> Int { match value { Ok(number) => number, Err(Failure.Raised {code: number}) => number + 1, Err(Failure.Other {code: number}) => number + 2, _ => 0 } }\nprint ${selected(Err(Failure.Raised(code: 7)))}\nprint ${selected(Err(Failure.Other(code: 8)))}\n";
            assert_pattern_runtime_after_frontend_drop(source, b"8\n10\n");
        });
    }

    #[test]
    fn cold_patterns_reject_missing_caller_and_foreign_qualified_source_owners() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared_pattern_program("enum Token { Number(Int) }\npure selected(value: Token) -> Int { match value { Number(number) => number + 1 } }\n");
            let _symbols = program.symbol_owner().enter();
            assert!(FullVerifier::verify(&program).is_ok());
            let source_id = program.store.generic.as_ref().unwrap().pattern_applications().next().unwrap().1.source;
            let mut missing = program.clone();
            missing.store.generic.as_mut().unwrap().test_pattern_source_mut(source_id).unwrap().caller = None;
            assert!(FullVerifier::verify(&missing).is_err(), "a declaration pattern cannot become an unowned driver pattern");
            let mut foreign = program.clone();
            foreign.store.generic.as_mut().unwrap().test_pattern_source_mut(source_id).unwrap().origin.namespace = Some(Name::intern("unrelated"));
            assert!(FullVerifier::verify(&foreign).is_err(), "the same local pattern number in another namespace does not authorize the original matcher");
        });
    }

    #[test]
    fn cold_nominal_patterns_keep_original_input_and_tested_declaration_owners() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared_pattern_program("error First = Missing(code: Int)\nerror Second = Missing(code: Int)\npure selected(value: First) -> Int { match value { First.Missing {code} => code, _ => 0 } }\npure unrelated(value: Second) -> Int { match value { Second.Missing {code} => code, _ => 0 } }\n");
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let (source_id, original) = generic.pattern_sources().find(|(_, source)| matches!(source.decision, PreparedPatternDecision::ErrorVariant { .. })).unwrap();
            let PreparedPatternDecision::ErrorVariant { identity, .. } = original.decision else { unreachable!() };
            use crate::sema::check::{QualifiedNominalIdentity as Q, NominalDeclaration};
            let Q::Source { source, namespace, declaration: NominalDeclaration::Error(error), member } = identity else { panic!("original error definition owns the member"); };
            assert!(member.is_some());
            assert_eq!(original.input_nominal, Some(Q::Source { source, namespace, declaration: NominalDeclaration::Error(error), member: None }));
            assert_eq!(original.tested_nominal, Some(identity));
            let other = generic.pattern_sources().find_map(|(_, candidate)| candidate.input_nominal.filter(|owner| Some(*owner) != original.input_nominal)).unwrap();
            let mut changed = program.clone();
            changed.store.generic.as_mut().unwrap().test_pattern_source_mut(source_id).unwrap().input_nominal = Some(other);
            assert!(FullVerifier::verify(&changed).is_err(), "a same-shaped registered error family cannot replace the original input owner");
            let mut removed = program.clone();
            removed.store.generic.as_mut().unwrap().test_pattern_source_mut(source_id).unwrap().tested_nominal = None;
            assert!(FullVerifier::verify(&removed).is_err(), "known tested nominal authority cannot disappear while executable members remain");
        });
    }

    #[test]
    fn cold_nominal_patterns_reject_coupled_same_shape_member_replacements() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared_pattern_program("enum Token { First(Int), Second(Int) }\npure selected(value: Token) -> Int { match value { First(number) => number + 1, Second(number) => number + 2 } }\n");
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let (_, application) = generic.pattern_applications().find(|(_, application)| {
                matches!(generic.pattern_source(application.source).unwrap().decision, PreparedPatternDecision::TagConstructor { member, .. } if member.as_str().as_str() == "First")
            }).unwrap();
            let other = generic.pattern_sources().find_map(|(_, source)| {
                match source.decision { PreparedPatternDecision::TagConstructor { identity, family, member } if member.as_str().as_str() == "Second" => Some((identity, family, member)), _ => None }
            }).unwrap();
            let mut changed = program.clone();
            changed.store.generic.as_mut().unwrap().test_pattern_source_mut(application.source).unwrap().decision = PreparedPatternDecision::TagConstructor { identity: other.0, family: other.1, member: other.2 };
            let range = changed.store.pattern_data[application.pattern as usize].range().bounds(changed.store.extra.len()).unwrap();
            changed.store.extra[range.start + 1] = other.2.symbol().raw();
            assert!(FullVerifier::verify(&changed).is_err(), "changing both a valid same-shaped member receipt and its opcode cannot rewrite the original selected variant");
        });
    }

    #[test]
    fn cold_literal_patterns_reject_coupled_saved_and_encoded_scalar_replacements() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared_pattern_program("pure selected(values: List[Int]) -> Int { match values { [7, number] => number + 1, [8, number] => number + 2, _ => 0 } }\n");
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let (source_id, original) = generic.pattern_sources().find(|(_, source)| matches!(source.shape, PreparedPatternShape::Literal(crate::sema::constants::LiteralConstant::Int(7)))).unwrap();
            let (original_row, _) = program.store.patterns.iter().enumerate().find(|(row, _)| generic.registered_pattern_origin(*row as u32).is_some_and(|(identity, _)| identity == original.origin)).unwrap();
            let other = generic.pattern_sources().find(|(_, source)| matches!(source.shape, PreparedPatternShape::Literal(crate::sema::constants::LiteralConstant::Int(8)))).unwrap().1;
            let (other_row, _) = program.store.patterns.iter().enumerate().find(|(row, _)| generic.registered_pattern_origin(*row as u32).is_some_and(|(identity, _)| identity == other.origin)).unwrap();
            let replacement = program.store.payload(program.store.pattern_data[other_row].range()).unwrap().to_vec();
            let mut changed = program.clone();
            changed.store.generic.as_mut().unwrap().test_pattern_source_mut(source_id).unwrap().shape = other.shape.clone();
            let range = changed.store.pattern_data[original_row].range().bounds(changed.store.extra.len()).unwrap();
            assert_eq!(range.len(), replacement.len());
            changed.store.extra[range].copy_from_slice(&replacement);
            assert!(FullVerifier::verify(&changed).is_err(), "changing the encoded literal and mutable expectation together cannot change its original authored scalar");
        });
    }

    #[test]
    fn cold_result_patterns_reject_coupled_carrier_and_payload_slot_changes() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared_pattern_program("pure selected(value: Result[Int, Str]) -> Int { match value { Ok(number) => number + 1, Err(_) => 0 } }\n");
            let _symbols = program.symbol_owner().enter();
            let generic = program.store.generic.as_ref().unwrap();
            let (_, application) = generic.pattern_applications().find(|(_, application)| matches!(generic.pattern_source(application.source).unwrap().decision, PreparedPatternDecision::Result { success: true, .. })).unwrap();
            let mut changed = program.clone();
            let source = changed.store.generic.as_mut().unwrap().test_pattern_source_mut(application.source).unwrap();
            let PreparedPatternDecision::Result { success, .. } = &mut source.decision else { unreachable!() };
            *success = false;
            changed.store.patterns[application.pattern as usize] = FullPatternTag::ResultErr;
            assert!(FullVerifier::verify(&changed).is_err(), "changing both the branch claim and opcode cannot make its Int capture consume the saved Str error payload");

            let mut slot = program.clone();
            let words = slot.store.pattern_data[application.pattern as usize].range().bounds(slot.store.extra.len()).unwrap();
            slot.store.extra[words.start + 1] = 0;
            assert!(FullVerifier::verify(&slot).is_err(), "optimized payload capture slots remain tied to the original child capture identity");
        });
    }

    #[test]
    fn unsigned_pattern_captures_preserve_constraints_and_integer_storage() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure selected(values: List[UInt]) -> Int { match values { [left, right] => left * 10 + right, _ => 0 } }\nprint ${selected([3, 4])}\n";
            assert_pattern_runtime_after_frontend_drop(source, b"34\n");
        });
    }

    #[test]
    fn original_pattern_aliases_execute_after_frontend_disposal_on_both_routes() {
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let source = format!("{}\nprint ${{reordered([3, 7])}}\nprint ${{select([[7, 3, 4], [99]])}}\nprint ${{select([[99], [7, 3, 4]])}}\n", include_str!("../../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh"));
                let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                    "pattern-capture-proofs.xsh", crate::loader::entry_source_from_text("pattern-capture-proofs.xsh", source.clone()), Vec::new());
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let source_id = SourceMap::files(&sources).first().unwrap().id();
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let solved = Arc::downgrade(&checked.solved);
                let counters = checked.solved.graph.counters().clone();
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).expect("original checked capture relationships prepare");
                assert_eq!(checked.solved.graph.counters(), &counters);
                drop(checked); drop(parsed);
                assert!(solved.upgrade().is_none(), "the executable cannot retain the inference graph or pattern source arena");
                let symbols = evaluator.indexed_program.as_ref().unwrap().symbol_owner().clone();
                let execute = || symbols.with_current(|| {
                    assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("prepared program stays installed"))
                });
                let output = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) } else { execute() };
                assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
                assert_eq!(output.stdout, b"37\n11\n11\n", "recursive={recursive}: {:?}; {:?}", output.diagnostics, output.traceback);
                assert!(output.stderr.is_empty(), "{:?}", output.stderr);
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            }
        });
    }
}
