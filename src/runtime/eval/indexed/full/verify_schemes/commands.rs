use super::{IrVerifyError, SchemeInstantiationBudget, SchemeType, Type, producers};
use super::super::decode::{ExprInstructionId, RunArgKindView, RunArgView, RunOperandsView, RunOptionView, RunTailOperandView};

pub(super) fn word<'a>(
    argument: RunArgView<'a>, role: producers::ProcessWordRole,
    child: &mut impl FnMut(ExprInstructionId<'a>, &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<bool, IrVerifyError> {
    budget.charge(1)?;
    let (value, spliced, optional_splice) = match argument.kind {
        RunArgKindView::Single(value) => (value, false, false),
        RunArgKindView::SingleOrSplice(value) => (value, false, true),
        RunArgKindView::Splice(value) => (value, true, false),
    };
    let actual = child(value, budget)?.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic command word has no closed conversion contract"))?;
    super::ensure_substitution_type(&actual)?;
    producers::process_word_type(&actual, spliced, optional_splice, role)
}

pub(super) fn operands<'a>(
    operands: RunOperandsView<'a>,
    child: &mut impl FnMut(ExprInstructionId<'a>, &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    budget.charge(1 + operands.options.len() + operands.env.len() + operands.tail.len())?;
    for option in operands.options {
        let (name, value) = match option? {
            RunOptionView::Timeout(value) => ("timeout", value),
            RunOptionView::CpuMax(value) => ("cpu_max", value),
            RunOptionView::Accept(value) => ("accept", value),
        };
        let actual = child(value, budget)?.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic process option has no closed contract"))?;
        producers::process_option_contract(name, Some(&actual), budget)?;
    }
    for assignment in operands.env { word(assignment?.value, producers::ProcessWordRole::Environment, child, budget)?; }
    word(operands.target, producers::ProcessWordRole::Argument, child, budget)?;
    let mut stdin_sources = 0usize;
    let mut bytes_input = false;
    for operand in operands.tail {
        match operand? {
            RunTailOperandView::Argument(argument) => { word(argument, producers::ProcessWordRole::Argument, child, budget)?; }
            RunTailOperandView::Redirection(redirection) => {
                stdin_sources += usize::from(matches!(redirection.kind, crate::syntax::node::RedirectionKind::StdinRead | crate::syntax::node::RedirectionKind::StdinDup));
                bytes_input |= word(redirection.target, producers::ProcessWordRole::Redirection(redirection.kind), child, budget)?;
            }
        }
    }
    if bytes_input && stdin_sources > 1 { return Err(IrVerifyError::new("process Bytes input competes with another stdin source")); }
    Ok(())
}

pub(super) fn native_call(
    decoder: &super::FullDecoder<'_>,
    op: crate::modules::RuntimeOp, arguments: &[Option<SchemeType>],
    budget: &mut SchemeInstantiationBudget,
) -> Result<Type, IrVerifyError> {
    if crate::modules::signature::native_static_operation_requires_producer_proof(op)
        && decoder.producer_demand != crate::runtime::eval::ProducerDemand::Needed
    { return Err(IrVerifyError::new("generic native operation requires its selected producer proof")); }
    let mut result = None;
    for signature in crate::modules::signature::native_static_operation_signatures(op) {
        budget.charge(1 + signature.params.len())?;
        if arguments.len() > signature.params.len() || signature.return_ty.contains_builtin_parameter()
            || signature.params.iter().any(|parameter| parameter.ty.contains_builtin_parameter()) { continue; }
        let mut fits = true;
        for (slot, parameter) in signature.params.iter().enumerate() {
            fits &= match arguments.get(slot).and_then(Option::as_ref) {
                Some(actual) => {
                    budget.charge_scheme_clone(actual)?;
                    actual.closed_ground_type().is_some_and(|actual| actual.matches_expected(&parameter.ty))
                },
                None => parameter.defaulted,
            };
        }
        if !fits { continue; }
        if result.as_ref().is_some_and(|previous| previous != &signature.return_ty) { return Err(IrVerifyError::new("generic native operation has ambiguous closed contracts")); }
        budget.charge_clone(&signature.return_ty)?;
        result = Some(signature.return_ty.clone());
    }
    result.ok_or_else(|| IrVerifyError::new("generic native operation disagrees with its canonical contract"))
}

pub(super) fn nullary_method(
    receiver: &SchemeType, name: &str, arguments: &[Option<SchemeType>], budget: &mut SchemeInstantiationBudget,
) -> Result<Type, IrVerifyError> {
    if !arguments.is_empty() { return Err(IrVerifyError::new("generic intrinsic method has unexpected arguments")); }
    budget.charge_scheme_clone(receiver)?;
    let receiver = receiver.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic intrinsic method has no closed receiver contract"))?;
    let signatures = super::FullDecoder::registered_method_signatures(&receiver, name)?.ok_or_else(|| IrVerifyError::new("generic intrinsic method has no canonical receiver contract"))?;
    let mut result = None;
    for signature in signatures {
        budget.charge(1 + signature.params.len())?;
        if signature.params.iter().any(|parameter| !parameter.defaulted)
            || signature.return_ty.contains_builtin_parameter()
        { continue; }
        super::ensure_closed_type(&signature.return_ty)?;
        if result.as_ref().is_some_and(|previous| previous != &signature.return_ty) { return Err(IrVerifyError::new("generic intrinsic method has ambiguous closed contracts")); }
        result = Some(signature.return_ty);
    }
    result.ok_or_else(|| IrVerifyError::new("generic intrinsic method disagrees with its canonical arity"))
}
