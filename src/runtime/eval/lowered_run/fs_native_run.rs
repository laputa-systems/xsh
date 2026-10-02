use super::*;

impl Evaluator {
    pub(super) fn eval_lowered_fs_walk_native(&mut self, operation: RuntimeOp, mut values: NativeArgumentValues, span: Span) -> Result<LoweredValue, RuntimeError> {
        let (name, emit, hidden_slot) = match operation {
            RuntimeOp::FsFiles => ("fs.files", fs_module::WalkEmit::Files, 4),
            RuntimeOp::FsWalk => ("fs.walk", fs_module::WalkEmit::All, 3),
            _ => return Err(RuntimeError::new("indexed-verification", "filesystem walk packet has another selected operation").with_span(span)),
        };
        let hidden = lowered_bool_arg_or(values.get(hidden_slot).cloned(), false, name, span)?;
        let stat = lowered_bool_arg_or(values.get(2).cloned(), true, name, span)?;
        let gitignore = lowered_bool_arg_or(values.get(1).cloned(), true, name, span)?;
        let exts = if operation == RuntimeOp::FsFiles {
            match values.get(3).cloned() {
                Some(value) => lowered_str_list_arg(Some(value), "fs.files exts", span)?,
                None => Vec::new(),
            }
        } else { Vec::new() };
        let path = lowered_path_arg(values.remove(0), name, span)?;
        self.lowered_stream_list_result(fs_module::walk_filesystem(self.host_path(&path), gitignore, stat, hidden, emit, exts, span), span)
    }
}
