use super::{ParamSig, Type};
pub(in crate::signature) fn command_callable(
    params: &[ParamSig],
    return_ty: &Type,
    pure: bool,
) -> bool {
    !pure
        && params
            .iter()
            .all(|param| !param.defaulted || param.ty == Type::Bool)
        && matches!(return_ty, Type::Result(ok, _) if matches!(ok.as_ref(), Type::Unit))
}
