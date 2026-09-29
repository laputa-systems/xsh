use crate::sema::types::{CallableParamType, Type};
use crate::symbol::Name;
use xsh_registry::stream_parameters::{
    StageParameterDefault, StageParameterType, stage_parameters,
};

pub(crate) fn stage_argument_params(stage: &str) -> Vec<CallableParamType> {
    stage_parameters(stage)
        .iter()
        .map(|parameter| CallableParamType {
            name: Name::intern(parameter.name),
            ty: match parameter.ty {
                StageParameterType::Int => Type::Int,
                StageParameterType::Bool => Type::Bool,
                StageParameterType::Value | StageParameterType::Sequence => Type::Unknown,
                StageParameterType::Columns => Type::List(Box::new(Type::Str)),
            },
            defaulted: parameter.default != StageParameterDefault::Required,
            rest: false,
        })
        .collect()
}
