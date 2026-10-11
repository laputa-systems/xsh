use super::{
    Evaluator, ControlFlow, Instant, LoweredValue, NativeArgumentValues, ProcessStatus,
    RuntimeError, Span, child_cpu_ns, cpu_ns_delta, lowered_bool_arg_or, lowered_command_arg,
    lowered_measured_command_record, lowered_result_err_value, lowered_result_ok,
    run_error_to_runtime, run_inherit_with_policy, run_quiet_with_policy,
};

impl Evaluator {
    pub(super) fn eval_lowered_time_measure_values(
        &mut self, mut values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let value = {
            let quiet =
                lowered_bool_arg_or(values.get(1).cloned(), false, "time.measure", span)?;
            let plan = lowered_command_arg(values.remove(0), "time.measure", span)?;
            let invocation = self.invocation_from_command_plan(&plan, span)?;
            let cpu_before = child_cpu_ns();
            let started = Instant::now();
            let outcome = if quiet {
                run_quiet_with_policy(&invocation, self)
            } else {
                run_inherit_with_policy(&invocation, self)
            };
            match outcome {
                Ok(end) => {
                    let wall_ns = started.elapsed().as_nanos().min(i64::MAX as u128) as i64;
                    let (user_ns, system_ns) = cpu_ns_delta(cpu_before);
                    let status = end.status.expect("measured command has status");
                    lowered_result_ok(lowered_measured_command_record(
                        status, wall_ns, user_ns, system_ns,
                    ))
                }
                Err(error) => {
                    if self.signal_state.shutdown_complete
                        && self.signal_state.shutdown_status.is_some()
                    {
                        let wall_ns = started.elapsed().as_nanos().min(i64::MAX as u128) as i64;
                        let (user_ns, system_ns) = cpu_ns_delta(cpu_before);
                        let status = error
                            .status
                            .as_deref()
                            .cloned()
                            .unwrap_or_else(|| ProcessStatus::signaled(libc::SIGTERM));
                        lowered_result_ok(lowered_measured_command_record(
                            status, wall_ns, user_ns, system_ns,
                        ))
                    } else {
                        lowered_result_err_value(run_error_to_runtime(error, span))
                    }
                }
            }
        };
        Ok(ControlFlow::Continue(value))
    }
}
