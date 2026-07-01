use crate::xsht::cli::{CliOutput, TraceOptions};

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod platform {
    use crate::xsht::cli::TraceFormat;
    use crate::xsht::cli::{CliOutput, TraceOptions};
    use rustix::io::Errno;
    use rustix::process::{Pid, WaitOptions, WaitStatus, waitpid};
    use std::collections::BTreeMap;
    use std::ffi::OsStr;
    use std::fs;
    use std::io::{self, Read};
    use std::os::unix::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};
    use xsh::trace::{SyscallSummary, SyscallSummaryRenderer, SyscallTraceRecord};

    pub(crate) fn run(options: TraceOptions) -> CliOutput {
        match Supervisor::new(options).run() {
            Ok(output) => output,
            Err(message) => CliOutput {
                status: 2,
                stdout: Vec::new(),
                stderr: format!("xsht trace: syscall tracing setup failed: {message}\n")
                    .into_bytes(),
                trace_text: String::new(),
                syscall_summary: None,
            },
        }
    }

    struct Supervisor {
        options: TraceOptions,
        tracees: BTreeMap<libc::pid_t, Tracee>,
        records: Vec<SyscallTraceRecord>,
        root_pid: libc::pid_t,
        root_status: Option<u8>,
        /// Monotonic Instant at which the child process was spawned.
        wall_start: Instant,
        /// Unix epoch in microseconds at wall_start, for absolute syscall timestamps.
        wall_start_epoch_us: u64,
    }

    impl Supervisor {
        fn new(options: TraceOptions) -> Self {
            Self {
                options,
                tracees: BTreeMap::new(),
                records: Vec::new(),
                root_pid: 0,
                root_status: None,
                wall_start: Instant::now(),
                wall_start_epoch_us: 0,
            }
        }

        fn run(mut self) -> Result<CliOutput, String> {
            let trace_file = tempfile::Builder::new()
                .prefix("xsh-syscall-trace-")
                .tempfile()
                .map_err(|error| format!("failed to create trace file: {error}"))?;
            let child_args = child_args(&self.options, trace_file.path());
            let mut command = Command::new(xsht_exe()?);
            command
                .args(child_args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            unsafe {
                command.pre_exec(|| {
                    if libc::ptrace(
                        libc::PTRACE_TRACEME,
                        0,
                        std::ptr::null_mut::<libc::c_void>(),
                        std::ptr::null_mut::<libc::c_void>(),
                    ) == -1
                    {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }

            let mut child = command
                .spawn()
                .map_err(|error| format!("failed to start traced xsht: {error}"))?;
            self.root_pid = child.id() as libc::pid_t;
            // Record epoch at spawn time so syscall timestamps can be expressed as
            // Unix µs for correlation with XSH trace event start_time_us.
            self.wall_start = Instant::now();
            self.wall_start_epoch_us = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_micros().min(u64::MAX as u128) as u64);
            let wall_start = self.wall_start;
            let stdout = child.stdout.take().expect("piped child stdout");
            let stderr = child.stderr.take().expect("piped child stderr");
            let stdout_reader = thread::spawn(move || read_pipe(stdout));
            let stderr_reader = thread::spawn(move || read_pipe(stderr));

            let run_result = self.trace_child();
            if run_result.is_err() {
                self.kill_tracees();
            }

            let stdout = join_reader(stdout_reader, "stdout")?;
            let stderr = join_reader(stderr_reader, "stderr")?;
            drop(child);
            run_result?;

            let wall_time_ns = wall_start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;

            let mut summary = SyscallSummary::from_records(&self.records);
            summary.wall_time_ns = Some(wall_time_ns);

            let trace_jsonl_raw = fs::read_to_string(trace_file.path())
                .map_err(|error| format!("failed to read child trace output: {error}"))?;

            // If the trace was written as JSONL, correlate syscall records with
            // XSH trace events to populate per-operation attribution.
            if self.options.format == TraceFormat::Jsonl {
                summary.attribute_operations(&self.records, &trace_jsonl_raw);
            }

            let mut rendered_trace = trace_jsonl_raw;
            append_syscall_summary(
                &mut rendered_trace,
                &self.options.format,
                &summary,
                self.options.top_syscalls,
            );

            let script_stderr = String::from_utf8_lossy(&stderr).into_owned();
            let trace_text = if let Some(path) = &self.options.file {
                fs::write(path, &rendered_trace)
                    .map_err(|error| format!("failed to write trace file '{path}': {error}"))?;
                String::new()
            } else {
                rendered_trace
            };

            Ok(CliOutput {
                status: self.root_status.unwrap_or(1),
                stdout,
                stderr: script_stderr.into_bytes(),
                trace_text,
                syscall_summary: Some(summary),
            })
        }

        fn trace_child(&mut self) -> Result<(), String> {
            let initial_status = wait_for_pid(self.root_pid)?;
            if !initial_status.stopped() {
                return Err("traced xsht exited before ptrace setup completed".to_string());
            }

            let program = process_name(self.root_pid);
            self.tracees.insert(
                self.root_pid,
                Tracee::new(program, true, ResumeMode::Syscall),
            );
            set_options(self.root_pid)?;
            resume_syscall(self.root_pid, 0)?;

            while !self.tracees.is_empty() {
                let (pid, status) = wait_any()?;
                if pid <= 0 {
                    continue;
                }

                if status.exited() {
                    if pid == self.root_pid {
                        self.root_status = Some(status.exit_status().unwrap_or(0) as u8);
                    }
                    self.tracees.remove(&pid);
                    continue;
                }

                if status.signaled() {
                    if pid == self.root_pid {
                        let signal = status.terminating_signal().unwrap_or(0);
                        self.root_status = Some((128 + signal).min(255) as u8);
                    }
                    self.tracees.remove(&pid);
                    continue;
                }

                if !status.stopped() {
                    continue;
                }

                if self.resume_mode(pid) == ResumeMode::Detach {
                    detach(pid, 0)?;
                    self.tracees.remove(&pid);
                    continue;
                }

                let signal = status.stopping_signal().unwrap_or(0);
                let event = ptrace_event(status.as_raw());
                if signal == (libc::SIGTRAP | PTRACE_SYSCALL_STOP) {
                    self.handle_syscall_stop(pid)?;
                    continue;
                }

                if signal == libc::SIGTRAP && event != 0 {
                    self.handle_ptrace_event(pid, event)?;
                    continue;
                }

                if signal == libc::SIGSTOP {
                    if let Some(tracee) = self.tracees.get_mut(&pid)
                        && !tracee.options_set
                    {
                        set_options(pid)?;
                        tracee.options_set = true;
                    }
                    self.resume_tracee(pid, 0)?;
                    continue;
                }

                let delivered = if signal == libc::SIGTRAP { 0 } else { signal };
                self.resume_tracee(pid, delivered)?;
            }

            Ok(())
        }

        fn handle_syscall_stop(&mut self, pid: libc::pid_t) -> Result<(), String> {
            let registers = syscall_registers(pid)?;
            let Some(tracee) = self.tracees.get_mut(&pid) else {
                resume_syscall(pid, 0)?;
                return Ok(());
            };

            if let Some(active) = tracee.active.take() {
                let elapsed_ns = active.start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                // Compute absolute Unix µs for the syscall entry.
                let offset_us = (active.start - self.wall_start)
                    .as_micros()
                    .min(u64::MAX as u128) as u64;
                let start_time_us = self.wall_start_epoch_us.saturating_add(offset_us);
                self.records.push(SyscallTraceRecord::new(
                    pid as u32,
                    tracee.program.clone(),
                    active.name,
                    syscall_return_is_error(registers.return_value),
                    elapsed_ns,
                    start_time_us,
                ));
            } else {
                tracee.active = Some(ActiveSyscall {
                    name: syscall_name(registers.number),
                    start: Instant::now(),
                });
            }

            resume_syscall(pid, 0)
        }

        fn handle_ptrace_event(&mut self, pid: libc::pid_t, event: i32) -> Result<(), String> {
            match event {
                libc::PTRACE_EVENT_FORK | libc::PTRACE_EVENT_VFORK => {
                    let child_pid = event_pid(pid)?;
                    let program = self
                        .tracees
                        .get(&pid)
                        .map(|tracee| tracee.program.clone())
                        .unwrap_or_else(|| process_name(child_pid));
                    self.tracees
                        .entry(child_pid)
                        .or_insert_with(|| Tracee::new(program, false, ResumeMode::Detach));
                }
                libc::PTRACE_EVENT_CLONE => {
                    let child_pid = event_pid(pid)?;
                    let program = self
                        .tracees
                        .get(&pid)
                        .map(|tracee| tracee.program.clone())
                        .unwrap_or_else(|| process_name(child_pid));
                    let mode = clone_resume_mode(pid, child_pid);
                    self.tracees
                        .entry(child_pid)
                        .or_insert_with(|| Tracee::new(program, false, mode));
                }
                libc::PTRACE_EVENT_EXEC => {
                    if let Some(tracee) = self.tracees.get_mut(&pid) {
                        tracee.program = process_name(pid);
                        tracee.active = None;
                        tracee.mode = ResumeMode::Syscall;
                    }
                }
                _ => {}
            }
            self.resume_tracee(pid, 0)
        }

        fn resume_mode(&self, pid: libc::pid_t) -> ResumeMode {
            self.tracees
                .get(&pid)
                .map(|tracee| tracee.mode)
                .unwrap_or(ResumeMode::Syscall)
        }

        fn resume_tracee(&self, pid: libc::pid_t, signal: libc::c_int) -> Result<(), String> {
            match self.resume_mode(pid) {
                ResumeMode::Syscall => resume_syscall(pid, signal),
                ResumeMode::Detach => detach(pid, signal),
            }
        }

        fn kill_tracees(&self) {
            for pid in self.tracees.keys().copied() {
                let _ = unsafe {
                    libc::ptrace(
                        libc::PTRACE_KILL,
                        pid,
                        std::ptr::null_mut::<libc::c_void>(),
                        std::ptr::null_mut::<libc::c_void>(),
                    )
                };
            }
        }
    }

    struct Tracee {
        program: String,
        active: Option<ActiveSyscall>,
        options_set: bool,
        mode: ResumeMode,
    }

    impl Tracee {
        fn new(program: String, options_set: bool, mode: ResumeMode) -> Self {
            Self {
                program,
                active: None,
                options_set,
                mode,
            }
        }
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum ResumeMode {
        Syscall,
        Detach,
    }

    struct ActiveSyscall {
        name: String,
        start: Instant,
    }

    fn xsht_exe() -> Result<PathBuf, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        Ok(exe)
    }

    fn child_args(options: &TraceOptions, trace_path: &Path) -> Vec<String> {
        let mut args = vec!["trace".to_string()];
        // When JSONL format is requested, force raw per-event output so the
        // supervisor can parse individual event timestamps for syscall attribution.
        // Text format keeps the summary rendering for human-readable output.
        let use_raw = options.raw || options.format == TraceFormat::Jsonl;
        if use_raw {
            args.push("--raw".to_string());
        }
        match options.format {
            TraceFormat::Jsonl => {
                args.push("--trace-format".to_string());
                args.push("jsonl".to_string());
            }
            TraceFormat::Flamegraph => {
                args.push("--trace-format".to_string());
                args.push("flamegraph".to_string());
            }
            TraceFormat::Text => {}
        }
        args.push("--trace-file".to_string());
        args.push(trace_path.to_string_lossy().into_owned());
        args.push(options.script.clone());
        if !options.args.is_empty() {
            args.push("--".to_string());
            args.extend(options.args.clone());
        }
        args
    }

    fn append_syscall_summary(
        rendered_trace: &mut String,
        trace_format: &TraceFormat,
        summary: &SyscallSummary,
        top: usize,
    ) {
        if !rendered_trace.is_empty() && !rendered_trace.ends_with('\n') {
            rendered_trace.push('\n');
        }
        let rendered = match trace_format {
            TraceFormat::Text | TraceFormat::Flamegraph => {
                SyscallSummaryRenderer::new(top).render_text(summary)
            }
            TraceFormat::Jsonl => SyscallSummaryRenderer::new(top).render_jsonl(summary),
        };
        rendered_trace.push_str(&rendered);
    }

    fn read_pipe(mut pipe: impl Read) -> io::Result<Vec<u8>> {
        let mut output = Vec::new();
        pipe.read_to_end(&mut output)?;
        Ok(output)
    }

    fn join_reader(
        reader: thread::JoinHandle<io::Result<Vec<u8>>>,
        stream: &str,
    ) -> Result<Vec<u8>, String> {
        reader
            .join()
            .map_err(|_| format!("failed to join {stream} reader"))?
            .map_err(|error| format!("failed to read child {stream}: {error}"))
    }

    fn wait_for_pid(pid: libc::pid_t) -> Result<WaitStatus, String> {
        let target = Pid::from_raw(pid).ok_or_else(|| format!("invalid pid {pid}"))?;
        loop {
            match waitpid(Some(target), WaitOptions::empty()) {
                Ok(Some((_, status))) => return Ok(status),
                Ok(None) => continue,
                Err(error) if error == Errno::INTR => continue,
                Err(error) => {
                    return Err(format!("waitpid({pid}) failed: {}", io::Error::from(error)));
                }
            }
        }
    }

    fn wait_any() -> Result<(libc::pid_t, WaitStatus), String> {
        // `__WALL` is not exposed by rustix's `WaitOptions`; it has no rustix
        // equivalent, so we keep the libc constant and pass its bits through.
        let options = WaitOptions::from_bits_retain(libc::__WALL as _);
        loop {
            match waitpid(None, options) {
                Ok(Some((pid, status))) => return Ok((pid.as_raw_pid(), status)),
                Ok(None) => continue,
                Err(error) if error == Errno::INTR => continue,
                Err(error) => {
                    return Err(format!("waitpid(-1) failed: {}", io::Error::from(error)));
                }
            }
        }
    }

    fn set_options(pid: libc::pid_t) -> Result<(), String> {
        let options = (libc::PTRACE_O_TRACESYSGOOD
            | libc::PTRACE_O_TRACEFORK
            | libc::PTRACE_O_TRACEVFORK
            | libc::PTRACE_O_TRACECLONE
            | libc::PTRACE_O_TRACEEXEC
            | PTRACE_O_EXITKILL) as usize;
        ptrace(
            libc::PTRACE_SETOPTIONS as libc::c_long,
            pid,
            std::ptr::null_mut(),
            options as *mut libc::c_void,
        )
        .map(|_| ())
        .map_err(|error| format!("PTRACE_SETOPTIONS failed for pid {pid}: {error}"))
    }

    fn resume_syscall(pid: libc::pid_t, signal: libc::c_int) -> Result<(), String> {
        ptrace(
            libc::PTRACE_SYSCALL as libc::c_long,
            pid,
            std::ptr::null_mut(),
            signal as usize as *mut libc::c_void,
        )
        .map(|_| ())
        .map_err(|error| format!("PTRACE_SYSCALL failed for pid {pid}: {error}"))
    }

    fn detach(pid: libc::pid_t, signal: libc::c_int) -> Result<(), String> {
        ptrace(
            libc::PTRACE_DETACH as libc::c_long,
            pid,
            std::ptr::null_mut(),
            signal as usize as *mut libc::c_void,
        )
        .map(|_| ())
        .map_err(|error| format!("PTRACE_DETACH failed for pid {pid}: {error}"))
    }

    fn clone_resume_mode(parent_pid: libc::pid_t, child_pid: libc::pid_t) -> ResumeMode {
        match (thread_group_id(parent_pid), thread_group_id(child_pid)) {
            (Some(parent_tgid), Some(child_tgid)) if parent_tgid != child_tgid => {
                ResumeMode::Detach
            }
            _ => ResumeMode::Syscall,
        }
    }

    fn thread_group_id(pid: libc::pid_t) -> Option<libc::pid_t> {
        let text = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        text.lines().find_map(|line| {
            line.strip_prefix("Tgid:")
                .and_then(|value| value.trim().parse().ok())
        })
    }

    struct SyscallRegisters {
        number: i64,
        return_value: i64,
    }

    #[cfg(target_arch = "x86_64")]
    fn syscall_registers(pid: libc::pid_t) -> Result<SyscallRegisters, String> {
        let mut registers = unsafe { std::mem::zeroed::<libc::user_regs_struct>() };
        ptrace(
            libc::PTRACE_GETREGS as libc::c_long,
            pid,
            std::ptr::null_mut(),
            (&mut registers as *mut libc::user_regs_struct).cast(),
        )
        .map(|_| SyscallRegisters {
            number: registers.orig_rax as i64,
            return_value: registers.rax as i64,
        })
        .map_err(|error| format!("PTRACE_GETREGS failed for pid {pid}: {error}"))
    }

    #[cfg(target_arch = "aarch64")]
    fn syscall_registers(pid: libc::pid_t) -> Result<SyscallRegisters, String> {
        let mut registers = unsafe { std::mem::zeroed::<libc::user_regs_struct>() };
        let mut iovec = libc::iovec {
            iov_base: (&mut registers as *mut libc::user_regs_struct).cast(),
            iov_len: std::mem::size_of::<libc::user_regs_struct>(),
        };
        ptrace(
            libc::PTRACE_GETREGSET as libc::c_long,
            pid,
            NT_PRSTATUS as usize as *mut libc::c_void,
            (&mut iovec as *mut libc::iovec).cast(),
        )
        .map(|_| SyscallRegisters {
            number: registers.regs[8] as i64,
            return_value: registers.regs[0] as i64,
        })
        .map_err(|error| format!("PTRACE_GETREGSET failed for pid {pid}: {error}"))
    }

    fn event_pid(pid: libc::pid_t) -> Result<libc::pid_t, String> {
        let mut message = 0 as libc::c_ulong;
        ptrace(
            libc::PTRACE_GETEVENTMSG as libc::c_long,
            pid,
            std::ptr::null_mut(),
            (&mut message as *mut libc::c_ulong).cast(),
        )
        .map(|_| message as libc::pid_t)
        .map_err(|error| format!("PTRACE_GETEVENTMSG failed for pid {pid}: {error}"))
    }

    fn ptrace(
        request: libc::c_long,
        pid: libc::pid_t,
        addr: *mut libc::c_void,
        data: *mut libc::c_void,
    ) -> io::Result<libc::c_long> {
        let result = unsafe { libc::ptrace(request as _, pid, addr, data) };
        if result == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(result)
        }
    }

    fn ptrace_event(status: libc::c_int) -> libc::c_int {
        status >> 16
    }

    fn syscall_return_is_error(value: i64) -> bool {
        (-4095..0).contains(&value)
    }

    fn process_name(pid: libc::pid_t) -> String {
        let comm = PathBuf::from(format!("/proc/{pid}/comm"));
        if let Ok(text) = fs::read_to_string(comm) {
            let text = text.trim();
            if !text.is_empty() {
                return text.to_string();
            }
        }

        let exe = PathBuf::from(format!("/proc/{pid}/exe"));
        if let Ok(path) = fs::read_link(exe)
            && let Some(name) = path.file_name().and_then(OsStr::to_str)
            && !name.is_empty()
        {
            return name.to_string();
        }

        format!("pid:{pid}")
    }

    fn syscall_name(number: i64) -> String {
        let Ok(id) = usize::try_from(number) else {
            return format!("syscall_{number}");
        };
        syscalls::Sysno::new(id)
            .map(|syscall| syscall.name().to_string())
            .unwrap_or_else(|| format!("syscall_{number}"))
    }

    const PTRACE_O_EXITKILL: libc::c_int = 1 << 20;
    const PTRACE_SYSCALL_STOP: libc::c_int = 0x80;

    #[cfg(target_arch = "aarch64")]
    const NT_PRSTATUS: libc::c_int = 1;
}

#[cfg(all(
    target_os = "linux",
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
mod platform {
    use crate::xsht::cli::{CliOutput, TraceOptions};

    pub(crate) fn run(_options: TraceOptions) -> CliOutput {
        CliOutput {
            status: 2,
            stdout: Vec::new(),
            stderr: "xsht trace: syscall tracing setup failed: Linux syscall tracing currently supports x86_64 and aarch64 only\n"
                .as_bytes()
                .to_vec(),
            trace_text: String::new(),
            syscall_summary: None,
        }
    }
}

#[cfg(not(target_os = "linux"))]
#[allow(clippy::single_call_fn)]
pub(crate) fn run(_options: TraceOptions) -> CliOutput {
    CliOutput {
        status: 2,
        stdout: Vec::new(),
        stderr: "xsht trace: `--syscalls` is only supported on Linux\n"
            .as_bytes()
            .to_vec(),
        trace_text: String::new(),
        syscall_summary: None,
    }
}

#[cfg(target_os = "linux")]
pub(crate) use platform::run;
