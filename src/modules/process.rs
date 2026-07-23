#![allow(clippy::single_call_fn)]

use crate::modules::time::{format_epoch_ms, now_epoch_ms};
use crate::modules::user::name_for_uid;
use crate::runtime::value::{LiveStream, RecordMap, RecordShape, RuntimeError, StreamValue, Value};
use crate::source::Span;
use std::sync::{Arc, LazyLock};

static K_PID: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("pid"));
static K_PARENT_PID: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("parent_pid"));
static K_COMMAND: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("command"));
static K_ARGV: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("argv"));
static K_ARGV0: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("argv0"));
static K_USER: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("user"));
static K_UID: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("uid"));
static K_STATUS: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("status"));
static K_START_TIME: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("start_time"));
static K_START_TIME_MS: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("start_time_ms"));
static K_RUNTIME_SECONDS: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("runtime_seconds"));
static K_OWNER_PID: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("owner_pid"));
static K_THREAD_ID: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("thread_id"));
static K_THREAD_NAME: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("thread_name"));
static PROCESS_SHAPE: LazyLock<RecordShape> = LazyLock::new(|| {
    RecordShape::new(vec![
        K_ARGV.clone(),
        K_ARGV0.clone(),
        K_COMMAND.clone(),
        K_PARENT_PID.clone(),
        K_PID.clone(),
        K_RUNTIME_SECONDS.clone(),
        K_START_TIME.clone(),
        K_START_TIME_MS.clone(),
        K_STATUS.clone(),
        K_UID.clone(),
        K_USER.clone(),
    ])
});
static PROCESS_THREAD_SHAPE: LazyLock<RecordShape> = LazyLock::new(|| {
    RecordShape::new(vec![
        K_ARGV.clone(),
        K_ARGV0.clone(),
        K_COMMAND.clone(),
        K_OWNER_PID.clone(),
        K_PARENT_PID.clone(),
        K_PID.clone(),
        K_RUNTIME_SECONDS.clone(),
        K_START_TIME.clone(),
        K_START_TIME_MS.clone(),
        K_STATUS.clone(),
        K_THREAD_ID.clone(),
        K_THREAD_NAME.clone(),
        K_UID.clone(),
        K_USER.clone(),
    ])
});
#[cfg(target_os = "linux")]
use std::os::unix::fs::MetadataExt;

#[derive(Clone, Debug)]
pub(crate) struct SignalInfo {
    pub(crate) name: String,
    pub(crate) number: i32,
}

pub(crate) fn list_processes(span: Span) -> Result<StreamValue, RuntimeError> {
    list_processes_stream(span)
}

#[cfg(target_os = "linux")]
fn list_processes_stream(span: Span) -> Result<StreamValue, RuntimeError> {
    let now_ms = now_epoch_ms();
    let boot_time_ms = linux_boot_time_ms(span)?;
    let ticks_per_second = match rustix::param::clock_ticks_per_second() {
        0 => 100,
        ticks => ticks as i64,
    };
    let mut entries = std::fs::read_dir("/proc")
        .map_err(|error| RuntimeError::new("process-list", error.to_string()).with_span(span))?
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse::<i64>().ok()?;
            let uid = entry.metadata().map(|metadata| metadata.uid()).unwrap_or(0);
            Some((pid, entry.path(), uid))
        })
        .collect::<Vec<_>>();
    entries.sort_unstable_by_key(|(pid, _, _)| *pid);
    Ok(StreamValue::from_live(
        "process.list",
        LinuxProcessStream {
            entries: entries.into_iter(),
            now_ms,
            boot_time_ms,
            ticks_per_second,
        },
    ))
}

#[cfg(target_os = "linux")]
struct LinuxProcessStream {
    entries: std::vec::IntoIter<(i64, std::path::PathBuf, u32)>,
    now_ms: i64,
    boot_time_ms: i64,
    ticks_per_second: i64,
}

#[cfg(target_os = "linux")]
impl LiveStream for LinuxProcessStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        loop {
            let Some((pid, path, uid)) = self.entries.next() else {
                return Ok(None);
            };
            let Some(record) =
                linux_process_record(pid, &path, self.boot_time_ms, self.ticks_per_second, uid)
            else {
                continue;
            };
            return Ok(Some(process_record_value(record, self.now_ms, span)));
        }
    }
}

#[cfg(target_os = "macos")]
fn list_processes_stream(span: Span) -> Result<StreamValue, RuntimeError> {
    use std::ffi::c_void;

    const PROC_ALL_PIDS: u32 = 1;
    let needed = unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if needed < 0 {
        return Err(
            RuntimeError::new("process-list", std::io::Error::last_os_error().to_string())
                .with_span(span),
        );
    }
    let mut pids = vec![0i32; needed as usize / std::mem::size_of::<i32>() + 64];
    let bytes = (pids.len() * std::mem::size_of::<i32>()) as i32;
    let returned =
        unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, pids.as_mut_ptr().cast::<c_void>(), bytes) };
    if returned < 0 {
        return Err(
            RuntimeError::new("process-list", std::io::Error::last_os_error().to_string())
                .with_span(span),
        );
    }
    let count = returned as usize / std::mem::size_of::<i32>();
    let mut pids = pids
        .into_iter()
        .take(count)
        .filter(|pid| *pid > 0)
        .collect::<Vec<_>>();
    pids.sort_unstable();
    Ok(StreamValue::from_live(
        "process.list",
        MacProcessStream {
            pids: pids.into_iter(),
            now_ms: now_epoch_ms(),
        },
    ))
}

#[cfg(target_os = "macos")]
struct MacProcessStream {
    pids: std::vec::IntoIter<i32>,
    now_ms: i64,
}

#[cfg(target_os = "macos")]
impl LiveStream for MacProcessStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        loop {
            let Some(pid) = self.pids.next() else {
                return Ok(None);
            };
            if let Some(record) = macos_process_record(pid) {
                return Ok(Some(process_record_value(record, self.now_ms, span)));
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn list_processes_stream(_span: Span) -> Result<StreamValue, RuntimeError> {
    Ok(StreamValue::from_live("process.list", EmptyProcessStream))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
struct EmptyProcessStream;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl LiveStream for EmptyProcessStream {
    fn next(&mut self, _span: Span) -> Result<Option<Value>, RuntimeError> {
        Ok(None)
    }
}

pub(crate) fn list_threads(pid: Option<i64>, span: Span) -> Result<StreamValue, RuntimeError> {
    if let Some(pid) = pid
        && !(1..=i32::MAX as i64).contains(&pid)
    {
        return Err(
            RuntimeError::new("pid-range", "pid must be a positive process id").with_span(span),
        );
    }
    list_threads_stream(pid, span)
}

#[cfg(target_os = "linux")]
fn list_threads_stream(pid: Option<i64>, span: Span) -> Result<StreamValue, RuntimeError> {
    let now_ms = now_epoch_ms();
    let boot_time_ms = linux_boot_time_ms(span)?;
    let ticks_per_second = match rustix::param::clock_ticks_per_second() {
        0 => 100,
        ticks => ticks as i64,
    };
    let mut processes = if let Some(owner_pid) = pid {
        let path = std::path::PathBuf::from(format!("/proc/{owner_pid}"));
        let uid = std::fs::metadata(&path)
            .map(|metadata| metadata.uid())
            .unwrap_or(0);
        vec![(owner_pid, path, uid)]
    } else {
        std::fs::read_dir("/proc")
            .map_err(|error| {
                RuntimeError::new("process-threads", error.to_string()).with_span(span)
            })?
            .flatten()
            .filter_map(|entry| {
                let owner_pid = entry.file_name().to_str()?.parse::<i64>().ok()?;
                let uid = entry.metadata().map(|metadata| metadata.uid()).unwrap_or(0);
                Some((owner_pid, entry.path(), uid))
            })
            .collect::<Vec<_>>()
    };
    processes.sort_unstable_by_key(|(owner_pid, _, _)| *owner_pid);
    Ok(StreamValue::from_live(
        "process.threads",
        LinuxThreadStream {
            processes: processes.into_iter(),
            current: None,
            now_ms,
            boot_time_ms,
            ticks_per_second,
        },
    ))
}

#[cfg(target_os = "linux")]
struct LinuxThreadCursor {
    process: ProcessRecord,
    tasks: std::vec::IntoIter<(i64, std::path::PathBuf)>,
}

#[cfg(target_os = "linux")]
struct LinuxThreadStream {
    processes: std::vec::IntoIter<(i64, std::path::PathBuf, u32)>,
    current: Option<LinuxThreadCursor>,
    now_ms: i64,
    boot_time_ms: i64,
    ticks_per_second: i64,
}

#[cfg(target_os = "linux")]
impl LiveStream for LinuxThreadStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        loop {
            if let Some(cursor) = &mut self.current {
                for (thread_id, path) in cursor.tasks.by_ref() {
                    let Ok(stat) = std::fs::read_to_string(path.join("stat")) else {
                        continue;
                    };
                    let Some(parsed) = parse_linux_stat(&stat) else {
                        continue;
                    };
                    return Ok(Some(thread_record_value(
                        ThreadRecord {
                            process: cursor.process.clone(),
                            pid: thread_id,
                            thread_id,
                            thread_name: parsed.command,
                            status: parsed.status,
                        },
                        self.now_ms,
                        span,
                    )));
                }
                self.current = None;
            }

            let Some((owner_pid, process_path, uid)) = self.processes.next() else {
                return Ok(None);
            };
            let Some(process) = linux_process_record(
                owner_pid,
                &process_path,
                self.boot_time_ms,
                self.ticks_per_second,
                uid,
            ) else {
                continue;
            };
            let Ok(mut tasks) = std::fs::read_dir(process_path.join("task")) else {
                continue;
            };
            let mut tasks = tasks
                .by_ref()
                .flatten()
                .filter_map(|task| {
                    let thread_id = task.file_name().to_str()?.parse::<i64>().ok()?;
                    Some((thread_id, task.path()))
                })
                .collect::<Vec<_>>();
            tasks.sort_unstable_by_key(|(thread_id, _)| *thread_id);
            self.current = Some(LinuxThreadCursor {
                process,
                tasks: tasks.into_iter(),
            });
        }
    }
}

#[cfg(target_os = "macos")]
fn list_threads_stream(pid: Option<i64>, span: Span) -> Result<StreamValue, RuntimeError> {
    let mut pids = if let Some(pid) = pid {
        vec![pid as i32]
    } else {
        macos_all_pids("process-threads", span)?
    };
    pids.sort_unstable();
    Ok(StreamValue::from_live(
        "process.threads",
        MacThreadStream {
            pids: pids.into_iter(),
            current: None,
            now_ms: now_epoch_ms(),
        },
    ))
}

#[cfg(target_os = "macos")]
struct MacThreadCursor {
    process: ProcessRecord,
    threads: std::vec::IntoIter<u64>,
}

#[cfg(target_os = "macos")]
struct MacThreadStream {
    pids: std::vec::IntoIter<i32>,
    current: Option<MacThreadCursor>,
    now_ms: i64,
}

#[cfg(target_os = "macos")]
impl LiveStream for MacThreadStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        loop {
            if let Some(cursor) = &mut self.current {
                for thread_id in cursor.threads.by_ref() {
                    let Some(info) = macos_thread_info(cursor.process.pid as i32, thread_id) else {
                        continue;
                    };
                    return Ok(Some(thread_record_value(
                        ThreadRecord {
                            process: cursor.process.clone(),
                            pid: cursor.process.pid,
                            thread_id: thread_id.min(i64::MAX as u64) as i64,
                            thread_name: c_chars_to_string(&info.pth_name),
                            status: macos_thread_status(info.pth_run_state),
                        },
                        self.now_ms,
                        span,
                    )));
                }
                self.current = None;
            }
            let Some(pid) = self.pids.next() else {
                return Ok(None);
            };
            let Some(process) = macos_process_record(pid) else {
                continue;
            };
            let mut threads = macos_thread_ids(pid);
            threads.sort_unstable();
            self.current = Some(MacThreadCursor {
                process,
                threads: threads.into_iter(),
            });
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_all_pids(kind: &str, span: Span) -> Result<Vec<i32>, RuntimeError> {
    use std::ffi::c_void;
    const PROC_ALL_PIDS: u32 = 1;
    let needed = unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if needed < 0 {
        return Err(
            RuntimeError::new(kind, std::io::Error::last_os_error().to_string()).with_span(span),
        );
    }
    let mut pids = vec![0i32; needed as usize / std::mem::size_of::<i32>() + 64];
    let bytes = (pids.len() * std::mem::size_of::<i32>()) as i32;
    let returned =
        unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, pids.as_mut_ptr().cast::<c_void>(), bytes) };
    if returned < 0 {
        return Err(
            RuntimeError::new(kind, std::io::Error::last_os_error().to_string()).with_span(span),
        );
    }
    let count = returned as usize / std::mem::size_of::<i32>();
    Ok(pids
        .into_iter()
        .take(count)
        .filter(|pid| *pid > 0)
        .collect())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn list_threads_stream(_pid: Option<i64>, _span: Span) -> Result<StreamValue, RuntimeError> {
    Ok(StreamValue::from_live(
        "process.threads",
        EmptyProcessStream,
    ))
}

pub(crate) fn process_stats(pid: i64, span: Span) -> Result<Value, RuntimeError> {
    if !(1..=i32::MAX as i64).contains(&pid) {
        return Err(
            RuntimeError::new("pid-range", "pid must be a positive process id").with_span(span),
        );
    }
    Ok(process_stats_record(process_stats_impl(pid, span)?))
}

pub(crate) fn port_processes(port: i64, span: Span) -> Result<StreamValue, RuntimeError> {
    if !(1..=u16::MAX as i64).contains(&port) {
        return Err(
            RuntimeError::new("port-range", "port must be between 1 and 65535").with_span(span),
        );
    }
    port_process_stream(Some(port as u16), None, span)
}

#[derive(Clone, Copy, Debug)]
struct ProcessStatsRecord {
    rss_kb: i64,
    vsz_kb: i64,
}

fn empty_process_stats() -> ProcessStatsRecord {
    ProcessStatsRecord {
        rss_kb: -1,
        vsz_kb: -1,
    }
}

fn process_stats_record(stats: ProcessStatsRecord) -> Value {
    Value::Record(RecordMap::from([
        (Arc::from("rss_kb"), Value::Int(stats.rss_kb)),
        (Arc::from("vsz_kb"), Value::Int(stats.vsz_kb)),
    ]))
}

pub(crate) fn listening_port_processes(span: Span) -> Result<StreamValue, RuntimeError> {
    port_process_stream(None, None, span)
}

pub(crate) fn pid_port_processes(pid: i64, span: Span) -> Result<StreamValue, RuntimeError> {
    if !(1..=i32::MAX as i64).contains(&pid) {
        return Err(
            RuntimeError::new("pid-range", "pid must be a positive process id").with_span(span),
        );
    }
    port_process_stream(None, Some(pid), span)
}

fn port_process_stream(
    port: Option<u16>,
    pid: Option<i64>,
    span: Span,
) -> Result<StreamValue, RuntimeError> {
    let records = port_process_records(port, pid, span)?;
    Ok(StreamValue::from_live(
        "process.ports",
        PortRecordStream {
            records: records.into_iter(),
        },
    ))
}

struct PortRecordStream {
    records: std::vec::IntoIter<PortProcessRecord>,
}

impl LiveStream for PortRecordStream {
    fn next(&mut self, _span: Span) -> Result<Option<Value>, RuntimeError> {
        Ok(self.records.next().map(port_process_record_value))
    }
}

#[cfg(target_os = "linux")]
fn port_process_records(
    port: Option<u16>,
    pid: Option<i64>,
    span: Span,
) -> Result<Vec<PortProcessRecord>, RuntimeError> {
    let sockets = linux_port_sockets(port, span)?;
    let target_inodes = sockets
        .iter()
        .filter_map(|socket| socket.inode)
        .collect::<rustc_hash::FxHashSet<_>>();
    if target_inodes.is_empty() {
        return Ok(Vec::new());
    }
    let owners = match pid {
        Some(pid) => linux_socket_owners_for_pid(pid, &target_inodes),
        None => linux_socket_owners(&target_inodes, span)?,
    };
    let boot_time_ms = linux_boot_time_ms(span)?;
    let ticks_per_second = match rustix::param::clock_ticks_per_second() {
        0 => 100,
        ticks => ticks as i64,
    };
    let mut records = Vec::new();
    for socket in sockets {
        let Some(inode) = socket.inode else {
            continue;
        };
        let Some(socket_owners) = owners.get(&inode) else {
            continue;
        };
        for owner in socket_owners {
            let proc_path = std::path::PathBuf::from(format!("/proc/{}", owner.pid));
            let uid = std::fs::metadata(&proc_path)
                .map(|metadata| metadata.uid())
                .unwrap_or(0);
            let Some(process) =
                linux_process_record(owner.pid, &proc_path, boot_time_ms, ticks_per_second, uid)
            else {
                continue;
            };
            records.push(PortProcessRecord {
                pid: owner.pid,
                fd: owner.fd,
                inode: inode.min(i64::MAX as u64) as i64,
                protocol: socket.protocol,
                local_address: socket.local_address.clone(),
                local_port: socket.local_port,
                remote_address: socket.remote_address.clone(),
                remote_port: socket.remote_port,
                state: socket.state.clone(),
                process,
            });
        }
    }
    records.sort_unstable_by_key(|record| {
        (
            record.pid,
            protocol_name(record.protocol).to_string(),
            record.fd,
        )
    });
    Ok(records)
}

#[cfg(target_os = "macos")]
fn port_process_records(
    port: Option<u16>,
    pid: Option<i64>,
    span: Span,
) -> Result<Vec<PortProcessRecord>, RuntimeError> {
    if let Some(pid) = pid {
        return Ok(macos_port_process_records(pid as i32, port));
    }
    let pids = macos_all_pids("process-port", span)?;
    Ok(macos_port_process_records_parallel(&pids, port))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn port_process_records(
    _port: Option<u16>,
    _pid: Option<i64>,
    _span: Span,
) -> Result<Vec<PortProcessRecord>, RuntimeError> {
    Ok(Vec::new())
}

pub(crate) fn argv_words(text: &str, span: Span) -> Result<Vec<String>, RuntimeError> {
    let mut parser = ArgvWordsParser::new(text, span);
    parser.parse()
}

pub(crate) fn signal_info(signal: &str, span: Span) -> Result<SignalInfo, RuntimeError> {
    let signal = signal.trim();
    if signal.is_empty() {
        return Err(RuntimeError::new("invalid-signal", "signal cannot be empty").with_span(span));
    }
    if let Ok(number) = signal.parse::<i32>() {
        if (0..=128).contains(&number) {
            return Ok(SignalInfo {
                name: signal_name(number)
                    .map(str::to_string)
                    .unwrap_or_else(|| number.to_string()),
                number,
            });
        }
        return Err(
            RuntimeError::new("invalid-signal", "signal number is out of range").with_span(span),
        );
    }

    let upper = signal.to_ascii_uppercase();
    let name = upper.strip_prefix("SIG").unwrap_or(&upper);
    for (candidate, number) in SIGNALS {
        if *candidate == name {
            return Ok(SignalInfo {
                name: candidate.to_string(),
                number: *number,
            });
        }
    }
    Err(RuntimeError::new("invalid-signal", "unknown signal").with_span(span))
}

pub(crate) fn signal_record(signal: SignalInfo) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("name"), Value::Str(signal.name.into())),
        (Arc::from("number"), Value::Int(signal.number as i64)),
    ]))
}

const SIGNALS: &[(&str, i32)] = &[
    ("HUP", libc::SIGHUP),
    ("INT", libc::SIGINT),
    ("QUIT", libc::SIGQUIT),
    ("ILL", libc::SIGILL),
    ("TRAP", libc::SIGTRAP),
    ("ABRT", libc::SIGABRT),
    ("FPE", libc::SIGFPE),
    ("KILL", libc::SIGKILL),
    ("BUS", libc::SIGBUS),
    ("SEGV", libc::SIGSEGV),
    ("SYS", libc::SIGSYS),
    ("PIPE", libc::SIGPIPE),
    ("ALRM", libc::SIGALRM),
    ("TERM", libc::SIGTERM),
    ("URG", libc::SIGURG),
    ("STOP", libc::SIGSTOP),
    ("TSTP", libc::SIGTSTP),
    ("CONT", libc::SIGCONT),
    ("CHLD", libc::SIGCHLD),
    ("TTIN", libc::SIGTTIN),
    ("TTOU", libc::SIGTTOU),
    ("IO", libc::SIGIO),
    ("XCPU", libc::SIGXCPU),
    ("XFSZ", libc::SIGXFSZ),
    ("VTALRM", libc::SIGVTALRM),
    ("PROF", libc::SIGPROF),
    ("USR1", libc::SIGUSR1),
    ("USR2", libc::SIGUSR2),
];

fn signal_name(number: i32) -> Option<&'static str> {
    SIGNALS
        .iter()
        .find_map(|(name, candidate)| (*candidate == number).then_some(*name))
}

struct ArgvWordsParser<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    span: Span,
}

impl<'a> ArgvWordsParser<'a> {
    fn new(text: &'a str, span: Span) -> Self {
        Self {
            chars: text.char_indices().peekable(),
            span,
        }
    }

    fn parse(&mut self) -> Result<Vec<String>, RuntimeError> {
        let mut words = Vec::new();
        loop {
            self.skip_whitespace();
            if self.chars.peek().is_none() {
                break;
            }
            words.push(self.parse_word()?);
        }
        Ok(words)
    }

    fn parse_word(&mut self) -> Result<String, RuntimeError> {
        let mut word = String::new();
        while let Some((_, ch)) = self.chars.peek().copied() {
            if ch.is_whitespace() {
                break;
            }
            self.chars.next();
            match ch {
                '\'' => self.parse_single_quoted(&mut word)?,
                '"' => self.parse_double_quoted(&mut word)?,
                '\\' => self.parse_escape(&mut word, false)?,
                ch if shell_syntax_char(ch) => return Err(self.shell_syntax_error(ch)),
                ch => word.push(ch),
            }
        }
        Ok(word)
    }

    fn parse_single_quoted(&mut self, word: &mut String) -> Result<(), RuntimeError> {
        for (_, ch) in self.chars.by_ref() {
            if ch == '\'' {
                return Ok(());
            }
            word.push(ch);
        }
        Err(RuntimeError::new("argv-words", "unterminated single quote").with_span(self.span))
    }

    fn parse_double_quoted(&mut self, word: &mut String) -> Result<(), RuntimeError> {
        while let Some((_, ch)) = self.chars.next() {
            match ch {
                '"' => return Ok(()),
                '\\' => self.parse_escape(word, true)?,
                '$' | '`' => return Err(self.shell_syntax_error(ch)),
                ch => word.push(ch),
            }
        }
        Err(RuntimeError::new("argv-words", "unterminated double quote").with_span(self.span))
    }

    fn parse_escape(&mut self, word: &mut String, quoted: bool) -> Result<(), RuntimeError> {
        let Some((_, ch)) = self.chars.next() else {
            return Err(RuntimeError::new("argv-words", "trailing escape").with_span(self.span));
        };
        if !quoted && shell_syntax_char(ch) {
            return Err(self.shell_syntax_error(ch));
        }
        word.push(ch);
        Ok(())
    }

    fn skip_whitespace(&mut self) {
        while self.chars.peek().is_some_and(|(_, ch)| ch.is_whitespace()) {
            self.chars.next();
        }
    }

    fn shell_syntax_error(&self, ch: char) -> RuntimeError {
        RuntimeError::new(
            "argv-words",
            format!("shell syntax character `{ch}` is not accepted"),
        )
        .with_span(self.span)
    }
}

fn shell_syntax_char(ch: char) -> bool {
    matches!(
        ch,
        '|' | '<' | '>' | ';' | '&' | '$' | '`' | '*' | '?' | '[' | ']' | '(' | ')' | '{' | '}'
    )
}

#[derive(Clone, Debug)]
struct ProcessRecord {
    pid: i64,
    parent_pid: i64,
    command: String,
    argv: String,
    argv0: String,
    user: String,
    uid: i64,
    status: String,
    start_time_ms: i64,
}

#[derive(Clone, Debug)]
struct ThreadRecord {
    process: ProcessRecord,
    pid: i64,
    thread_id: i64,
    thread_name: String,
    status: String,
}

#[derive(Clone, Debug)]
struct PortProcessRecord {
    pid: i64,
    fd: i64,
    inode: i64,
    protocol: SocketProtocol,
    local_address: String,
    local_port: u16,
    remote_address: String,
    remote_port: u16,
    state: String,
    process: ProcessRecord,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SocketProtocol {
    Tcp,
    Tcp6,
    Udp,
    Udp6,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug)]
struct SocketRecord {
    protocol: SocketProtocol,
    local_address: String,
    local_port: u16,
    remote_address: String,
    remote_port: u16,
    state: String,
    inode: Option<u64>,
}

fn process_record_value(record: ProcessRecord, now_ms: i64, span: Span) -> Value {
    let runtime_seconds = if record.start_time_ms > 0 {
        (now_ms.saturating_sub(record.start_time_ms) / 1000).max(0)
    } else {
        0
    };
    let start_time = if record.start_time_ms > 0 {
        format_epoch_ms(record.start_time_ms, "%Y-%m-%dT%H:%M:%SZ", true, span).unwrap_or_default()
    } else {
        String::new()
    };
    Value::Record(RecordMap::shaped(
        &PROCESS_SHAPE,
        vec![
            Value::Str(record.argv.into()),
            Value::Str(record.argv0.into()),
            Value::Str(record.command.into()),
            Value::Int(record.parent_pid),
            Value::Int(record.pid),
            Value::Int(runtime_seconds),
            Value::Str(start_time.into()),
            Value::Int(record.start_time_ms),
            Value::Str(record.status.into()),
            Value::Int(record.uid),
            Value::Str(record.user.into()),
        ],
    ))
}

fn thread_record_value(record: ThreadRecord, now_ms: i64, span: Span) -> Value {
    let runtime_seconds = if record.process.start_time_ms > 0 {
        (now_ms.saturating_sub(record.process.start_time_ms) / 1000).max(0)
    } else {
        0
    };
    let start_time = if record.process.start_time_ms > 0 {
        format_epoch_ms(
            record.process.start_time_ms,
            "%Y-%m-%dT%H:%M:%SZ",
            true,
            span,
        )
        .unwrap_or_default()
    } else {
        String::new()
    };
    Value::Record(RecordMap::shaped(
        &PROCESS_THREAD_SHAPE,
        vec![
            Value::Str(record.process.argv.into()),
            Value::Str(record.process.argv0.into()),
            Value::Str(record.process.command.into()),
            Value::Int(record.process.pid),
            Value::Int(record.process.parent_pid),
            Value::Int(record.pid),
            Value::Int(runtime_seconds),
            Value::Str(start_time.into()),
            Value::Int(record.process.start_time_ms),
            Value::Str(record.status.into()),
            Value::Int(record.thread_id),
            Value::Str(record.thread_name.into()),
            Value::Int(record.process.uid),
            Value::Str(record.process.user.into()),
        ],
    ))
}

fn port_process_record_value(record: PortProcessRecord) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (K_PID.clone(), Value::Int(record.pid)),
        (K_PARENT_PID.clone(), Value::Int(record.process.parent_pid)),
        (
            K_COMMAND.clone(),
            Value::Str(record.process.command.as_str().into()),
        ),
        (
            K_ARGV.clone(),
            Value::Str(record.process.argv.as_str().into()),
        ),
        (
            K_ARGV0.clone(),
            Value::Str(record.process.argv0.as_str().into()),
        ),
        (
            K_USER.clone(),
            Value::Str(record.process.user.as_str().into()),
        ),
        (K_UID.clone(), Value::Int(record.process.uid)),
        (
            Arc::from("protocol"),
            Value::Str(protocol_name(record.protocol).into()),
        ),
        (
            Arc::from("local_address"),
            Value::Str(record.local_address.as_str().into()),
        ),
        (
            Arc::from("local_port"),
            Value::Int(record.local_port as i64),
        ),
        (
            Arc::from("local"),
            Value::Str(format_endpoint(&record.local_address, record.local_port).into()),
        ),
        (
            Arc::from("remote_address"),
            Value::Str(record.remote_address.as_str().into()),
        ),
        (
            Arc::from("remote_port"),
            Value::Int(record.remote_port as i64),
        ),
        (
            Arc::from("remote"),
            Value::Str(format_endpoint(&record.remote_address, record.remote_port).into()),
        ),
        (Arc::from("state"), Value::Str(record.state.into())),
        (Arc::from("fd"), Value::Int(record.fd)),
        (Arc::from("inode"), Value::Int(record.inode)),
    ]))
}

fn protocol_name(protocol: SocketProtocol) -> &'static str {
    match protocol {
        SocketProtocol::Tcp => "tcp",
        SocketProtocol::Tcp6 => "tcp6",
        SocketProtocol::Udp => "udp",
        SocketProtocol::Udp6 => "udp6",
    }
}

fn format_endpoint(address: &str, port: u16) -> String {
    if port == 0 {
        if address.contains(':') {
            format!("[{address}]:*")
        } else {
            format!("{address}:*")
        }
    } else if address.contains(':') {
        format!("[{address}]:{port}")
    } else {
        format!("{address}:{port}")
    }
}

fn sort_port_records(records: &mut [Value]) {
    records.sort_unstable_by_key(|record| match record {
        Value::Record(fields) => (
            record_int_field(fields, "pid"),
            record_str_field(fields, "protocol"),
            record_int_field(fields, "fd"),
        ),
        _ => (i64::MAX, String::new(), i64::MAX),
    });
}

fn sort_thread_records(records: &mut [Value]) {
    records.sort_unstable_by_key(|record| match record {
        Value::Record(fields) => (
            record_int_field(fields, "owner_pid"),
            record_int_field(fields, "thread_id"),
        ),
        _ => (i64::MAX, i64::MAX),
    });
}

fn record_int_field(fields: &crate::runtime::value::RecordMap, name: &str) -> i64 {
    match fields.get(name) {
        Some(Value::Int(value)) => *value,
        _ => i64::MAX,
    }
}

fn record_str_field(fields: &crate::runtime::value::RecordMap, name: &str) -> String {
    match fields.get(name) {
        Some(Value::Str(value)) => value.to_string(),
        _ => String::new(),
    }
}

fn display_argv(parts: &[String], fallback: &str) -> (String, String) {
    if parts.is_empty() {
        return (fallback.to_string(), fallback.to_string());
    }
    (parts.join(" "), parts[0].clone())
}

fn uid_name(uid: u32) -> String {
    name_for_uid(uid).unwrap_or_else(|| uid.to_string())
}

#[cfg(target_os = "linux")]
fn list_processes_impl(span: Span) -> Result<Vec<Value>, RuntimeError> {
    let now_ms = now_epoch_ms();
    let boot_time_ms = linux_boot_time_ms(span)?;
    let ticks_per_second = match rustix::param::clock_ticks_per_second() {
        0 => 100,
        ticks => ticks as i64,
    };

    let mut records = Vec::new();
    let entries = std::fs::read_dir("/proc")
        .map_err(|error| RuntimeError::new("process-list", error.to_string()).with_span(span))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid_text) = name.to_str() else {
            continue;
        };
        let Ok(pid) = pid_text.parse::<i64>() else {
            continue;
        };
        let uid = entry.metadata().map(|m| m.uid()).unwrap_or(0);
        let Some(record) =
            linux_process_record(pid, &entry.path(), boot_time_ms, ticks_per_second, uid)
        else {
            continue;
        };
        records.push(process_record_value(record, now_ms, span));
    }
    records.sort_unstable_by_key(|record| match record {
        Value::Record(fields) => match fields.get("pid") {
            Some(Value::Int(pid)) => *pid,
            _ => i64::MAX,
        },
        _ => i64::MAX,
    });
    Ok(records)
}

#[cfg(target_os = "linux")]
fn process_stats_impl(pid: i64, span: Span) -> Result<ProcessStatsRecord, RuntimeError> {
    let statm = std::fs::read_to_string(format!("/proc/{pid}/statm"));
    let Ok(statm) = statm else {
        return Ok(empty_process_stats());
    };
    let fields = statm.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 2 {
        return Ok(empty_process_stats());
    }
    let page_kb = match rustix::param::page_size() {
        0 => 4,
        page_size => page_size as i64 / 1024,
    };
    let pages = fields[0]
        .parse::<i64>()
        .map_err(|error| RuntimeError::new("process-stats", error.to_string()).with_span(span))?;
    let resident = fields[1]
        .parse::<i64>()
        .map_err(|error| RuntimeError::new("process-stats", error.to_string()).with_span(span))?;
    Ok(ProcessStatsRecord {
        rss_kb: resident.saturating_mul(page_kb),
        vsz_kb: pages.saturating_mul(page_kb),
    })
}

#[cfg(target_os = "linux")]
fn list_threads_impl(pid: Option<i64>, span: Span) -> Result<Vec<Value>, RuntimeError> {
    let now_ms = now_epoch_ms();
    let boot_time_ms = linux_boot_time_ms(span)?;
    let ticks_per_second = match rustix::param::clock_ticks_per_second() {
        0 => 100,
        ticks => ticks as i64,
    };

    let mut records = Vec::new();
    if let Some(owner_pid) = pid {
        let process_path = std::path::PathBuf::from(format!("/proc/{owner_pid}"));
        let uid = std::fs::metadata(&process_path)
            .map(|m| m.uid())
            .unwrap_or(0);
        append_linux_thread_records(
            &mut records,
            owner_pid,
            &process_path,
            uid,
            boot_time_ms,
            ticks_per_second,
            now_ms,
            span,
        );
        sort_thread_records(&mut records);
        return Ok(records);
    }

    let entries = std::fs::read_dir("/proc")
        .map_err(|error| RuntimeError::new("process-threads", error.to_string()).with_span(span))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid_text) = name.to_str() else {
            continue;
        };
        let Ok(owner_pid) = pid_text.parse::<i64>() else {
            continue;
        };
        let process_path = entry.path();
        let uid = entry.metadata().map(|m| m.uid()).unwrap_or(0);
        append_linux_thread_records(
            &mut records,
            owner_pid,
            &process_path,
            uid,
            boot_time_ms,
            ticks_per_second,
            now_ms,
            span,
        );
    }
    sort_thread_records(&mut records);
    Ok(records)
}

#[cfg(target_os = "linux")]
fn append_linux_thread_records(
    records: &mut Vec<Value>,
    owner_pid: i64,
    process_path: &std::path::Path,
    uid: u32,
    boot_time_ms: i64,
    ticks_per_second: i64,
    now_ms: i64,
    span: Span,
) {
    let Some(process) =
        linux_process_record(owner_pid, process_path, boot_time_ms, ticks_per_second, uid)
    else {
        return;
    };
    let Ok(tasks) = std::fs::read_dir(process_path.join("task")) else {
        return;
    };
    for task in tasks.flatten() {
        let task_name = task.file_name();
        let Some(tid_text) = task_name.to_str() else {
            continue;
        };
        let Ok(tid) = tid_text.parse::<i64>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(task.path().join("stat")) else {
            continue;
        };
        let Some(parsed) = parse_linux_stat(&stat) else {
            continue;
        };
        records.push(thread_record_value(
            ThreadRecord {
                process: process.clone(),
                pid: tid,
                thread_id: tid,
                thread_name: parsed.command,
                status: parsed.status,
            },
            now_ms,
            span,
        ));
    }
}

#[cfg(target_os = "linux")]
fn port_processes_impl(
    port: Option<u16>,
    pid: Option<i64>,
    span: Span,
) -> Result<Vec<Value>, RuntimeError> {
    let sockets = linux_port_sockets(port, span)?;
    let target_inodes = sockets
        .iter()
        .filter_map(|socket| socket.inode)
        .collect::<rustc_hash::FxHashSet<_>>();
    if target_inodes.is_empty() {
        return Ok(Vec::new());
    }
    let owners = match pid {
        Some(pid) => linux_socket_owners_for_pid(pid, &target_inodes),
        None => linux_socket_owners(&target_inodes, span)?,
    };
    let boot_time_ms = linux_boot_time_ms(span)?;
    let ticks_per_second = match rustix::param::clock_ticks_per_second() {
        0 => 100,
        ticks => ticks as i64,
    };
    let mut records = Vec::new();
    for socket in sockets {
        let Some(inode) = socket.inode else {
            continue;
        };
        let Some(socket_owners) = owners.get(&inode) else {
            continue;
        };
        for owner in socket_owners {
            let proc_path = std::path::PathBuf::from(format!("/proc/{}", owner.pid));
            let uid = std::fs::metadata(&proc_path).map(|m| m.uid()).unwrap_or(0);
            let Some(process) =
                linux_process_record(owner.pid, &proc_path, boot_time_ms, ticks_per_second, uid)
            else {
                continue;
            };
            records.push(port_process_record_value(PortProcessRecord {
                pid: owner.pid,
                fd: owner.fd,
                inode: inode.min(i64::MAX as u64) as i64,
                protocol: socket.protocol,
                local_address: socket.local_address.clone(),
                local_port: socket.local_port,
                remote_address: socket.remote_address.clone(),
                remote_port: socket.remote_port,
                state: socket.state.clone(),
                process,
            }));
        }
    }
    sort_port_records(&mut records);
    Ok(records)
}

#[cfg(target_os = "linux")]
fn linux_socket_owners_for_pid(
    pid: i64,
    target_inodes: &rustc_hash::FxHashSet<u64>,
) -> rustc_hash::FxHashMap<u64, Vec<SocketOwner>> {
    let mut owners: rustc_hash::FxHashMap<u64, Vec<SocketOwner>> = rustc_hash::FxHashMap::default();
    let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
        return owners;
    };
    for fd_entry in fds.flatten() {
        let fd_name = fd_entry.file_name();
        let Some(fd_name) = fd_name.to_str() else {
            continue;
        };
        let Ok(fd) = fd_name.parse::<i64>() else {
            continue;
        };
        let Ok(target) = std::fs::read_link(fd_entry.path()) else {
            continue;
        };
        let Some(inode) = linux_socket_inode(&target) else {
            continue;
        };
        if !target_inodes.contains(&inode) {
            continue;
        }
        owners
            .entry(inode)
            .or_default()
            .push(SocketOwner { pid, fd });
    }
    owners
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug)]
struct SocketOwner {
    pid: i64,
    fd: i64,
}

#[cfg(target_os = "linux")]
fn linux_port_sockets(port: Option<u16>, span: Span) -> Result<Vec<SocketRecord>, RuntimeError> {
    let mut sockets = Vec::new();
    for (protocol, path) in [
        (SocketProtocol::Tcp, "/proc/net/tcp"),
        (SocketProtocol::Tcp6, "/proc/net/tcp6"),
        (SocketProtocol::Udp, "/proc/net/udp"),
        (SocketProtocol::Udp6, "/proc/net/udp6"),
    ] {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(RuntimeError::new("process-port", error.to_string()).with_span(span));
            }
        };
        for line in text.lines().skip(1).filter(|line| !line.trim().is_empty()) {
            let socket = parse_linux_socket_line(protocol, line, span)?;
            let port_matches = port.is_none_or(|port| socket.local_port == port);
            let listener = socket.state == "LISTEN"
                || matches!(socket.protocol, SocketProtocol::Udp | SocketProtocol::Udp6);
            if port_matches && listener && socket.local_port > 0 {
                sockets.push(socket);
            }
        }
    }
    Ok(sockets)
}

#[cfg(target_os = "linux")]
fn parse_linux_socket_line(
    protocol: SocketProtocol,
    line: &str,
    span: Span,
) -> Result<SocketRecord, RuntimeError> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 10 {
        return Err(
            RuntimeError::new("process-port", "malformed socket table entry").with_span(span),
        );
    }
    let (local_address, local_port) = parse_linux_socket_endpoint(protocol, fields[1], span)?;
    let (remote_address, remote_port) = parse_linux_socket_endpoint(protocol, fields[2], span)?;
    let state = parse_linux_socket_state(protocol, fields[3], span)?;
    Ok(SocketRecord {
        protocol,
        local_address,
        local_port,
        remote_address,
        remote_port,
        state,
        inode: fields[9].parse::<u64>().ok(),
    })
}

#[cfg(target_os = "linux")]
fn parse_linux_socket_endpoint(
    protocol: SocketProtocol,
    value: &str,
    span: Span,
) -> Result<(String, u16), RuntimeError> {
    let (address, port) = value.split_once(':').ok_or_else(|| {
        RuntimeError::new("process-port", "malformed socket address").with_span(span)
    })?;
    let port = u16::from_str_radix(port, 16)
        .map_err(|_| RuntimeError::new("process-port", "invalid socket port").with_span(span))?;
    let address = match protocol {
        SocketProtocol::Tcp | SocketProtocol::Udp => parse_linux_ipv4_hex(address, span)?,
        SocketProtocol::Tcp6 | SocketProtocol::Udp6 => parse_linux_ipv6_hex(address, span)?,
    };
    Ok((address, port))
}

#[cfg(target_os = "linux")]
fn parse_linux_ipv4_hex(value: &str, span: Span) -> Result<String, RuntimeError> {
    if value.len() != 8 {
        return Err(
            RuntimeError::new("process-port", "invalid IPv4 socket address").with_span(span),
        );
    }
    let raw = u32::from_str_radix(value, 16).map_err(|_| {
        RuntimeError::new("process-port", "invalid IPv4 socket address").with_span(span)
    })?;
    Ok(std::net::Ipv4Addr::from(raw.to_le_bytes()).to_string())
}

#[cfg(target_os = "linux")]
fn parse_linux_ipv6_hex(value: &str, span: Span) -> Result<String, RuntimeError> {
    if value.len() != 32 {
        return Err(
            RuntimeError::new("process-port", "invalid IPv6 socket address").with_span(span),
        );
    }
    let mut bytes = [0_u8; 16];
    for (chunk_index, chunk) in value.as_bytes().chunks(8).enumerate() {
        let word = std::str::from_utf8(chunk).map_err(|_| {
            RuntimeError::new("process-port", "invalid IPv6 socket address").with_span(span)
        })?;
        let raw = u32::from_str_radix(word, 16).map_err(|_| {
            RuntimeError::new("process-port", "invalid IPv6 socket address").with_span(span)
        })?;
        bytes[chunk_index * 4..chunk_index * 4 + 4].copy_from_slice(&raw.to_le_bytes());
    }
    Ok(std::net::Ipv6Addr::from(bytes).to_string())
}

#[cfg(target_os = "linux")]
fn parse_linux_socket_state(
    protocol: SocketProtocol,
    value: &str,
    span: Span,
) -> Result<String, RuntimeError> {
    match protocol {
        SocketProtocol::Udp | SocketProtocol::Udp6 => Ok(String::new()),
        SocketProtocol::Tcp | SocketProtocol::Tcp6 => Ok(match value {
            "01" => "ESTABLISHED",
            "02" => "SYN_SENT",
            "03" => "SYN_RECV",
            "04" => "FIN_WAIT1",
            "05" => "FIN_WAIT2",
            "06" => "TIME_WAIT",
            "07" => "CLOSE",
            "08" => "CLOSE_WAIT",
            "09" => "LAST_ACK",
            "0A" => "LISTEN",
            "0B" => "CLOSING",
            _ => {
                return Err(
                    RuntimeError::new("process-port", "unknown TCP socket state").with_span(span),
                );
            }
        }
        .to_string()),
    }
}

#[cfg(target_os = "linux")]
fn linux_socket_owners(
    target_inodes: &rustc_hash::FxHashSet<u64>,
    span: Span,
) -> Result<rustc_hash::FxHashMap<u64, Vec<SocketOwner>>, RuntimeError> {
    let mut owners: rustc_hash::FxHashMap<u64, Vec<SocketOwner>> = rustc_hash::FxHashMap::default();
    let entries = std::fs::read_dir("/proc")
        .map_err(|error| RuntimeError::new("process-port", error.to_string()).with_span(span))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Ok(pid) = name.parse::<i64>() else {
            continue;
        };
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        for fd_entry in fds.flatten() {
            let fd_name = fd_entry.file_name();
            let Some(fd_name) = fd_name.to_str() else {
                continue;
            };
            let Ok(fd) = fd_name.parse::<i64>() else {
                continue;
            };
            let Ok(target) = std::fs::read_link(fd_entry.path()) else {
                continue;
            };
            let Some(inode) = linux_socket_inode(&target) else {
                continue;
            };
            if !target_inodes.contains(&inode) {
                continue;
            }
            owners
                .entry(inode)
                .or_default()
                .push(SocketOwner { pid, fd });
        }
    }
    Ok(owners)
}

#[cfg(target_os = "linux")]
fn linux_socket_inode(path: &std::path::Path) -> Option<u64> {
    let text = path.to_str()?;
    text.strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse::<u64>()
        .ok()
}

#[cfg(target_os = "linux")]
fn linux_process_record(
    pid: i64,
    path: &std::path::Path,
    boot_time_ms: i64,
    ticks_per_second: i64,
    uid: u32,
) -> Option<ProcessRecord> {
    let stat = std::fs::read_to_string(path.join("stat")).ok()?;
    let parsed = parse_linux_stat(&stat)?;
    let is_kernel_thread = parsed.command.starts_with('[') && parsed.command.ends_with(']');
    let argv_parts = if is_kernel_thread {
        Vec::new()
    } else {
        linux_cmdline(&path.join("cmdline")).unwrap_or_default()
    };
    let command = if parsed.command.is_empty() {
        argv_parts
            .first()
            .and_then(|argv0| std::path::Path::new(argv0).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string()
    } else {
        parsed.command
    };
    let (argv, argv0) = display_argv(&argv_parts, &command);
    let start_time_ms = boot_time_ms.saturating_add(
        parsed
            .start_ticks
            .saturating_mul(1000)
            .saturating_div(ticks_per_second),
    );
    Some(ProcessRecord {
        pid,
        parent_pid: parsed.parent_pid,
        command,
        argv,
        argv0,
        user: uid_name(uid),
        uid: uid as i64,
        status: parsed.status,
        start_time_ms,
    })
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug)]
struct LinuxStat {
    command: String,
    status: String,
    parent_pid: i64,
    start_ticks: i64,
}

#[cfg(target_os = "linux")]
fn parse_linux_stat(stat: &str) -> Option<LinuxStat> {
    let open = stat.find('(')?;
    let close = stat.rfind(") ")?;
    let command = stat[open + 1..close].to_string();
    let rest = &stat[close + 2..];
    let fields = rest.split_whitespace().collect::<Vec<_>>();
    let status = fields.first()?.to_string();
    let parent_pid = fields.get(1)?.parse::<i64>().ok()?;
    let start_ticks = fields.get(19)?.parse::<i64>().ok()?;
    Some(LinuxStat {
        command,
        status,
        parent_pid,
        start_ticks,
    })
}

#[cfg(target_os = "linux")]
fn linux_cmdline(path: &std::path::Path) -> Option<Vec<String>> {
    let bytes = std::fs::read(path).ok()?;
    Some(
        bytes
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect(),
    )
}

#[cfg(target_os = "linux")]
fn linux_boot_time_ms(span: Span) -> Result<i64, RuntimeError> {
    let stat = std::fs::read_to_string("/proc/stat")
        .map_err(|error| RuntimeError::new("process-list", error.to_string()).with_span(span))?;
    for line in stat.lines() {
        if let Some(rest) = line.strip_prefix("btime ") {
            let seconds = rest.trim().parse::<i64>().map_err(|_| {
                RuntimeError::new("process-list", "could not parse /proc/stat btime")
                    .with_span(span)
            })?;
            return Ok(seconds.saturating_mul(1000));
        }
    }
    Err(RuntimeError::new("process-list", "could not find /proc/stat btime").with_span(span))
}

#[cfg(target_os = "macos")]
fn list_processes_impl(span: Span) -> Result<Vec<Value>, RuntimeError> {
    use std::ffi::c_void;

    const PROC_ALL_PIDS: u32 = 1;
    let needed = unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if needed < 0 {
        return Err(
            RuntimeError::new("process-list", std::io::Error::last_os_error().to_string())
                .with_span(span),
        );
    }
    let mut pids = vec![0i32; needed as usize / std::mem::size_of::<i32>() + 64];
    let bytes = (pids.len() * std::mem::size_of::<i32>()) as i32;
    let returned =
        unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, pids.as_mut_ptr().cast::<c_void>(), bytes) };
    if returned < 0 {
        return Err(
            RuntimeError::new("process-list", std::io::Error::last_os_error().to_string())
                .with_span(span),
        );
    }
    let now_ms = now_epoch_ms();
    let count = returned as usize / std::mem::size_of::<i32>();
    let mut records = Vec::new();
    for pid in pids.into_iter().take(count).filter(|pid| *pid > 0) {
        if let Some(record) = macos_process_record(pid) {
            records.push(process_record_value(record, now_ms, span));
        }
    }
    records.sort_unstable_by_key(|record| match record {
        Value::Record(fields) => match fields.get("pid") {
            Some(Value::Int(pid)) => *pid,
            _ => i64::MAX,
        },
        _ => i64::MAX,
    });
    Ok(records)
}

#[cfg(target_os = "macos")]
fn list_threads_impl(pid: Option<i64>, span: Span) -> Result<Vec<Value>, RuntimeError> {
    use std::ffi::c_void;

    let now_ms = now_epoch_ms();
    let mut records = Vec::new();
    if let Some(pid) = pid {
        append_macos_thread_records(&mut records, pid as i32, now_ms, span);
        sort_thread_records(&mut records);
        return Ok(records);
    }

    const PROC_ALL_PIDS: u32 = 1;
    let needed = unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if needed < 0 {
        return Err(RuntimeError::new(
            "process-threads",
            std::io::Error::last_os_error().to_string(),
        )
        .with_span(span));
    }
    let mut pids = vec![0i32; needed as usize / std::mem::size_of::<i32>() + 64];
    let bytes = (pids.len() * std::mem::size_of::<i32>()) as i32;
    let returned =
        unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, pids.as_mut_ptr().cast::<c_void>(), bytes) };
    if returned < 0 {
        return Err(RuntimeError::new(
            "process-threads",
            std::io::Error::last_os_error().to_string(),
        )
        .with_span(span));
    }

    let count = returned as usize / std::mem::size_of::<i32>();
    for pid in pids.into_iter().take(count).filter(|pid| *pid > 0) {
        append_macos_thread_records(&mut records, pid, now_ms, span);
    }
    sort_thread_records(&mut records);
    Ok(records)
}

#[cfg(target_os = "macos")]
fn process_stats_impl(pid: i64, _span: Span) -> Result<ProcessStatsRecord, RuntimeError> {
    use std::ffi::c_void;

    const PROC_PIDTASKINFO: i32 = 4;
    let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_taskinfo>() as i32;
    let returned = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            PROC_PIDTASKINFO,
            0,
            (&mut info as *mut libc::proc_taskinfo).cast::<c_void>(),
            size,
        )
    };
    if returned != size {
        return Ok(empty_process_stats());
    }
    Ok(ProcessStatsRecord {
        rss_kb: (info.pti_resident_size / 1024).min(i64::MAX as u64) as i64,
        vsz_kb: (info.pti_virtual_size / 1024).min(i64::MAX as u64) as i64,
    })
}

#[cfg(target_os = "macos")]
fn append_macos_thread_records(records: &mut Vec<Value>, pid: i32, now_ms: i64, span: Span) {
    let Some(process) = macos_process_record(pid) else {
        return;
    };
    for thread_id in macos_thread_ids(pid) {
        let Some(info) = macos_thread_info(pid, thread_id) else {
            continue;
        };
        records.push(thread_record_value(
            ThreadRecord {
                process: process.clone(),
                pid: pid as i64,
                thread_id: thread_id.min(i64::MAX as u64) as i64,
                thread_name: c_chars_to_string(&info.pth_name),
                status: macos_thread_status(info.pth_run_state),
            },
            now_ms,
            span,
        ));
    }
}

#[cfg(target_os = "macos")]
fn port_processes_impl(
    port: Option<u16>,
    pid: Option<i64>,
    span: Span,
) -> Result<Vec<Value>, RuntimeError> {
    use std::ffi::c_void;

    if let Some(pid) = pid {
        let mut records = Vec::new();
        append_macos_port_process_records(&mut records, pid as i32, port);
        sort_port_records(&mut records);
        return Ok(records);
    }

    const PROC_ALL_PIDS: u32 = 1;
    let needed = unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if needed < 0 {
        return Err(
            RuntimeError::new("process-port", std::io::Error::last_os_error().to_string())
                .with_span(span),
        );
    }
    let mut pids = vec![0i32; needed as usize / std::mem::size_of::<i32>() + 64];
    let bytes = (pids.len() * std::mem::size_of::<i32>()) as i32;
    let returned =
        unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, pids.as_mut_ptr().cast::<c_void>(), bytes) };
    if returned < 0 {
        return Err(
            RuntimeError::new("process-port", std::io::Error::last_os_error().to_string())
                .with_span(span),
        );
    }

    let count = returned as usize / std::mem::size_of::<i32>();
    let pids = pids
        .into_iter()
        .take(count)
        .filter(|pid| *pid > 0)
        .collect::<Vec<_>>();
    let mut records = macos_port_process_records_parallel(&pids, port)
        .into_iter()
        .map(port_process_record_value)
        .collect::<Vec<_>>();
    sort_port_records(&mut records);
    Ok(records)
}

#[cfg(target_os = "macos")]
fn append_macos_port_process_records(records: &mut Vec<Value>, pid: i32, port: Option<u16>) {
    records.extend(
        macos_port_process_records(pid, port)
            .into_iter()
            .map(port_process_record_value),
    );
}

#[cfg(target_os = "macos")]
fn macos_port_process_records_parallel(pids: &[i32], port: Option<u16>) -> Vec<PortProcessRecord> {
    let jobs = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(8)
        .min(pids.len().max(1));
    if jobs <= 1 || pids.len() <= 1 {
        return macos_port_process_records_for_pids(pids, port);
    }

    let chunk_size = pids.len().div_ceil(jobs);
    std::thread::scope(|scope| {
        let handles = pids
            .chunks(chunk_size)
            .map(|chunk| scope.spawn(move || macos_port_process_records_for_pids(chunk, port)))
            .collect::<Vec<_>>();
        let mut records = Vec::new();
        for handle in handles {
            records.extend(handle.join().expect("macOS port worker panicked"));
        }
        records
    })
}

#[cfg(target_os = "macos")]
fn macos_port_process_records_for_pids(pids: &[i32], port: Option<u16>) -> Vec<PortProcessRecord> {
    let mut records = Vec::new();
    for pid in pids {
        records.extend(macos_port_process_records(*pid, port));
    }
    records
}

#[cfg(target_os = "macos")]
fn macos_port_process_records(pid: i32, port: Option<u16>) -> Vec<PortProcessRecord> {
    let sockets = macos_process_port_sockets(pid, port);
    if sockets.is_empty() {
        return Vec::new();
    }
    let Some(process) = macos_process_record(pid) else {
        return Vec::new();
    };
    sockets
        .into_iter()
        .map(|socket| PortProcessRecord {
            pid: pid as i64,
            fd: socket.fd,
            inode: socket.inode,
            protocol: socket.protocol,
            local_address: socket.local_address,
            local_port: socket.local_port,
            remote_address: socket.remote_address,
            remote_port: socket.remote_port,
            state: socket.state,
            process: process.clone(),
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn macos_process_port_sockets(pid: i32, port: Option<u16>) -> Vec<MacPortSocket> {
    use std::ffi::c_void;

    let needed =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        return Vec::new();
    }
    let fd_count = needed as usize / std::mem::size_of::<libc::proc_fdinfo>() + 16;
    let mut fds: Vec<libc::proc_fdinfo> = vec![
        libc::proc_fdinfo {
            proc_fd: 0,
            proc_fdtype: 0,
        };
        fd_count
    ];
    let returned = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast::<c_void>(),
            (fds.len() * std::mem::size_of::<libc::proc_fdinfo>()) as i32,
        )
    };
    if returned <= 0 {
        return Vec::new();
    }
    fds.truncate(returned as usize / std::mem::size_of::<libc::proc_fdinfo>());
    let mut sockets = Vec::new();
    for fd in fds {
        if fd.proc_fdtype != libc::PROX_FDTYPE_SOCKET as u32 {
            continue;
        }
        let Some(socket) = macos_socket_for_fd(pid, fd.proc_fd, port) else {
            continue;
        };
        sockets.push(socket);
    }
    sockets
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct MacPortSocket {
    fd: i64,
    inode: i64,
    protocol: SocketProtocol,
    local_address: String,
    local_port: u16,
    remote_address: String,
    remote_port: u16,
    state: String,
}

#[cfg(target_os = "macos")]
fn macos_socket_for_fd(pid: i32, fd: i32, port: Option<u16>) -> Option<MacPortSocket> {
    use std::ffi::c_void;

    const PROC_PIDFDSOCKETINFO: i32 = 3;
    let mut buffer = vec![0u8; 2048];
    let returned = unsafe {
        libc::proc_pidfdinfo(
            pid,
            fd,
            PROC_PIDFDSOCKETINFO,
            buffer.as_mut_ptr().cast::<c_void>(),
            buffer.len() as i32,
        )
    };
    let socket_header_offset = std::mem::size_of::<MacProcFileInfo>();
    let proto_offset = socket_header_offset + std::mem::size_of::<MacSocketInfoHeader>();
    if returned < proto_offset as i32 {
        return None;
    }
    let header =
        unsafe { &*(buffer.as_ptr().add(socket_header_offset) as *const MacSocketInfoHeader) };
    let protocol = match (header.soi_family, header.soi_protocol) {
        (libc::AF_INET, libc::IPPROTO_TCP) => SocketProtocol::Tcp,
        (libc::AF_INET6, libc::IPPROTO_TCP) => SocketProtocol::Tcp6,
        (libc::AF_INET, libc::IPPROTO_UDP) => SocketProtocol::Udp,
        (libc::AF_INET6, libc::IPPROTO_UDP) => SocketProtocol::Udp6,
        _ => return None,
    };
    let (info, state) = match header.soi_kind {
        MAC_SOCKINFO_TCP
            if returned as usize >= proto_offset + std::mem::size_of::<MacTcpSockInfo>() =>
        {
            let tcp = unsafe { &*(buffer.as_ptr().add(proto_offset) as *const MacTcpSockInfo) };
            (&tcp.tcpsi_ini, macos_tcp_state(tcp.tcpsi_state).to_string())
        }
        MAC_SOCKINFO_IN
            if returned as usize >= proto_offset + std::mem::size_of::<MacInSockInfo>() =>
        {
            let info = unsafe { &*(buffer.as_ptr().add(proto_offset) as *const MacInSockInfo) };
            (info, String::new())
        }
        _ => return None,
    };
    let local_port = macos_socket_port(info.insi_lport);
    let port_matches = port.is_none_or(|port| local_port == port);
    let listener =
        state == "LISTEN" || matches!(protocol, SocketProtocol::Udp | SocketProtocol::Udp6);
    if !port_matches || !listener || local_port == 0 {
        return None;
    }
    let remote_port = macos_socket_port(info.insi_fport);
    let (local_address, remote_address) = macos_socket_addresses(protocol, info);
    Some(MacPortSocket {
        fd: fd as i64,
        inode: header.soi_so.min(i64::MAX as u64) as i64,
        protocol,
        local_address,
        local_port,
        remote_address,
        remote_port,
        state,
    })
}

#[cfg(target_os = "macos")]
fn macos_socket_port(value: i32) -> u16 {
    u16::from_be((value as u32 & 0xffff) as u16)
}

#[cfg(target_os = "macos")]
fn macos_socket_addresses(protocol: SocketProtocol, info: &MacInSockInfo) -> (String, String) {
    match protocol {
        SocketProtocol::Tcp | SocketProtocol::Udp => unsafe {
            (
                macos_ipv4_addr(info.insi_laddr.ina_46.i46a_addr4),
                macos_ipv4_addr(info.insi_faddr.ina_46.i46a_addr4),
            )
        },
        SocketProtocol::Tcp6 | SocketProtocol::Udp6 => unsafe {
            (
                macos_ipv6_addr(info.insi_laddr.ina_6),
                macos_ipv6_addr(info.insi_faddr.ina_6),
            )
        },
    }
}

#[cfg(target_os = "macos")]
fn macos_ipv4_addr(addr: libc::in_addr) -> String {
    std::net::Ipv4Addr::from(u32::from_be(addr.s_addr)).to_string()
}

#[cfg(target_os = "macos")]
fn macos_ipv6_addr(addr: libc::in6_addr) -> String {
    std::net::Ipv6Addr::from(addr.s6_addr).to_string()
}

#[cfg(target_os = "macos")]
fn macos_tcp_state(state: i32) -> &'static str {
    match state {
        0 => "CLOSED",
        1 => "LISTEN",
        2 => "SYN_SENT",
        3 => "SYN_RECV",
        4 => "ESTABLISHED",
        5 => "CLOSE_WAIT",
        6 => "FIN_WAIT1",
        7 => "CLOSING",
        8 => "LAST_ACK",
        9 => "FIN_WAIT2",
        10 => "TIME_WAIT",
        _ => "",
    }
}

#[cfg(target_os = "macos")]
const MAC_SOCKINFO_IN: i32 = 1;
#[cfg(target_os = "macos")]
const MAC_SOCKINFO_TCP: i32 = 2;

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacProcFileInfo {
    fi_openflags: u32,
    fi_status: u32,
    fi_offset: i64,
    fi_type: i32,
    fi_guardflags: u32,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacVinfoStat {
    vst_dev: u32,
    vst_mode: u16,
    vst_nlink: u16,
    vst_ino: u64,
    vst_uid: u32,
    vst_gid: u32,
    vst_atime: i64,
    vst_atimensec: i64,
    vst_mtime: i64,
    vst_mtimensec: i64,
    vst_ctime: i64,
    vst_ctimensec: i64,
    vst_birthtime: i64,
    vst_birthtimensec: i64,
    vst_size: i64,
    vst_blocks: i64,
    vst_blksize: i32,
    vst_flags: u32,
    vst_gen: u32,
    vst_rdev: u32,
    vst_qspare: [i64; 2],
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacSockbufInfo {
    sbi_cc: u32,
    sbi_hiwat: u32,
    sbi_mbcnt: u32,
    sbi_mbmax: u32,
    sbi_lowat: u32,
    sbi_flags: i16,
    sbi_timeo: i16,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacSocketInfoHeader {
    soi_stat: MacVinfoStat,
    soi_so: u64,
    soi_pcb: u64,
    soi_type: i32,
    soi_protocol: i32,
    soi_family: i32,
    soi_options: i16,
    soi_linger: i16,
    soi_state: i16,
    soi_qlen: i16,
    soi_incqlen: i16,
    soi_qlimit: i16,
    soi_timeo: i16,
    soi_error: u16,
    soi_oobmark: u32,
    soi_rcv: MacSockbufInfo,
    soi_snd: MacSockbufInfo,
    soi_kind: i32,
    rfu_1: u32,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacIn4In6Addr {
    i46a_pad32: [u32; 3],
    i46a_addr4: libc::in_addr,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
union MacInAddr {
    ina_46: MacIn4In6Addr,
    ina_6: libc::in6_addr,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacInSockV4 {
    in4_tos: u8,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacInSockV6 {
    in6_hlim: u8,
    in6_cksum: i32,
    in6_ifindex: u16,
    in6_hops: i16,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacInSockInfo {
    insi_fport: i32,
    insi_lport: i32,
    insi_gencnt: u64,
    insi_flags: u32,
    insi_flow: u32,
    insi_vflag: u8,
    insi_ip_ttl: u8,
    rfu_1: u32,
    insi_faddr: MacInAddr,
    insi_laddr: MacInAddr,
    insi_v4: MacInSockV4,
    insi_v6: MacInSockV6,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacTcpSockInfo {
    tcpsi_ini: MacInSockInfo,
    tcpsi_state: i32,
    tcpsi_timer: [i32; 4],
    tcpsi_mss: i32,
    tcpsi_flags: u32,
    rfu_1: u32,
    tcpsi_tp: u64,
}

#[cfg(target_os = "macos")]
fn macos_process_record(pid: i32) -> Option<ProcessRecord> {
    use std::ffi::c_void;

    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let returned = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast::<c_void>(),
            size,
        )
    };
    if returned != size {
        return None;
    }
    let command = first_non_empty([
        c_chars_to_string(&info.pbi_comm),
        c_chars_to_string(&info.pbi_name),
        macos_proc_path(pid)
            .and_then(|path| {
                std::path::Path::new(&path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_default(),
    ]);
    let argv_parts = macos_proc_args(pid).unwrap_or_else(|| {
        macos_proc_path(pid)
            .map(|path| vec![path])
            .unwrap_or_else(|| vec![command.clone()])
    });
    let (argv, argv0) = display_argv(&argv_parts, &command);
    let start_time_ms = (info.pbi_start_tvsec as i64)
        .saturating_mul(1000)
        .saturating_add((info.pbi_start_tvusec as i64) / 1000);
    Some(ProcessRecord {
        pid: pid as i64,
        parent_pid: info.pbi_ppid as i64,
        command,
        argv,
        argv0,
        user: uid_name(info.pbi_uid),
        uid: info.pbi_uid as i64,
        status: macos_status(info.pbi_status),
        start_time_ms,
    })
}

#[cfg(target_os = "macos")]
fn macos_thread_ids(pid: i32) -> Vec<u64> {
    use std::ffi::c_void;

    const PROC_PIDLISTTHREADS: i32 = 6;
    let mut ids = vec![0u64; 256];
    loop {
        let returned = unsafe {
            libc::proc_pidinfo(
                pid,
                PROC_PIDLISTTHREADS,
                0,
                ids.as_mut_ptr().cast::<c_void>(),
                (ids.len() * std::mem::size_of::<u64>()) as i32,
            )
        };
        if returned <= 0 {
            return Vec::new();
        }
        let count = returned as usize / std::mem::size_of::<u64>();
        if count < ids.len() {
            ids.truncate(count);
            break;
        }
        ids.resize(ids.len() * 2, 0);
    }
    ids.into_iter().filter(|id| *id > 0).collect()
}

#[cfg(target_os = "macos")]
fn macos_thread_info(pid: i32, thread_id: u64) -> Option<libc::proc_threadinfo> {
    use std::ffi::c_void;

    let mut info: libc::proc_threadinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_threadinfo>() as i32;
    let returned = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTHREADINFO,
            thread_id,
            (&mut info as *mut libc::proc_threadinfo).cast::<c_void>(),
            size,
        )
    };
    (returned == size).then_some(info)
}

#[cfg(target_os = "macos")]
fn macos_proc_path(pid: i32) -> Option<String> {
    use std::ffi::c_void;

    let mut buffer = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let returned = unsafe {
        libc::proc_pidpath(
            pid,
            buffer.as_mut_ptr().cast::<c_void>(),
            buffer.len() as u32,
        )
    };
    if returned <= 0 {
        return None;
    }
    Some(String::from_utf8_lossy(&buffer[..returned as usize]).into_owned())
}

#[cfg(target_os = "macos")]
fn macos_proc_args(pid: i32) -> Option<Vec<String>> {
    use std::ffi::c_void;

    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size = 0usize;
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || size <= std::mem::size_of::<libc::c_int>() {
        return None;
    }
    let mut buffer = vec![0u8; size];
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            buffer.as_mut_ptr().cast::<c_void>(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || size <= std::mem::size_of::<libc::c_int>() {
        return None;
    }
    buffer.truncate(size);
    let argc = i32::from_ne_bytes(buffer[..4].try_into().ok()?).max(0) as usize;
    let mut index = 4usize;
    while index < buffer.len() && buffer[index] != 0 {
        index += 1;
    }
    while index < buffer.len() && buffer[index] == 0 {
        index += 1;
    }
    let mut args = Vec::new();
    for _ in 0..argc {
        if index >= buffer.len() {
            break;
        }
        let start = index;
        while index < buffer.len() && buffer[index] != 0 {
            index += 1;
        }
        if index > start {
            args.push(String::from_utf8_lossy(&buffer[start..index]).into_owned());
        }
        while index < buffer.len() && buffer[index] == 0 {
            index += 1;
        }
    }
    (!args.is_empty()).then_some(args)
}

#[cfg(target_os = "macos")]
fn c_chars_to_string(chars: &[libc::c_char]) -> String {
    let bytes = chars
        .iter()
        .map(|ch| *ch as u8)
        .take_while(|byte| *byte != 0)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(target_os = "macos")]
fn first_non_empty(values: [String; 3]) -> String {
    values
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn macos_status(status: u32) -> String {
    match status {
        1 => "I",
        2 => "R",
        3 => "S",
        4 => "T",
        5 => "Z",
        _ => "?",
    }
    .to_string()
}

#[cfg(target_os = "macos")]
fn macos_thread_status(state: i32) -> String {
    match state {
        1 => "R".to_string(),
        2 => "S".to_string(),
        3 => "W".to_string(),
        4 => "U".to_string(),
        5 => "H".to_string(),
        _ => state.to_string(),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn list_processes_impl(span: Span) -> Result<Vec<Value>, RuntimeError> {
    Err(RuntimeError::new(
        "unsupported-platform",
        "process.list is implemented on Linux and macOS",
    )
    .with_span(span))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn list_threads_impl(_pid: Option<i64>, span: Span) -> Result<Vec<Value>, RuntimeError> {
    Err(RuntimeError::new(
        "unsupported-platform",
        "process.threads is implemented on Linux and macOS",
    )
    .with_span(span))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_stats_impl(_pid: i64, _span: Span) -> Result<ProcessStatsRecord, RuntimeError> {
    Ok(empty_process_stats())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn port_processes_impl(
    _port: Option<u16>,
    _pid: Option<i64>,
    span: Span,
) -> Result<Vec<Value>, RuntimeError> {
    Err(RuntimeError::new(
        "unsupported-platform",
        "process.port and process.ports are implemented on Linux and macOS",
    )
    .with_span(span))
}

#[cfg(test)]
mod tests {
    use super::{Span, argv_words};
    use crate::source::SourceId;

    fn span() -> Span {
        Span::new(SourceId::new(0), 0, 0)
    }

    #[test]
    fn argv_words_parses_quotes_and_escapes() {
        assert_eq!(
            argv_words(
                "cmd 'two words' \"double quoted\" escaped\\ space 'literal *'",
                span()
            )
            .unwrap(),
            vec![
                "cmd",
                "two words",
                "double quoted",
                "escaped space",
                "literal *"
            ]
        );
    }

    #[test]
    fn argv_words_rejects_shell_syntax() {
        for text in [
            "echo hi | wc",
            "echo $HOME",
            "echo *",
            "echo $(date)",
            "echo `date`",
            "echo > file",
            "unterminated 'quote",
        ] {
            let error = argv_words(text, span()).unwrap_err();
            assert_eq!(error.kind, "argv-words");
        }
    }
}
