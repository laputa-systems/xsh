use super::{
    Evaluator, LoweredValue, NativeArgumentValues, RuntimeError, Span, lowered_bool_arg_or,
    lowered_command_arg, lowered_path_list_arg,
};
use crate::runtime::confinement::{Confinement, TcpAccess};

impl Evaluator {
    pub(super) fn eval_run_confined(
        &mut self,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        const OP: &str = "linux.run_confined";
        let plan = lowered_command_arg(
            values
                .get(0)
                .cloned()
                .ok_or_else(|| RuntimeError::new("arity", format!("{OP} expected a command")).with_span(span))?,
            OP,
            span,
        )?;
        let paths = |index: usize| -> Result<Vec<std::path::PathBuf>, RuntimeError> {
            match values.get(index) {
                Some(value) => Ok(lowered_path_list_arg(value.clone(), OP, span)?
                    .iter()
                    .map(|path| self.host_path(path))
                    .collect()),
                None => Ok(Vec::new()),
            }
        };
        let read = paths(1)?;
        let write = paths(2)?;
        let tcp = if lowered_bool_arg_or(values.get(3).cloned(), false, OP, span)? {
            TcpAccess::Allowed
        } else {
            TcpAccess::Denied
        };
        let mut invocation = self.invocation_from_command_plan(&plan, span)?;
        invocation.confinement = Some(Confinement { read, write, tcp });
        Ok(self.run_process_invocation(invocation, span))
    }
}
