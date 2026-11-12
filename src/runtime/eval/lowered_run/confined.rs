use super::{
    Evaluator, LoweredValue, NativeArgumentValues, RuntimeError, Span, lowered_bool_arg_or,
    lowered_command_arg, lowered_fs_root_dir, lowered_process_run_error,
};
use crate::runtime::confinement::{Confinement, FilesystemAuthority, TcpAccess};
use crate::runtime::value::RunError;
use std::sync::Arc;

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
        let paths = |index: usize| -> Result<Vec<FilesystemAuthority>, RuntimeError> {
            let items = match values.get(index) {
                Some(LoweredValue::List(items)) => items.clone(),
                Some(LoweredValue::SharedList(items)) => items.iter().cloned().collect(),
                None => return Ok(Vec::new()),
                _ => return Err(RuntimeError::new("type-error", format!("{OP} expected List[Path | FsRoot]")).with_span(span)),
            };
            items.into_iter().map(|item| match item {
                LoweredValue::Path(path) => Ok(FilesystemAuthority::Path(self.host_path(&path))),
                root @ LoweredValue::FsRoot(_) => {
                    let root = lowered_fs_root_dir(&self.fs_capabilities, &self.resource_owner, &root, span)?;
                    rustix::io::fcntl_dupfd_cloexec(root, 3)
                        .map(|descriptor| FilesystemAuthority::Root(Arc::new(descriptor)))
                        .map_err(|error| RuntimeError::host("confinement", &error).with_span(span))
                }
                _ => Err(RuntimeError::new("type-error", format!("{OP} expected Path or FsRoot")).with_span(span)),
            }).collect()
        };
        let (read, write) = match paths(1).and_then(|read| paths(2).map(|write| (read, write))) {
            Ok(authorities) => authorities,
            Err(error) => return Ok(lowered_process_run_error(
                RunError::new("confinement", error.message).with_span(span),
            )),
        };
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
