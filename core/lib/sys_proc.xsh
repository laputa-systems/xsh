##! Reads process identity, UID, CPU ticks, and RSS from one rooted procfs view.
##!
##! The stat row is read on both sides of its dependent fields. PID reuse or
##! disappearance during those reads withholds the whole process observation.
use sys_source as src
use system_report as report
use system_report_live as live

## Identifies a process instance and retains the panel's raw CPU and RSS inputs.
export type Process = {
  pid: Int,
  parent_pid: Int,
  start_ticks: Int,
  command: Str,
  thread_count: Int?,
  uid: Int?,
  cpu_ticks: Int?,
  rss_bytes: Int?,
}
## Represents a complete PID listing, including an observed empty process set.
export type ProcessSource = {
  state: report.ObservationState,
  processes: List[Process],
  issues: List[src.Issue],
}
## Holds user-facing process text outside observations and exported model frames.
export type LocalProcessDisplay = {
  pid: Int,
  start_ticks: Int,
  command_line: Str?,
  user_name: Str,
}

## Holds one accepted process or the issues that withheld its identity.
export type ProcessRead = {process: Process?, issues: List[src.Issue]}
## Records the ordered rooted reads needed to verify one PID lifetime.
export type ProcessReadTape = {
  before_stat: src.SourceRead,
  statm: src.SourceRead,
  status: src.SourceRead,
  after_stat: src.SourceRead,
}
type UidParse = {uid: Int?, issues: List[src.Issue]}
type StatmParse = {rss_bytes: Int?, issues: List[src.Issue]}

pure page_bytes(pages: Int?, page_size_bytes: Int) -> Int? {
  return null when pages == null or (pages ?? -1) < 0 or page_size_bytes <= 0
  let count = pages
  return null when count > 9223372036854775807 / page_size_bytes
  count * page_size_bytes
}

pure display_bytes(data: Bytes) -> Str {
  var start = 0
  let parts = collect {
    for at in range(data.len()) {
      if data.byte_at(at) == 0 {
        if at > start {
          let argument = data[start..at]
          let text = if let Ok(decoded) = argument.utf8() {
            decoded
          } else if let Ok(display_path) = Path.parse_bytes(argument) {
            display_path.display()
          } else {
            "?"
          }
          yield text
        }
        start = at + 1
      }
    }
    if start < data.len() {
      let argument = data[start..data.len()]
      let text = if let Ok(decoded) = argument.utf8() {
        decoded
      } else if let Ok(display_path) = Path.parse_bytes(argument) {
        display_path.display()
      } else {
        "?"
      }
      yield text
    }
  }
  parts.join(" ")
}

pure stat_matches_process(source: src.SourceRead, process_row: Process) -> Bool {
  if source.observation.state != .Observed or source.observation.value == null {
    return false
  }
  if let Ok(stat) = live.parse_proc_stat_for_monitor(source.observation.value) {
    return stat.pid == process_row.pid and stat.start_ticks == process_row.start_ticks
  }
  false
}

## Reads display-only text from the host procfs and verifies its process lifetime.
## Command-line bytes stay outside observations because they can contain secrets.
export proc collect_local_display(root: FsRoot, process_rows: List[Process]) [fs, error] -> List[LocalProcessDisplay] {
  var user_names: Map[Int, Str] = {}
  let displays: List[LocalProcessDisplay] = collect {
    for row in process_rows {
      let process_path = fp"proc/{row.pid}"
      let before = src.read_source_text(root, fp"{process_path}/stat", max_bytes: 16384)
      let command = root.read_result(fp"{process_path}/cmdline", max_bytes: 4096)?
      let after = src.read_source_text(root, fp"{process_path}/stat", max_bytes: 16384)
      var command_line = null
      if stat_matches_process(before, row) and stat_matches_process(after, row) and
          command.state == "observed" and command.data != null {
        let text = display_bytes(command.data)
        if text != "" {
          command_line = if command.truncated { f"{text} …" } else { text }
        }
      }
      let user_name = if row.uid == null {
        "?"
      } else {
        let uid = row.uid
        if let Ok(name) = user_names.get(uid) {
          name
        } else {
          let name = match user.by_uid(uid) {
            Ok(account) => account.name
            Err(_) => f"{uid}"
          }
          user_names = user_names.set(uid, name)
          name
        }
      }
      yield {pid: row.pid, start_ticks: row.start_ticks, command_line: command_line, user_name: user_name}
    }
  }
  displays
}

pure parse_status_uid(source: src.SourceRead, pid: Int) -> UidParse {
  if source.observation.state != .Observed or source.observation.value == null {
    return {
      uid: null,
      issues: [src.issue(f"processes.{pid}.uid", source.observation.state, source.error_kind, source.errno)],
    }
  }
  var rows = 0
  var uid = null
  var issue: src.Issue? = null
  for line in source.observation.value.lines() {
    continue when ! line.starts_with("Uid:")
    rows += 1
    let fields = src.parse_words((line.split(":", maxsplit: 1).get(1) ?? "").replace("\t", with: " "))
    if fields.len() != 4 {
      issue = src.issue(f"processes.{pid}.uid", report.Malformed, "invalid_uid_row", null)
      continue
    }
    let parsed = src.parse_integer(fields[0])
    if parsed == null or (parsed ?? -1) < 0 {
      let state = if src.decimal_digits(fields[0]) { report.RangeFailure } else { report.Malformed }
      issue = src.issue(f"processes.{pid}.uid", state, "invalid_numeric_uid", null)
      continue
    }
    uid = parsed
  }
  if rows != 1 {
    return {
      uid: null,
      issues: [src.issue(f"processes.{pid}.uid", report.Malformed, "missing_or_duplicate_uid_row", null)],
    }
  }
  if issue != null {
    return {uid: null, issues: [issue ?? src.issue(f"processes.{pid}.uid", report.Malformed, "invalid_numeric_uid", null)]}
  }
  {uid: uid, issues: []}
}

pure parse_statm(source: src.SourceRead, page_size_bytes: Int, pid: Int) -> StatmParse {
  if source.observation.state != .Observed or source.observation.value == null {
    return {
      rss_bytes: null,
      issues: [src.issue(f"processes.{pid}.statm", source.observation.state, source.error_kind, source.errno)],
    }
  }
  let fields = src.parse_words(source.observation.value)
  if fields.len() != 7 {
    return {
      rss_bytes: null,
      issues: [src.issue(f"processes.{pid}.statm", report.Malformed, "invalid_statm_row", null)],
    }
  }
  let parsed = collect {
    for field in fields {
      let value = src.parse_integer(field)
      if value == null or (value ?? -1) < 0 {
        let state = if src.decimal_digits(field) { report.RangeFailure } else { report.Malformed }
        return {rss_bytes: null, issues: [src.issue(f"processes.{pid}.statm", state, "invalid_statm_page_count", null)]}
      }
      yield value
    }
  }
  let resident = page_bytes(parsed[1], page_size_bytes)
  if resident == null {
    return {
      rss_bytes: null,
      issues: [src.issue(f"processes.{pid}.rss", report.RangeFailure, "rss_byte_count_out_of_range", null)],
    }
  }
  {rss_bytes: resident, issues: []}
}

## Applies identity checks to the exact ordered reads used by one PID observation.
export pure process_read_from_tape(pid: Int, page_size_bytes: Int, tape: ProcessReadTape) -> ProcessRead {
  let before_source = tape.before_stat
  if before_source.observation.state != .Observed or before_source.observation.value == null {
    return {
      process: null,
      issues: [src.issue(f"processes.{pid}.stat", before_source.observation.state, before_source.error_kind, before_source.errno)],
    }
  }
  let before = live.parse_proc_stat_for_monitor(before_source.observation.value)
  if let Ok(stat) = before {
    guard stat.pid == pid else {
      return {process: null, issues: [src.issue(f"processes.{pid}.stat", report.Raced, "pid_directory_reused", null)]}
    }

    let statm = parse_statm(tape.statm, page_size_bytes, pid)
    let status = parse_status_uid(tape.status, pid)
    let after_source = tape.after_stat
    if after_source.observation.state != .Observed or after_source.observation.value == null {
      let state = if after_source.observation.state == .Absent { report.Disappeared } else { after_source.observation.state }
      return {
        process: null,
        issues: [src.issue(f"processes.{pid}.stat", state, after_source.error_kind, after_source.errno)],
      }
    }
    let after = live.parse_proc_stat_for_monitor(after_source.observation.value)
    if let Ok(final_stat) = after {
      if final_stat.pid != pid or final_stat.start_ticks != stat.start_ticks {
        return {process: null, issues: [src.issue(f"processes.{pid}.stat", report.Raced, "process_identity_changed", null)]}
      }

      var issues = statm.issues.extend(status.issues)
      if stat.cpu_ticks_state != null {
        issues += [src.issue(f"processes.{pid}.cpu_ticks", stat.cpu_ticks_state ?? report.Malformed, "invalid_process_cpu_ticks", null)]
      }
      return {
        process: {
          pid: stat.pid,
          parent_pid: stat.parent_pid,
          start_ticks: stat.start_ticks,
          command: stat.command,
          thread_count: stat.thread_count,
          uid: status.uid,
          cpu_ticks: stat.cpu_ticks,
          rss_bytes: statm.rss_bytes,
        },
        issues: issues,
      }
    } else {
      return {process: null, issues: [src.issue(f"processes.{pid}.stat", report.Raced, "process_stat_changed", null)]}
    }
  } else {
    return {process: null, issues: [src.issue(f"processes.{pid}.stat", report.Malformed, "invalid_process_stat", null)]}
  }
}

proc process_read(root: FsRoot, directory: Path, pid: Int, page_size_bytes: Int) [fs, error] -> ProcessRead {
  let before_source = src.read_source_text(root, fp"{directory}/stat", max_bytes: 16384)
  if before_source.observation.state != .Observed or before_source.observation.value == null {
    return {
      process: null,
      issues: [src.issue(f"processes.{pid}.stat", before_source.observation.state, before_source.error_kind, before_source.errno)],
    }
  }
  let before = live.parse_proc_stat_for_monitor(before_source.observation.value)
  if let Err(_) = before {
    return {process: null, issues: [src.issue(f"processes.{pid}.stat", report.Malformed, "invalid_process_stat", null)]}
  }
  let before_stat = before?
  if before_stat.pid != pid {
    return {process: null, issues: [src.issue(f"processes.{pid}.stat", report.Raced, "pid_directory_reused", null)]}
  }

  process_read_from_tape(pid, page_size_bytes, {
    before_stat: before_source,
    statm: src.read_source_text(root, fp"{directory}/statm", max_bytes: 4096),
    status: src.read_source_text(root, fp"{directory}/status", max_bytes: 16384),
    after_stat: src.read_source_text(root, fp"{directory}/stat", max_bytes: 16384),
  })
}

## Enumerates numeric PID directories and discards rows whose identity changed mid-read.
export proc collect(root: FsRoot, page_size_bytes: Int) [fs, error] -> ProcessSource {
  guard page_size_bytes > 0 else {
    return {state: report.Malformed, processes: [], issues: [src.issue("processes.page_size", report.Malformed, "invalid_page_size", null)]}
  }
  let listing = root.children(p"proc", max_entries: 65536)?
  let listing_state = if listing.state == "complete" { report.Observed } else { src.source_state(listing.state, false) }
  var issues: List[src.Issue] = []
  if listing.state != "complete" {
    issues += [src.issue("processes", listing_state, listing.error_kind, listing.errno)]
  }
  var processes: List[Process] = []
  for directory in listing.children {
    let name = directory.name()
    continue when ! src.decimal_digits(name)
    let parsed_pid = src.parse_integer(name)
    if parsed_pid == null or (parsed_pid) <= 0 {
      issues += [src.issue(f"processes.{name}.pid", report.RangeFailure, "invalid_pid", null)]
      continue
    }
    let pid = parsed_pid
    let row = process_read(root, directory, pid, page_size_bytes)
    issues = issues.extend(row.issues)
    if let process_row = row.process {
      processes += [process_row]
    }
  }
  {
    state: if listing.state == "complete" { report.Observed } else { listing_state },
    processes: processes |> sort-by .pid,
    issues: issues,
  }
}
