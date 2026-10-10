//! Running a prepared command invocation, with the namespace changes
//! `linux.run_in_namespaces` asks the child to make before it executes.

use super::{
    Evaluator, LoweredValue, NativeArgumentValues, ProcessEnd, ProcessInvocation, ProcessStatus,
    RuntimeError, Span, Value, lowered_bool_arg_or, lowered_command_arg, lowered_int_arg,
    lowered_path_arg, lowered_path_list_arg, lowered_result_ok, lowered_str_arg_owned,
    lowered_str_list_arg, run_capture_with_stderr_policy, run_inherit_with_policy,
};
use crate::runtime::namespace::{NamespaceEntry, NamespaceKind, Propagation};

impl Evaluator {
    /// Runs an invocation to completion with the evaluator's capture and
    /// cancellation policy and yields the `Result[Status, ProcessError]` a
    /// script sees. A failed namespace step is worded as the util-linux tools
    /// word it.
    pub(super) fn run_process_invocation(
        &mut self,
        invocation: ProcessInvocation,
        span: Span,
    ) -> LoweredValue {
        self.trace_process_run_start(span, &invocation);
        let outcome = if self.capture_process_output {
            run_capture_with_stderr_policy(&invocation, self).map(|output| {
                self.stdout.extend_from_slice(&output.stdout);
                self.stderr.extend_from_slice(&output.stderr);
                output.end
            })
        } else {
            run_inherit_with_policy(&invocation, self)
        };
        match outcome {
            Ok(end) => {
                let status = end.status.clone().expect("completed process has status");
                // A step that failed in the child is reported by the spawn as
                // an exec failure; the namespace caller wants the step named.
                if let Some(failure) = invocation
                    .namespaces
                    .as_ref()
                    .and_then(|entry| entry.status_failure(&status))
                {
                    self.last_status = Some(status);
                    return LoweredValue::ResultErr(Box::new(Value::RunError(Box::new(
                        failure.with_span(span),
                    ))));
                }
                self.last_status = Some(status.clone());
                self.trace_process_run_end(span, &end);
                lowered_result_ok(LoweredValue::Status(Box::new(status)))
            }
            Err(error) => {
                let error = match &invocation.namespaces {
                    Some(entry) => entry.describe_failure(error),
                    None => error,
                };
                let error = error.with_span(span);
                let end = ProcessEnd {
                    pid: None,
                    status: error.status.as_deref().cloned(),
                    error: Some(error.clone()),
                };
                if let Some(status) = &end.status {
                    self.last_status = Some(status.clone());
                }
                self.trace_process_run_end(span, &end);
                if self.signal_state.shutdown_complete && self.signal_state.shutdown_status.is_some()
                {
                    let status = end
                        .status
                        .clone()
                        .unwrap_or_else(|| ProcessStatus::signaled(libc::SIGTERM));
                    lowered_result_ok(LoweredValue::Status(Box::new(status)))
                } else {
                    LoweredValue::ResultErr(Box::new(Value::RunError(Box::new(error))))
                }
            }
        }
    }

    /// `linux.run_in_namespaces(command, unshare, join, map_root_user,
    /// propagation, mount_proc, fork, root, cwd, uid, gid, drop_groups)`.
    pub(super) fn eval_run_in_namespaces(
        &mut self,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        match self.namespace_invocation(values, span) {
            Ok(invocation) => Ok(self.run_process_invocation(invocation, span)),
            // A request the call refuses is a failed call a script can match,
            // not a fault in the script itself.
            Err(error) if error.kind == "invalid-argument" => {
                Ok(LoweredValue::ResultErr(Box::new(Value::Error(Box::new(error)))))
            }
            Err(error) => Err(error),
        }
    }

    fn namespace_invocation(
        &mut self,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<ProcessInvocation, RuntimeError> {
        const OP: &str = "linux.run_in_namespaces";
        let plan = lowered_command_arg(
            values
                .get(0)
                .cloned()
                .ok_or_else(|| RuntimeError::new("arity", format!("{OP} expected a command")).with_span(span))?,
            OP,
            span,
        )?;
        let invalid = |message: String| RuntimeError::new("invalid-argument", message).with_span(span);

        let mut unshare = Vec::new();
        if let Some(value) = values.get(1) {
            for name in lowered_str_list_arg(Some(value.clone()), OP, span)? {
                let kind = NamespaceKind::parse(&name).ok_or_else(|| {
                    invalid(format!(
                        "unshare names a namespace by its /proc/PID/ns name ({}), found `{name}`",
                        NamespaceKind::NAMES.join(", ")
                    ))
                })?;
                if !unshare.contains(&kind) {
                    unshare.push(kind);
                }
            }
        }
        let join = match values.get(2) {
            Some(value) => lowered_path_list_arg(value.clone(), OP, span)?
                .iter()
                .map(|path| self.host_path(path))
                .collect(),
            None => Vec::new(),
        };
        let map_root_user = lowered_bool_arg_or(values.get(3).cloned(), false, OP, span)?;
        let propagation = lowered_str_arg_owned(values.get(4).cloned(), "unchanged", OP, span)?;
        let propagation = Propagation::parse(&propagation).ok_or_else(|| {
            invalid(format!(
                "propagation must be private, shared, slave or unchanged, found `{propagation}`"
            ))
        })?;
        let optional_path = |index: usize| -> Result<Option<std::path::PathBuf>, RuntimeError> {
            match values.get(index) {
                None | Some(LoweredValue::Null) => Ok(None),
                Some(value) => Ok(Some(
                    self.host_path(&lowered_path_arg(value.clone(), OP, span)?),
                )),
            }
        };
        let mount_proc = optional_path(5)?;
        let fork = lowered_bool_arg_or(values.get(6).cloned(), false, OP, span)?;
        let root = optional_path(7)?;
        let cwd = optional_path(8)?;
        let optional_id = |index: usize, name: &str| -> Result<Option<u32>, RuntimeError> {
            match values.get(index) {
                None | Some(LoweredValue::Null) => Ok(None),
                Some(value) => {
                    let id = lowered_int_arg(Some(value.clone()), OP, span)?;
                    u32::try_from(id)
                        .map(Some)
                        .map_err(|_| invalid(format!("{name} must be a nonnegative 32-bit ID")))
                }
            }
        };
        let uid = optional_id(9, "uid")?;
        let gid = optional_id(10, "gid")?;
        let drop_groups = lowered_bool_arg_or(values.get(11).cloned(), false, OP, span)?;

        // These requests would otherwise act on the caller's own namespaces
        // or do nothing, which a script that asked for them did not intend.
        let mount_unshared = unshare.contains(&NamespaceKind::Mount);
        if mount_proc.is_some() && !mount_unshared {
            return Err(invalid(
                "mount_proc mounts in the child's mount namespace, so mnt must be in unshare"
                    .to_string(),
            ));
        }
        if propagation != Propagation::Unchanged && !mount_unshared {
            return Err(invalid(
                "propagation changes the mount tree of a new mount namespace, so mnt must be in unshare"
                    .to_string(),
            ));
        }
        if map_root_user && !unshare.contains(&NamespaceKind::User) {
            return Err(invalid(
                "map_root_user maps a new user namespace, so user must be in unshare".to_string(),
            ));
        }

        let mut invocation = self.invocation_from_command_plan(&plan, span)?;
        invocation.namespaces = Some(NamespaceEntry {
            unshare,
            join,
            map_root_user,
            propagation,
            mount_proc,
            fork,
            root,
            cwd,
            drop_groups,
            uid,
            gid,
        });
        Ok(invocation)
    }
}
