##! Typed Linux System V and POSIX IPC observations and kernel limits.

## Kernel System V table families.
export enum SystemVKind { MessageQueues, MemorySegments, SemaphoreSets }
## POSIX IPC object families.
export enum PosixKind { PosixQueues, PosixMemory, PosixSemaphores }
## Required identity fields; an incomplete identity is withheld as a unit.
export type Identity = {key: Int, mode: Int, uid: Int, gid: Int, creator_uid: Int, creator_gid: Int}
## Message-queue fields remain absent when the kernel field cannot be parsed.
export type QueueFields = {used_bytes: Int?, messages: Int?, sender_pid: Int?, receiver_pid: Int?, send_seconds: Int?, receive_seconds: Int?, change_seconds: Int?}
## Shared-memory fields retain byte counts, process IDs and epoch seconds.
export type SharedMemoryFields = {size_bytes: Int?, attachments: Int?, creator_pid: Int?, last_pid: Int?, attach_seconds: Int?, detach_seconds: Int?, change_seconds: Int?}
## Semaphore-set fields retain element counts and epoch seconds.
export type SemaphoreFields = {elements: Int?, operation_seconds: Int?, change_seconds: Int?}
## Each System V observation has exactly one family-specific payload.
export enum SystemVDetails { Queue(QueueFields), SharedMemory(SharedMemoryFields), SemaphoreSet(SemaphoreFields) }
## Complete table rows remain countable even when individual fields are invalid.
export type SystemVRecord = {id: Int?, identity: Identity?, details: SystemVDetails}
## POSIX queue occupancy is observed without receiving messages.
export type PosixQueueFields = {used_bytes: Int, messages: Int}
## POSIX payloads distinguish byte size, queue occupancy and semaphore value.
export enum PosixDetails { PosixQueue(PosixQueueFields), PosixSharedMemory(Int), PosixSemaphore(Int) }
## Object names retain the family's native display convention and raw identity.
export type PosixObject = {name: Str, native_name: Bytes, uid: Int, gid: Int, mode: Int, modified_ns: Int, details: PosixDetails}
## Individually readable kernel limits preserve demand-driven observation order.
export enum Limit { MessageQueueCount, MessageBytes, QueueBytes, PosixQueueCount, PosixMessageBytes, PosixQueueMessages, SharedMemoryCount, SharedMemoryPages, SharedMemoryBytes }
## Semaphore limits follow the kernel's four-field ABI order.
export type SemaphoreLimits = {elements_per_set: Int, total_elements: Int, operations_per_call: Int, sets: Int}
error InvalidObservation = QueueStatus

pure source_path(root: Path, relative: Path) -> Path {
  if root == p"/" { fp"/{relative}" } else { fp"{root}/{relative}" }
}

pure words(text: Str) -> List[Str] {
  text.replace("\t", with: " ").split(" ") |> where . != ""
}

pure number(record: Map[Str, Str], key: Str) -> Int? {
  if let Ok(value) = (record.get(key) ?? "").parse_int() { return value }
  null
}

pure octal(value: Str) -> Int? {
  return null when value.is_empty()
  var number = 0
  for digit in value {
    if digit < "0" or digit > "7" { return null }
    number = number * 8 + (digit.byte_at(0) ?? 48) - 48
  }
  number
}

pure identity(record: Map[Str, Str]) -> Identity? {
  let key = number(record, "key")
  let mode = octal(record.get("perms") ?? "")
  let uid = number(record, "uid")
  let gid = number(record, "gid")
  let cuid = number(record, "cuid")
  let cgid = number(record, "cgid")
  if key == null or mode == null or uid == null or gid == null or cuid == null or cgid == null { return null }
  {key: key, mode: mode, uid: uid, gid: gid, creator_uid: cuid, creator_gid: cgid}
}

pure system_v_record(kind: SystemVKind, record: Map[Str, Str]) -> SystemVRecord {
  let id = number(record, match kind { MessageQueues => "msqid", MemorySegments => "shmid", SemaphoreSets => "semid" })
  let details = match kind {
    MessageQueues => Queue({used_bytes: number(record, "cbytes"), messages: number(record, "qnum"), sender_pid: number(record, "lspid"), receiver_pid: number(record, "lrpid"), send_seconds: number(record, "stime"), receive_seconds: number(record, "rtime"), change_seconds: number(record, "ctime")})
    MemorySegments => SharedMemory({size_bytes: number(record, "size"), attachments: number(record, "nattch"), creator_pid: number(record, "cpid"), last_pid: number(record, "lpid"), attach_seconds: number(record, "atime"), detach_seconds: number(record, "dtime"), change_seconds: number(record, "ctime")})
    SemaphoreSets => SemaphoreSet({elements: number(record, "nsems"), operation_seconds: number(record, "otime"), change_seconds: number(record, "ctime")})
  }
  {id: id, identity: identity(record), details: details}
}

## Reads one System V table; unreadable sources retain their native errors.
export proc collect_system_v(kind: SystemVKind, root: Path = p"/") [fs, error] -> List[SystemVRecord] {
  let name = match kind { MessageQueues => "msg", MemorySegments => "shm", SemaphoreSets => "sem" }
  let lines = source_path(root, fp"proc/sysvipc/{name}").read_text()?.split("\n")
  return [] when lines.is_empty()
  let header = words(lines[0])
  var records: List[SystemVRecord] = []
  for line in lines[1..] {
    let fields = words(line)
    continue when fields.len() < header.len()
    var record: Map[Str, Str] = {}
    for index in range(header.len()) { record = record.set(header[index], fields[index]) }
    records += [system_v_record(kind, record)]
  }
  records
}

## Reads unbounded native argv bytes without extra process-lifetime observations.
export proc read_process_arguments(pid: Int, root: Path = p"/") [fs, error] -> Bytes {
  source_path(root, fp"proc/{pid}/cmdline").read_bytes()?
}

pure posix_directory(kind: PosixKind, root: Path) -> Path {
  if kind == PosixQueues { source_path(root, p"dev/mqueue") } else { source_path(root, p"dev/shm") }
}

## Tests mount-directory availability without changing listing or error policy.
export proc posix_directory_available(kind: PosixKind, root: Path = p"/") [fs] -> Bool {
  fs.stat(posix_directory(kind, root)) is Ok(_)
}

proc read_posix_object(kind: PosixKind, entry_path: Path, name: Str, native_name: Bytes) [fs, process, error] -> PosixObject {
  let info = fs.stat(entry_path)?
  let details = if kind == PosixQueues {
    let status = words(entry_path.read_text()?)
    guard !status.is_empty() and status[0].starts_with("QSIZE:") else { return Err(InvalidObservation.QueueStatus("invalid POSIX message queue status"))? }
    let inline = status[0].byte_slice("QSIZE:".byte_len())
    let text = if inline.is_empty() and status.len() > 1 { status[1] } else { inline }
    let used_bytes = text.parse_int()?
    let messages = linux.posix_message_queue_messages(native_name)?
    PosixQueue({used_bytes: used_bytes, messages: messages})
  } else if kind == PosixMemory {
    PosixSharedMemory(info.size)
  } else {
    PosixSemaphore(linux.posix_semaphore_value(native_name)?)
  }
  {name: name, native_name: native_name, uid: info.uid, gid: info.gid, mode: info.mode, modified_ns: info.mtime_ns, details: details}
}

## Lists in filesystem order and retains each resource's independent observation error.
export proc collect_posix(kind: PosixKind, root: Path = p"/") [fs, process, error] -> List[Result[PosixObject, Error]] {
  let found = fs.children(posix_directory(kind, root), ordered: false)?
  var objects: List[Result[PosixObject, Error]] = []
  for child in found {
    let raw_name = child.path.components()[-1].bytes()
    if kind == PosixQueues {
      objects += [try { read_posix_object(kind, child.path, f"/{child.name}", bytes.concat([b"/", raw_name])) }]
    } else if kind == PosixMemory {
      continue when raw_name.starts_with(b"sem.")
      objects += [try { read_posix_object(kind, child.path, child.name, raw_name) }]
    } else {
      continue when !raw_name.starts_with(b"sem.")
      objects += [try { read_posix_object(kind, child.path, child.name.byte_slice(4), raw_name[4..]) }]
    }
  }
  objects
}

## Reads one limit as trimmed ABI text; numeric interpretation belongs to its consumer.
export proc read_limit_text(limit: Limit, root: Path = p"/") [fs, error] -> Str {
  let relative = match limit {
    MessageQueueCount => "kernel/msgmni"
    MessageBytes => "kernel/msgmax"
    QueueBytes => "kernel/msgmnb"
    PosixQueueCount => "fs/mqueue/queues_max"
    PosixMessageBytes => "fs/mqueue/msgsize_max"
    PosixQueueMessages => "fs/mqueue/msg_max"
    SharedMemoryCount => "kernel/shmmni"
    SharedMemoryPages => "kernel/shmall"
    SharedMemoryBytes => "kernel/shmmax"
  }
  source_path(root, fp"proc/sys/{relative}").read_text()?.trim()
}

## Reads semaphore limits without folding them into command summary rows.
export proc read_semaphore_limits(root: Path = p"/") [fs, error] -> SemaphoreLimits {
  let fields = words(source_path(root, p"proc/sys/kernel/sem").read_text()?)
  let elements_per_set = fields[0].parse_int()?
  let total_elements = fields[1].parse_int()?
  let operations_per_call = fields[2].parse_int()?
  let sets = fields[3].parse_int()?
  {elements_per_set: elements_per_set, total_elements: total_elements, operations_per_call: operations_per_call, sets: sets}
}
