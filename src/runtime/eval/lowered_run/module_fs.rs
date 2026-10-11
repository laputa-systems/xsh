use super::{
    Evaluator, ControlFlow, LoweredValue, NativeArgumentValues, RuntimeError, RuntimeOp, Span,
    fs_module, lowered_bool_arg_or, lowered_bytes_or_str_owned, lowered_fs_root_dir,
    lowered_fs_root_read_result, lowered_int_arg, lowered_int_arg_or, lowered_optional_int_arg,
    lowered_path_arg, lowered_path_list_arg, lowered_record_arg, lowered_result_err_value,
    lowered_result_ok, lowered_root_id, lowered_runtime_result, lowered_str_arg_owned,
    lowered_unit_result, lowered_value_from_runtime_any, pathbuf_from_path_value, record_int_field,
};

impl Evaluator {
    pub(super) fn eval_lowered_fs_executable_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            match values.pop().expect("checked value length") {
                LoweredValue::Path(path) => {
                    match fs_module::executable(self.host_path(&path), span) {
                        Ok(executable) => lowered_result_ok(LoweredValue::Bool(executable)),
                        Err(error) => lowered_result_err_value(error),
                    }
                }
                LoweredValue::Int(mode) => LoweredValue::Bool(fs_module::mode_executable(mode)),
                other => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!(
                            "fs.executable expected Path or Int, found {}",
                            other.type_name()
                        ),
                    )
                    .with_span(span));
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_close_root_values(
        &mut self, op: RuntimeOp, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let root = values.pop().expect("checked value length");
            let closed_ok = op == RuntimeOp::FsCloseRootIfOpen;

            match lowered_root_id(&root, &self.resource_owner, span)
                .ok()
                .and_then(|id| {
                    id.checked_sub(1)
                        .and_then(|index| usize::try_from(index).ok())
                })
                .and_then(|index| self.fs_roots.get_mut(index))
            {
                Some(slot) => {
                    if slot.take().is_some() || closed_ok {
                        lowered_result_ok(LoweredValue::Unit)
                    } else {
                        lowered_result_err_value(
                            RuntimeError::new("fs-root", "root handle is not active")
                                .with_span(span),
                        )
                    }
                }
                None => lowered_result_err_value(
                    RuntimeError::new("fs-root", "root handle is not active").with_span(span),
                ),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_root_read_result_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let max_bytes = lowered_int_arg_or(
                values.get(2).cloned(),
                1_048_576,
                "fs.root_read_result",
                span,
            )?;
            let path = lowered_path_arg(
                values.get(1).cloned().expect("checked value length"),
                "fs.root_read_result",
                span,
            )?;
            let root = values.first().cloned().expect("checked value length");
            let rel = pathbuf_from_path_value(&path);
            match lowered_fs_root_dir(&self.fs_roots, &self.resource_owner, &root, span)
                .and_then(|dir| fs_module::rooted_read_result(dir, &rel, max_bytes, span))
            {
                Ok(result) => lowered_result_ok(lowered_fs_root_read_result(result)),
                Err(error) => lowered_result_err_value(error),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_root_read_text_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let path = lowered_path_arg(
                values.pop().expect("checked value length"),
                "fs.root_read_text",
                span,
            )?;
            let root = values.pop().expect("checked value length");
            let rel = pathbuf_from_path_value(&path);
            match lowered_fs_root_dir(&self.fs_roots, &self.resource_owner, &root, span)
                .and_then(|dir| fs_module::rooted_read(dir, &rel, span))
            {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => lowered_result_ok(LoweredValue::Str(text.into())),
                    Err(error) => lowered_result_err_value(
                        RuntimeError::new(
                            "invalid-utf8",
                            format!(
                                "file is not valid UTF-8 at byte {}",
                                error.utf8_error().valid_up_to()
                            ),
                        )
                        .with_span(span),
                    ),
                },
                Err(error) => lowered_result_err_value(error),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_root_write_values(
        &mut self, op: RuntimeOp, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let operation = if op == RuntimeOp::FsRootWriteAtomic {
                "fs.root_write_atomic"
            } else {
                "fs.root_write"
            };
            let data = lowered_bytes_or_str_owned(
                values.pop().expect("checked value length"),
                operation,
                span,
            )?;
            let path =
                lowered_path_arg(values.pop().expect("checked value length"), operation, span)?;
            let root = values.pop().expect("checked value length");
            let rel = pathbuf_from_path_value(&path);
            let result = lowered_fs_root_dir(&self.fs_roots, &self.resource_owner, &root, span)
                .and_then(|dir| {
                    if op == RuntimeOp::FsRootWriteAtomic {
                        fs_module::rooted_write_atomic(dir, &rel, &data, span)
                    } else {
                        fs_module::rooted_write(dir, &rel, &data, span)
                    }
                });
            lowered_unit_result(result)
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_root_metadata_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let path = lowered_path_arg(
                values.pop().expect("checked value length"),
                "fs.root_metadata",
                span,
            )?;
            let root = values.pop().expect("checked value length");
            let rel = pathbuf_from_path_value(&path);
            match lowered_fs_root_dir(&self.fs_roots, &self.resource_owner, &root, span)
                .and_then(|dir| fs_module::rooted_metadata(dir, &rel, span))
            {
                Ok(record) => match lowered_value_from_runtime_any(&record) {
                    Some(value) => lowered_result_ok(value),
                    None => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!(
                                "fs.root_metadata produced unsupported {}",
                                record.type_name()
                            ),
                        )
                        .with_span(span));
                    }
                },
                Err(error) => lowered_result_err_value(error),
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_root_stat_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let follow_symlinks = lowered_bool_arg_or(
                values.get(2).cloned(),
                false,
                "fs.root_stat",
                span,
            )?;
            let path = lowered_path_arg(
                values.remove(1),
                "fs.root_stat",
                span,
            )?;
            let root = values.remove(0);
            let rel = pathbuf_from_path_value(&path);
            lowered_runtime_result(
                lowered_fs_root_dir(&self.fs_roots, &self.resource_owner, &root, span)
                    .and_then(|dir| {
                        fs_module::rooted_stat(dir, &rel, follow_symlinks, span)
                    }),
                span,
            )?
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_root_symlink_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let parents =
                lowered_bool_arg_or(values.get(3).cloned(), true, "fs.root_symlink", span)?;
            let overwrite =
                lowered_bool_arg_or(values.get(4).cloned(), false, "fs.root_symlink", span)?;
            let path = lowered_path_arg(
                values.get(2).cloned().expect("checked value length"),
                "fs.root_symlink",
                span,
            )?;
            let target = lowered_path_arg(
                values.get(1).cloned().expect("checked value length"),
                "fs.root_symlink",
                span,
            )?;
            let root = values.first().cloned().expect("checked value length");
            let rel = pathbuf_from_path_value(&path);
            let target_rel = pathbuf_from_path_value(&target);
            lowered_unit_result(
                lowered_fs_root_dir(&self.fs_roots, &self.resource_owner, &root, span).and_then(
                    |dir| {
                        fs_module::rooted_symlink(
                            dir,
                            &target_rel,
                            &rel,
                            parents,
                            overwrite,
                            span,
                        )
                    },
                ),
            )
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_root_install_file_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let parents = lowered_bool_arg_or(
                values.get(5).cloned(),
                true,
                "fs.root_install_file",
                span,
            )?;
            let overwrite = lowered_bool_arg_or(
                values.get(6).cloned(),
                false,
                "fs.root_install_file",
                span,
            )?;
            let mode = lowered_int_arg(values.get(4).cloned(), "fs.root_install_file", span)?;
            let dest = lowered_path_arg(
                values.get(3).cloned().expect("checked value length"),
                "fs.root_install_file",
                span,
            )?;
            let dest_root = values.get(2).cloned().expect("checked value length");
            let source = lowered_path_arg(
                values.get(1).cloned().expect("checked value length"),
                "fs.root_install_file",
                span,
            )?;
            let source_root = values.first().cloned().expect("checked value length");
            let dest_rel = pathbuf_from_path_value(&dest);
            let source_rel = pathbuf_from_path_value(&source);
            let result = match (
                lowered_fs_root_dir(&self.fs_roots, &self.resource_owner, &source_root, span),
                lowered_fs_root_dir(&self.fs_roots, &self.resource_owner, &dest_root, span),
            ) {
                (Ok(source_dir), Ok(dest_dir)) => fs_module::rooted_install_file(
                    source_dir,
                    &source_rel,
                    dest_dir,
                    &dest_rel,
                    fs_module::RootedInstallOptions {
                        mode,
                        parents,
                        overwrite,
                        span,
                    },
                ),
                (Err(error), _) | (_, Err(error)) => Err(error),
            };
            lowered_unit_result(result)
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_copy_tree_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let follow_symlinks =
                lowered_bool_arg_or(values.get(4).cloned(), false, "fs.copy_tree", span)?;
            let overwrite =
                lowered_bool_arg_or(values.get(3).cloned(), false, "fs.copy_tree", span)?;
            let parents =
                lowered_bool_arg_or(values.get(2).cloned(), true, "fs.copy_tree", span)?;
            let dest = lowered_path_arg(
                values.get(1).cloned().expect("checked value length"),
                "fs.copy_tree",
                span,
            )?;
            let source = lowered_path_arg(
                values.first().cloned().expect("checked value length"),
                "fs.copy_tree",
                span,
            )?;
            lowered_runtime_result(
                fs_module::copy_tree(
                    self.host_path(&source),
                    self.host_path(&dest),
                    overwrite,
                    parents,
                    follow_symlinks,
                    span,
                ),
                span,
            )?
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_remove_manifest_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let prune_dirs =
                lowered_bool_arg_or(values.get(3).cloned(), true, "fs.remove_manifest", span)?;
            let missing_ok =
                lowered_bool_arg_or(values.get(2).cloned(), false, "fs.remove_manifest", span)?;
            let manifest = lowered_path_list_arg(
                values.get(1).cloned().expect("checked value length"),
                "fs.remove_manifest",
                span,
            )?;
            let root = lowered_path_arg(
                values.first().cloned().expect("checked value length"),
                "fs.remove_manifest",
                span,
            )?;
            lowered_runtime_result(
                fs_module::remove_manifest(
                    self.host_path(&root),
                    manifest,
                    missing_ok,
                    prune_dirs,
                    span,
                ),
                span,
            )?
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_install_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let overwrite =
                lowered_bool_arg_or(values.get(4).cloned(), false, "fs.install", span)?;
            let parents =
                lowered_bool_arg_or(values.get(3).cloned(), true, "fs.install", span)?;
            let mode = lowered_int_arg(values.get(2).cloned(), "fs.install", span)?;
            let dest = lowered_path_arg(
                values.get(1).cloned().expect("checked value length"),
                "fs.install",
                span,
            )?;
            let source = lowered_path_arg(
                values.first().cloned().expect("checked value length"),
                "fs.install",
                span,
            )?;
            lowered_unit_result(fs_module::install_file(
                self.host_path(&source),
                self.host_path(&dest),
                mode,
                parents,
                overwrite,
                None,
                None,
                span,
            ))
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_install_as_values(
        &mut self, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let overwrite =
                lowered_bool_arg_or(values.get(6).cloned(), false, "fs.install_as", span)?;
            let parents =
                lowered_bool_arg_or(values.get(5).cloned(), true, "fs.install_as", span)?;
            let group = lowered_record_arg(values.get(4).cloned(), "fs.install_as", span)?;
            let owner = lowered_record_arg(values.get(3).cloned(), "fs.install_as", span)?;
            let mode = lowered_int_arg(values.get(2).cloned(), "fs.install_as", span)?;
            let dest = lowered_path_arg(
                values.get(1).cloned().expect("checked value length"),
                "fs.install_as",
                span,
            )?;
            let source = lowered_path_arg(
                values.first().cloned().expect("checked value length"),
                "fs.install_as",
                span,
            )?;
            let owner_uid = record_int_field(&owner, "uid", "fs-install", span)?;
            let group_gid = record_int_field(&group, "gid", "fs-install", span)?;
            lowered_unit_result(fs_module::install_file(
                self.host_path(&source),
                self.host_path(&dest),
                mode,
                parents,
                overwrite,
                Some(owner_uid),
                Some(group_gid),
                span,
            ))
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_set_times_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let operation = "fs.set_times";
            let times = fs_module::SetTimes {
                atime: fs_module::TimeRequest {
                    ns: lowered_optional_int_arg(values.get(1), operation, span)?,
                    sec: lowered_optional_int_arg(values.get(6), operation, span)?,
                    nsec: lowered_optional_int_arg(values.get(7), operation, span)?,
                    now: lowered_bool_arg_or(values.get(3).cloned(), false, operation, span)?,
                },
                mtime: fs_module::TimeRequest {
                    ns: lowered_optional_int_arg(values.get(2), operation, span)?,
                    sec: lowered_optional_int_arg(values.get(8), operation, span)?,
                    nsec: lowered_optional_int_arg(values.get(9), operation, span)?,
                    now: lowered_bool_arg_or(values.get(4).cloned(), false, operation, span)?,
                },
                follow_symlinks: lowered_bool_arg_or(
                    values.get(5).cloned(),
                    false,
                    operation,
                    span,
                )?,
            };
            let path = lowered_path_arg(values.remove(0), operation, span)?;
            lowered_unit_result(fs_module::set_times(self.host_path(&path), times, span))
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_copy_file_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let operation = "fs.copy_file";
            let force = lowered_bool_arg_or(values.get(6).cloned(), false, operation, span)?;
            let mode = lowered_optional_int_arg(values.get(5), operation, span)?;
            let overwrite = lowered_bool_arg_or(values.get(4).cloned(), true, operation, span)?;
            let reflink =
                lowered_str_arg_owned(values.get(3).cloned(), "never", operation, span)?;
            let sparse =
                lowered_str_arg_owned(values.get(2).cloned(), "auto", operation, span)?;
            let dest = lowered_path_arg(values.remove(1), operation, span)?;
            let source = lowered_path_arg(values.remove(0), operation, span)?;
            let policies =
                fs_module::Policy::parse(&sparse, "sparse", span).and_then(|sparse| {
                    fs_module::Policy::parse(&reflink, "reflink", span)
                        .map(|reflink| (sparse, reflink))
                });
            match policies {
                Err(error) => lowered_result_err_value(error),
                Ok((sparse, reflink)) => lowered_runtime_result(
                    fs_module::copy_file_with(
                        self.host_path(&source),
                        self.host_path(&dest),
                        fs_module::CopyFile {
                            force,
                            sparse,
                            reflink,
                            overwrite,
                            mode,
                        },
                        span,
                    ),
                    span,
                )?,
            }
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_fs_unlock_values(
        &mut self, op: RuntimeOp, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let Some(LoweredValue::FsLock(lock)) = values.pop() else {
                return Err(RuntimeError::new("type-error", "fs.unlock expected FsLock").with_span(span));
            };
            if !std::sync::Arc::ptr_eq(&lock.owner, &self.resource_owner) {
                return Ok(ControlFlow::Continue(lowered_result_err_value(
                    RuntimeError::new("fs-lock", "lock handle is not active").with_span(span),
                )));
            }
            let Some(slot) = lock.id
                .checked_sub(1)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| self.fs_locks.get_mut(index))
            else {
                return Ok(ControlFlow::Continue(lowered_result_err_value(
                    RuntimeError::new("fs-lock", "lock handle is not active").with_span(span),
                )));
            };
            let Some(file) = slot.take() else {
                if op == RuntimeOp::FsUnlockIfHeld {
                    return Ok(ControlFlow::Continue(lowered_result_ok(LoweredValue::Unit)));
                }
                return Ok(ControlFlow::Continue(lowered_result_err_value(
                    RuntimeError::new("fs-lock", "lock handle is not active").with_span(span),
                )));
            };
            lowered_unit_result(fs_module::unlock_file(&file, span))
        };
        Ok(ControlFlow::Continue(value))
    }
}
