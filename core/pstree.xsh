#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  return AppletError.Usage(f"${applet_name}: unsupported option '${flag}'")
}

type Process = {
  pid: Int,
  parent_pid: Int,
  command: Str,
  argv: Str,
  argv0: Str,
  user: Str,
  uid: Int,
  status: Str,
  start_time: Str,
  start_time_ms: Int,
  runtime_seconds: Int,
}

let process_records: List[Process] = process.list()? |> sort-by .parent_pid * 100000000 + .pid

let process_groups = process_records
  |> group-by .parent_pid
  |> sort-by .key

let process_group_count = process_groups.len()

pure display_args(row: Process) -> Str {
  let argv0 = if row.argv0 == "" { row.command } else { row.argv0 }

  if row.argv == "" or row.argv == argv0 {
    return ""
  }

  let prefix = f"${argv0} "

  if row.argv.starts_with(prefix) {
    return row.argv.replace(prefix, "")
  }

  return row.argv
}

pure process_label(row: Process, show_args: Bool, show_pids: Bool) -> Str {
  let out = if show_pids { f"${row.command} [${row.pid}]" } else { row.command }

  if show_args {
    let arg_text = display_args(row)

    if arg_text != "" {
      return f"${out} ${arg_text}"
    }
  }

  return out
}

pure process_by_pid(pid: Int) -> List[Process] {
  return process_records |> where .pid == pid
}

pure child_group_between(parent_pid: Int, low: Int, high: Int) -> List[Process] {
  if low >= high {
    let empty: List[Process] = []
    return empty
  }

  let middle = (low + high) / 2
  let key = process_groups[middle].key

  if key == parent_pid {
    return process_groups[middle].items
  }

  if key < parent_pid {
    return child_group_between(parent_pid, middle + 1, high)
  }

  return child_group_between(parent_pid, low, middle)
}

pure child_group(parent_pid: Int) -> List[Process] {
  return child_group_between(parent_pid, 0, process_group_count)
}

pure has_same_user_parent(row: Process) -> Bool {
  let parents = process_records |> where .pid == row.parent_pid and .uid == row.uid
  return parents.len() > 0
}

pure has_same_named_user_parent(row: Process, name: Str) -> Bool {
  let parents = process_records |> where .pid == row.parent_pid and .user == name
  return parents.len() > 0
}

pure has_known_parent(row: Process) -> Bool {
  let parents = process_records |> where .pid == row.parent_pid
  return parents.len() > 0
}

pure connector(last: Bool, ascii: Bool) -> Str {
  if ascii {
    return if last { "`-" } else { "|-" }
  }

  return if last { "\u{2514}\u{2500}" } else { "\u{251c}\u{2500}" }
}

pure vertical(ascii: Bool) -> Str {
  return if ascii { "| " } else { "\u{2502} " }
}

proc print_help() [error] {
  print "usage: pstree [-aAcGhlpstT] [PID|USER]"
  print "options:"
  print "  -a, --arguments     show command line arguments"
  print "  -A, --ascii         use ASCII line drawing characters"
  print "  -c, --compact-not   don't compact identical subtrees"
  print "  -G, --vt100         use VT100 line drawing characters"
  print "  -h, --help          show this help"
  print "  -l, --long          don't truncate long lines"
  print "  -p, --show-pids     show PIDs; implies -c"
  print "  -s, --show-parents  show parents of the selected process"
  print "  -t, --thread-names  show full thread names"
  print "  -T, --hide-threads  hide threads, show only processes"
}

proc print_children(
  parent_pid: Int,
  prefix: Str,
  show_args: Bool,
  show_pids: Bool,
  ascii: Bool,
  visited: List[Int],
) [error] {
  if parent_pid in visited {
    return
  }

  let next_visited = visited.push(parent_pid)
  let children = child_group(parent_pid)
  let child_count = children.len()

  for item in children |> enumerate() {
    let child = item.value
    let child_is_last = item.index + 1 == child_count
    print f"${prefix}${connector(child_is_last, ascii)}${process_label(child, show_args, show_pids)}"
    let child_prefix = if child_is_last { f"${prefix}  " } else { f"${prefix}${vertical(ascii)}" }
    print_children(child.pid, child_prefix, show_args, show_pids, ascii, next_visited)
  }
}

proc print_process(row: Process, show_args: Bool, show_pids: Bool, ascii: Bool) [error] {
  let visited: List[Int] = []
  print process_label(row, show_args, show_pids)
  print_children(row.pid, "  ", show_args, show_pids, ascii, visited)
}

proc print_pid_root(pid: Int, show_args: Bool, show_pids: Bool, ascii: Bool) [error] {
  let roots = process_by_pid(pid)

  if roots.len() == 0 {
    return Err(AppletError.Usage(f"pstree: no such pid '${pid}'"))
  }

  print_process(roots[0], show_args, show_pids, ascii)
}

proc print_user_roots(name: Str, show_args: Bool, show_pids: Bool, ascii: Bool) [error] {
  let roots = process_records
    |> where .user == name and ! has_same_named_user_parent(., name)
    |> sort-by .pid

  if roots.len() == 0 {
    return Err(AppletError.Usage("pstree: no matching processes"))
  }

  for item in roots |> enumerate() {
    if item.index > 0 {
      print ""
    }

    print_process(item.value, show_args, show_pids, ascii)
  }
}

proc print_default_roots(show_args: Bool, show_pids: Bool, ascii: Bool) [error] {
  let roots = process_records
    |> where .parent_pid <= 0 or ! has_known_parent(.)
    |> sort-by .pid

  if roots.len() == 0 and process_records.len() > 0 {
    print_process(process_records[0], show_args, show_pids, ascii)?
    return
  }

  for item in roots |> enumerate() {
    if item.index > 0 {
      print ""
    }

    print_process(item.value, show_args, show_pids, ascii)?
  }
}

proc print_parent_chain(
  pid: Int,
  show_args: Bool,
  show_pids: Bool,
  ascii: Bool,
  visited: List[Int],
) [error] -> Result[Str] {
  if pid in visited {
    return ""
  }

  let rows = process_by_pid(pid)

  if rows.len() == 0 {
    return Err(AppletError.Usage(f"pstree: no such pid '${pid}'"))
  }

  let row = rows[0]
  let next_visited = visited.push(pid)
  let parents = process_by_pid(row.parent_pid)

  if row.parent_pid <= 0 or parents.len() == 0 {
    print process_label(row, show_args, show_pids)
    return "  "
  }

  let prefix = print_parent_chain(row.parent_pid, show_args, show_pids, ascii, next_visited)?
  print f"${prefix}${connector(true, ascii)}${process_label(row, show_args, show_pids)}"
  return f"${prefix}  "
}

proc main(...argv: List[Str]) [fs, process, error] {
  var show_args = true
  var show_pids = true
  var show_parents = false
  var help = false
  var ascii = false
  var parsing_flags = true
  var operands: List[Str] = []

  for arg in argv {
    if parsing_flags and arg == "--" {
      parsing_flags = false
    } else if parsing_flags and (arg == "-a" or arg == "--arguments") {
      show_args = true
    } else if parsing_flags and (arg == "-A" or arg == "--ascii") {
      ascii = true
    } else if parsing_flags and (arg == "-c" or arg == "--compact-not") {
      let _ = arg
    } else if parsing_flags and (arg == "-G" or arg == "--vt100") {
      ascii = false
    } else if parsing_flags and (arg == "-h" or arg == "--help") {
      help = true
    } else if parsing_flags and (arg == "-l" or arg == "--long") {
      let _ = arg
    } else if parsing_flags and (arg == "-p" or arg == "--show-pids") {
      show_pids = true
    } else if parsing_flags and (arg == "-s" or arg == "--show-parents") {
      show_parents = true
    } else if parsing_flags and (arg == "-t" or arg == "--thread-names") {
      let _ = arg
    } else if parsing_flags and (arg == "-T" or arg == "--hide-threads") {
      let _ = arg
    } else if parsing_flags and arg.starts_with("-") and arg.count_chars() > 1 {
      for flag in arg.replace("-", "").split("") {
        match flag {
          "a" => show_args = true
          "A" => ascii = true
          "c" => let _ = flag
          "G" => ascii = false
          "h" => help = true
          "l" => let _ = flag
          "p" => show_pids = true
          "s" => show_parents = true
          "t" | "T" => let _ = flag
          _ => return Err(reject_unsupported("pstree", arg))
        }
      }
    } else {
      operands = operands.push(arg)
    }
  }

  if help {
    print_help()
    return
  }

  if operands.len() > 1 {
    return Err(usage_error("pstree", "[-aAcGhlpstT] [PID|USER]"))
  }

  if show_parents and operands.len() == 0 {
    return Err(AppletError.Usage("pstree: -s requires a PID selector"))
  }

  if operands.len() == 0 {
    print_default_roots(show_args, show_pids, ascii)?
    return
  }

  match operands[0].parse_int() {
    Ok(pid) => {
      if show_parents {
        let visited: List[Int] = []
        let _ = print_parent_chain(pid, show_args, show_pids, ascii, visited)?
      } else {
        print_pid_root(pid, show_args, show_pids, ascii)?
      }
    }
    Err(_) => print_user_roots(operands[0], show_args, show_pids, ascii)?
  }
}
