#!/bin/xsh
use lib.gnu
use lib.util_linux_io as ul_io
use core.lib.ipc_report as report
use core.lib.sys_ipc as ipc

type IpcEntry = {values: Map[Str, Str], elements: List[report.Element]}
type DecimalDivision = {quotient: Str, remainder: Int}

proc help_column(name: Str, description: Str) -> Str {
  f" {tui.left_pad(name, 14)}  {description}\n"
}

proc help_option(name: Str, description: Str) -> Str {
  f"{tui.right_pad(name, 26)}{description}\n"
}

proc help_text() -> Str {
  var out = "\nUsage:\n lsipc [options]\n\nShow information on IPC facilities.\n\nResource options:\n"
  out += " -m, --shmems             shared memory segments\n"
  out += " -M, --posix-shmems       POSIX shared memory segments\n"
  out += " -q, --queues             message queues\n"
  out += " -Q, --posix-mqueues      POSIX message queues\n"
  out += " -s, --semaphores         semaphores\n"
  out += " -S, --posix-semaphores   POSIX semaphores\n"
  out += " -g, --global             info about system-wide usage\n                            (may be used with -m, -q and -s)\n"
  out += " -i, --id <id>            System V resource identified by <id>\n"
  out += " -N, --name <name>        POSIX resource identified by <name>\n"
  out += "\nOptions:\n"
  out += "     --noheadings         don't print headings\n"
  out += "     --notruncate         don't truncate output\n"
  out += "     --time-format=<type> display dates in short, full or iso format\n"
  out += " -b, --bytes              print SIZE in bytes rather than in human-readable form\n"
  out += " -c, --creator            show creator and owner\n"
  out += " -e, --export             display in an export-able output format\n"
  out += " -J, --json               use the JSON output format\n"
  out += " -n, --newline            display each piece of information on a new line\n"
  out += " -l, --list               force list output format (for example with --id)\n"
  out += " -o, --output[=<list>]    define the columns to output\n"
  out += " -P, --numeric-perms      print numeric permissions (PERMS column)\n"
  out += " -r, --raw                display in raw mode\n"
  out += " -t, --time               show attach, detach and change times\n"
  out += " -y, --shell              use column names to be usable as shell variables\n"
  out += "\n"
  out += help_option(" -h, --help", "display this help")
  out += help_option(" -V, --version", "display version")
  out += "\nGeneric System V columns:\n"
  out += help_column("KEY", "Resource key") + help_column("ID", "Resource ID")
  out += help_column("OWNER", "Owner's username or UID") + help_column("PERMS", "Permissions")
  out += help_column("CUID", "Creator UID") + help_column("CUSER", "Creator user")
  out += help_column("CGID", "Creator GID") + help_column("CGROUP", "Creator group")
  out += help_column("UID", "User ID") + help_column("USER", "User name")
  out += help_column("GID", "Group ID") + help_column("GROUP", "Group name")
  out += help_column("CTIME", "Time of the last change")
  out += "\nGeneric POSIX columns:\n"
  out += help_column("NAME", "POSIX resource name")
  out += help_column("OWNER", "Owner's username or UID") + help_column("PERMS", "Permissions")
  out += help_column("CUID", "Creator UID") + help_column("CUSER", "Creator user")
  out += help_column("CGID", "Creator GID") + help_column("CGROUP", "Creator group")
  out += help_column("MTIME", "Time of last action")
  out += "\nSystem V Shared-memory columns (--shmems):\n"
  out += help_column("SIZE", "Segment size") + help_column("NATTCH", "Number of attached processes")
  out += help_column("STATUS", "Status") + help_column("ATTACH", "Attach time")
  out += help_column("DETACH", "Detach time") + help_column("COMMAND", "Creator command line")
  out += help_column("CPID", "PID of the creator") + help_column("LPID", "PID of last user")
  out += "\nSystem V Message-queue columns (--queues):\n"
  out += help_column("USEDBYTES", "Bytes used") + help_column("MSGS", "Number of messages")
  out += help_column("SEND", "Time of last msg sent") + help_column("RECV", "Time of last msg received")
  out += help_column("LSPID", "PID of the last msg sender") + help_column("LRPID", "PID of the last msg receiver")
  out += "\nSystem V Semaphore columns (--semaphores):\n"
  out += help_column("NSEMS", "Number of semaphores") + help_column("OTIME", "Time of the last operation")
  out += "\nPOSIX Semaphore columns (--posix-semaphores):\n"
  out += help_column("SVAL", "Semaphore value")
  out += "\nSummary columns (--global):\n"
  out += help_column("RESOURCE", "Resource name") + help_column("DESCRIPTION", "Resource description")
  out += help_column("LIMIT", "System-wide limit") + help_column("USED", "Currently used")
  out += help_column("USE%", "Currently use percentage")
  out + "\nFor more details see lsipc(1).\n"
}

pure permission_text(mode: Int) -> Str {
  f"{if mode.bit_and(0o400) != 0 { "r" } else { "-" }}{if mode.bit_and(0o200) != 0 { "w" } else { "-" }}{if mode.bit_and(0o100) != 0 { "x" } else { "-" }}{if mode.bit_and(0o40) != 0 { "r" } else { "-" }}{if mode.bit_and(0o20) != 0 { "w" } else { "-" }}{if mode.bit_and(0o10) != 0 { "x" } else { "-" }}{if mode.bit_and(0o4) != 0 { "r" } else { "-" }}{if mode.bit_and(0o2) != 0 { "w" } else { "-" }}{if mode.bit_and(0o1) != 0 { "x" } else { "-" }}"
}

pure octal_text(value: Int) -> Str {
  return "0" when value == 0
  const DIGITS = "01234567"
  var number = value
  var out = ""
  while number > 0 {
    out = DIGITS.byte_slice(number % 8, length: 1) + out
    number = number / 8
  }
  "0" + out
}

pure hex_key(value: Int) -> Str {
  const DIGITS = "0123456789abcdef"
  var number = if value < 0 { value + 4294967296 } else { value }
  var out = ""
  for _ in range(8) {
    out = DIGITS.byte_slice(number % 16, length: 1) + out
    number = number / 16
  }
  f"0x{out}"
}

pure human_units(whole_value: Int, fraction: Int, divisor: Int, unit: Str) -> Str {
  var whole = whole_value
  let thousandths = if fraction >= 18446744073709551 {
    fraction / 1024 * 1000 / (divisor / 1024)
  } else {
    fraction * 1000 / divisor
  }
  var tenth = (thousandths + 50) / 100
  if tenth == 10 { whole += 1; tenth = 0 }
  if tenth == 0 { f"{whole}{unit}" } else { f"{whole}.{tenth}{unit}" }
}

pure human_size(value: Int) -> Str {
  const LETTERS = "BKMGTPE"
  var exponent = 0
  var divisor = 1
  while exponent < 60 and value >= divisor * 1024 { exponent += 10; divisor *= 1024 }
  let unit = LETTERS.byte_slice(exponent / 10, length: 1)
  if exponent == 0 { return f"{value}B" }
  human_units(value / divisor, value % divisor, divisor, unit)
}

pure divide_decimal_1024(value: Str) -> DecimalDivision {
  var quotient = ""
  var remainder = 0
  for digit in value {
    remainder = remainder * 10 + (digit.byte_at(0) ?? 48) - 48
    let next = remainder / 1024
    if next != 0 or !quotient.is_empty() { quotient += f"{next}" }
    remainder = remainder % 1024
  }
  {quotient: if quotient.is_empty() { "0" } else { quotient }, remainder: remainder}
}

proc human_limit(value: Str) [error] -> Str {
  if let Ok(number) = value.parse_int() { return human_size(number) }
  let divided = divide_decimal_1024(value)
  let quotient = divided.quotient.parse_int()?
  const DIVISOR = 1125899906842624
  human_units(quotient / DIVISOR, quotient % DIVISOR * 1024 + divided.remainder, DIVISOR * 1024, "E")
}

proc display_time(seconds: Int, format: Str) [time, env, error] -> Str? {
  return null when seconds == 0
  let ns = seconds * 1000000000
  if format == "full" { return time.format(ns, "%a %b %e %H:%M:%S %Y")? }
  if format == "iso" { return time.format(ns, "%Y-%m-%dT%H:%M:%S%:z")? }
  let now = time.now() * 1000000
  let date = time.format(ns, "%Y-%m-%d")?
  let today = time.format(now, "%Y-%m-%d")?
  if date == today { return time.format(ns, "%H:%M")? }
  if date.byte_slice(0, length: 4) == today.byte_slice(0, length: 4) {
    return time.format(ns, "%b%d")?
  }
  time.format(ns, "%Y-%b%d")?
}

proc add_user_fields(values0: Map[Str, Str], owner_uid: Int, owner_gid: Int, creator_uid: Int, creator_gid: Int, mode: Int, numeric_perms: Bool, system_v: Bool) [fs] -> Map[Str, Str] {
  var values = values0
  let owner = if let Ok(account) = user.by_uid(owner_uid) { account.name } else { f"{owner_uid}" }
  values = values.set("OWNER", owner)
  values = values.set("PERMS", if numeric_perms { octal_text(mode.bit_and(0o777)) } else { permission_text(mode.bit_and(0o777)) })
  values = values.set("CUID", f"{creator_uid}")
  values = values.set("CGID", f"{creator_gid}")
  if let Ok(account) = user.by_uid(creator_uid) { values = values.set("CUSER", account.name) }
  if let Ok(account) = group.by_gid(creator_gid) { values = values.set("CGROUP", account.name) }
  if system_v {
    values = values.set("UID", f"{owner_uid}")
    values = values.set("GID", f"{owner_gid}")
    if let Ok(account) = user.by_uid(owner_uid) { values = values.set("USER", account.name) }
    if let Ok(account) = group.by_gid(owner_gid) { values = values.set("GROUP", account.name) }
  }
  values
}

pure system_v_kind(kind: Str) -> ipc.SystemVKind {
  if kind == "queues" { ipc.MessageQueues } else if kind == "shmems" { ipc.MemorySegments } else { ipc.SemaphoreSets }
}

pure posix_kind(kind: Str) -> ipc.PosixKind {
  if kind == "posix_mqueues" { ipc.PosixQueues } else if kind == "posix_shmems" { ipc.PosixMemory } else { ipc.PosixSemaphores }
}

pure number(record: Map[Str, Str], key: Str) -> Int? {
  let text = record.get(key) ?? ""
  if let Ok(value) = text.parse_int() { return value }
  null
}

proc command_line(pid: Int) [fs] -> Str {
  return "" when pid <= 0
  let raw = if let Ok(content) = try { ipc.read_process_arguments(pid) } { content } else { b"" }
  var at = 0
  var words: List[Str] = []
  for index in range(raw.len()) {
    if raw.byte_at(index) == 0 {
      if index > at {
        let argument = raw[at..index]
        words += [if let Ok(text) = argument.utf8() { text } else if let Ok(parsed_path) = Path.parse_bytes(argument) { parsed_path.display() } else { "?" }]
      }
      at = index + 1
    }
  }
  if at < raw.len() {
    let argument = raw[at..raw.len()]
    words += [if let Ok(text) = argument.utf8() { text } else if let Ok(parsed_path) = Path.parse_bytes(argument) { parsed_path.display() } else { "?" }]
  }
  words.join(" ")
}

proc make_sysv_entry(kind: Str, record: ipc.SystemVRecord, selection: report.Selection) [fs, process, time, env, error] -> IpcEntry? {
  let id = record.id
  let identity = record.identity
  if id == null or identity == null { return null }
  let key = identity.key
  let mode = identity.mode
  let uid = identity.uid
  let gid = identity.gid
  let cuid = identity.creator_uid
  let cgid = identity.creator_gid
  var values: Map[Str, Str] = {}
  values = values.set("KEY", hex_key(key))
  values = values.set("ID", f"{id}")
  values = add_user_fields(values, uid, gid, cuid, cgid, mode, selection.numeric_perms, true)
  if kind == "shmems" { values = values.set("CGID", f"{cuid}") }
  var elements: List[report.Element] = []
  match record.details {
    ipc.Queue(queue) => {
    if selection.numeric_perms { values = values.remove("PERMS") }
    if let byte_count = queue.used_bytes { values = values.set("USEDBYTES", if selection.bytes { f"{byte_count}" } else { human_size(byte_count) }) }
    if let messages = queue.messages { values = values.set("MSGS", f"{messages}") }
    if let pid = queue.sender_pid { values = values.set("LSPID", f"{pid}") }
    if let pid = queue.receiver_pid { values = values.set("LRPID", f"{pid}") }
    if let seconds = queue.send_seconds {
      if let text = display_time(seconds, selection.time_format) { values = values.set("SEND", text) }
    }
    if let seconds = queue.receive_seconds {
      if let text = display_time(seconds, selection.time_format) { values = values.set("RECV", text) }
    }
    if let seconds = queue.change_seconds {
      if let text = display_time(seconds, selection.time_format) { values = values.set("CTIME", text) }
    }
    }
    ipc.SharedMemory(segment) => {
    if let byte_count = segment.size_bytes { values = values.set("SIZE", if selection.bytes { f"{byte_count}" } else { human_size(byte_count) }) }
    if let count = segment.attachments { values = values.set("NATTCH", f"{count}") }
    var status: List[Str] = []
    if mode.bit_and(0o1000) != 0 { status += ["dest"] }
    if mode.bit_and(0o2000) != 0 { status += ["locked"] }
    if mode.bit_and(0o4000) != 0 { status += ["hugetlb"] }
    if mode.bit_and(0o10000) != 0 { status += ["noreserve"] }
    if !status.is_empty() or selection.output_mode == "pretty" {
      values = values.set("STATUS", status.join(","))
    }
    if let seconds = segment.attach_seconds {
      if let text = display_time(seconds, selection.time_format) { values = values.set("ATTACH", text) }
    }
    if let seconds = segment.detach_seconds {
      if let text = display_time(seconds, selection.time_format) { values = values.set("DETACH", text) }
    }
    if let seconds = segment.change_seconds {
      if let text = display_time(seconds, selection.time_format) { values = values.set("CTIME", text) }
    }
    if let pid = segment.creator_pid {
      values = values.set("CPID", f"{pid}")
      if "COMMAND" in selection.columns { values = values.set("COMMAND", command_line(pid)) }
    }
    if let pid = segment.last_pid { values = values.set("LPID", f"{pid}") }
    }
    ipc.SemaphoreSet(semaphores) => {
    if let count = semaphores.elements { values = values.set("NSEMS", f"{count}") }
    if let seconds = semaphores.operation_seconds {
      if let text = display_time(seconds, selection.time_format) { values = values.set("OTIME", text) }
    }
    if let seconds = semaphores.change_seconds {
      if let text = display_time(seconds, selection.time_format) { values = values.set("CTIME", text) }
    }
    if let selected_id = selection.id {
      if selected_id >= 0 {
        if let count = semaphores.elements {
          for index in range(count) {
            let observation = linux.system_v_semaphore_element(id, index)?
            elements += [{number: index, value: observation.value, waiting_for_increase: observation.waiting_for_increase, waiting_for_zero: observation.waiting_for_zero, pid: observation.pid, command: command_line(observation.pid)}]
          }
        }
      }
    }
  }
  }
  {values: values, elements: elements}
}

proc posix_entry(observation: ipc.PosixObject, selection: report.Selection) [fs, time, env, error] -> IpcEntry {
  var values: Map[Str, Str] = {"NAME": observation.name}
  values = add_user_fields(values, observation.uid, observation.gid, observation.uid, observation.gid, observation.mode, selection.numeric_perms, false)
  match observation.details {
    ipc.PosixQueue(queue) => {
      if selection.numeric_perms { values = values.remove("PERMS") }
      values = values.set("USEDBYTES", if selection.bytes { f"{queue.used_bytes}" } else { human_size(queue.used_bytes) })
      values = values.set("MSGS", f"{queue.messages}")
    }
    ipc.PosixSharedMemory(size_bytes) => {
      values = values.set("SIZE", if selection.bytes { f"{size_bytes}" } else { human_size(size_bytes) })
    }
    ipc.PosixSemaphore(value) => { values = values.set("SVAL", f"{value}") }
  }
  if let text = display_time(observation.modified_ns / 1000000000, selection.time_format) { values = values.set("MTIME", text) }
  {values: values, elements: []}
}

proc posix_entries(kind: Str, selection: report.Selection) [fs, process, time, env, error] -> List[IpcEntry] {
  let family = posix_kind(kind)
  if kind == "posix_mqueues" {
    if !ipc.posix_directory_available(family) {
      gnu.error("/dev/mqueue does not seem to be mounted. Use 'mount -t mqueue none /dev/mqueue' to mount it.")
      return []
    }
    if let name = selection.name {
      if !name.starts_with("/") {
        gnu.error(f"message queue name must start with '/': {name}")
        return []
      }
    }
  }
  let found = try { ipc.collect_posix(family) }
  if let Err(_) = found {
    if kind == "posix_mqueues" { gnu.error("cannot open /dev/mqueue") } else { gnu.error("cannot open /dev/shm") }
    return []
  }
  var entries: List[IpcEntry] = []
  for observation in found? {
    if let Ok(object) = observation { entries += [posix_entry(object, selection)] }
  }
  if let wanted = selection.name {
    var found_name = false
    for entry in entries {
      if let Ok(name) = entry.values.get("NAME") {
        if name == wanted { found_name = true; break }
      }
    }
    if !found_name {
      if kind == "posix_shmems" { gnu.error(f"shared memory segment {wanted} not found") } else { gnu.error(f"message queue {wanted} not found") }
    }
  }
  entries
}

proc row_for(selection: report.Selection, entry: IpcEntry, resource: Str) -> report.Row {
  let cells: List[report.Cell] = collect {
    for column in selection.columns {
      match entry.values.get(column) {
        Ok(text) => yield {text: text, present: true}
        Err(_) => yield {text: "", present: false}
      }
    }
  }
  {resource: resource, cells: cells, elements: entry.elements}
}

proc selected_rows(selection: report.Selection) [fs, process, time, env, error] -> List[report.Row] {
  var rows: List[report.Row] = []
  for kind in selection.resources {
    continue when selection.global
    var entries: List[IpcEntry] = []
    if kind in ["queues", "shmems", "semaphores"] {
      for record in ipc.collect_system_v(system_v_kind(kind)) {
        if let wanted_id = selection.id {
          if wanted_id >= 0 {
            let record_id = record.id
            continue when record_id != wanted_id
          }
        }
        if let entry = make_sysv_entry(kind, record, selection) { entries += [entry] }
      }
    } else {
      entries = posix_entries(kind, selection)
    }
    for entry in entries {
      if kind in ["queues", "shmems", "semaphores"] {
        if let wanted_id = selection.id {
          if wanted_id >= 0 {
            let found_id = number(entry.values, "ID")
            continue when found_id != wanted_id
          }
        }
      }
      if kind.starts_with("posix_") {
        if let wanted_name = selection.name {
          continue when (entry.values.get("NAME") ?? "") != wanted_name
        }
      }
      rows += [row_for(selection, entry, kind)]
    }
  }
  rows
}

proc global_row(selection: report.Selection, resource: Str, description: Str, used: Int, limit: Str, usage: Bool, byte_unit: Bool) [error] -> report.Row {
  var values: Map[Str, Str] = {"RESOURCE": resource, "DESCRIPTION": description}
  values = values.set("LIMIT", if byte_unit and !selection.bytes { human_limit(limit) } else { limit })
  if usage {
    values = values.set("USED", if byte_unit and !selection.bytes { human_size(used) } else { f"{used}" })
    let percentage = used.float() / limit.parse_float()? * 100.0
    values = values.set("USE%", f"{percentage.format(precision: 2)}%")
  } else {
    values = values.set("USED", "-")
    values = values.set("USE%", "-")
  }
  row_for(selection, {values: values, elements: []}, "global")
}

proc global_rows(selection: report.Selection) [fs, process, time, env, error] -> List[report.Row] {
  var rows: List[report.Row] = []
  if "queues" in selection.resources {
    let records = ipc.collect_system_v(ipc.MessageQueues)
    rows += [global_row(selection, "MSGMNI", "Number of System V message queues", records.len(), ipc.read_limit_text(ipc.MessageQueueCount), true, false)]
    rows += [global_row(selection, "MSGMAX", "Max size of System V message (bytes)", 0, ipc.read_limit_text(ipc.MessageBytes), false, true)]
    rows += [global_row(selection, "MSGMNB", "Default max size of System V queue (bytes)", 0, ipc.read_limit_text(ipc.QueueBytes), false, true)]
  }
  if "posix_mqueues" in selection.resources {
    let posix = posix_entries("posix_mqueues", selection)
    rows += [global_row(selection, "MQUMNI", "Number of POSIX message queues", posix.len(), ipc.read_limit_text(ipc.PosixQueueCount), true, false)]
    rows += [global_row(selection, "MQUMAX", "Max size of POSIX message (bytes)", 0, ipc.read_limit_text(ipc.PosixMessageBytes), false, true)]
    rows += [global_row(selection, "MQUMNB", "Number of messages in POSIX message queue", 0, ipc.read_limit_text(ipc.PosixQueueMessages), false, false)]
  }
  if "shmems" in selection.resources {
    let records = ipc.collect_system_v(ipc.MemorySegments)
    var bytes_used = 0
    for record in records {
      match record.details { ipc.SharedMemory(segment) => { bytes_used += segment.size_bytes ?? 0 }, else => {} }
    }
    let page_size = system.execution_units()?.page_size_bytes
    rows += [global_row(selection, "SHMMNI", "Shared memory segments", records.len(), ipc.read_limit_text(ipc.SharedMemoryCount), true, false)]
    rows += [global_row(selection, "SHMALL", "Shared memory pages", bytes_used / page_size, ipc.read_limit_text(ipc.SharedMemoryPages), true, false)]
    rows += [global_row(selection, "SHMMAX", "Max size of shared memory segment (bytes)", 0, ipc.read_limit_text(ipc.SharedMemoryBytes), false, true)]
    rows += [global_row(selection, "SHMMIN", "Min size of shared memory segment (bytes)", 0, "1", false, true)]
  }
  if "semaphores" in selection.resources {
    let records = ipc.collect_system_v(ipc.SemaphoreSets)
    var elements_used = 0
    for record in records {
      match record.details { ipc.SemaphoreSet(semaphores) => { elements_used += semaphores.elements ?? 0 }, else => {} }
    }
    let limits = ipc.read_semaphore_limits()
    let semmsl = limits.elements_per_set
    let semmns = limits.total_elements
    let semopm = limits.operations_per_call
    let semmni = limits.sets
    rows += [global_row(selection, "SEMMNI", "Number of semaphore identifiers", records.len(), f"{semmni}", true, false)]
    rows += [global_row(selection, "SEMMNS", "Total number of semaphores", elements_used, f"{semmns}", true, false)]
    rows += [global_row(selection, "SEMMSL", "Max semaphores per semaphore set.", 0, f"{semmsl}", false, false)]
    rows += [global_row(selection, "SEMOPM", "Max number of operations per semop(2)", 0, f"{semopm}", false, false)]
    rows += [global_row(selection, "SEMVMX", "Semaphore max value", 0, "32767", false, false)]
  }
  rows
}

pure prepare_arguments(argv: List[Bytes]) -> Result[gnu.PreparedArguments] {
  var text: List[Str] = []
  var raw: List[gnu.RawArgument] = []
  for index in range(argv.len()) {
    let argument = argv[index]
    if let Ok(decoded) = argument.utf8() {
      text += [decoded]
      continue
    }
    var prefix = ""
    var offset = 0
    var marker = f"\0lsipc-argument-{index}\0"
    var length = argument.len()
    if argument.starts_with(b"--") {
      prefix = "--"
      offset = 2
      for at in range(2, argument.len()) {
        if argument.byte_at(at) == 61 {
          if let Ok(decoded) = argument[0..at + 1].utf8() { prefix = decoded; offset = at + 1 }
          break
        }
      }
    } else if argument.starts_with(b"-") {
      for at in range(1, argument.len()) {
        let byte = argument.byte_at(at) ?? 0
        if byte >= 128 {
          prefix = argument[0..at].utf8()?
          offset = at
          length = at + 1
          marker = "\0"
          break
        }
        if byte in [105, 78, 111] {
          prefix = argument[0..at + 1].utf8()?
          offset = at + 1
          break
        }
      }
    }
    text += [prefix + marker]
    raw += [{marker: marker, value: argument[offset..length]}]
  }
  Ok({text: text, raw: raw})
}

pure diagnostic_bytes(text: Str, raw: List[gnu.RawArgument]) -> Bytes {
  var at = 0
  var parts: List[Bytes] = []
  while at < text.byte_len() {
    var next = text.byte_len()
    var replacement = b""
    var marker_length = 0
    for argument in raw {
      if let offset = text.byte_slice(at).find(argument.marker) {
        if at + offset < next {
          next = at + offset
          replacement = argument.value
          marker_length = argument.marker.byte_len()
        }
      }
    }
    parts += [bytes.from_text(text.byte_slice(at, length: next - at)), replacement]
    at = next + marker_length
  }
  bytes.concat(parts)
}

proc option_diagnostic(detail: Str, raw: List[gnu.RawArgument], usage: Bool) [process, env, io, error] -> Unit {
  let hint = if usage { f"Try '{gnu.phrase()} --help' for more information.\n" } else { "" }
  let message = diagnostic_bytes(f"{gnu.prog()}: {detail}\n{hint}", raw)
  io.write_stderr_bytes(message)?
  io.flush_stderr()?
  exit 1
}

proc report_failure(failure: report.OptionError, raw: List[gnu.RawArgument]) [process, env, io, error] -> Unit {
  match failure {
    report.OptionError.Invalid {detail} => { option_diagnostic(detail, raw, false) }
    report.OptionError.Usage {detail} => { option_diagnostic(detail, raw, true) }
    report.OptionError.EmptyColumn => { exit 1 }
    report.OptionError.TooManyColumns => { exit 1 }
  }
}

proc missing_resource(selection: report.Selection) [process, env] {
  if selection.resources[0] in ["queues", "shmems", "semaphores"] {
    if let id = selection.id {
      if id >= 0 { gnu.error(f"id {id} not found") }
    }
  }
}

proc output_width() [env, process] -> Int {
  if !unix.isatty(1) { return 0 }
  if let Ok(text) = env.get("COLUMNS") {
    if let Ok(width) = text.parse_int() {
      if width > 0 { return width }
    }
  }
  if let Ok(size) = unix.window_size(1) { if size.cols > 0 { return size.cols } }
  80
}

proc main(...arguments: List[Bytes]) [fs, process, env, time, io, error] {
  if process.signal_action("PIPE")? == "default" { process.set_signal_action("PIPE", "default")? }
  let prepared = prepare_arguments(arguments)?
  let argv = prepared.text
  let permute = env.get("POSIXLY_CORRECT") is Err(_)
  if let Err(failure) = report.validate_cli_options(argv, permute) { report_failure(failure, prepared.raw); return }
  let options: report.Options = cli.applet(argv, {
    gnu: {status: 1},
    shmems: {form: "-m --shmems", default: false},
    posix_shmems: {form: "-M --posix-shmems", default: false},
    queues: {form: "-q --queues", default: false},
    posix_mqueues: {form: "-Q --posix-mqueues", default: false},
    semaphores: {form: "-s --semaphores", default: false},
    posix_semaphores: {form: "-S --posix-semaphores", default: false},
    global: {form: "-g --global", default: false},
    id: {form: "-i --id ID"},
    name: {form: "-N --name NAME"},
    output: {form: "-o --output LIST"},
    noheadings: {form: "--noheadings", default: false},
    notruncate: {form: "--notruncate", default: false},
    time_format: {form: "--time-format TYPE"},
    bytes: {form: "-b --bytes", default: false},
    creator: {form: "-c --creator", default: false},
    export: {form: "-e --export", default: false},
    json: {form: "-J --json", default: false},
    newline: {form: "-n --newline", default: false},
    list: {form: "-l --list", default: false},
    numeric_perms: {form: "-P --numeric-perms", default: false},
    raw: {form: "-r --raw", default: false},
    time: {form: "-t --time", default: false},
    shell: {form: "-y --shell", default: false},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...ARG"},
  })?
  if options.help { ul_io.help(help_text()); return }
  if options.version { ul_io.version("lsipc"); return }
  let selection = match report.resolve(options, argv, permute) {
    Ok(value) => value
    Err(failure) => { report_failure(failure, prepared.raw); return }
  }
  let rows = if selection.global { global_rows(selection) } else { selected_rows(selection) }
  if rows.is_empty() { missing_resource(selection) }
  ul_io.write_text(report.render(selection, rows, output_width()))
}
