use xsh_registry::managed_resource::ResourceKind;

use super::{
    Arc, ControlFlow, Evaluator, LoweredValue, NativeArgumentValues, RuntimeError, Span, Value,
    lowered_record_arg, lowered_result_err_value, lowered_result_ok, lowered_runtime_value,
    lowered_str_arg_owned, ssh_module,
};
use crate::runtime::value::SshSessionValue;

fn session_arg(
    value: Option<LoweredValue>,
    callee: &str,
    span: Span,
) -> Result<SshSessionValue, RuntimeError> {
    match value {
        Some(LoweredValue::SshSession(session)) => Ok(session),
        _ => Err(RuntimeError::new("type-error", format!("{callee} expected SshSession")).with_span(span)),
    }
}

impl Evaluator {
    pub(super) fn eval_lowered_ssh_connect_values(
        &mut self,
        mut values: NativeArgumentValues,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let options = lowered_record_arg(values.pop(), "ssh.connect", span)?;
        let scope = self.current_scope_id();
        match ssh_module::connect(
            &options,
            Arc::clone(&self.resource_owner),
            scope,
            span,
        ) {
            Ok(value) => {
                let lowered = lowered_runtime_value(Value::ok(value), span)?;
                self.register_owned_resource(scope, ResourceKind::Capability);
                Ok(ControlFlow::Continue(lowered))
            }
            Err(error) => Ok(ControlFlow::Continue(lowered_result_err_value(error))),
        }
    }

    pub(super) fn eval_lowered_ssh_connect_all_values(
        &mut self,
        mut values: NativeArgumentValues,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let batch = lowered_record_arg(values.pop(), "ssh.connect_all", span)?;
        let scope = self.current_scope_id();
        match ssh_module::connect_all(&batch, &self.resource_owner, scope, span) {
            Ok(results) => {
                for result in &results {
                    if matches!(result, Value::Result(crate::runtime::value::ResultValue::Ok(_))) {
                        self.register_owned_resource(scope, ResourceKind::Capability);
                    }
                }
                Ok(ControlFlow::Continue(lowered_runtime_value(Value::ok(Value::List(results)), span)?))
            }
            Err(error) => Ok(ControlFlow::Continue(lowered_result_err_value(error))),
        }
    }

    pub(super) fn eval_lowered_ssh_exec_values(
        &mut self,
        mut values: NativeArgumentValues,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let options = lowered_record_arg(values.pop(), "ssh.exec", span)?;
        let command = lowered_str_arg_owned(values.pop(), "", "ssh.exec", span)?;
        let session = session_arg(values.pop(), "ssh.exec", span)?;
        match ssh_module::exec(
            session,
            &command,
            &options,
            &self.resource_owner,
            span,
        ) {
            Ok(value) => Ok(ControlFlow::Continue(lowered_runtime_value(
                Value::ok(value),
                span,
            )?)),
            Err(error) => Ok(ControlFlow::Continue(lowered_result_err_value(error))),
        }
    }

    pub(super) fn eval_lowered_ssh_info_values(
        &mut self,
        mut values: NativeArgumentValues,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let session = session_arg(values.pop(), "ssh.info", span)?;
        match ssh_module::info(session, span) {
            Ok(value) => Ok(ControlFlow::Continue(lowered_runtime_value(
                Value::ok(value),
                span,
            )?)),
            Err(error) => Ok(ControlFlow::Continue(lowered_result_err_value(error))),
        }
    }

    pub(super) fn eval_lowered_ssh_close_values(
        &mut self,
        op: xsh_registry::RuntimeOp,
        mut values: NativeArgumentValues,
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let session = session_arg(values.pop(), "ssh.close", span)?;
        let if_open = op == xsh_registry::RuntimeOp::SshCloseIfOpen;
        match ssh_module::close(session, &self.resource_owner, if_open, span) {
            Ok(Some(scope)) => {
                self.forget_owned_resource(scope, ResourceKind::Capability);
                Ok(ControlFlow::Continue(lowered_result_ok(LoweredValue::Unit)))
            }
            Ok(None) => Ok(ControlFlow::Continue(lowered_result_ok(LoweredValue::Unit))),
            Err(error) => Ok(ControlFlow::Continue(lowered_result_err_value(error))),
        }
    }
}
