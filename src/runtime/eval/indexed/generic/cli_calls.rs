use super::*;
use crate::modules::RuntimeOp;
use std::collections::BTreeMap;

/// The normalized descriptor is the original checked constant authority.
/// Sharing its allocation authenticates its defaults and validation policy;
/// equal output records do not make independently normalized plans equivalent.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedCliDescriptor {
    pub plan: Arc<crate::modules::cli::CliDescriptorPlan>,
    pub rows: Arc<[CliDescriptorRow]>,
    pub canonical_result: crate::sema::types::Type,
    pub result_layout: Arc<crate::runtime::eval::require::PreparedSchema>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct CliDescriptorRow {
    pub instruction: u32,
    pub tag: u16,
    pub payload: Box<[u32]>,
    pub block: Option<(u32, Box<[u32]>)>,
    pub text: Option<Arc<str>>,
    pub bytes: Option<Arc<[u8]>>,
    pub constant: Option<crate::runtime::eval::LoweredValue>,
}

impl PartialEq for PreparedCliDescriptor {
    fn eq(&self, other: &Self) -> bool { Arc::ptr_eq(&self.plan, &other.plan) && Arc::ptr_eq(&self.rows, &other.rows) && self.canonical_result == other.canonical_result && Arc::ptr_eq(&self.result_layout, &other.result_layout) }
}

impl Eq for PreparedCliDescriptor {}

impl GroundNativeCallContract {
    pub(in crate::runtime::eval) fn verify_cli_descriptor(&self, pools: &SemanticPools) -> Result<bool, IrVerifyError> {
        let Some(descriptor) = &self.cli_descriptor else { return Ok(false); };
        let PreparedOperationAuthority::Registry { operation, semantic_rule, binding: crate::modules::signature::ImplBinding::Native, .. } = self.authority else {
            return Err(failure("CLI descriptor lacks its original registry authority"));
        };
        if self.receiver.is_some() || !matches!(self.registry_owner, crate::sema::registry_graph::RegistryOwner::Module(_))
            || !matches!((semantic_rule, operation),
                (crate::modules::signature::SemanticRule::CliDescriptor, RuntimeOp::CliParse | RuntimeOp::CliParseFull | RuntimeOp::CliApplet)
                | (crate::modules::signature::SemanticRule::CliCommands, RuntimeOp::CliCommands))
            || !descriptor.plan.matches_operation(operation) {
            return Err(failure("CLI descriptor changes its original operation policy"));
        }
        let TypeRef::Ground(result) = self.result else { return Err(failure("CLI descriptor result is not closed")); };
        if pools.to_type(pools.signature_return_type(self.signature)?)? != descriptor.canonical_result
            || pools.to_type(result)? != descriptor.plan.return_type(operation == RuntimeOp::CliParseFull) {
            return Err(failure("CLI descriptor changes its original parsed result carrier"));
        }
        let expected = descriptor.plan.return_type(operation == RuntimeOp::CliParseFull);
        let crate::sema::types::Type::Result(success, _) = expected else { return Err(failure("CLI descriptor lacks a parsed result carrier")); };
        if !descriptor.result_layout.matches_type(&success) {
            return Err(failure("CLI descriptor parsed record layout changes its original carrier"));
        }
        Ok(true)
    }
}

impl PreparedCliDescriptor {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.result_layout.retained_bytes() + self.canonical_result.retained_bytes().saturating_sub(size_of::<crate::sema::types::Type>()) + self.rows.len() * size_of::<CliDescriptorRow>() + self.rows.iter().map(|row| {
            row.payload.len() * size_of::<u32>() + row.block.as_ref().map_or(0, |(_, words)| words.len() * size_of::<u32>())
                + row.text.as_ref().map_or(0, |text| text.len()) + row.bytes.as_ref().map_or(0, |bytes| bytes.len())
        }).sum::<usize>()
    }
}

impl PreparedCliDescriptor {
    pub(in crate::runtime::eval) fn materialize_result(&self, evaluator: &crate::runtime::eval::Evaluator, value: crate::runtime::eval::LoweredValue, span: crate::source::Span) -> Result<crate::runtime::eval::LoweredValue, crate::runtime::value::RuntimeError> {
        use crate::runtime::eval::LoweredValue;
        match value {
            LoweredValue::ResultOk(payload) => crate::runtime::eval::require::materialize_record_layout(evaluator, &self.result_layout, *payload, span)
                .map(|value| LoweredValue::ResultOk(Box::new(value))),
            value @ LoweredValue::ResultErr(_) => Ok(value),
            _ => Err(crate::runtime::value::RuntimeError::new("indexed-ir", "CLI descriptor operation returned a value outside its prepared Result carrier").with_span(span)),
        }
    }
}

impl GroundNativeCallContract {
    pub(in crate::runtime::eval) fn verify_dynamic_cli_carrier(&self, pools: &SemanticPools) -> Result<bool, IrVerifyError> {
        let PreparedOperationAuthority::Registry { operation, semantic_rule, binding: crate::modules::signature::ImplBinding::Native, .. } = self.authority else { return Ok(false); };
        if !matches!((semantic_rule, operation),
            (crate::modules::signature::SemanticRule::CliDescriptor, RuntimeOp::CliParse | RuntimeOp::CliParseFull | RuntimeOp::CliApplet)
            | (crate::modules::signature::SemanticRule::CliCommands, RuntimeOp::CliCommands)) || self.cli_descriptor.is_some() { return Ok(false); }
        if self.registry_owner != crate::sema::registry_graph::RegistryOwner::Module("cli") || self.receiver.is_some() {
            return Err(failure("dynamic CLI carrier loses its original module declaration"));
        }
        let TypeRef::Ground(result) = self.result else { return Err(failure("dynamic CLI carrier requires its original closed result")); };
        if result != pools.signature_return_type(self.signature)? || pools.to_type(result)? != Self::dynamic_cli_result_type(operation) {
            return Err(failure("dynamic CLI carrier changes its canonical result or promises unvalidated descriptor fields"));
        }
        Ok(true)
    }
}

impl GroundNativeCallContract {
    pub(in crate::runtime::eval::indexed) fn dynamic_cli_result_type(operation: RuntimeOp) -> crate::sema::types::Type {
    use crate::sema::types::Type;
    let success = if operation == RuntimeOp::CliParseFull {
        Type::Record(BTreeMap::from(xsh_registry::types::cli_full_fields(
            Type::ErasedRecord, Type::ErasedRecord, Type::List(Box::new(Type::Str)),
        ).map(|(name, ty)| (Name::intern(name), ty))))
    } else { Type::ErasedRecord };
    Type::Result(Box::new(success), Box::new(Type::Error))
}
}
