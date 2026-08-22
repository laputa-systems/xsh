//! Evaluator-owned transport driver.
//!
//! The driver owns only plain transport requests and results. It deliberately
//! does not know about XSH values, source spans, scopes, signal hooks, or trace
//! buffers; those remain in the evaluator that owns this runtime.

use super::{NetAgent, NetDownload, NetError, NetRequest, NetResponse, NetResult, NetUpload};
use async_executor::{Executor, Task};
use async_io::{Async, Timer};
use crossbeam_channel::{Receiver, Select, SelectTimeoutError, Sender, bounded};
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::task::Poll;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_ACTIVE_TRANSPORT: usize = 32;
const MAX_PENDING_TRANSPORT: usize = 128;
const MAX_FILE_WORKERS: usize = 2;
const MAX_QUEUED_FILE_OPERATIONS: usize = 32;
const DRIVER_OPEN: u8 = 0;
const DRIVER_SHUTTING_DOWN: u8 = 1;
const DRIVER_FAILED: u8 = 2;
const DRIVER_STOPPED: u8 = 3;

/// An evaluator-local owner for exactly one lazy network driver.
///
/// Creating an owner allocates no thread. Its first accepted submission starts
/// the named driver, which runs every transport future on this owner's executor.
pub struct NetRuntimeOwner {
    runtime: NetRuntime,
    driver: Option<JoinHandle<()>>,
}

/// A cheap submission/executor handle. Cloning this never owns or stops the
/// driver; only `NetRuntimeOwner` performs teardown and joining.
#[derive(Clone)]
pub struct NetRuntime {
    inner: Arc<RuntimeInner>,
}

struct RuntimeInner {
    executor: Arc<Executor<'static>>,
    queue: Mutex<VecDeque<DriverCommand>>,
    wake_writer: Mutex<UnixStream>,
    wake_reader: Mutex<Option<UnixStream>>,
    state: AtomicU8,
    driver_started: AtomicBool,
    admitted: AtomicUsize,
    active: AtomicUsize,
    queued: AtomicUsize,
    next_operation_id: AtomicU64,
    terminals: Mutex<BTreeMap<u64, Arc<TerminalCompletion>>>,
    file_lane: Mutex<Option<FileLane>>,
    #[cfg(test)]
    file_lane_stall: Mutex<Option<FileLaneStall>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetRuntimeState {
    Open,
    ShuttingDown,
    Failed,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetRuntimeSnapshot {
    pub driver_started: bool,
    pub state: NetRuntimeState,
    pub active_transport: usize,
    pub queued_transport: usize,
    pub file_io_active: usize,
    pub file_io_queued: usize,
}

/// Safe operation facts captured by the transport runtime and read later by
/// the evaluator. The timestamps are Unix microseconds so structured traces
/// retain the actual runtime timing rather than the time a caller happened to
/// observe completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetOperationMetrics {
    pub accepted_at_us: u64,
    pub transport_started_at_us: Option<u64>,
    pub completed_at_us: Option<u64>,
    pub queue_duration_us: Option<u64>,
    pub transport_duration_us: Option<u64>,
    pub status: Option<i64>,
    pub response_bytes: usize,
    pub terminal_error_kind: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetProtocol {
    Http1,
    Auto,
}

enum NetTransport {
    Request(NetRequest),
    Download(NetDownload),
    Upload(NetUpload),
    PreparedDownload(NetDownload, std::path::PathBuf),
    PreparedUpload(NetUpload, Vec<u8>),
}

struct SubmittedOperation {
    id: u64,
    accepted_at: Instant,
    agent: NetAgent,
    protocol: NetProtocol,
    transport: NetTransport,
    terminal: Arc<TerminalCompletion>,
}

enum DriverCommand {
    Submit(SubmittedOperation),
    PreflightFinished(SubmittedOperation, NetResult<()>),
    Finished(u64),
    Cancel(u64),
    DeadlineExpired(u64),
    Shutdown,
}

struct TerminalCompletion {
    id: u64,
    runtime: Weak<RuntimeInner>,
    sender: Sender<NetResult<NetResponse>>,
    completed: AtomicBool,
    completed_response_bytes: AtomicUsize,
    accepted_at_us: u64,
    transport_started_at_us: AtomicU64,
    completed_at_us: AtomicU64,
    response_status: AtomicUsize,
    terminal_error_kind: Mutex<Option<String>>,
}

struct ActiveOperation {
    task: Task<()>,
    atomic_download_output: Option<PathBuf>,
}

/// One accepted transport operation. The receiver is intentionally private to
/// the evaluator boundary so a language `NetJob` can remain an opaque ID.
pub struct NetOperation {
    id: u64,
    runtime: NetRuntime,
    terminal: Arc<TerminalCompletion>,
    receiver: Receiver<NetResult<NetResponse>>,
}

struct FileLane {
    sender: Option<Sender<FileCommand>>,
    workers: Vec<JoinHandle<()>>,
    active: Arc<AtomicUsize>,
    queued: Arc<AtomicUsize>,
}

/// A unit-test-only gate for one file-lane action. Keeping this on the runtime
/// lets the preflight test stop a real `body_file` read without coupling the
/// transport proof to filesystem timing.
#[cfg(test)]
#[derive(Clone)]
struct FileLaneStall {
    entered: Sender<()>,
    release: Receiver<()>,
    armed: Arc<AtomicBool>,
}

struct FileCommand(Box<dyn FnOnce() + Send + 'static>);

struct FileCompletion<T> {
    reader: Async<UnixStream>,
    receiver: Receiver<NetResult<T>>,
}

/// Waits for the first terminal completion among active operations without
/// imposing input ordering on the transport scheduler.
pub fn receive_any(
    operations: &[&NetOperation],
    timeout: Duration,
) -> NetResult<Option<(usize, NetResult<NetResponse>)>> {
    if operations.is_empty() {
        return Ok(None);
    }
    let mut select = Select::new();
    for operation in operations {
        select.recv(&operation.receiver);
    }
    let selected = match select.select_timeout(timeout) {
        Ok(selected) => selected,
        Err(SelectTimeoutError) => return Ok(None),
    };
    let index = selected.index();
    let result = selected.recv(&operations[index].receiver).map_err(|_| {
        NetError::new(
            "net-runtime",
            "network operation completion channel disconnected",
        )
    })?;
    Ok(Some((index, result)))
}

impl NetRuntimeOwner {
    pub fn new() -> NetResult<Self> {
        let (reader, writer) =
            UnixStream::pair().map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        reader
            .set_nonblocking(true)
            .map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        writer
            .set_nonblocking(true)
            .map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        set_close_on_exec(&reader)
            .map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        set_close_on_exec(&writer)
            .map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        Ok(Self {
            runtime: NetRuntime {
                inner: Arc::new(RuntimeInner {
                    executor: Arc::new(Executor::new()),
                    queue: Mutex::new(VecDeque::new()),
                    wake_writer: Mutex::new(writer),
                    wake_reader: Mutex::new(Some(reader)),
                    state: AtomicU8::new(DRIVER_OPEN),
                    driver_started: AtomicBool::new(false),
                    admitted: AtomicUsize::new(0),
                    active: AtomicUsize::new(0),
                    queued: AtomicUsize::new(0),
                    next_operation_id: AtomicU64::new(1),
                    terminals: Mutex::new(BTreeMap::new()),
                    file_lane: Mutex::new(None),
                    #[cfg(test)]
                    file_lane_stall: Mutex::new(None),
                }),
            },
            driver: None,
        })
    }

    pub fn executor(&self) -> Arc<Executor<'static>> {
        Arc::clone(&self.runtime.inner.executor)
    }

    pub fn snapshot(&self) -> NetRuntimeSnapshot {
        self.runtime.snapshot()
    }

    pub fn submit_request(
        &mut self,
        agent: NetAgent,
        request: NetRequest,
        protocol: NetProtocol,
    ) -> NetResult<NetOperation> {
        self.submit(agent, protocol, NetTransport::Request(request))
    }

    pub fn submit_download(
        &mut self,
        agent: NetAgent,
        download: NetDownload,
        protocol: NetProtocol,
    ) -> NetResult<NetOperation> {
        self.submit(agent, protocol, NetTransport::Download(download))
    }

    pub fn submit_upload(&mut self, agent: NetAgent, upload: NetUpload) -> NetResult<NetOperation> {
        self.submit(agent, NetProtocol::Http1, NetTransport::Upload(upload))
    }

    fn submit(
        &mut self,
        agent: NetAgent,
        protocol: NetProtocol,
        transport: NetTransport,
    ) -> NetResult<NetOperation> {
        self.start_driver()?;
        self.runtime.submit(agent, protocol, transport)
    }

    fn start_driver(&mut self) -> NetResult<()> {
        if self.driver.is_some() {
            return Ok(());
        }
        let reader = self
            .runtime
            .inner
            .wake_reader
            .lock()
            .map_err(|_| NetError::new("net-runtime", "network runtime wake reader is poisoned"))?
            .take()
            .ok_or_else(|| NetError::new("net-runtime", "network runtime cannot restart"))?;
        let runtime = self.runtime.clone();
        let driver = thread::Builder::new()
            .name("xsh-net-driver".to_string())
            .spawn(move || {
                let outcome =
                    catch_unwind(AssertUnwindSafe(|| driver_main(runtime.clone(), reader)));
                if outcome.is_err() {
                    runtime.fail_all("network driver panicked");
                }
            })
            .map_err(|error| {
                self.runtime.fail_all("network driver could not start");
                NetError::new("net-runtime", error.to_string())
            })?;
        self.runtime
            .inner
            .driver_started
            .store(true, Ordering::Release);
        self.driver = Some(driver);
        Ok(())
    }

    pub fn shutdown(&mut self) {
        let previous = self
            .runtime
            .inner
            .state
            .swap(DRIVER_SHUTTING_DOWN, Ordering::AcqRel);
        if previous != DRIVER_STOPPED {
            self.runtime.enqueue(DriverCommand::Shutdown);
        }
        if let Some(driver) = self.driver.take()
            && driver.thread().id() != thread::current().id()
        {
            let _ = driver.join();
        }
        self.runtime
            .inner
            .state
            .store(DRIVER_STOPPED, Ordering::Release);
        self.runtime
            .complete_all("net-canceled", "network runtime stopped");
        if let Ok(mut lane) = self.runtime.inner.file_lane.lock()
            && let Some(lane) = lane.as_mut()
        {
            lane.shutdown();
        }
    }
}

impl Drop for NetRuntimeOwner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl NetRuntime {
    fn submit(
        &self,
        agent: NetAgent,
        protocol: NetProtocol,
        transport: NetTransport,
    ) -> NetResult<NetOperation> {
        if self.inner.state.load(Ordering::Acquire) != DRIVER_OPEN {
            return Err(NetError::new(
                "net-runtime",
                "network runtime is not accepting work",
            ));
        }
        let admitted =
            self.inner
                .admitted
                .try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    (current < MAX_ACTIVE_TRANSPORT + MAX_PENDING_TRANSPORT).then_some(current + 1)
                });
        if admitted.is_err() {
            return Err(NetError::new(
                "net-overload",
                "network operation admission is full",
            ));
        }
        let id = self.inner.next_operation_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = bounded(1);
        let terminal = Arc::new(TerminalCompletion {
            id,
            runtime: Arc::downgrade(&self.inner),
            sender,
            completed: AtomicBool::new(false),
            completed_response_bytes: AtomicUsize::new(0),
            accepted_at_us: epoch_micros(),
            transport_started_at_us: AtomicU64::new(0),
            completed_at_us: AtomicU64::new(0),
            response_status: AtomicUsize::new(0),
            terminal_error_kind: Mutex::new(None),
        });
        self.inner
            .terminals
            .lock()
            .map_err(|_| NetError::new("net-runtime", "network completion registry is poisoned"))?
            .insert(id, Arc::clone(&terminal));
        self.enqueue(DriverCommand::Submit(SubmittedOperation {
            id,
            accepted_at: Instant::now(),
            agent,
            protocol,
            transport,
            terminal: Arc::clone(&terminal),
        }));
        Ok(NetOperation {
            id,
            runtime: self.clone(),
            terminal,
            receiver,
        })
    }

    fn enqueue(&self, command: DriverCommand) {
        if let Ok(mut queue) = self.inner.queue.lock() {
            queue.push_back(command);
        } else {
            self.fail_all("network scheduler queue is poisoned");
            return;
        }
        if let Ok(mut writer) = self.inner.wake_writer.lock() {
            match writer.write(&[1]) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => self.fail_all("network driver wakeup failed"),
            }
        } else {
            self.fail_all("network runtime wake writer is poisoned");
        }
    }

    fn pop_command(&self) -> Option<DriverCommand> {
        self.inner.queue.lock().ok()?.pop_front()
    }

    fn snapshot(&self) -> NetRuntimeSnapshot {
        let state = match self.inner.state.load(Ordering::Acquire) {
            DRIVER_OPEN => NetRuntimeState::Open,
            DRIVER_SHUTTING_DOWN => NetRuntimeState::ShuttingDown,
            DRIVER_FAILED => NetRuntimeState::Failed,
            _ => NetRuntimeState::Stopped,
        };
        NetRuntimeSnapshot {
            driver_started: self.inner.driver_started.load(Ordering::Acquire),
            state,
            active_transport: self.inner.active.load(Ordering::Acquire),
            queued_transport: self.inner.queued.load(Ordering::Acquire),
            file_io_active: self
                .inner
                .file_lane
                .lock()
                .ok()
                .and_then(|lane| lane.as_ref().map(FileLane::active))
                .unwrap_or(0),
            file_io_queued: self
                .inner
                .file_lane
                .lock()
                .ok()
                .and_then(|lane| lane.as_ref().map(FileLane::queued))
                .unwrap_or(0),
        }
    }

    fn fail_all(&self, message: &str) {
        self.inner.state.store(DRIVER_FAILED, Ordering::Release);
        self.complete_all("net-runtime", message);
    }

    fn complete_all(&self, kind: &str, message: &str) {
        let terminals = self
            .inner
            .terminals
            .lock()
            .map(|terminals| terminals.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for terminal in terminals {
            terminal.complete(Err(NetError::new(kind, message)));
        }
    }

    pub(crate) async fn run_file<T, F>(&self, action: F) -> NetResult<T>
    where
        T: Send + 'static,
        F: FnOnce() -> NetResult<T> + Send + 'static,
    {
        #[cfg(test)]
        let action = {
            let stall = self
                .inner
                .file_lane_stall
                .lock()
                .map_err(|_| NetError::new("net-runtime", "network file lane stall is poisoned"))?
                .clone();
            move || {
                if let Some(stall) = stall {
                    stall.wait_once()?;
                }
                action()
            }
        };
        let completion = {
            let mut lane = self
                .inner
                .file_lane
                .lock()
                .map_err(|_| NetError::new("net-runtime", "network file lane is poisoned"))?;
            if lane.is_none() {
                *lane = Some(FileLane::new()?);
            }
            lane.as_ref()
                .expect("network file lane was initialized")
                .submit(action)?
        };
        completion.receive().await
    }

    #[cfg(test)]
    fn install_file_lane_stall(&self, entered: Sender<()>, release: Receiver<()>) {
        let mut stall = self
            .inner
            .file_lane_stall
            .lock()
            .expect("network file lane stall is not poisoned");
        *stall = Some(FileLaneStall {
            entered,
            release,
            armed: Arc::new(AtomicBool::new(true)),
        });
    }
}

#[cfg(test)]
impl FileLaneStall {
    fn wait_once(&self) -> NetResult<()> {
        if self.armed.swap(false, Ordering::AcqRel) {
            self.entered
                .send(())
                .map_err(|_| NetError::new("net-runtime", "file-lane stall observer stopped"))?;
            self.release
                .recv()
                .map_err(|_| NetError::new("net-runtime", "file-lane stall release stopped"))?;
        }
        Ok(())
    }
}

impl NetOperation {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn try_receive(&self, timeout: Duration) -> NetResult<Option<NetResult<NetResponse>>> {
        match self.receiver.recv_timeout(timeout) {
            Ok(result) => Ok(Some(result)),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => Ok(None),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => Err(NetError::new(
                "net-runtime",
                "network operation completion channel disconnected",
            )),
        }
    }

    pub fn receive(self) -> NetResult<NetResult<NetResponse>> {
        self.receiver.recv().map_err(|_| {
            NetError::new(
                "net-runtime",
                "network operation completion channel disconnected",
            )
        })
    }

    pub fn cancel(&self) -> NetResult<()> {
        if self.runtime.inner.state.load(Ordering::Acquire) != DRIVER_OPEN {
            return Err(NetError::new(
                "net-runtime",
                "network runtime is not accepting cancellation",
            ));
        }
        self.runtime.enqueue(DriverCommand::Cancel(self.id));
        Ok(())
    }

    pub fn completed_response_bytes(&self) -> usize {
        self.terminal
            .completed_response_bytes
            .load(Ordering::Acquire)
    }

    pub fn metrics(&self) -> NetOperationMetrics {
        self.terminal.metrics()
    }
}

impl TerminalCompletion {
    fn mark_transport_started(&self) {
        let _ = self.transport_started_at_us.compare_exchange(
            0,
            epoch_micros(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn complete(&self, result: NetResult<NetResponse>) {
        if self.completed.swap(true, Ordering::AcqRel) {
            return;
        }
        let bytes = result
            .as_ref()
            .ok()
            .and_then(|response| response.body.as_ref())
            .map_or(0, Vec::len);
        self.completed_response_bytes
            .store(bytes, Ordering::Release);
        if let Ok(response) = &result {
            self.response_status
                .store(response.status.max(0) as usize, Ordering::Release);
        }
        if let Err(error) = &result
            && let Ok(mut kind) = self.terminal_error_kind.lock()
        {
            *kind = Some(error.kind.clone());
        }
        self.completed_at_us
            .store(epoch_micros(), Ordering::Release);
        let _ = self.sender.send(result);
        if let Some(runtime) = self.runtime.upgrade() {
            if let Ok(mut terminals) = runtime.terminals.lock() {
                terminals.remove(&self.id);
            }
            runtime.admitted.fetch_sub(1, Ordering::AcqRel);
        }
    }

    fn metrics(&self) -> NetOperationMetrics {
        let started = nonzero_micros(self.transport_started_at_us.load(Ordering::Acquire));
        let completed = nonzero_micros(self.completed_at_us.load(Ordering::Acquire));
        NetOperationMetrics {
            accepted_at_us: self.accepted_at_us,
            transport_started_at_us: started,
            completed_at_us: completed,
            queue_duration_us: started.map(|started| started.saturating_sub(self.accepted_at_us)),
            transport_duration_us: started
                .zip(completed)
                .map(|(started, completed)| completed.saturating_sub(started)),
            status: nonzero_micros(self.response_status.load(Ordering::Acquire) as u64)
                .map(|status| status as i64),
            response_bytes: self.completed_response_bytes.load(Ordering::Acquire),
            terminal_error_kind: self
                .terminal_error_kind
                .lock()
                .ok()
                .and_then(|kind| kind.clone()),
        }
    }
}

fn epoch_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn nonzero_micros(value: u64) -> Option<u64> {
    (value != 0).then_some(value)
}

impl FileLane {
    fn new() -> NetResult<Self> {
        let (sender, receiver) = bounded::<FileCommand>(MAX_QUEUED_FILE_OPERATIONS);
        let active = Arc::new(AtomicUsize::new(0));
        let queued = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::with_capacity(MAX_FILE_WORKERS);
        for index in 0..MAX_FILE_WORKERS {
            let receiver = receiver.clone();
            let active = Arc::clone(&active);
            let queued = Arc::clone(&queued);
            let worker = thread::Builder::new()
                .name(format!("xsh-net-file-{index}"))
                .spawn(move || {
                    while let Ok(FileCommand(command)) = receiver.recv() {
                        queued.fetch_sub(1, Ordering::AcqRel);
                        active.fetch_add(1, Ordering::AcqRel);
                        command();
                        active.fetch_sub(1, Ordering::AcqRel);
                    }
                })
                .map_err(|error| NetError::new("net-runtime", error.to_string()))?;
            workers.push(worker);
        }
        Ok(Self {
            sender: Some(sender),
            workers,
            active,
            queued,
        })
    }

    fn submit<T, F>(&self, action: F) -> NetResult<FileCompletion<T>>
    where
        T: Send + 'static,
        F: FnOnce() -> NetResult<T> + Send + 'static,
    {
        let (reader, mut writer) =
            UnixStream::pair().map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        reader
            .set_nonblocking(true)
            .map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        writer
            .set_nonblocking(true)
            .map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        set_close_on_exec(&reader)
            .map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        set_close_on_exec(&writer)
            .map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        let reader =
            Async::new(reader).map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        let (sender, receiver) = bounded(1);
        let command = FileCommand(Box::new(move || {
            let result = catch_unwind(AssertUnwindSafe(action)).unwrap_or_else(|_| {
                Err(NetError::new(
                    "net-runtime",
                    "network file operation panicked",
                ))
            });
            let _ = sender.send(result);
            let _ = writer.write(&[1]);
        }));
        let sender = self
            .sender
            .as_ref()
            .ok_or_else(|| NetError::new("net-runtime", "network file lane is stopped"))?;
        self.queued.fetch_add(1, Ordering::AcqRel);
        sender.try_send(command).map_err(|error| {
            self.queued.fetch_sub(1, Ordering::AcqRel);
            match error {
                crossbeam_channel::TrySendError::Full(_) => {
                    NetError::new("net-overload", "network file operation admission is full")
                }
                crossbeam_channel::TrySendError::Disconnected(_) => {
                    NetError::new("net-runtime", "network file lane is stopped")
                }
            }
        })?;
        Ok(FileCompletion { reader, receiver })
    }

    fn active(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }

    fn queued(&self) -> usize {
        self.queued.load(Ordering::Acquire)
    }

    fn shutdown(&mut self) {
        self.sender.take();
        for worker in self.workers.drain(..) {
            if worker.thread().id() != thread::current().id() {
                let _ = worker.join();
            }
        }
    }
}

impl<T> FileCompletion<T> {
    async fn receive(self) -> NetResult<T> {
        loop {
            match self.receiver.try_recv() {
                Ok(result) => return result,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    return Err(NetError::new(
                        "net-runtime",
                        "network file completion channel disconnected",
                    ));
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
            let mut byte = [0_u8; 1];
            self.reader
                .read_with(|mut reader| match reader.read(&mut byte) {
                    Ok(_) => Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => Err(error),
                    Err(error) => Err(error),
                })
                .await
                .map_err(|error| NetError::new("net-runtime", error.to_string()))?;
        }
    }
}

fn set_close_on_exec(stream: &UnixStream) -> io::Result<()> {
    let result = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn driver_main(runtime: NetRuntime, reader: UnixStream) {
    let reader = Async::new(reader).expect("network runtime wake reader must register");
    let executor = Arc::clone(&runtime.inner.executor);
    futures_lite::future::block_on(executor.run(driver_loop(runtime, reader)));
}

async fn driver_loop(runtime: NetRuntime, reader: Async<UnixStream>) {
    let mut pending = VecDeque::new();
    let mut preparing: BTreeMap<u64, Task<()>> = BTreeMap::new();
    let mut active: BTreeMap<u64, ActiveOperation> = BTreeMap::new();
    // A deadline is part of an accepted operation's lifetime. Keep its task
    // owned by the driver so a successful operation cannot retain the runtime
    // through a detached timer until its original timeout would have elapsed.
    let mut deadlines: BTreeMap<u64, Task<()>> = BTreeMap::new();
    loop {
        while let Some(command) = runtime.pop_command() {
            match command {
                DriverCommand::Submit(operation) => {
                    runtime.inner.queued.fetch_add(1, Ordering::AcqRel);
                    let id = operation.id;
                    if let Some(timeout) = transport_total_timeout(&operation.transport) {
                        let remaining = timeout
                            .checked_sub(operation.accepted_at.elapsed())
                            .unwrap_or(Duration::ZERO);
                        let timeout_runtime = runtime.clone();
                        let deadline = runtime.inner.executor.spawn(async move {
                            Timer::after(remaining).await;
                            timeout_runtime.enqueue(DriverCommand::DeadlineExpired(id));
                        });
                        deadlines.insert(id, deadline);
                    }
                    let preflight_runtime = runtime.clone();
                    let task = runtime.inner.executor.spawn(async move {
                        let mut operation = operation;
                        let result = prepare_transport(&preflight_runtime, &mut operation).await;
                        preflight_runtime
                            .enqueue(DriverCommand::PreflightFinished(operation, result));
                    });
                    preparing.insert(id, task);
                }
                DriverCommand::PreflightFinished(operation, result) => {
                    let was_preparing = preparing.remove(&operation.id).is_some();
                    if was_preparing || !operation.terminal.completed.load(Ordering::Acquire) {
                        match result {
                            Ok(()) => pending.push_back(operation),
                            Err(error) => {
                                runtime.inner.queued.fetch_sub(1, Ordering::AcqRel);
                                operation.terminal.complete(Err(error));
                                cancel_deadline(&mut deadlines, operation.id).await;
                            }
                        }
                    }
                }
                DriverCommand::Finished(id) => {
                    if active.remove(&id).is_some() {
                        runtime.inner.active.fetch_sub(1, Ordering::AcqRel);
                    }
                    cancel_deadline(&mut deadlines, id).await;
                }
                DriverCommand::Cancel(id) => {
                    cancel_deadline(&mut deadlines, id).await;
                    if let Some(task) = preparing.remove(&id) {
                        runtime.inner.queued.fetch_sub(1, Ordering::AcqRel);
                        let terminal = runtime
                            .inner
                            .terminals
                            .lock()
                            .ok()
                            .and_then(|terminals| terminals.get(&id).cloned());
                        if let Some(terminal) = &terminal {
                            terminal.complete(Err(NetError::new(
                                "net-canceled",
                                "network operation canceled during file preparation",
                            )));
                        }
                        runtime
                            .inner
                            .executor
                            .spawn(async move {
                                task.cancel().await;
                            })
                            .detach();
                    } else if let Some(index) =
                        pending.iter().position(|operation| operation.id == id)
                    {
                        let operation = pending.remove(index).expect("pending index was checked");
                        runtime.inner.queued.fetch_sub(1, Ordering::AcqRel);
                        operation.terminal.complete(Err(NetError::new(
                            "net-canceled",
                            "network operation canceled before transport started",
                        )));
                    } else if let Some(active_operation) = active.remove(&id) {
                        runtime.inner.active.fetch_sub(1, Ordering::AcqRel);
                        let terminal = runtime
                            .inner
                            .terminals
                            .lock()
                            .ok()
                            .and_then(|terminals| terminals.get(&id).cloned());
                        let cancellation_runtime = runtime.clone();
                        runtime
                            .inner
                            .executor
                            .spawn(async move {
                                active_operation.task.cancel().await;
                                if let Some(output) = active_operation.atomic_download_output {
                                    let _ = cancellation_runtime
                                        .run_file(move || super::remove_download_output(&output))
                                        .await;
                                }
                                if let Some(terminal) = terminal {
                                    terminal.complete(Err(NetError::new(
                                        "net-canceled",
                                        "network operation canceled",
                                    )));
                                }
                            })
                            .detach();
                    }
                }
                DriverCommand::DeadlineExpired(id) => {
                    // This is the task that delivered this command, so drop
                    // its completed handle instead of trying to cancel itself.
                    deadlines.remove(&id);
                    if let Some(task) = preparing.remove(&id) {
                        runtime.inner.queued.fetch_sub(1, Ordering::AcqRel);
                        if let Some(terminal) = runtime
                            .inner
                            .terminals
                            .lock()
                            .ok()
                            .and_then(|terminals| terminals.get(&id).cloned())
                        {
                            terminal.complete(Err(total_timeout_error()));
                        }
                        runtime
                            .inner
                            .executor
                            .spawn(async move {
                                task.cancel().await;
                            })
                            .detach();
                    } else if let Some(index) =
                        pending.iter().position(|operation| operation.id == id)
                    {
                        let operation = pending.remove(index).expect("pending index was checked");
                        runtime.inner.queued.fetch_sub(1, Ordering::AcqRel);
                        operation.terminal.complete(Err(total_timeout_error()));
                    }
                    // Active transports enforce the same deadline inside their
                    // request future, where download cleanup can retain its
                    // atomic-file guarantee.
                }
                DriverCommand::Shutdown => {
                    runtime
                        .inner
                        .state
                        .store(DRIVER_SHUTTING_DOWN, Ordering::Release);
                    for (_, deadline) in std::mem::take(&mut deadlines) {
                        deadline.cancel().await;
                    }
                    while let Some(operation) = pending.pop_front() {
                        runtime.inner.queued.fetch_sub(1, Ordering::AcqRel);
                        operation.terminal.complete(Err(NetError::new(
                            "net-canceled",
                            "network runtime shut down before transport started",
                        )));
                    }
                    for (_, task) in std::mem::take(&mut preparing) {
                        task.cancel().await;
                    }
                    for (id, active_operation) in std::mem::take(&mut active) {
                        runtime.inner.active.fetch_sub(1, Ordering::AcqRel);
                        active_operation.task.cancel().await;
                        if let Some(output) = active_operation.atomic_download_output {
                            let _ = runtime
                                .run_file(move || super::remove_download_output(&output))
                                .await;
                        }
                        if let Some(terminal) = runtime
                            .inner
                            .terminals
                            .lock()
                            .ok()
                            .and_then(|terminals| terminals.get(&id).cloned())
                        {
                            terminal.complete(Err(NetError::new(
                                "net-canceled",
                                "network runtime shut down",
                            )));
                        }
                    }
                    runtime.inner.state.store(DRIVER_STOPPED, Ordering::Release);
                    return;
                }
            }
        }

        while active.len() < MAX_ACTIVE_TRANSPORT {
            let Some(operation) = pending.pop_front() else {
                break;
            };
            runtime.inner.queued.fetch_sub(1, Ordering::AcqRel);
            runtime.inner.active.fetch_add(1, Ordering::AcqRel);
            let operation_runtime = runtime.clone();
            let id = operation.id;
            let terminal = Arc::clone(&operation.terminal);
            let atomic_download_output = atomic_download_output(&operation.transport);
            terminal.mark_transport_started();
            let task = runtime.inner.executor.spawn(async move {
                let result =
                    match catch_unwind_future(run_transport(&operation_runtime, operation)).await {
                        Ok(result) => result,
                        Err(()) => Err(NetError::new(
                            "net-runtime",
                            "network transport task panicked",
                        )),
                    };
                terminal.complete(result);
                operation_runtime.enqueue(DriverCommand::Finished(id));
            });
            active.insert(
                id,
                ActiveOperation {
                    task,
                    atomic_download_output,
                },
            );
        }

        if wait_for_wakeup(&reader).await.is_err() {
            runtime.fail_all("network driver wakeup failed");
            return;
        }
    }
}

async fn cancel_deadline(deadlines: &mut BTreeMap<u64, Task<()>>, id: u64) {
    if let Some(deadline) = deadlines.remove(&id) {
        deadline.cancel().await;
    }
}

/// Converts a panic while polling one transport future into an ordinary driver
/// failure. `catch_unwind` around thread entry is not enough: async tasks are
/// polled after that entry function has returned control to the executor.
async fn catch_unwind_future<F>(future: F) -> Result<F::Output, ()>
where
    F: Future,
{
    let mut future = Box::pin(future);
    std::future::poll_fn(move |context| {
        match catch_unwind(AssertUnwindSafe(|| Pin::as_mut(&mut future).poll(context))) {
            Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(_) => Poll::Ready(Err(())),
        }
    })
    .await
}

async fn wait_for_wakeup(reader: &Async<UnixStream>) -> io::Result<()> {
    let mut bytes = [0_u8; 64];
    reader
        .read_with(|mut stream| match stream.read(&mut bytes) {
            Ok(read) => Ok(read),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Err(error),
            Err(error) => Err(error),
        })
        .await
        .map(|_| ())
}

/// Performs the bounded blocking work that must finish before the transport
/// scheduler grants one of its active permits. This keeps a slow file read or
/// destination setup visible as admitted/queued work without making it an
/// active DNS/socket/TLS operation.
async fn prepare_transport(
    runtime: &NetRuntime,
    operation: &mut SubmittedOperation,
) -> NetResult<()> {
    let total_timeout = transport_total_timeout(&operation.transport);
    let replacement = match &mut operation.transport {
        NetTransport::Request(request) => {
            if let super::NetBody::File(path) = &request.body {
                let path = path.clone();
                let bytes = super::async_with_total_timeout(
                    total_timeout,
                    operation.accepted_at,
                    runtime.run_file(move || super::read_request_body_file(&path)),
                )
                .await?;
                request.body = super::NetBody::Bytes(bytes);
            }
            None
        }
        NetTransport::Download(download) => {
            let prepared_download = download.clone();
            let output = super::async_with_total_timeout(
                total_timeout,
                operation.accepted_at,
                runtime.run_file(move || super::prepare_download_destination(&prepared_download)),
            )
            .await?;
            Some(NetTransport::PreparedDownload(download.clone(), output))
        }
        NetTransport::Upload(upload) => {
            let upload = upload.clone();
            let source = upload.source.clone();
            let body = super::async_with_total_timeout(
                total_timeout,
                operation.accepted_at,
                runtime.run_file(move || super::read_upload_source(&source)),
            )
            .await?;
            Some(NetTransport::PreparedUpload(upload, body))
        }
        NetTransport::PreparedDownload(_, _) | NetTransport::PreparedUpload(_, _) => {
            return Err(NetError::new(
                "net-runtime",
                "network transport was prepared more than once",
            ));
        }
    };
    if let Some(transport) = replacement {
        operation.transport = transport;
    }
    Ok(())
}

fn transport_total_timeout(transport: &NetTransport) -> Option<Duration> {
    match transport {
        NetTransport::Request(request) => request.timeout,
        NetTransport::Download(download) | NetTransport::PreparedDownload(download, _) => {
            download.timeout
        }
        NetTransport::Upload(upload) | NetTransport::PreparedUpload(upload, _) => upload.timeout,
    }
}

fn atomic_download_output(transport: &NetTransport) -> Option<PathBuf> {
    match transport {
        NetTransport::PreparedDownload(download, output) if download.atomic => Some(output.clone()),
        _ => None,
    }
}

fn total_timeout_error() -> NetError {
    NetError::new("net-timeout", "request timed out")
}

async fn run_transport(
    runtime: &NetRuntime,
    operation: SubmittedOperation,
) -> NetResult<NetResponse> {
    match operation.transport {
        NetTransport::Request(request) => {
            super::run_request(
                &operation.agent,
                request,
                operation.protocol,
                operation.accepted_at,
            )
            .await
        }
        NetTransport::Download(_) | NetTransport::Upload(_) => Err(NetError::new(
            "net-runtime",
            "network transport reached the active scheduler without file preparation",
        )),
        NetTransport::PreparedDownload(download, output) => {
            super::run_download(
                runtime,
                &operation.agent,
                download,
                output,
                operation.protocol,
                operation.accepted_at,
            )
            .await
        }
        NetTransport::PreparedUpload(upload, body) => {
            super::run_upload(&operation.agent, upload, body, operation.accepted_at).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NetAgentKey, NetBody};

    #[test]
    fn runtime_is_lazy_and_shutdown_is_stopped_not_failed() {
        let mut owner = NetRuntimeOwner::new().expect("create runtime owner");
        assert_eq!(owner.snapshot().state, NetRuntimeState::Open);
        assert!(!owner.snapshot().driver_started);

        let agent = crate::make_agent(
            &NetAgentKey {
                pool: "test".to_string(),
                tls_verify: false,
                ca_certificate: None,
                max_idle_per_host: 1,
                idle_timeout: Duration::from_secs(1),
            },
            owner.executor(),
        )
        .expect("make test agent");
        let operation = owner
            .submit_request(
                agent,
                NetRequest {
                    method: "GET".to_string(),
                    url: "ftp://fixture.invalid/".to_string(),
                    headers: Vec::new(),
                    body: NetBody::Empty,
                    timeout: None,
                    dns_timeout: None,
                    connect_timeout: None,
                    tls_timeout: None,
                    headers_timeout: None,
                    body_idle_timeout: None,
                    redirects: 0,
                    fail_status: false,
                    max_body_bytes: 1024,
                },
                NetProtocol::Http1,
            )
            .expect("submit operation");
        assert!(owner.snapshot().driver_started);
        let response = operation
            .try_receive(Duration::from_secs(1))
            .expect("completion channel is healthy")
            .expect("invalid URL completes promptly");
        assert_eq!(
            response.expect_err("invalid URL must fail").kind,
            "net-scheme"
        );
        let metrics = operation.metrics();
        assert!(metrics.transport_started_at_us.is_some());
        assert!(metrics.completed_at_us.is_some());
        assert_eq!(metrics.terminal_error_kind.as_deref(), Some("net-scheme"));

        owner.shutdown();
        assert_eq!(owner.snapshot().state, NetRuntimeState::Stopped);
    }

    #[test]
    fn file_lane_runs_without_starting_the_transport_driver() {
        let owner = NetRuntimeOwner::new().expect("create runtime owner");
        let value = futures_lite::future::block_on(owner.runtime.run_file(|| Ok(7_u8)))
            .expect("file operation succeeds");
        assert_eq!(value, 7);
        assert!(!owner.snapshot().driver_started);
    }

    #[test]
    fn stalled_file_lane_work_does_not_stall_an_in_memory_transport() {
        let mut owner = NetRuntimeOwner::new().expect("create runtime owner");
        let (entered_tx, entered_rx) = bounded(1);
        let (release_tx, release_rx) = bounded(1);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind HTTP listener");
        let address = listener.local_addr().expect("HTTP listener address");
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept HTTP request");
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .expect("set HTTP read timeout");
                let mut request = Vec::new();
                let mut buffer = [0_u8; 256];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let read = stream.read(&mut buffer).expect("read HTTP request");
                    assert_ne!(read, 0, "client closed before finishing HTTP request");
                    request.extend_from_slice(&buffer[..read]);
                }
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .expect("write HTTP response");
                stream.flush().expect("flush HTTP response");
            }
        });
        let agent = crate::make_agent(
            &NetAgentKey {
                pool: "file-lane-stall".to_string(),
                tls_verify: false,
                ca_certificate: None,
                max_idle_per_host: 1,
                idle_timeout: Duration::from_secs(1),
            },
            owner.executor(),
        )
        .expect("make test agent");
        let source =
            std::env::temp_dir().join(format!("xsh-net-file-lane-stall-{}", std::process::id()));
        std::fs::write(&source, b"file body").expect("write file-backed request body");
        owner
            .runtime
            .install_file_lane_stall(entered_tx, release_rx);
        let file_backed = owner
            .submit_request(
                agent.clone(),
                NetRequest {
                    method: "POST".to_string(),
                    url: format!("http://{address}/"),
                    headers: Vec::new(),
                    body: NetBody::File(source.clone()),
                    timeout: Some(Duration::from_secs(1)),
                    dns_timeout: None,
                    connect_timeout: None,
                    tls_timeout: None,
                    headers_timeout: None,
                    body_idle_timeout: None,
                    redirects: 0,
                    fail_status: false,
                    max_body_bytes: 1024,
                },
                NetProtocol::Http1,
            )
            .expect("submit file-backed request");
        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("file lane reaches deterministic preflight stall point");
        assert_eq!(owner.snapshot().active_transport, 0);
        assert_eq!(owner.snapshot().file_io_active, 1);

        let in_memory = owner
            .submit_request(
                agent,
                NetRequest {
                    method: "GET".to_string(),
                    url: format!("http://{address}/"),
                    headers: Vec::new(),
                    body: NetBody::Empty,
                    timeout: Some(Duration::from_secs(1)),
                    dns_timeout: None,
                    connect_timeout: None,
                    tls_timeout: None,
                    headers_timeout: None,
                    body_idle_timeout: None,
                    redirects: 0,
                    fail_status: false,
                    max_body_bytes: 1024,
                },
                NetProtocol::Http1,
            )
            .expect("submit in-memory request");
        let in_memory_result = in_memory
            .try_receive(Duration::from_secs(1))
            .expect("completion channel remains healthy");

        release_tx.send(()).expect("release stalled file operation");
        let file_backed_result = file_backed
            .try_receive(Duration::from_secs(1))
            .expect("file-backed completion channel remains healthy");
        server.join().expect("join HTTP server");
        let response = in_memory_result
            .expect("in-memory request completes while file work is stalled")
            .expect("in-memory request succeeds while file work is stalled");
        assert_eq!(response.status, 200);
        let response = file_backed_result
            .expect("file-backed request completes after the file lane is released")
            .expect("file-backed request succeeds after the file lane is released");
        assert_eq!(response.status, 200);
        owner.shutdown();
        let _ = std::fs::remove_file(source);
    }

    #[test]
    fn total_timeout_expires_while_file_preflight_is_stalled() {
        let mut owner = NetRuntimeOwner::new().expect("create runtime owner");
        let source =
            std::env::temp_dir().join(format!("xsh-net-preflight-timeout-{}", std::process::id()));
        std::fs::write(&source, b"file body").expect("write file-backed request body");
        let (entered_tx, entered_rx) = bounded(1);
        let (release_tx, release_rx) = bounded(1);
        owner
            .runtime
            .install_file_lane_stall(entered_tx, release_rx);
        let agent = crate::make_agent(
            &NetAgentKey {
                pool: "preflight-timeout".to_string(),
                tls_verify: false,
                ca_certificate: None,
                max_idle_per_host: 1,
                idle_timeout: Duration::from_secs(1),
            },
            owner.executor(),
        )
        .expect("make test agent");
        let operation = owner
            .submit_request(
                agent,
                NetRequest {
                    method: "POST".to_string(),
                    url: "http://127.0.0.1:9/".to_string(),
                    headers: Vec::new(),
                    body: NetBody::File(source.clone()),
                    timeout: Some(Duration::from_millis(20)),
                    dns_timeout: None,
                    connect_timeout: None,
                    tls_timeout: None,
                    headers_timeout: None,
                    body_idle_timeout: None,
                    redirects: 0,
                    fail_status: false,
                    max_body_bytes: 1024,
                },
                NetProtocol::Http1,
            )
            .expect("submit file-backed request");
        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("file preflight reaches deterministic stall point");
        assert_eq!(owner.snapshot().active_transport, 0);

        let result = operation
            .try_receive(Duration::from_secs(1))
            .expect("completion channel remains healthy")
            .expect("total timeout completes during file preflight");
        assert_eq!(
            result.expect_err("preflight deadline must fail").kind,
            "net-timeout"
        );

        release_tx.send(()).expect("release stalled file preflight");
        owner.shutdown();
        let _ = std::fs::remove_file(source);
    }

    #[test]
    fn total_timeout_expires_while_waiting_for_a_transport_permit() {
        let mut owner = NetRuntimeOwner::new().expect("create runtime owner");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind HTTP listener");
        let address = listener.local_addr().expect("HTTP listener address");
        let (accepted_tx, accepted_rx) = bounded(MAX_ACTIVE_TRANSPORT);
        let (release_tx, release_rx) = bounded(MAX_ACTIVE_TRANSPORT);
        let server = thread::spawn(move || {
            let mut workers = Vec::with_capacity(MAX_ACTIVE_TRANSPORT);
            for _ in 0..MAX_ACTIVE_TRANSPORT {
                let (mut stream, _) = listener.accept().expect("accept HTTP request");
                let accepted_tx = accepted_tx.clone();
                let release_rx = release_rx.clone();
                workers.push(thread::spawn(move || {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .expect("set HTTP read timeout");
                    let mut request = Vec::new();
                    let mut buffer = [0_u8; 256];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                        let read = stream.read(&mut buffer).expect("read HTTP request");
                        assert_ne!(read, 0, "client closed before finishing HTTP request");
                        request.extend_from_slice(&buffer[..read]);
                    }
                    accepted_tx.send(()).expect("report active transport");
                    release_rx.recv().expect("release active transport");
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                        )
                        .expect("write HTTP response");
                    stream.flush().expect("flush HTTP response");
                }));
            }
            for worker in workers {
                worker.join().expect("join HTTP worker");
            }
        });
        let agent = crate::make_agent(
            &NetAgentKey {
                pool: "scheduler-timeout".to_string(),
                tls_verify: false,
                ca_certificate: None,
                max_idle_per_host: 1,
                idle_timeout: Duration::from_secs(1),
            },
            owner.executor(),
        )
        .expect("make test agent");
        let request = || NetRequest {
            method: "GET".to_string(),
            url: format!("http://{address}/"),
            headers: Vec::new(),
            body: NetBody::Empty,
            timeout: None,
            dns_timeout: None,
            connect_timeout: None,
            tls_timeout: None,
            headers_timeout: None,
            body_idle_timeout: None,
            redirects: 0,
            fail_status: false,
            max_body_bytes: 1024,
        };
        let mut active_operations = Vec::with_capacity(MAX_ACTIVE_TRANSPORT);
        for _ in 0..MAX_ACTIVE_TRANSPORT {
            active_operations.push(
                owner
                    .submit_request(agent.clone(), request(), NetProtocol::Http1)
                    .expect("submit active request"),
            );
        }
        for _ in 0..MAX_ACTIVE_TRANSPORT {
            accepted_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("active transport reaches fixture barrier");
        }
        assert_eq!(owner.snapshot().active_transport, MAX_ACTIVE_TRANSPORT);
        let mut queued_request = request();
        queued_request.timeout = Some(Duration::from_millis(150));
        let queued = owner
            .submit_request(agent, queued_request, NetProtocol::Http1)
            .expect("submit queued request");
        let queued_result = queued
            .try_receive(Duration::from_secs(1))
            .expect("queued completion channel remains healthy")
            .expect("queued timeout completes without an active permit");

        for _ in 0..MAX_ACTIVE_TRANSPORT {
            release_tx.send(()).expect("release active transport");
        }
        for operation in &active_operations {
            let result = operation
                .try_receive(Duration::from_secs(1))
                .expect("active completion channel remains healthy")
                .expect("released active transport completes");
            assert_eq!(
                result.expect("released active transport succeeds").status,
                200
            );
        }
        server.join().expect("join HTTP server");
        assert_eq!(
            queued_result
                .expect_err("queued request must expire before transport starts")
                .kind,
            "net-timeout"
        );
        owner.shutdown();
    }

    #[test]
    fn transport_future_panics_become_driver_errors() {
        let outcome: Result<(), ()> = futures_lite::future::block_on(catch_unwind_future(async {
            panic!("fixture transport panic");
        }));
        assert_eq!(outcome, Err(()));
    }
}
