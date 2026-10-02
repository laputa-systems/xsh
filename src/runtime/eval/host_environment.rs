use super::indexed::generic::HostBindingCaptureId;
use super::lower::host_bindings::HostBinding;
use crate::runtime::value::Value;
use std::sync::Arc;

/// Entry arguments remain separate from the lexical scope that exposes them.
/// Function hydration therefore cannot select a caller's shadowing binding.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct HostBindingEnvironment {
    args: Arc<[Arc<str>]>,
}

impl HostBindingEnvironment {
    pub(in crate::runtime::eval) fn new(argv: Vec<String>) -> Self {
        Self { args: argv.into_iter().map(Arc::<str>::from).collect::<Vec<_>>().into() }
    }

    pub(in crate::runtime::eval) fn for_binding(&self, binding: HostBinding) -> Value {
        match binding {
            HostBinding::Args => Value::List(self.args.iter().cloned().map(Value::Str).collect()),
        }
    }
}

/// The capture keeps its creation environment when transported to another
/// evaluator. Its value is derived from that environment and cannot be paired
/// independently with a list supplied through the ordinary value boundary.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct CapturedHostBinding {
    environment: HostBindingEnvironment,
    capture: HostBindingCaptureId,
}

impl CapturedHostBinding {
    pub(in crate::runtime::eval) fn new(environment: &HostBindingEnvironment, capture: HostBindingCaptureId) -> Self {
        Self { environment: environment.clone(), capture }
    }

    pub(in crate::runtime::eval) fn capture_id(&self) -> HostBindingCaptureId { self.capture }

    pub(in crate::runtime::eval) fn value(&self) -> Value {
        self.environment.for_binding(HostBinding::Args)
    }
}
