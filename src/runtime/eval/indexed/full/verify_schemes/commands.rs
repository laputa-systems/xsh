use super::{IrVerifyError, SchemeInstantiationBudget, SchemeType, Type, producers};
use super::super::decode::{ExprInstructionId, RunArgKindView, RunArgView, RunOperandsView, RunOptionView, RunTailOperandView};

pub(super) fn word<'a>(
    argument: RunArgView<'a>, role: producers::ProcessWordRole, scope: &super::SchemeScope,
    child: &mut impl FnMut(ExprInstructionId<'a>, &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<bool, IrVerifyError> {
    budget.charge(1)?;
    let (value, spliced, optional_splice) = match argument.kind {
        RunArgKindView::Single(value) => (value, false, false),
        RunArgKindView::SingleOrSplice(value) => (value, false, true),
        RunArgKindView::Splice(value) => (value, true, false),
    };
    let actual = child(value, budget)?;
    if matches!(role, producers::ProcessWordRole::Argument) {
        let conversion = if spliced { super::ValueConversion::ArgvList } else if optional_splice { super::ValueConversion::ArgvOrList } else { super::ValueConversion::Argv };
        if !super::scheme_conversion(&actual, conversion, scope, budget)? { return Err(IrVerifyError::new("generic command word disagrees with its conversion requirement")); }
        return Ok(false);
    }
    let actual = actual.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic command word has no closed conversion contract"))?;
    super::ensure_substitution_type(&actual)?;
    producers::process_word_type(&actual, spliced, optional_splice, role)
}

pub(super) fn operands<'a>(
    operands: RunOperandsView<'a>, scope: &super::SchemeScope,
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
    for assignment in operands.env { word(assignment?.value, producers::ProcessWordRole::Environment, scope, child, budget)?; }
    word(operands.target, producers::ProcessWordRole::Argument, scope, child, budget)?;
    let mut stdin_sources = 0usize;
    let mut bytes_input = false;
    for operand in operands.tail {
        match operand? {
            RunTailOperandView::Argument(argument) => { word(argument, producers::ProcessWordRole::Argument, scope, child, budget)?; }
            RunTailOperandView::Redirection(redirection) => {
                stdin_sources += usize::from(matches!(redirection.kind, crate::syntax::node::RedirectionKind::StdinRead | crate::syntax::node::RedirectionKind::StdinDup));
                bytes_input |= word(redirection.target, producers::ProcessWordRole::Redirection(redirection.kind), scope, child, budget)?;
            }
        }
    }
    if bytes_input && stdin_sources > 1 { return Err(IrVerifyError::new("process Bytes input competes with another stdin source")); }
    Ok(())
}

pub(super) fn process_command_argv(
    arguments: &[Option<SchemeType>], scope: &super::SchemeScope,
    budget: &mut SchemeInstantiationBudget,
) -> Result<Type, IrVerifyError> {
    if arguments.len() != 16 { return Err(IrVerifyError::new("generic command has an invalid argument layout")); }
    let target = arguments[0].as_ref().ok_or_else(|| IrVerifyError::new("generic command has no target"))?;
    if !super::scheme_conversion(target, super::ValueConversion::ProcessWord, scope, budget)? {
        return Err(IrVerifyError::new("generic command target disagrees with its strict word requirement"));
    }
    let mut argv = arguments[1].as_ref().ok_or_else(|| IrVerifyError::new("generic command has no argv"))?;
    while let SchemeType::Validated(_, inner) = argv { budget.charge(1)?; argv = inner; }
    let valid_argv = match argv {
        SchemeType::List(item) => super::scheme_conversion(item, super::ValueConversion::ProcessWord, scope, budget)?,
        SchemeType::Ground(ty) => matches!(ty.unvalidated(), Type::List(item) if super::ValueConversion::ProcessWord.accepts(item)),
        _ => false,
    };
    if !valid_argv { return Err(IrVerifyError::new("generic command argv has no strict word list contract")); }
    let mut selected = false;
    for signature in crate::modules::signature::native_process_command_option_signatures() {
        budget.charge(1 + signature.params.len())?;
        if signature.params.len() != arguments.len() { continue; }
        let mut fits = true;
        for (offset, (parameter, actual)) in signature.params[2..].iter().zip(&arguments[2..]).enumerate() {
            fits &= match actual {
                Some(actual) => {
                    budget.charge_scheme_clone(actual)?;
                    if offset == 2 && actual.closed_ground_type().is_none() {
                        super::scheme_conversion(actual, super::ValueConversion::ProcessInput, scope, budget)?
                    } else {
                        super::body_type_matches_ground(actual, &parameter.ty)
                    }
                }
                None => parameter.defaulted,
            };
        }
        selected |= fits;
    }
    if !selected { return Err(IrVerifyError::new("generic command options disagree with their native contract")); }
    Ok(Type::Command)
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

pub(super) fn environment_overlay(ty: &SchemeType, scope: &super::SchemeScope, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    budget.charge(1)?;
    match ty {
        SchemeType::Record { fields, tail: crate::sema::schemes::SchemeRowTail::Closed } => {
            for value in fields.values() {
                if !super::scheme_conversion(value, super::ValueConversion::Env, scope, budget)? {
                    return Err(IrVerifyError::new("generic environment overlay value has no Env conversion requirement"));
                }
            }
        }
        SchemeType::Map(key, value) if key.closed_ground_type() == Some(Type::Str) => {
            if !super::scheme_conversion(value, super::ValueConversion::Env, scope, budget)? {
                return Err(IrVerifyError::new("generic environment overlay value has no Env conversion requirement"));
            }
        }
        actual => {
            budget.charge_scheme_clone(actual)?;
            let actual = actual.closed_ground_type().ok_or_else(|| IrVerifyError::new("generic environment overlay has no complete record or Map contract"))?;
            if !actual.accepts_context_scope_input(crate::syntax::arena::ContextScopeKind::Env) {
                return Err(IrVerifyError::new("generic context scope input disagrees with its kind"));
            }
        }
    }
    Ok(())
}
