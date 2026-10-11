use super::{
    Evaluator, ControlFlow, LoweredValue, NativeArgumentValues, RuntimeError, Span, archive_module,
    lowered_bool_arg_or, lowered_int_arg_or, lowered_path_arg, lowered_path_list_arg,
    lowered_str_arg_owned, lowered_unit_result,
};

impl Evaluator {
    pub(super) fn eval_lowered_archive_compress_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let overwrite =
                lowered_bool_arg_or(values.get(4).cloned(), false, "archive.compress", span)?;
            let level =
                lowered_int_arg_or(values.get(3).cloned(), 6, "archive.compress", span)?;
            let format = lowered_str_arg_owned(
                values.get(2).cloned(),
                "auto",
                "archive.compress",
                span,
            )?;
            let dest = lowered_path_arg(values.remove(1), "archive.compress", span)?;
            let source = lowered_path_arg(values.remove(0), "archive.compress", span)?;
            lowered_unit_result(archive_module::compress_file(
                self.host_path(&source),
                self.host_path(&dest),
                &format,
                level,
                overwrite,
                span,
            ))
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_archive_tar_create_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let overwrite =
                lowered_bool_arg_or(values.get(4).cloned(), false, "archive.tar_create", span)?;
            let compression = lowered_str_arg_owned(
                values.get(3).cloned(),
                "auto",
                "archive.tar_create",
                span,
            )?;
            let entries = lowered_path_list_arg(values.remove(2), "archive.tar_create", span)?;
            let root = lowered_path_arg(values.remove(1), "archive.tar_create", span)?;
            let path = lowered_path_arg(values.remove(0), "archive.tar_create", span)?;
            lowered_unit_result(archive_module::tar_create(
                self.host_path(&path),
                self.host_path(&root),
                entries,
                &compression,
                overwrite,
                span,
            ))
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_archive_tar_extract_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let members = if values.get(5).is_some() {
                lowered_path_list_arg(values.remove(5), "archive.tar_extract", span)?
            } else {
                Vec::new()
            };
            let overwrite = lowered_bool_arg_or(
                values.get(4).cloned(),
                false,
                "archive.tar_extract",
                span,
            )?;
            let compression = lowered_str_arg_owned(
                values.get(3).cloned(),
                "auto",
                "archive.tar_extract",
                span,
            )?;
            let strip_components =
                lowered_int_arg_or(values.get(2).cloned(), 0, "archive.tar_extract", span)?;
            let dest = lowered_path_arg(values.remove(1), "archive.tar_extract", span)?;
            let path = lowered_path_arg(values.remove(0), "archive.tar_extract", span)?;
            lowered_unit_result(archive_module::tar_extract(
                self.host_path(&path),
                self.host_path(&dest),
                strip_components,
                &compression,
                overwrite,
                members,
                span,
            ))
        };
        Ok(ControlFlow::Continue(value))
    }
}
