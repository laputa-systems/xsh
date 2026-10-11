use super::{
    Evaluator, ControlFlow, LoweredValue, NativeArgumentValues, PathValue, RuntimeError, RuntimeOp,
    Span, Value, bytes_module, hash_module, lowered_bool_arg_or, lowered_int_arg,
    lowered_int_arg_or, lowered_path_arg, lowered_runtime_result,
};

impl Evaluator {
    pub(super) fn eval_lowered_hash_md5_values(
        &mut self, op: RuntimeOp, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let algorithm = match op {
                RuntimeOp::HashMd5 => hash_module::HashAlgorithm::Md5,
                RuntimeOp::HashSha1 => hash_module::HashAlgorithm::Sha1,
                RuntimeOp::HashSha256 => hash_module::HashAlgorithm::Sha256,
                RuntimeOp::HashSha512 => hash_module::HashAlgorithm::Sha512,
                _ => unreachable!("checked hash digest op"),
            };
            match values.pop().expect("checked value length") {
                LoweredValue::Bytes(bytes) => {
                    LoweredValue::Digest(Box::new(hash_module::digest_bytes(algorithm, &bytes)))
                }
                LoweredValue::BytesView(bytes) => LoweredValue::Digest(Box::new(
                    hash_module::digest_bytes(algorithm, bytes.as_slice()),
                )),
                LoweredValue::Path(path) => lowered_runtime_result(
                    hash_module::digest_file(algorithm, &self.host_path(&path), span)
                        .map(Value::digest),
                    span,
                )?,
                LoweredValue::Str(text) => {
                    let path =
                        PathValue::from_text(text).map_err(|error| error.with_span(span))?;
                    lowered_runtime_result(
                        hash_module::digest_file(algorithm, &self.host_path(&path), span)
                            .map(Value::digest),
                        span,
                    )?
                }
                LoweredValue::StrView(text) => {
                    let path = PathValue::from_text(text.as_str())
                        .map_err(|error| error.with_span(span))?;
                    lowered_runtime_result(
                        hash_module::digest_file(algorithm, &self.host_path(&path), span)
                            .map(Value::digest),
                        span,
                    )?
                }
                other => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!(
                            "hash digest expected Bytes or Path, found {}",
                            other.type_name()
                        ),
                    )
                    .with_span(span));
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_bytes_copy_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let overwrite =
                lowered_bool_arg_or(values.get(6).cloned(), false, "bytes.copy", span)?;
            let seek = lowered_int_arg_or(values.get(5).cloned(), 0, "bytes.copy", span)?;
            let skip = lowered_int_arg_or(values.get(4).cloned(), 0, "bytes.copy", span)?;
            let count = match values.get(3).cloned() {
                Some(value) => Some(lowered_int_arg(Some(value), "bytes.copy", span)?),
                None => None,
            };
            let block_size =
                lowered_int_arg_or(values.get(2).cloned(), 512, "bytes.copy", span)?;
            let dest = lowered_path_arg(values.remove(1), "bytes.copy", span)?;
            let source = lowered_path_arg(values.remove(0), "bytes.copy", span)?;
            lowered_runtime_result(
                bytes_module::copy_blocks(
                    self.host_path(&source),
                    self.host_path(&dest),
                    block_size,
                    count,
                    skip,
                    seek,
                    overwrite,
                    span,
                ),
                span,
            )?
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_bytes_copy_file_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let truncate =
                lowered_bool_arg_or(values.get(6).cloned(), false, "bytes.copy_file", span)?;
            let create =
                lowered_bool_arg_or(values.get(5).cloned(), true, "bytes.copy_file", span)?;
            let length = match values.get(4).cloned() {
                Some(value) => Some(lowered_int_arg(Some(value), "bytes.copy_file", span)?),
                None => None,
            };
            let dest_offset =
                lowered_int_arg_or(values.get(3).cloned(), 0, "bytes.copy_file", span)?;
            let source_offset =
                lowered_int_arg_or(values.get(2).cloned(), 0, "bytes.copy_file", span)?;
            let dest = lowered_path_arg(values.remove(1), "bytes.copy_file", span)?;
            let source = lowered_path_arg(values.remove(0), "bytes.copy_file", span)?;
            lowered_runtime_result(
                bytes_module::copy_file(
                    self.host_path(&source),
                    self.host_path(&dest),
                    source_offset,
                    dest_offset,
                    length,
                    create,
                    truncate,
                    span,
                ),
                span,
            )?
        };
        Ok(ControlFlow::Continue(value))
    }
}
