#!/bin/xsh
error AppletError = Usage : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

type Process = ProcessEntry

let process_records: List[Process] = process.list()? |> sort-by .parent_pid * 100000000 + .pid

let process_groups = process_records
  |> group-by .parent_pid
  |> sort-by .key

let process_group_count = process_groups.len()
let processes_by_pid_order = process_records |> sort-by .pid

pure display_args(row: Process) -> Str {
  let argv0 = if row.argv0 == "" { row.command } else { row.argv0 }

  return "" when row.argv == "" or row.argv == argv0

  let prefix = f"{argv0} "

  return row.argv.replace(prefix, with: "") when row.argv.starts_with(prefix)

  row.argv
}

pure process_label(row: Process, show_args: Bool, show_pids: Bool) -> Str {
  let out = if show_pids { f"{row.command} [{row.pid}]" } else { row.command }

  if show_args {
    let arg_text = display_args(row)

    return f"{out} {arg_text}" when arg_text != ""
  }

  out
}

pure process_by_pid(pid: Int) -> List[Process] {
  var low = 0
  var high = processes_by_pid_order.len()

  while low < high {
    let middle = (low + high) / 2
    let row = processes_by_pid_order[middle]

    return [row] when row.pid == pid

    if row.pid < pid {
      low = middle + 1
    } else {
      high = middle
    }
  }

  []
}

pure child_group_between(parent_pid: Int, low: Int, high: Int) -> List[Process] {
  if low >= high {
    let empty: List[Process] = []
    return empty
  }

  let middle = (low + high) / 2
  let key = process_groups[middle].key

  return process_groups[middle].items when key == parent_pid

  return child_group_between(parent_pid, middle + 1, high) when key < parent_pid

  child_group_between(parent_pid, low, middle)
}

pure child_group(parent_pid: Int) -> List[Process] {
  child_group_between(parent_pid, 0, process_group_count)
}

pure has_same_named_user_parent(row: Process, name: Str) -> Bool {
  let parents = process_by_pid(row.parent_pid)
  return false when parents.is_empty()

  parents[0].user == name
}

pure connector(last: Bool, ascii: Bool) -> Str {
  return if last { "`-" } else { "|-" } when ascii

  if last {
    "└─"
  } else {
    "├─"
  }
}

pure vertical(ascii: Bool) -> Str {
  if ascii {
    "| "
  } else {
    "│ "
  }
}

proc print_help() [error] {
  print "usage: pstree [-aAchlpsT] [PID|USER]"
  print "options:"
  print "  -a, --arguments     show command line arguments"
  print "  -A, --ascii         use ASCII line drawing characters"
  print "  -c, --compact-not   don't compact identical subtrees"
  print "  -h, --help          show this help"
  print "  -l, --long          don't truncate long lines"
  print "  -p, --show-pids     show PIDs; implies -c"
  print "  -s, --show-parents  show parents of the selected process"
  print "  -T, --hide-threads  hide threads, show only processes"
}

pure render_children(
  parent_pid: Int,
  prefix: Str,
  show_args: Bool,
  show_pids: Bool,
  ascii: Bool,
  visited: List[Int],
) -> Str {
  return "" when parent_pid in visited

  let next_visited = visited.push(parent_pid)
  let children = child_group(parent_pid)
  let child_count = children.len()
  var output = ""

  for item in children |> enumerate() {
    let child = item.value
    let child_is_last = item.index + 1 == child_count
    output = f"""{output}{prefix}{connector(child_is_last, ascii)}{process_label(child, show_args, show_pids)}
"""
    let child_prefix = if child_is_last { f"{prefix}  " } else { f"{prefix}{vertical(ascii)}" }
    output = f"{output}{render_children(child.pid, child_prefix, show_args, show_pids, ascii, next_visited)}"
  }

  output
}

pure render_process(row: Process, show_args: Bool, show_pids: Bool, ascii: Bool) -> Str {
  let visited = []
  f"""{process_label(row, show_args, show_pids)}
{render_children(row.pid, "  ", show_args, show_pids, ascii, visited)}"""
}

proc print_pid_root(pid: Int, show_args: Bool, show_pids: Bool, ascii: Bool) [error] {
  let roots = process_by_pid(pid)

  return Err(AppletError.Usage(f"pstree: no such pid '{pid}'")) when roots.is_empty()

  print render_process(roots[0], show_args, show_pids, ascii)
}

proc print_user_roots(name: Str, show_args: Bool, show_pids: Bool, ascii: Bool) [error] {
  let roots = process_records
    |> where .user == name and ! has_same_named_user_parent(., name)
    |> sort-by .pid

  return Err(AppletError.Usage("pstree: no matching processes")) when roots.is_empty()

  for item in roots |> enumerate() {
    if item.index > 0 {
      print ""
    }

    print render_process(item.value, show_args, show_pids, ascii)
  }
}

proc print_default_roots(show_args: Bool, show_pids: Bool, ascii: Bool) [error] {
  let roots = process_by_pid(1)

  if ! roots.is_empty() {
    print render_process(roots[0], show_args, show_pids, ascii)
    return
  }

  if ! process_records.is_empty() {
    print render_process(process_records[0], show_args, show_pids, ascii)
  }
}

proc print_parent_chain(
  pid: Int,
  show_args: Bool,
  show_pids: Bool,
  ascii: Bool,
  visited: List[Int],
) [error] -> Result[Str] {
  return "" when pid in visited

  let rows = process_by_pid(pid)

  return Err(AppletError.Usage(f"pstree: no such pid '{pid}'")) when rows.is_empty()

  let row = rows[0]
  let next_visited = visited.push(pid)
  let parents = process_by_pid(row.parent_pid)

  if row.parent_pid <= 0 or parents.is_empty() {
    print process_label(row, show_args, show_pids)
    return "  "
  }

  let prefix = print_parent_chain(row.parent_pid, show_args, show_pids, ascii, next_visited)?
  print f"{prefix}{connector(true, ascii)}{process_label(row, show_args, show_pids)}"
  f"{prefix}  "
}

type PstreeOptions = {
  show_args: Bool,
  ascii: Bool,
  long: Bool,
  hide_threads: Bool,
  compact_not: Bool,
  show_help: Bool,
  show_pids: Bool,
  show_parents: Bool,
  operands: List[Str],
}

proc main(...argv: List[Str]) [fs, process, error] {
  let opts: PstreeOptions = cli.applet(
    argv,
    {
      gnu: {status: 2, unsupported: {"-G": "VT100 drawing is unsupported", "--vt100": "VT100 drawing is unsupported", "-t": "thread name rendering is unsupported", "--thread-names": "thread name rendering is unsupported"}},
      show_args: {
        form: "-a --arguments",
        default: false,
      },
      ascii: {
        form: "-A --ascii",
        default: false,
      },
      show_help: {
        form: "-h --help",
        default: false,
      },
      show_pids: {
        form: "-p --show-pids",
        default: false,
      },
      show_parents: {
        form: "-s --show-parents",
        default: false,
      },
      long: {form: "-l --long", default: false},
      compact_not: {form: "-c --compact-not", default: false},
      hide_threads: {form: "-T --hide-threads", default: false},
      operands: {
        form: "...ARG",
      },
    },
  )?
  let {show_args, show_pids, show_parents, show_help: help, ..} = opts
  let ascii = opts.ascii
  let operands = opts.operands

  if help {
    print_help()
    return
  }

  return Err(usage_error("pstree", "[-aAchlpsT] [PID|USER]")) when operands.len() > 1

  if show_parents and operands.is_empty() {
    return Err(AppletError.Usage("pstree: -s requires a PID selector"))
  }

  if operands.is_empty() {
    print_default_roots(show_args, show_pids, ascii)
    return
  }

  if let Ok(pid) = operands[0].parse_int() {
    if show_parents {
      let visited = []
      let _ = print_parent_chain(pid, show_args, show_pids, ascii, visited)?
    } else {
      print_pid_root(pid, show_args, show_pids, ascii)
    }
  } else {
    print_user_roots(operands[0], show_args, show_pids, ascii)
  }
}
