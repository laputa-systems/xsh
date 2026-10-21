use super::{BTreeMap, BTreeSet, FullDecoder, IrVerifyError, Name, SchemeInstantiationBudget, SchemeRowTail, SchemeType, Type, schema};
use super::super::schemes::FullSchemeBodyFactKind;

pub(super) type ConstructorFacts = BTreeMap<usize, FullSchemeBodyFactKind>;

enum ProducerShape {
    Typed(SchemeType),
    Empty,
    Record { fields: BTreeMap<Name, ProducerShape>, tail: SchemeRowTail },
}

impl ProducerShape {
    fn has_empty(&self) -> bool {
        !matches!(self, Self::Typed(_))
    }

    fn materialize(self) -> Result<SchemeType, IrVerifyError> {
        match self {
            Self::Typed(ty) => Ok(ty),
            Self::Empty | Self::Record { .. } => Err(IrVerifyError::new("nonmaterial constructor escapes its discarded or overwritten use")),
        }
    }
}

struct ShapeProof<'a, 'b, F> {
    decoder: &'a FullDecoder<'b>,
    constructors: &'a ConstructorFacts,
    visited: &'a mut BTreeSet<usize>,
    typed: &'a mut F,
}

impl<F: FnMut(usize, &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError>> ShapeProof<'_, '_, F> {
    fn child(&mut self, instruction: usize, depth: usize, budget: &mut SchemeInstantiationBudget) -> Result<ProducerShape, IrVerifyError> {
        budget.charge(1)?;
        if depth > 128 { return Err(IrVerifyError::new("constructor producer nesting is too deep")); }
        let Some(&kind) = self.constructors.get(&instruction) else { return (self.typed)(instruction, budget).map(ProducerShape::Typed) };
        if !self.visited.insert(instruction) { return Err(IrVerifyError::new("constructor producer is shared or cyclic")); }
        match (kind, self.decoder.expression_view(instruction)?) {
            (FullSchemeBodyFactKind::EmptyList, schema::ExprView::List(items)) if items.len() == 0 => Ok(ProducerShape::Empty),
            (FullSchemeBodyFactKind::EmptyMap, schema::ExprView::EmptyMap) => Ok(ProducerShape::Empty),
            (FullSchemeBodyFactKind::RecordProducer, schema::ExprView::Record(_)) => {
                let shape = self.record(instruction, depth + 1, budget)?;
                if !shape.has_empty() { return Err(IrVerifyError::new("record producer has no nonmaterial payload")); }
                if !matches!(&shape, ProducerShape::Record { tail: SchemeRowTail::Closed, .. }) { return Err(IrVerifyError::new("nonmaterial record producer has an open row")); }
                Ok(shape)
            }
            _ => Err(IrVerifyError::new("constructor role disagrees with its physical producer")),
        }
    }

    fn record(&mut self, instruction: usize, depth: usize, budget: &mut SchemeInstantiationBudget) -> Result<ProducerShape, IrVerifyError> {
        let schema::ExprView::Record(mut entries) = self.decoder.expression_view(instruction)? else { return Err(IrVerifyError::new("record producer has another physical shape")) };
        let mut fields = BTreeMap::new();
        let mut has_spread = false;
        let mut tail = SchemeRowTail::Closed;
        for entry in &mut entries {
            budget.charge(1)?;
            match entry? {
                super::super::decode::RecordEntryView::Field(name, value) => {
                    if fields.contains_key(&name) && !has_spread { return Err(IrVerifyError::new("generic record producer repeats a field")); }
                    fields.insert(name, self.child(value.raw() as usize, depth + 1, budget)?);
                }
                super::super::decode::RecordEntryView::Spread(value) => {
                    has_spread = true;
                    let shape = self.child(value.raw() as usize, depth + 1, budget)?;
                    let (spread, spread_tail) = match shape {
                        ProducerShape::Record { fields, tail } => (fields, tail),
                        ProducerShape::Typed(mut ty) => {
                            while let SchemeType::Validated(_, inner) = ty { budget.charge(1)?; ty = *inner; }
                            match ty {
                                SchemeType::Record { fields, tail } => (fields.into_iter().map(|(name, ty)| (name, ProducerShape::Typed(ty))).collect(), tail),
                                SchemeType::Ground(ty) => {
                                    let Type::Record(fields) = ty.unvalidated() else { return Err(IrVerifyError::new("generic record spread has no record producer")) };
                                    let mut spread = BTreeMap::new();
                                    for (name, ty) in fields.iter() { budget.charge_clone(ty)?; spread.insert(*name, ProducerShape::Typed(SchemeType::Ground(ty.clone()))); }
                                    (spread, SchemeRowTail::Closed)
                                }
                                _ => return Err(IrVerifyError::new("generic record spread has no record producer")),
                            }
                        }
                        ProducerShape::Empty => return Err(IrVerifyError::new("empty collection is not a record spread")),
                    };
                    if spread_tail != SchemeRowTail::Closed {
                        if tail != SchemeRowTail::Closed && tail != spread_tail { return Err(IrVerifyError::new("generic record spreads disagree on their row domain")); }
                        tail = spread_tail;
                    }
                    fields.extend(spread);
                }
            }
        }
        budget.charge(fields.len())?;
        if fields.values().any(ProducerShape::has_empty) { Ok(ProducerShape::Record { fields, tail }) }
        else { Ok(ProducerShape::Typed(SchemeType::Record {
            fields: fields.into_iter().map(|(name, shape)| shape.materialize().map(|ty| (name, ty))).collect::<Result<_, _>>()?, tail,
        })) }
    }
}

pub(super) fn record_type(
    decoder: &FullDecoder<'_>, instruction: usize, constructors: &ConstructorFacts,
    visited: &mut BTreeSet<usize>, budget: &mut SchemeInstantiationBudget,
    mut typed: impl FnMut(usize, &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError>,
) -> Result<SchemeType, IrVerifyError> {
    ShapeProof { decoder, constructors, visited, typed: &mut typed }.record(instruction, 0, budget)?.materialize()
}

pub(super) fn discard(
    decoder: &FullDecoder<'_>, instruction: usize, constructors: &ConstructorFacts,
    visited: &mut BTreeSet<usize>, budget: &mut SchemeInstantiationBudget,
    mut typed: impl FnMut(usize, &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError>,
) -> Result<(), IrVerifyError> {
    ShapeProof { decoder, constructors, visited, typed: &mut typed }.child(instruction, 0, budget)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::{FullBuilder, FullProgram, FullVerifier, function_view_decoder};
    use super::super::super::schemes::FullSchemeBodyFactValue;
    use crate::sema::check::Checker;
    use crate::source::SourceMap;
    use crate::syntax::parser::Parser;
    use std::sync::Arc;

    fn prepare() -> FullProgram {
        let source = "pure keep(payload) { let state = {...{items: []}, ...{items: [7]}, payload}; state.payload }\npure other() -> Int { 7 }\n";
        let mut sources = SourceMap::default();
        let source_id = sources.add_file("constructor-proof.xsh", source.to_string());
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let program = FullBuilder::build_compact(&parsed.arena, &checked, source, Arc::new(sources), source_id).unwrap();
        FullVerifier::verify(&program).unwrap();
        program
    }

    fn entries(program: &FullProgram, instruction: usize) -> std::ops::Range<usize> {
        let owner = program.store.functions.iter().enumerate().find(|(owner, _)| program.store.function_instruction_range(*owner).unwrap().contains(&instruction)).unwrap().0;
        let decoder = function_view_decoder(&program.store, owner);
        let schema::ExprView::Record(fields) = decoder.expression_view(instruction).unwrap() else { panic!("the selected instruction is a record") };
        program.store.blocks[fields.block_identity_for_test().index()].instructions.bounds(program.store.extra.len()).unwrap()
    }

    #[test]
    fn nonmaterial_constructor_proof_requires_the_actual_later_overwrite() {
        let program = prepare();
        let partial = program.store.scheme_body_facts.iter().find(|fact| fact.kind() == FullSchemeBodyFactKind::RecordProducer).unwrap();
        let decoder = function_view_decoder(&program.store, partial.function.index());
        let outer = program.store.function_instruction_range(partial.function.index()).unwrap().find(|&instruction| {
            match decoder.expression_view(instruction) {
                Ok(schema::ExprView::Record(mut fields)) => matches!(fields.next(), Some(Ok(super::super::super::decode::RecordEntryView::Spread(value))) if value.raw() == partial.instruction),
                _ => false,
            }
        }).unwrap();
        let words = entries(&program, outer);
        assert_eq!(program.store.extra[words.start], 3);
        assert_eq!(program.store.extra[words.start + 1], 1);
        assert_eq!(program.store.extra[words.start + 3], 1);
        let mut reversed = program.clone();
        reversed.store.extra.swap(words.start + 2, words.start + 4);
        let error = FullVerifier::verify(&reversed).unwrap_err();
        assert_eq!(error.message, "nonmaterial constructor escapes its discarded or overwritten use");
        let mut removed = program.clone();
        removed.store.extra[words.start + 3] = 0;
        assert!(FullVerifier::verify(&removed).is_err());
    }

    #[test]
    fn nonmaterial_constructor_proof_rejects_changed_shapes_and_role_links() {
        let program = prepare();
        let empty = program.store.scheme_body_facts.iter().position(|fact| fact.kind() == FullSchemeBodyFactKind::EmptyList).unwrap();
        let partial = program.store.scheme_body_facts.iter().find(|fact| fact.kind() == FullSchemeBodyFactKind::RecordProducer).unwrap();
        let words = entries(&program, partial.instruction as usize);
        let mut changed_name = program.clone();
        changed_name.store.extra[words.start + 2] = program.symbol_owner().with_current(|| Name::intern("payload").symbol().raw());
        FullVerifier::verify(&changed_name).unwrap();
        let mut stranded = program.clone();
        stranded.store.extra[words.start + 2] = program.symbol_owner().with_current(|| Name::intern("stranded").symbol().raw());
        assert!(FullVerifier::verify(&stranded).is_err());
        let mut changed_role = program.clone();
        changed_role.store.scheme_body_facts[empty].value = FullSchemeBodyFactValue::EmptyMap;
        assert!(FullVerifier::verify(&changed_role).is_err());
        let mut changed_link = program.clone();
        changed_link.store.scheme_body_facts[empty].instruction = partial.instruction;
        assert!(FullVerifier::verify(&changed_link).is_err());
        let mut changed_owner = program.clone();
        let other = program.store.functions.iter().enumerate().find(|(owner, _)| *owner != partial.function.index()).unwrap().0;
        changed_owner.store.scheme_body_facts[empty].function = super::super::super::IrFunctionId::new(other).unwrap();
        assert!(FullVerifier::verify(&changed_owner).is_err());
        let decoder = function_view_decoder(&program.store, partial.function.index());
        let nonempty = program.store.function_instruction_range(partial.function.index()).unwrap().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::List(items)) if items.len() != 0)).unwrap();
        let mut changed_length = program.clone();
        changed_length.store.data[program.store.scheme_body_facts[empty].instruction as usize] = program.store.data[nonempty];
        assert!(FullVerifier::verify(&changed_length).is_err());
    }
}
