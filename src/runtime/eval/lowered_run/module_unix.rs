use super::{
    Evaluator, Arc, BTreeMap, CommandPlan, Duration, Instant, LoweredValue, NativeArgumentValues,
    ProcessStatus, RecordMap, RuntimeError, RuntimeOp, Span, Value, display_spawn_argv,
    lowered_bool_arg_or, lowered_command_arg, lowered_duration_arg, lowered_int_arg,
    lowered_int_arg_or, lowered_int_list_arg, lowered_path_arg, lowered_record_arg,
    lowered_str_arg_owned, lowered_str_list_arg, lowered_value_from_runtime_any, module_error,
    pid1_event_record, pid1_shutdown_record, process_module, spawned_child_record,
    unix_fake_tty_attrs, unix_module, unix_require_arg,
};
use std::os::unix::fs::OpenOptionsExt;

impl Evaluator {
    pub(super) fn eval_lowered_unix_call(
        &mut self,
        op: RuntimeOp,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let out = self.eval_unix_call_value(op, values, span)?;
        lowered_value_from_runtime_any(&out).ok_or_else(|| {
            RuntimeError::new(
                "type-error",
                format!("cannot lower unix result {}", out.type_name()),
            )
            .with_span(span)
        })
    }

    fn eval_unix_call_value(
        &mut self,
        op: RuntimeOp,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match op {
            RuntimeOp::UnixKillAll => {
                let name =
                    lowered_str_arg_owned(values.first().cloned(), "", "unix.kill_all", span)?;
                let signal =
                    lowered_str_arg_owned(values.get(1).cloned(), "TERM", "unix.kill_all", span)?;
                unix_module::kill_all(&name, &signal, span)
            }
            RuntimeOp::UnixTty if self.unix_fake_active() => {
                let tty = self.unix_fake_value("tty", "/dev/tty");
                self.unix_fake_log("tty", &[("tty", tty.clone())], span)?;
                Ok(Value::ok(Value::Str(tty.into())))
            }
            RuntimeOp::UnixId if self.unix_fake_active() => {
                self.unix_fake_log("id", &[], span)?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("uid"), Value::Int(0)),
                    (Arc::from("euid"), Value::Int(0)),
                    (Arc::from("gid"), Value::Int(0)),
                    (Arc::from("egid"), Value::Int(0)),
                    (
                        Arc::from("groups"),
                        Value::List(vec![Value::Record(RecordMap::from([
                            (Arc::from("gid"), Value::Int(0)),
                            (Arc::from("name"), Value::Str("root".into())),
                        ]))]),
                    ),
                    (Arc::from("supplementary"), Value::List(vec![Value::Int(0)])),
                ]))))
            }
            RuntimeOp::UnixTtyAttrs if self.unix_fake_active() => {
                let fd = lowered_int_arg_or(values.first().cloned(), 0, "unix.tty_attrs", span)?;
                self.unix_fake_log("tty_attrs", &[("fd", fd.to_string())], span)?;
                Ok(Value::ok(unix_fake_tty_attrs()))
            }
            RuntimeOp::UnixSetTtyAttrs if self.unix_fake_active() => {
                let _attrs =
                    lowered_record_arg(values.first().cloned(), "unix.set_tty_attrs", span)?;
                let fd = lowered_int_arg_or(values.get(1).cloned(), 0, "unix.set_tty_attrs", span)?;
                self.unix_fake_log("set_tty_attrs", &[("fd", fd.to_string())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixSetHostname if self.unix_fake_active() => {
                let hostname =
                    lowered_str_arg_owned(values.first().cloned(), "", "unix.set_hostname", span)?;
                self.unix_fake_log("set_hostname", &[("hostname", hostname.clone())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixReapChildEvents
            | RuntimeOp::UnixPid1Setup
            | RuntimeOp::UnixWaitPid1Event
            | RuntimeOp::UnixShutdownProcessGroups
            | RuntimeOp::UnixSpawnProcessGroup
            | RuntimeOp::UnixSpawnProcessGroupLog
            | RuntimeOp::UnixSpawnLoggedProcessGroup
            | RuntimeOp::UnixSpawnWithTty
            | RuntimeOp::UnixNotifyReady
            | RuntimeOp::UnixNotifyClose
            | RuntimeOp::UnixKillProcessGroup
            | RuntimeOp::UnixExec
                if self.unix_fake_active() =>
            {
                self.eval_unix_fake_call(op, values, span)
            }
            RuntimeOp::UnixReapChildEvents => unix_module::reap_child_events(span),
            RuntimeOp::UnixPid1Setup => {
                let signals =
                    lowered_str_list_arg(values.first().cloned(), "unix.pid1_setup", span)?;
                let subreaper =
                    lowered_bool_arg_or(values.get(1).cloned(), true, "unix.pid1_setup", span)?;
                let allow_non_pid1 =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "unix.pid1_setup", span)?;
                unix_module::pid1_setup_native(&signals, subreaper, allow_non_pid1, span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixWaitPid1Event => {
                let deadline = match values.first().cloned() {
                    Some(value) => {
                        let timeout =
                            lowered_duration_arg(Some(value), "unix.wait_pid1_event", span)?;
                        Some(Instant::now() + Duration::from_millis(timeout.millis))
                    }
                    None => None,
                };
                let event = unix_module::wait_pid1_event_native(deadline, span)?;
                Ok(Value::ok(pid1_event_record(event)))
            }
            RuntimeOp::UnixShutdownProcessGroups => {
                let groups = lowered_int_list_arg(
                    values.first().cloned(),
                    "unix.shutdown_process_groups",
                    span,
                )?;
                let term_timeout = lowered_duration_arg(
                    values.get(1).cloned(),
                    "unix.shutdown_process_groups",
                    span,
                )?;
                let kill_timeout = match values.get(2).cloned() {
                    Some(value) => {
                        lowered_duration_arg(Some(value), "unix.shutdown_process_groups", span)?
                            .millis
                    }
                    None => 0,
                };
                let shutdown = unix_module::shutdown_process_groups_native(
                    &groups,
                    Duration::from_millis(term_timeout.millis),
                    Duration::from_millis(kill_timeout),
                    span,
                )?;
                Ok(Value::ok(pid1_shutdown_record(shutdown)))
            }
            RuntimeOp::UnixSpawnProcessGroup => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.spawn_process_group", span)?,
                    "unix.spawn_process_group",
                    span,
                )?;
                let notify = lowered_bool_arg_or(
                    values.get(1).cloned(),
                    false,
                    "unix.spawn_process_group",
                    span,
                )?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                unix_module::spawn_process_group(&invocation, notify, span)
            }
            RuntimeOp::UnixNotifyReady => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.notify_ready", span)?;
                match unix_module::notify_ready_native(fd, span) {
                    Ok(ready) => Ok(Value::ok(Value::Bool(ready))),
                    Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
                }
            }
            RuntimeOp::UnixNotifyClose => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.notify_close", span)?;
                match unix_module::notify_close_native(fd, span) {
                    Ok(()) => Ok(Value::ok(Value::Unit)),
                    Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
                }
            }
            RuntimeOp::UnixSpawnProcessGroupLog => {
                let plan = lowered_command_arg(
                    unix_require_arg(
                        values.first().cloned(),
                        "unix.spawn_process_group_log",
                        span,
                    )?,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                let log = lowered_path_arg(
                    unix_require_arg(values.get(1).cloned(), "unix.spawn_process_group_log", span)?,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                let host_log = self.host_path(&log);
                if let Some(parent) = host_log.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        RuntimeError::host("unix-spawn-log", &error).with_span(span)
                    })?;
                }
                let stdout = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .mode(0o600)
                    .open(&host_log)
                    .map_err(|error| {
                        RuntimeError::host("unix-spawn-log", &error).with_span(span)
                    })?;
                let stderr = stdout.try_clone().map_err(|error| {
                    RuntimeError::host("unix-spawn-log", &error).with_span(span)
                })?;
                let notify = lowered_bool_arg_or(
                    values.get(2).cloned(),
                    false,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                match unix_module::spawn_process_group_with_stdio_native(
                    &invocation,
                    stdout,
                    stderr,
                    notify,
                    span,
                ) {
                    Ok(child) => Ok(spawned_child_record(child)),
                    Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
                }
            }
            RuntimeOp::UnixSpawnLoggedProcessGroup => {
                let plan = lowered_command_arg(
                    unix_require_arg(
                        values.first().cloned(),
                        "unix.spawn_logged_process_group",
                        span,
                    )?,
                    "unix.spawn_logged_process_group",
                    span,
                )?;
                let logger_plan = lowered_command_arg(
                    unix_require_arg(
                        values.get(1).cloned(),
                        "unix.spawn_logged_process_group",
                        span,
                    )?,
                    "unix.spawn_logged_process_group",
                    span,
                )?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                let logger_invocation = self.invocation_from_command_plan(&logger_plan, span)?;
                unix_module::spawn_logged_process_group(&invocation, &logger_invocation, span)
            }
            RuntimeOp::UnixSpawnWithTty => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.spawn_with_tty", span)?,
                    "unix.spawn_with_tty",
                    span,
                )?;
                let tty =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "unix.spawn_with_tty", span)?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                unix_module::spawn_with_tty(&invocation, &tty, span)
            }
            RuntimeOp::UnixKillProcessGroup => {
                let pid =
                    lowered_int_arg(values.first().cloned(), "unix.kill_process_group", span)?;
                let signal = lowered_str_arg_owned(
                    values.get(1).cloned(),
                    "",
                    "unix.kill_process_group",
                    span,
                )?;
                let signal = match process_module::signal_info(&signal, span) {
                    Ok(signal) => signal,
                    Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
                };
                unix_module::kill_process_group(pid, signal.number, span)
            }
            RuntimeOp::UnixExecEnv => {
                let plan = lowered_command_arg(unix_require_arg(values.first().cloned(), "unix.exec_env", span)?, "unix.exec_env", span)?;
                let mut invocation = self.invocation_from_command_plan(&plan, span)?;
                let Some(LoweredValue::Map(environment)) = values.get(1) else {
                    return Err(RuntimeError::new("type-error", "unix.exec_env expected Map[Str, Str] or Map[Str, Bytes]").with_span(span));
                };
                let mut replacement = BTreeMap::new();
                for (key, value) in environment.iter() {
                    let Some(key) = key.as_str() else { return Err(RuntimeError::new("type-error", "unix.exec_env expected Str keys").with_span(span)); };
                    let value: Vec<u8> = match value {
                        LoweredValue::Str(value) => value.as_bytes().to_vec(),
                        LoweredValue::Bytes(value) => value.to_vec(),
                        LoweredValue::BytesView(value) => value.as_slice().to_vec(),
                        _ => return Err(RuntimeError::new("type-error", "unix.exec_env expected Str or Bytes values").with_span(span)),
                    };
                    if key.is_empty() || key.as_bytes().contains(&0) || key.contains('=') || value.contains(&0) {
                        return Ok(Value::err(Value::Error(Box::new(RuntimeError::new("unix-exec-env", "invalid environment key or value").with_span(span)))));
                    }
                    replacement.insert(key.as_bytes().to_vec(), value);
                }
                let argv0 = match values.get(2).cloned() {
                    None | Some(LoweredValue::Null) => None,
                    Some(value) => Some(lowered_str_arg_owned(Some(value), "", "unix.exec_env", span)?),
                };
                if argv0.as_ref().is_some_and(|value| value.as_bytes().contains(&0)) {
                    return Ok(Value::err(Value::Error(Box::new(RuntimeError::new("unix-exec-env", "argv0 contains NUL").with_span(span)))));
                }
                let mut block_signals = Vec::new();
                if let Some(names) = values.get(3).cloned() {
                    for name in lowered_str_list_arg(Some(names), "unix.exec_env", span)? {
                        match process_module::signal_info(&name, span) {
                            Ok(signal) if signal.number > 0 => block_signals.push(signal.number),
                            Ok(_) => return Ok(Value::err(Value::Error(Box::new(RuntimeError::new("unix-exec-env", format!("cannot block signal {name}")).with_span(span))))),
                            Err(error) => return Ok(Value::err(Value::Error(Box::new(error)))),
                        }
                    }
                }
                invocation.env = replacement;
                invocation.env_overlay.clear();
                if self.unix_fake_active() {
                    self.unix_fake_log("exec_env", &[("command", String::from_utf8_lossy(&plan.target).into_owned()), ("argv", display_spawn_argv(&plan.target, &plan.argv)), ("argv0", argv0.unwrap_or_default())], span)?;
                    return Ok(Value::ok(Value::Unit));
                }
                self.flush_shared_stdio();
                unix_module::exec_env(&invocation, argv0.as_deref(), &block_signals, span)
            }
            RuntimeOp::UnixExec => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.exec", span)?,
                    "unix.exec",
                    span,
                )?;
                let invocation = self.invocation_from_command_plan(&plan, span)?;
                // Buffered output would otherwise die with the replaced image.
                self.flush_shared_stdio();
                unix_module::exec(&invocation, span)
            }
            RuntimeOp::UnixSetHostname => {
                let hostname =
                    lowered_str_arg_owned(values.first().cloned(), "", "unix.set_hostname", span)?;
                unix_module::set_hostname(&hostname, span)
            }
            RuntimeOp::UnixUptimeSeconds => unix_module::uptime_seconds(span),
            RuntimeOp::UnixTty => unix_module::tty(span),
            RuntimeOp::UnixId => unix_module::id(span),
            RuntimeOp::UnixTtyAttrs => {
                let fd = lowered_int_arg_or(values.first().cloned(), 0, "unix.tty_attrs", span)?;
                unix_module::tty_attrs(fd, span)
            }
            RuntimeOp::UnixSetTtyAttrs => {
                let attrs =
                    lowered_record_arg(values.first().cloned(), "unix.set_tty_attrs", span)?;
                let fd = lowered_int_arg_or(values.get(1).cloned(), 0, "unix.set_tty_attrs", span)?;
                let when = lowered_str_arg_owned(
                    values.get(2).cloned(),
                    "now",
                    "unix.set_tty_attrs",
                    span,
                )?;
                unix_module::set_tty_attrs(&attrs, fd, &when, span)
            }
            _ => unreachable!("unix operation expected"),
        }
    }

    fn eval_unix_fake_call(
        &mut self,
        op: RuntimeOp,
        values: NativeArgumentValues,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match op {
            RuntimeOp::UnixReapChildEvents => {
                self.unix_fake_log("reap_child_events", &[], span)?;
                Ok(Value::ok(Value::List(self.unix_fake_child_events())))
            }
            RuntimeOp::UnixPid1Setup => {
                let signals =
                    lowered_str_list_arg(values.first().cloned(), "unix.pid1_setup", span)?;
                let subreaper =
                    lowered_bool_arg_or(values.get(1).cloned(), true, "unix.pid1_setup", span)?;
                let allow_non_pid1 =
                    lowered_bool_arg_or(values.get(2).cloned(), false, "unix.pid1_setup", span)?;
                self.unix_fake_log(
                    "pid1_setup",
                    &[
                        ("signals", signals.join(",")),
                        ("subreaper", subreaper.to_string()),
                        ("allow_non_pid1", allow_non_pid1.to_string()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixWaitPid1Event => {
                let kind = self.unix_fake_value("event_kind", "signal");
                let signal = self.unix_fake_value("signal", "TERM");
                let pid = self.unix_fake_wait_pid();
                self.unix_fake_log(
                    "wait_pid1_event",
                    &[
                        ("kind", kind.clone()),
                        ("signal", signal.clone()),
                        ("pid", pid.to_string()),
                    ],
                    span,
                )?;
                let children = self.unix_fake_wait_children(pid);
                let kind = if kind == "children" || kind == "child" {
                    "children"
                } else if kind == "poll" {
                    "poll"
                } else if kind == "timeout" {
                    "timeout"
                } else {
                    "signal"
                };
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("kind"), Value::Str(kind.into())),
                    (Arc::from("signal"), Value::Str(signal.into())),
                    (Arc::from("children"), Value::List(children)),
                ]))))
            }
            RuntimeOp::UnixShutdownProcessGroups => {
                let groups = lowered_int_list_arg(
                    values.first().cloned(),
                    "unix.shutdown_process_groups",
                    span,
                )?;
                let term_timeout = lowered_duration_arg(
                    values.get(1).cloned(),
                    "unix.shutdown_process_groups",
                    span,
                )?;
                let kill_timeout = match values.get(2).cloned() {
                    Some(value) => {
                        lowered_duration_arg(Some(value), "unix.shutdown_process_groups", span)?
                            .millis
                    }
                    None => 0,
                };
                self.unix_fake_log(
                    "pid1_shutdown",
                    &[
                        (
                            "groups",
                            groups
                                .iter()
                                .map(i64::to_string)
                                .collect::<Vec<_>>()
                                .join(","),
                        ),
                        ("term_timeout_ms", term_timeout.millis.to_string()),
                        ("kill_timeout_ms", kill_timeout.to_string()),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Record(RecordMap::from([
                    (Arc::from("term_sent"), Value::Int(groups.len() as i64)),
                    (Arc::from("kill_sent"), Value::Int(0)),
                    (Arc::from("reaped"), Value::List(Vec::new())),
                    (Arc::from("remaining"), Value::List(Vec::new())),
                ]))))
            }
            RuntimeOp::UnixSpawnProcessGroup => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.spawn_process_group", span)?,
                    "unix.spawn_process_group",
                    span,
                )?;
                let notify = lowered_bool_arg_or(
                    values.get(1).cloned(),
                    false,
                    "unix.spawn_process_group",
                    span,
                )?;
                self.unix_fake_spawn(plan, None, notify, span)
            }
            RuntimeOp::UnixSpawnProcessGroupLog => {
                let plan = lowered_command_arg(
                    unix_require_arg(
                        values.first().cloned(),
                        "unix.spawn_process_group_log",
                        span,
                    )?,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                let log = lowered_path_arg(
                    unix_require_arg(values.get(1).cloned(), "unix.spawn_process_group_log", span)?,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                let notify = lowered_bool_arg_or(
                    values.get(2).cloned(),
                    false,
                    "unix.spawn_process_group_log",
                    span,
                )?;
                self.unix_fake_spawn_log(plan, log.display(), notify, span)
            }
            RuntimeOp::UnixSpawnLoggedProcessGroup => {
                let plan = lowered_command_arg(
                    unix_require_arg(
                        values.first().cloned(),
                        "unix.spawn_logged_process_group",
                        span,
                    )?,
                    "unix.spawn_logged_process_group",
                    span,
                )?;
                let logger_plan = lowered_command_arg(
                    unix_require_arg(
                        values.get(1).cloned(),
                        "unix.spawn_logged_process_group",
                        span,
                    )?,
                    "unix.spawn_logged_process_group",
                    span,
                )?;
                self.unix_fake_logged_spawn(plan, logger_plan, span)
            }
            RuntimeOp::UnixSpawnWithTty => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.spawn_with_tty", span)?,
                    "unix.spawn_with_tty",
                    span,
                )?;
                let tty =
                    lowered_str_arg_owned(values.get(1).cloned(), "", "unix.spawn_with_tty", span)?;
                self.unix_fake_spawn(plan, Some(tty), false, span)
            }
            RuntimeOp::UnixNotifyReady => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.notify_ready", span)?;
                let ready = if fd < 0 {
                    false
                } else {
                    let value = self.unix_fake_value("ready", "1");
                    value == "1" || value == "true" || value == "yes"
                };
                self.unix_fake_log("notify_ready", &[("ready", ready.to_string())], span)?;
                Ok(Value::ok(Value::Bool(ready)))
            }
            RuntimeOp::UnixNotifyClose => {
                let fd = lowered_int_arg(values.first().cloned(), "unix.notify_close", span)?;
                self.unix_fake_log("notify_close", &[("fd", fd.to_string())], span)?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixKillProcessGroup => {
                let pid =
                    lowered_int_arg(values.first().cloned(), "unix.kill_process_group", span)?;
                if pid <= 0 {
                    return Ok(module_error(
                        "pid-range",
                        "pid must be a positive process id",
                        span,
                    ));
                }
                let signal = lowered_str_arg_owned(
                    values.get(1).cloned(),
                    "",
                    "unix.kill_process_group",
                    span,
                )?;
                if let Err(error) = process_module::signal_info(&signal, span) {
                    return Ok(Value::err(Value::Error(Box::new(error))));
                }
                self.unix_fake_log(
                    "kill_process_group",
                    &[("pid", pid.to_string()), ("signal", signal)],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            RuntimeOp::UnixExec => {
                let plan = lowered_command_arg(
                    unix_require_arg(values.first().cloned(), "unix.exec", span)?,
                    "unix.exec",
                    span,
                )?;
                self.unix_fake_log(
                    "exec",
                    &[
                        (
                            "command",
                            String::from_utf8_lossy(&plan.target).into_owned(),
                        ),
                        ("argv", display_spawn_argv(&plan.target, &plan.argv)),
                    ],
                    span,
                )?;
                Ok(Value::ok(Value::Unit))
            }
            _ => unreachable!("unix fake operation expected"),
        }
    }

    fn unix_fake_status_kind(&self) -> String {
        self.unix_fake_value("status_kind", "exit")
    }

    fn unix_fake_status_code(&self) -> i32 {
        self.unix_fake_value("status_code", "0")
            .parse::<i32>()
            .unwrap_or(0)
    }

    fn unix_fake_status(&self) -> ProcessStatus {
        let kind = self.unix_fake_status_kind();
        let code = self.unix_fake_status_code();
        if kind == "signal" {
            ProcessStatus::signaled(if code > 0 { code } else { libc::SIGTERM })
        } else {
            ProcessStatus::exited(code)
        }
    }

    fn unix_fake_child_events(&self) -> Vec<Value> {
        let pid = self
            .unix_fake_value("child_pid", "0")
            .parse::<i64>()
            .unwrap_or(0);
        if pid <= 0 {
            return Vec::new();
        }
        self.unix_fake_wait_children(pid)
    }

    fn unix_fake_wait_pid(&self) -> i64 {
        let child_pid = self
            .unix_fake_value("child_pid", "0")
            .parse::<i64>()
            .unwrap_or(0);
        if child_pid > 0 {
            return child_pid;
        }
        self.unix_fake_value("pid", "0").parse::<i64>().unwrap_or(0)
    }

    fn unix_fake_wait_children(&self, pid: i64) -> Vec<Value> {
        if pid <= 0 {
            return Vec::new();
        }
        vec![Value::Record(RecordMap::from([
            (Arc::from("pid"), Value::Int(pid)),
            (Arc::from("status"), Value::Status(self.unix_fake_status())),
        ]))]
    }

    fn unix_fake_spawn(
        &mut self,
        plan: CommandPlan,
        tty: Option<String>,
        notify: bool,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let pid = self.unix_next_pid;
        self.unix_next_pid += 1;
        let command = String::from_utf8_lossy(&plan.target).into_owned();
        let argv = std::iter::once(plan.target.as_slice())
            .chain(plan.argv.iter().map(Vec::as_slice))
            .map(|item| Value::Str(String::from_utf8_lossy(item).into_owned().into()))
            .collect::<Vec<_>>();
        let new_session = tty.is_some();
        // No real pipe under the fake; report the fake pid as the notify fd so a
        // supervisor treats the unit as notify-capable and polls it.
        let notify_fd = if notify { pid } else { -1 };
        let op = if new_session {
            "spawn_with_tty"
        } else {
            "spawn_process_group"
        };
        let mut fields = vec![
            ("pid", pid.to_string()),
            ("command", command.clone()),
            ("argv", display_spawn_argv(&plan.target, &plan.argv)),
            ("detach", "true".to_string()),
            ("new_session", new_session.to_string()),
            ("ignore_hup", "true".to_string()),
            ("notify_fd", notify_fd.to_string()),
        ];
        if let Some(tty) = tty {
            fields.push(("tty", tty));
        }
        self.unix_fake_log(op, &fields, span)?;
        Ok(Value::ok(Value::Record(RecordMap::from([
            (Arc::from("pid"), Value::Int(pid)),
            (Arc::from("command"), Value::Str(command.into())),
            (Arc::from("argv"), Value::List(argv)),
            (Arc::from("detach"), Value::Bool(true)),
            (Arc::from("new_session"), Value::Bool(new_session)),
            (Arc::from("ignore_hup"), Value::Bool(true)),
            (Arc::from("notify_fd"), Value::Int(notify_fd)),
        ]))))
    }

    fn unix_fake_spawn_log(
        &mut self,
        plan: CommandPlan,
        log_path: String,
        notify: bool,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let pid = self.unix_next_pid;
        self.unix_next_pid += 1;
        let command = String::from_utf8_lossy(&plan.target).into_owned();
        let argv = std::iter::once(plan.target.as_slice())
            .chain(plan.argv.iter().map(Vec::as_slice))
            .map(|item| Value::Str(String::from_utf8_lossy(item).into_owned().into()))
            .collect::<Vec<_>>();
        let notify_fd = if notify { pid } else { -1 };
        self.unix_fake_log(
            "spawn_process_group",
            &[
                ("pid", pid.to_string()),
                ("command", command.clone()),
                ("argv", display_spawn_argv(&plan.target, &plan.argv)),
                ("log", "append".to_string()),
                ("log_path", log_path),
                ("detach", "true".to_string()),
                ("new_session", "false".to_string()),
                ("ignore_hup", "true".to_string()),
                ("notify_fd", notify_fd.to_string()),
            ],
            span,
        )?;
        Ok(Value::ok(Value::Record(RecordMap::from([
            (Arc::from("pid"), Value::Int(pid)),
            (Arc::from("command"), Value::Str(command.into())),
            (Arc::from("argv"), Value::List(argv)),
            (Arc::from("detach"), Value::Bool(true)),
            (Arc::from("new_session"), Value::Bool(false)),
            (Arc::from("ignore_hup"), Value::Bool(true)),
            (Arc::from("notify_fd"), Value::Int(notify_fd)),
        ]))))
    }

    fn unix_fake_logged_spawn(
        &mut self,
        plan: CommandPlan,
        logger_plan: CommandPlan,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let pid = self.unix_next_pid;
        self.unix_next_pid += 1;
        let log_pid = self.unix_next_pid;
        self.unix_next_pid += 1;
        let command = String::from_utf8_lossy(&plan.target).into_owned();
        let logger = String::from_utf8_lossy(&logger_plan.target).into_owned();
        let argv = std::iter::once(plan.target.as_slice())
            .chain(plan.argv.iter().map(Vec::as_slice))
            .map(|item| Value::Str(String::from_utf8_lossy(item).into_owned().into()))
            .collect::<Vec<_>>();
        self.unix_fake_log(
            "spawn_logged_process_group",
            &[
                ("pid", pid.to_string()),
                ("log_pid", log_pid.to_string()),
                ("command", command.clone()),
                ("argv", display_spawn_argv(&plan.target, &plan.argv)),
                ("logger", logger),
                (
                    "logger_argv",
                    display_spawn_argv(&logger_plan.target, &logger_plan.argv),
                ),
                ("detach", "true".to_string()),
                ("new_session", "false".to_string()),
                ("ignore_hup", "true".to_string()),
            ],
            span,
        )?;
        Ok(Value::ok(Value::Record(RecordMap::from([
            (Arc::from("pid"), Value::Int(pid)),
            (Arc::from("log_pid"), Value::Int(log_pid)),
            (Arc::from("command"), Value::Str(command.into())),
            (Arc::from("argv"), Value::List(argv)),
            (Arc::from("detach"), Value::Bool(true)),
            (Arc::from("new_session"), Value::Bool(false)),
            (Arc::from("ignore_hup"), Value::Bool(true)),
        ]))))
    }
}
