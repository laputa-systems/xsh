use support.uu
use support.util_linux as ul

type LockReady = {
  method: Str,
  pid: Int,
  fd: Int,
  duplicate_fds: List[Int],
  device: Str,
  inode: Int,
  path: Str,
  lock_id: Int,
}

type WaiterReady = {
  method: Str,
  pid: Int,
  fd: Int,
  duplicate_fds: List[Int],
  device: Str,
  inode: Int,
  path: Str,
  lock_id: Int,
  owner_pid: Int,
}

type BytePathReady = {
  method: Str,
  pid: Int,
  fd: Int,
  duplicate_fds: List[Int],
  device: Str,
  inode: Int,
  path_bytes_hex: Str,
  lock_id: Int,
}

proc compared(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let reference = ul.reference(s, "lslocks", args, timeout: 5s, vars: vars)?
  let candidate = uu.invoke(s, "lslocks", args, vars: vars, timeout: 5s)?
  ul.compare(reference, candidate)
  Ok(candidate)
}

proc parity(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[Unit, Error] {
  let _ = compared(s, args, vars:)?
  Ok()
}

proc compared_with_stdout_dump(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let reference = ul.reference(s, "lslocks", args, timeout: 5s, vars: vars)?
  let candidate = uu.invoke(s, "lslocks", args, vars: vars, timeout: 5s)?
  let environment = json.encode(vars)?
  let invocation = args.join(" ") + " env=" + environment
  if reference.stdout.len() <= 1024 and candidate.stdout.len() <= 1024 {
    assert reference.stdout == candidate.stdout, f"lslocks stdout differs for {invocation}: reference={reference.stdout.base64()} candidate={candidate.stdout.base64()}"
  }
  ul.compare(reference, candidate)
  Ok(candidate)
}

proc rejected(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference(s, "lslocks", args, timeout: 5s)?
  let candidate = uu.invoke(s, "lslocks", args, timeout: 5s)?
  let invocation = args.join(" ")
  if reference.stderr.len() <= 1024 and candidate.stderr.len() <= 1024 {
    assert reference.stderr == candidate.stderr, f"lslocks stderr differs for {invocation}: reference={reference.stderr.base64()} candidate={candidate.stderr.base64()}"
  }
  ul.compare(reference, candidate)
  assert reference.status != 0 and candidate.status != 0, args.join(" ")
  Ok()
}

proc upstream_lock_case(s: uu.Scene, method: Str, holders: Bool) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let lock_path = uu.at(s, f"lock-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let descriptor_mode = if holders { "duplicate" } else { "none" }
  let keeper = ul.start_keeper(s, script, ["hold", method, lock_path.display(), descriptor_mode])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  assert ready.method == method and ready.pid > 0 and ready.fd == 17
  assert ready.duplicate_fds == (if holders { [18] } else { [] })
  assert ready.path == lock_path.display() and ready.inode > 0 and ready.lock_id > 0
  let pid = f"{ready.pid}"
  let typed = compared(s, ["--json", "--pid", pid, "--output", "PID,TYPE"])?
  let typed_document = json.decode(typed.stdout.utf8() ?? "")?
  let typed_rows = json.get(typed_document, ["locks"])?.require(List[Any])?
  let expected_type = if method.starts_with("flock-") { "FLOCK" } else if method.starts_with("posix-") { "POSIX" } else if method.starts_with("ofd-") { "OFDLCK" } else { "LEASE" }
  let expected_rows = if method.ends_with("-rw") { 2 } else { 1 }
  assert typed_rows.len() == expected_rows
  for index in range(typed_rows.len()) {
    assert json.get(typed_document, ["locks", index, "pid"])?.require(Int)? == ready.pid
    assert json.get(typed_document, ["locks", index, "type"])?.require(Str)? == expected_type
  }
  if holders {
    let observed_holders = compared(s, ["--json", "--pid", pid, "--output", "HOLDERS"])?
    let holder_document = json.decode(observed_holders.stdout.utf8() ?? "")?
    let holder_rows = json.get(holder_document, ["locks"])?.require(List[Any])?
    assert holder_rows.len() == expected_rows
    for index in range(holder_rows.len()) {
      let entries = json.get(holder_document, ["locks", index, "holders"])?.require(List[Str])?
      assert ! entries.is_empty()
      for entry in entries {
        assert entry.starts_with(f"{pid},")
      }
      if ! method.starts_with("posix-") {
        assert entries.len() == 2
        for fd in [17, 18] {
          assert [entry for entry in entries if entry.ends_with(f",{fd}")].len() == 1
        }
      }
    }
  }
  let columns = if holders {
    "COMMAND,TYPE,SIZE,MODE,START,END,HOLDERS"
  } else {
    "COMMAND,TYPE,SIZE,MODE,START,END"
  }
  parity(s, ["--raw", "--noheadings", "--pid", pid, "--output", columns])?
  if ! holders {
    parity(s, ["--raw", "--noheadings", "--pid", pid, "--output", "PATH"])?
  }
  ul.keeper_send(keeper, "DONE")
  ul.finish_keeper(keeper)
  Ok()
}

proc blocked_lock_case(s: uu.Scene, method: Str) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let lock_path = uu.at(s, f"blocked-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let owner = ul.start_keeper(s, script, ["hold", method, lock_path.display(), "none"])?
  defer ul.stop_keeper(owner)
  let owner_ready = json.decode(owner.ready)?.require(LockReady)?
  assert owner_ready.method == method and owner_ready.pid > 0 and owner_ready.inode > 0
  let waiter = ul.start_keeper(s, script, ["wait", method, lock_path.display(), f"{owner_ready.pid}"])?
  defer ul.stop_keeper(waiter)
  let waiter_ready = json.decode(waiter.ready)?.require(WaiterReady)?
  assert waiter_ready.method == method and waiter_ready.pid > 0
  assert waiter_ready.owner_pid == owner_ready.pid and waiter_ready.inode == owner_ready.inode
  assert waiter_ready.device == owner_ready.device and waiter_ready.lock_id > 0
  let args = if method == "ofd--w" {
    ["--json", "--filter", "MODE == 'WRITE*'", "--output", "PID,TYPE,MODE,BLOCKER,PATH,INODE,MAJ:MIN"]
  } else {
    ["--json", "--pid", f"{waiter_ready.pid}", "--output", "PID,TYPE,MODE,BLOCKER,PATH,INODE,MAJ:MIN"]
  }
  let candidate = compared(s, args)?
  let document = json.decode(candidate.stdout.utf8() ?? "")?
  assert json.get(document, ["locks"])?.require(List[Any])?.len() == 1
  assert json.get(document, ["locks", 0, "pid"])?.require(Int)? == (if method == "ofd--w" { -1 } else { waiter_ready.pid })
  assert json.get(document, ["locks", 0, "type"])?.require(Str)? == (if method == "flock-ex" { "FLOCK" } else if method == "posix--w" { "POSIX" } else { "OFDLCK" })
  assert json.get(document, ["locks", 0, "mode"])?.require(Str)? == "WRITE*"
  assert json.get(document, ["locks", 0, "blocker"])?.require(Int)? == owner_ready.pid
  assert json.get(document, ["locks", 0, "path"])?.require(Str)? != ""
  assert json.get(document, ["locks", 0, "inode"])?.require(Int)? == owner_ready.inode
  assert json.get(document, ["locks", 0, "maj:min"])?.require(Str)? == owner_ready.device
  if method != "ofd--w" {
    assert json.get(document, ["locks", 0, "path"])?.require(Str)? == lock_path.display()
  } else {
    let waiter_pid = compared(s, ["--json", "--pid", f"{waiter_ready.pid}", "--output", "PID"])?
    let waiter_document = json.decode(waiter_pid.stdout.utf8() ?? "")?
    assert json.get(waiter_document, ["locks"])?.require(List[Any])?.is_empty()
  }
  ul.keeper_send(owner, "DONE")
  ul.finish_keeper(owner)
  ul.finish_keeper(waiter)
  Ok()
}

proc all_columns_json_case(s: uu.Scene) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let method = "posix-r-"
  let lock_path = uu.at(s, f"json-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", method, lock_path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  let args = ["--json", "--pid", f"{ready.pid}", "--output-all"]
  let candidate = compared(s, args)?
  let document = json.decode(candidate.stdout.utf8() ?? "")?
  assert json.get(document, ["locks"])?.require(List[Any])?.len() == 1
  assert json.get(document, ["locks", 0, "command"])?.require(Str)? != ""
  assert json.get(document, ["locks", 0, "pid"])?.require(Int)? == ready.pid
  assert json.get(document, ["locks", 0, "type"])?.require(Str)? == "POSIX"
  assert json.get(document, ["locks", 0, "size"])?.require(Str)? == "1B"
  assert json.get(document, ["locks", 0, "inode"])?.require(Int)? == ready.inode
  assert json.get(document, ["locks", 0, "maj:min"])?.require(Str)? == ready.device
  assert json.get(document, ["locks", 0, "mode"])?.require(Str)? == "READ"
  assert json.get(document, ["locks", 0, "m"])?.require(Bool)? == false
  assert json.get(document, ["locks", 0, "start"])?.require(Int)? == 0
  assert json.get(document, ["locks", 0, "end"])?.require(Int)? == 0
  assert json.get(document, ["locks", 0, "path"])?.require(Str)? == lock_path.display()
  assert json.get(document, ["locks", 0, "blocker"])? == null
  let entries = json.get(document, ["locks", 0, "holders"])?.require(List[Str])?
  assert entries.len() == 1
  assert entries[0].starts_with(f"{ready.pid},") and entries[0].ends_with(",17")
  ul.keeper_send(keeper, "DONE")
  ul.finish_keeper(keeper)
  Ok()
}

proc byte_size_case(s: uu.Scene) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let method = "posix-size"
  let lock_path = uu.at(s, f"bytes-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", method, lock_path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  let human_size = compared(s, ["--json", "--pid", f"{ready.pid}", "--output", "SIZE"])?
  let human_document = json.decode(human_size.stdout.utf8() ?? "")?
  assert json.get(human_document, ["locks", 0, "size"])?.require(Str)? == "1.5K"
  let human_table = compared(s, ["--raw", "--noheadings", "--pid", f"{ready.pid}", "--output", "SIZE"])?
  assert human_table.stdout.utf8()? == "1.5K\n"
  let byte_size = compared(s, ["--json", "--bytes", "--pid", f"{ready.pid}", "--output", "SIZE"])?
  let byte_document = json.decode(byte_size.stdout.utf8() ?? "")?
  assert json.get(byte_document, ["locks", 0, "size"])?.require(Int)? == 1536
  let byte_table = compared(s, ["--bytes", "--raw", "--noheadings", "--pid", f"{ready.pid}", "--output", "SIZE"])?
  assert byte_table.stdout.utf8()? == "1536\n"
  let short_bytes = compared(s, ["-J", "-b", "-p", f"{ready.pid}", "-o", "SIZE"])?
  let short_document = json.decode(short_bytes.stdout.utf8() ?? "")?
  assert json.get(short_document, ["locks", 0, "size"])?.require(Int)? == 1536
  ul.keeper_send(keeper, "DONE")
  ul.finish_keeper(keeper)
  Ok()
}

proc ranged_lock_case(s: uu.Scene, method: Str, expected: List[Str]) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let lock_path = uu.at(s, f"ranges-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", method, lock_path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  let candidate = compared(s, ["--json", "--pid", f"{ready.pid}", "--output", "MODE,START,END"])?
  let document = json.decode(candidate.stdout.utf8() ?? "")?
  let rows = json.get(document, ["locks"])?.require(List[Any])?
  var actual: List[Str] = []
  for index in range(rows.len()) {
    let mode = json.get(document, ["locks", index, "mode"])?.require(Str)?
    let start = json.get(document, ["locks", index, "start"])?.require(Int)?
    let end = json.get(document, ["locks", index, "end"])?.require(Int)?
    actual += [f"{start}:{end}:{mode}"]
  }
  let actual_sorted = actual |> sort() |> collect()
  let wanted = expected |> sort() |> collect()
  assert actual_sorted == wanted, method
  ul.keeper_send(keeper, "DONE")
  ul.finish_keeper(keeper)
  Ok()
}

proc filtered_lock_case(s: uu.Scene) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let method = "posix-size"
  let lock_path = uu.at(s, f"filter-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", method, lock_path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  let pid = f"{ready.pid}"
  let expressions = [
    "TYPE == 'POSIX'", "TYPE != 'FLOCK'", "TYPE == 'FLOCK'",
    f"PID == {pid}", f"PID > 0 AND TYPE == 'POSIX'",
    "PID > 0 && (TYPE == 'POSIX' || TYPE == 'FLOCK')",
    "NOT (TYPE == 'FLOCK')", "PATH =~ '.*filter-posix-size.*'",
    "PATH !~ 'missing-lock-path'", "SIZE >= 1", "SIZE >= 1K",
    "SIZE > 1K", "SIZE < 2K", "SIZE < 1K", "SIZE <= 1536",
    "TYPE EQ 'POSIX'", "TYPE NE 'FLOCK'", "PID GT 0",
    f"PID GE {pid}", "PID LT 2147483647", "PID LE 2147483647",
    f"PID == {pid}.0", "PID > 0.5",
    "M == false", "M EQ TRUE",
    "TYPE == 'POSIX' OR TYPE == 'FLOCK' AND TYPE == 'FLOCK'",
  ]
  let counts = [1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0]
  for index in range(expressions.len()) {
    let expression = expressions[index]
    let count = counts[index]
    let candidate = compared(s, ["--json", "--filter", expression, "--pid", pid, "--output", "TYPE"])?
    let document = json.decode(candidate.stdout.utf8() ?? "")?
    assert json.get(document, ["locks"])?.require(List[Any])?.len() == count, expression
  }
  let short_filter = compared(s, ["-Q", "TYPE == 'POSIX'", "-p", pid, "-o", "TYPE"])?
  let short_document = json.decode(short_filter.stdout.utf8() ?? "")?
  assert json.get(short_document, ["locks"])?.require(List[Any])?.len() == 1
  let hidden_column = compared(s, ["--json", "--filter", f"PID == {pid}", "--pid", pid, "--output", "TYPE"])?
  let hidden_document = json.decode(hidden_column.stdout.utf8() ?? "")?
  assert json.get(hidden_document, ["locks"])?.require(List[Any])?.len() == 1
  ul.keeper_send(keeper, "DONE")
  ul.finish_keeper(keeper)
  Ok()
}

proc selected_lock_rows_case(s: uu.Scene) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let first_path = uu.at(s, "rows-first")
  let second_path = uu.at(s, "rows-second")
  let first = ul.start_keeper(s, script, ["hold", "flock-ex", first_path.display(), "none"])?
  defer ul.stop_keeper(first)
  let first_ready = json.decode(first.ready)?.require(LockReady)?
  let second = ul.start_keeper(s, script, ["hold", "posix-r-", second_path.display(), "none"])?
  defer ul.stop_keeper(second)
  let second_ready = json.decode(second.ready)?.require(LockReady)?
  let all = compared(s, ["--raw"], {LC_ALL: "C"})?
  let all_lines = all.stdout.utf8()?.lines()
  for column in ["COMMAND", "PID", "TYPE", "SIZE", "MODE", "M", "START", "END", "PATH"] {
    assert column in all_lines[0], column
  }
  assert all_lines.len() == 3
  let type_only = compared(s, ["--raw"], {LSLOCKS_COLUMNS: "TYPE"})?
  let type_lines = type_only.stdout.utf8()?.lines()
  assert type_lines[0] == "TYPE" and type_lines.len() == 3
  let extended = compared(s, ["--raw"], {LSLOCKS_COLUMNS: "+BLOCKER"})?
  let extended_lines = extended.stdout.utf8()?.lines()
  for column in ["COMMAND", "PID", "TYPE", "SIZE", "MODE", "M", "START", "END", "PATH", "BLOCKER"] {
    assert column in extended_lines[0], column
  }
  assert extended_lines.len() == 3
  let selected = compared(s, ["--raw", "--output", "PATH,TYPE"], {LSLOCKS_COLUMNS: "PID"})?
  let selected_lines = selected.stdout.utf8()?.lines()
  assert "PATH" in selected_lines[0] and "TYPE" in selected_lines[0]
  assert ! ("PID" in selected_lines[0]) and selected_lines.len() == 3
  for ready in [first_ready, second_ready] {
    let only = compared(s, ["--json", "--pid", f"{ready.pid}", "--output", "PID,PATH"])?
    let document = json.decode(only.stdout.utf8() ?? "")?
    assert json.get(document, ["locks"])?.require(List[Any])?.len() == 1
    assert json.get(document, ["locks", 0, "pid"])?.require(Int)? == ready.pid
    assert json.get(document, ["locks", 0, "path"])?.require(Str)? == ready.path
  }
  let absent = compared(s, ["--json", "--pid", "2147483647", "--output", "PID"])?
  let absent_document = json.decode(absent.stdout.utf8() ?? "")?
  assert json.get(absent_document, ["locks"])?.require(List[Any])?.is_empty()
  ul.keeper_send(second, "DONE")
  ul.finish_keeper(second)
  ul.keeper_send(first, "DONE")
  ul.finish_keeper(first)
  Ok()
}

proc output_lock_modes_case(s: uu.Scene) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let method = "posix-r-"
  let lock_path = uu.at(s, f"output-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", method, lock_path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  let pid = f"{ready.pid}"
  for args in [
    ["--raw", "--pid", pid], ["--noheadings", "--pid", pid],
    ["--json", "--pid", pid], ["--noheadings", "--json", "--pid", pid],
    ["-r", "-p", pid], ["-n", "-p", pid],
    ["-J", "-p", pid], ["-n", "-J", "-p", pid],
  ] {
    let candidate = compared_with_stdout_dump(s, args)?
    assert ! candidate.stdout.is_empty(), args.join(" ")
    if "--json" in args or "-J" in args {
      let document = json.decode(candidate.stdout.utf8() ?? "")?
      assert json.get(document, ["locks"])?.require(List[Any])?.len() == 1
    }
  }
  for args in [
    ["operand", "--json", "--pid", pid],
    ["--json", "--pid", pid, "operand"],
  ] {
    let operand = compared(s, args)?
    assert operand.status == 0 and ! operand.stdout.is_empty()
  }
  let conflict = compared(s, ["--json", "--raw", "--pid", pid])?
  assert conflict.status != 0
  let short_conflict = compared(s, ["-J", "-r", "-p", pid])?
  assert short_conflict.status != 0
  ul.keeper_send(keeper, "DONE")
  ul.finish_keeper(keeper)
  Ok()
}

proc json_environment_case(s: uu.Scene) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let method = "posix-r-"
  let lock_path = uu.at(s, f"json-environment-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", method, lock_path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  for value in ["compact", "lines", "unknown"] {
    let candidate = compared(s, ["--json", "--pid", f"{ready.pid}"], {LIBSMARTCOLS_JSON: value})?
    let document = json.decode(candidate.stdout.utf8() ?? "")?
    assert "POSIX" in (candidate.stdout.utf8() ?? ""), value
    if value == "lines" {
      assert json.get(document, ["pid"])?.require(Int)? == ready.pid
      assert json.get(document, ["type"])?.require(Str)? == "POSIX"
      assert json.get(document, ["locks"])? == null
    } else {
      assert json.get(document, ["locks"])?.require(List[Any])?.len() == 1
    }
  }
  let unset = compared(s, ["--json", "--pid", f"{ready.pid}"])?
  let document = json.decode(unset.stdout.utf8() ?? "")?
  assert json.get(document, ["locks"])?.require(List[Any])?.len() == 1
  ul.keeper_send(keeper, "DONE")
  ul.finish_keeper(keeper)
  Ok()
}

proc deleted_path_and_owner_exit_case(s: uu.Scene) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let method = "posix-r-"
  let lock_path = uu.at(s, f"deleted-{method}")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", method, lock_path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  lock_path.remove()
  let candidate = compared(s, ["--json", "--pid", f"{ready.pid}", "--output", "PATH"])?
  let document = json.decode(candidate.stdout.utf8() ?? "")?
  assert json.get(document, ["locks"])?.require(List[Any])?.len() == 1
  assert json.get(document, ["locks", 0, "path"])?.require(Str)? == f"{ready.path} (deleted)"
  ul.keeper_send(keeper, "DONE")
  ul.finish_keeper(keeper)
  let after_exit = compared(s, ["--json", "--pid", f"{ready.pid}"])?
  assert after_exit.status == 0 and after_exit.stdout.is_empty()
  Ok()
}

proc path_escaping_case(s: uu.Scene) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let method = "posix-r-"
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let escaped = uu.at(s, "space name\ttab\nline\\slash")
  let escaped_keeper = ul.start_keeper(s, script, ["hold", method, escaped.display(), "none"])?
  defer ul.stop_keeper(escaped_keeper)
  let escaped_ready = json.decode(escaped_keeper.ready)?.require(LockReady)?
  assert escaped_ready.path == escaped.display()
  let escaped_pid = f"{escaped_ready.pid}"
  for args in [
    ["--noheadings", "--pid", escaped_pid, "--output", "PATH"],
    ["--json", "--pid", escaped_pid, "--output", "PATH"],
    ["--raw", "--noheadings", "--pid", escaped_pid, "--output", "PATH"],
  ] {
    let result = compared(s, args)?
    assert ! result.stdout.is_empty()
  }
  ul.keeper_send(escaped_keeper, "DONE")
  ul.finish_keeper(escaped_keeper)

  let invalid_utf8 = uu.at_bytes(s, b"lock-\xff")?
  let invalid_utf8_keeper = ul.start_keeper(s, script, [
    "hold-bytes", method, s.root.display(), "6c6f636b2dff", "none",
  ])?
  defer ul.stop_keeper(invalid_utf8_keeper)
  let invalid_ready = json.decode(invalid_utf8_keeper.ready)?.require(BytePathReady)?
  assert invalid_ready.path_bytes_hex.ends_with("2f6c6f636b2dff")
  assert fs.stat(invalid_utf8)?.ino == invalid_ready.inode
  let invalid_pid = f"{invalid_ready.pid}"
  for args in [
    ["--noheadings", "--pid", invalid_pid, "--output", "PATH"],
    ["--json", "--pid", invalid_pid, "--output", "PATH"],
    ["--raw", "--noheadings", "--pid", invalid_pid, "--output", "PATH"],
  ] {
    let result = compared(s, args)?
    assert ! result.stdout.is_empty()
  }
  ul.keeper_send(invalid_utf8_keeper, "DONE")
  ul.finish_keeper(invalid_utf8_keeper)
  Ok()
}

proc reference_path_case(s: uu.Scene) [fs, process, env, net, time, error] -> Result[Unit, Error] {
  let method = "posix-r-"
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let escaped = uu.at(s, "space name\ttab\nline\\slash")
  let escaped_keeper = ul.start_keeper(s, script, ["hold", method, escaped.display(), "none"])?
  defer ul.stop_keeper(escaped_keeper)
  let escaped_ready = json.decode(escaped_keeper.ready)?.require(LockReady)?
  assert escaped_ready.path == escaped.display()
  let escaped_pid = f"{escaped_ready.pid}"
  let json_result = ul.reference(s, "lslocks", ["--json", "--pid", escaped_pid, "--output", "PID,INODE,PATH"])?
  assert json_result.status == 0 and json_result.stderr.is_empty()
  let json_document = json.decode(json_result.stdout.utf8() ?? "")?
  let json_rows = json.get(json_document, ["locks"])?.require(List[Any])?
  assert json_rows.len() == 1
  assert json.get(json_document, ["locks", 0, "pid"])?.require(Int)? == escaped_ready.pid
  assert json.get(json_document, ["locks", 0, "inode"])?.require(Int)? == escaped_ready.inode
  assert json.get(json_document, ["locks", 0, "path"])?.require(Str)? == escaped.display()
  let normal = ul.reference(s, "lslocks", ["--noheadings", "--pid", escaped_pid, "--output", "PATH"])?
  assert normal.status == 0 and normal.stderr.is_empty()
  assert normal.stdout == bytes.concat([
    s.root.bytes(), b"/space name\\x09tab\\x0aline\\slash\n",
  ])
  let raw = ul.reference(s, "lslocks", ["--raw", "--noheadings", "--pid", escaped_pid, "--output", "PATH"])?
  assert raw.status == 0 and raw.stderr.is_empty()
  assert raw.stdout == bytes.concat([
    s.root.bytes(), b"/space\\x20name\\x09tab\\x0aline\\x5cslash\n",
  ])
  ul.keeper_send(escaped_keeper, "DONE")
  ul.finish_keeper(escaped_keeper)

  let invalid_utf8 = uu.at_bytes(s, b"lock-\xff")?
  let invalid_keeper = ul.start_keeper(s, script, [
    "hold-bytes", method, s.root.display(), "6c6f636b2dff", "none",
  ])?
  defer ul.stop_keeper(invalid_keeper)
  let invalid_ready = json.decode(invalid_keeper.ready)?.require(BytePathReady)?
  assert invalid_ready.path_bytes_hex.ends_with("2f6c6f636b2dff")
  assert fs.stat(invalid_utf8)?.ino == invalid_ready.inode
  let invalid_pid = f"{invalid_ready.pid}"
  let invalid_raw = ul.reference(s, "lslocks", ["--raw", "--noheadings", "--pid", invalid_pid, "--output", "PATH"])?
  assert invalid_raw.status == 0 and invalid_raw.stderr.is_empty()
  assert invalid_raw.stdout == bytes.concat([s.root.bytes(), b"/lock-\\xff\n"])
  let invalid_json = ul.reference(s, "lslocks", ["--json", "--pid", invalid_pid, "--output", "PATH"])?
  assert invalid_json.status == 0 and invalid_json.stderr.is_empty()
  assert invalid_json.stdout == bytes.concat([
    b"{\n   \"locks\": [\n      {\n         \"path\": \"",
    invalid_utf8.bytes(), b"\"\n      }\n   ]\n}\n",
  ])
  ul.keeper_send(invalid_keeper, "DONE")
  ul.finish_keeper(invalid_keeper)

  let duplicate_path = uu.at(s, "posix-duplicate")
  let duplicate_keeper = ul.start_keeper(s, script, ["hold", method, duplicate_path.display(), "duplicate"])?
  defer ul.stop_keeper(duplicate_keeper)
  let duplicate_ready = json.decode(duplicate_keeper.ready)?.require(LockReady)?
  let holders = ul.reference(s, "lslocks", ["--json", "--pid", f"{duplicate_ready.pid}", "--output", "PID,TYPE,HOLDERS"])?
  assert holders.status == 0 and holders.stderr.is_empty()
  let holder_document = json.decode(holders.stdout.utf8() ?? "")?
  let holder_rows = json.get(holder_document, ["locks"])?.require(List[Any])?
  assert holder_rows.len() == 1
  assert json.get(holder_document, ["locks", 0, "pid"])?.require(Int)? == duplicate_ready.pid
  assert json.get(holder_document, ["locks", 0, "type"])?.require(Str)? == "POSIX"
  let entries = json.get(holder_document, ["locks", 0, "holders"])?.require(List[Str])?
  assert ! entries.is_empty()
  for entry in entries { assert entry.starts_with(f"{duplicate_ready.pid},") }
  ul.keeper_send(duplicate_keeper, "DONE")
  ul.finish_keeper(duplicate_keeper)
  Ok()
}

test test_lslocks_list_columns_modes { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for args in [
    ["-H"], ["--list-columns", "--raw"], ["-H", "--json"],
    ["-H", "-r"], ["-H", "-J"], ["-H", "-b"],
  ] {
    parity(s, args)?
  }
  let listing = compared(s, ["--list-columns", "--raw"])?
  let text = listing.stdout.utf8()?
  for column in ["COMMAND", "PID", "TYPE", "SIZE", "INODE", "MAJ:MIN", "MODE", "M", "START", "END", "PATH", "BLOCKER", "HOLDERS"] {
    assert column in text, column
  }
  assert "<string|number>" in text
}

test test_lslocks_raw_noheadings_and_json { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  output_lock_modes_case(s)?
}

test test_lslocks_defaults_and_lslocks_columns { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  let lock_path = uu.at(s, "selected-columns")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", "posix-r-", lock_path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  let pid = f"{ready.pid}"
  let _ = compared_with_stdout_dump(s, ["--pid", pid])?
  let _ = compared_with_stdout_dump(s, ["--output-all", "--pid", pid])?
  let _ = compared_with_stdout_dump(s, ["--output-all", "--pid", pid], {LSLOCKS_COLUMNS: "TYPE"})?
  let _ = compared_with_stdout_dump(s, ["--pid", pid], {LSLOCKS_COLUMNS: "TYPE,PID,COMMAND"})?
  let _ = compared_with_stdout_dump(s, ["--pid", pid], {LSLOCKS_COLUMNS: "+BLOCKER"})?
  let _ = compared_with_stdout_dump(s, ["--output", "PATH,TYPE", "--pid", pid], {LSLOCKS_COLUMNS: "PID"})?
  ul.keeper_send(keeper, "DONE")
  ul.finish_keeper(keeper)
}

test test_lslocks_output_column_names_and_errors { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  let lock_path = uu.at(s, "output-columns")
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py"
  let keeper = ul.start_keeper(s, script, ["hold", "posix-r-", lock_path.display(), "none"])?
  defer ul.stop_keeper(keeper)
  let ready = json.decode(keeper.ready)?.require(LockReady)?
  let pid = f"{ready.pid}"
  for args in [
    ["--output", "path,type", "--pid", pid], ["--output", "+BLOCKER", "--pid", pid],
    ["--output", "PID,PID", "--pid", pid], ["--output", "PID", "--output", "TYPE", "--pid", pid],
    ["-o", "path,type", "--pid", pid],
  ] {
    let result = compared_with_stdout_dump(s, args)?
    assert result.status == 0 and ! result.stdout.is_empty(), args.join(" ")
  }
  ul.keeper_send(keeper, "DONE")
  ul.finish_keeper(keeper)
  for args in [
    ["--output", ""], ["--output", "UNKNOWN"], ["-o", "UNKNOWN"], ["-o"],
  ] {
    rejected(s, args)?
  }
}

test test_lslocks_help_and_usage_errors { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for args in [["-h"], ["--help"]] {
    let result = compared(s, args)?
    assert result.status == 0 and ! result.stdout.is_empty()
  }
  let stray_operand = compared(s, ["stray"])?
  assert stray_operand.status == 0
  for args in [
    ["--output"], ["--pid", "bad"], ["--unknown"], ["-z"], ["--o"], ["--raw=ignored"],
    ["--pid"], ["--pid", "0"],
    ["--pid", "-1"], ["--pid", "2147483648"], ["--filter"], ["-Q"],
    ["--json", "--raw"], ["-J", "-r"],
  ] {
    rejected(s, args)?
  }
}

test test_lslocks_smartcols_json_environment { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  json_environment_case(s)?
}

test test_lslocks_deleted_path_and_owner_exit { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  deleted_path_and_owner_exit_case(s)?
}

test test_lslocks_path_escaping_and_non_utf8_names { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  path_escaping_case(s)?
}

test test_lslocks_reference_path_bytes { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  reference_path_case(s)?
}

test test_lslocks_filter_errors { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  for expression in ["TYPE ==", "UNKNOWN == 'POSIX'", "PID == -1", "TYPE =~ ["] {
    rejected(s, ["--filter", expression])?
  }
}

test test_lslocks_filter_values_and_hidden_columns { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  filtered_lock_case(s)?
}

test test_lslocks_default_and_selected_lock_rows { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  selected_lock_rows_case(s)?
}

test test_lslocks_version_provenance { |ctx|
  let s = uu.scene(ctx)?
  for args in [["--version"], ["-V"]] {
    let reference = ul.reference(s, "lslocks", args, timeout: 5s)?
    let candidate = uu.invoke(s, "lslocks", args, timeout: 5s)?
    ul.version_identity(reference, candidate)
  }
}

test test_lslocks_upstream_flock_sh { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "flock-sh", false)?
}

test test_lslocks_upstream_flock_ex { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "flock-ex", false)?
}

test test_lslocks_upstream_posix_read { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "posix-r-", false)?
}

test test_lslocks_upstream_posix_write { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "posix--w", false)?
}

test test_lslocks_upstream_posix_read_write { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "posix-rw", false)?
}

test test_lslocks_upstream_ofd_read { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd-r-", false)?
}

test test_lslocks_upstream_ofd_write { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd--w", false)?
}

test test_lslocks_upstream_ofd_read_write { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd-rw", false)?
}

test test_lslocks_upstream_lease_write { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "lease-w", false)?
}

test test_lslocks_upstream_flock_sh_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "flock-sh", true)?
}

test test_lslocks_upstream_flock_ex_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "flock-ex", true)?
}

test test_lslocks_upstream_ofd_read_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd-r-", true)?
}

test test_lslocks_upstream_ofd_write_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd--w", true)?
}

test test_lslocks_upstream_ofd_read_write_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "ofd-rw", true)?
}

test test_lslocks_posix_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "posix-r-", true)?
}

test test_lslocks_upstream_lease_write_holders { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  upstream_lock_case(s, "lease-w", true)?
}

test test_lslocks_blocker_flock { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  blocked_lock_case(s, "flock-ex")?
}

test test_lslocks_blocker_posix { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  blocked_lock_case(s, "posix--w")?
}

test test_lslocks_blocker_ofd { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  blocked_lock_case(s, "ofd--w")?
}

test test_lslocks_all_columns_types_and_json { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  all_columns_json_case(s)?
}

test test_lslocks_bytes_and_human_size { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  byte_size_case(s)?
}

test test_lslocks_ranges_and_eof { |ctx|
  let s = uu.scene(ctx)?
  ul.require_private_namespace()
  ranged_lock_case(s, "posix-rw", ["0:0:READ", "2:2:WRITE"])?
  ranged_lock_case(s, "posix-eof-w", ["2:0:WRITE"])?
  ranged_lock_case(s, "ofd-rw", ["0:0:READ", "2:2:WRITE"])?
  ranged_lock_case(s, "ofd-eof-w", ["2:0:WRITE"])?
}
