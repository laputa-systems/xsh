use support.uu
use support.util_linux as ul

type OutputFailure = {termination: ul.Termination, stdout: Bytes, stderr: Bytes}

type CommandCase = {util: Str, args: List[Path], error_args: List[Path]}

pure termination_label(termination: ul.Termination) -> Str {
  match termination {
    ul.Signaled(number) => f"signal {number}"
    ul.Exited(status) => f"exit {status}"
  }
}

proc output_failure(s: uu.Scene, util: Str, args: List[Path], is_reference: Bool, output_stream: Str) [fs, process, env, error] -> Result[OutputFailure, Error] {
  assert output_stream in ["stdout", "stderr"]
  let vars = {LC_ALL: "C", TZ: "UTC"}
  let words = if is_reference {
    ul.reference_argv(s, util, args, vars: vars)?
  } else {
    uu.argv(s, util, args, vars: vars)?
  }
  let stdout = if output_stream == "stdout" { p"/dev/full" } else { uu.at(s, f".{util}-stdout") }
  let stderr = if output_stream == "stderr" { p"/dev/full" } else { uu.at(s, f".{util}-stderr") }
  let status = process.run(process.command_argv(
    words[0], words, s.root, vars, b"", stdout, stderr, timeout: 3s,
  ))?
  let stdout_bytes = if output_stream == "stderr" { stdout.read_bytes()? } else { b"" }
  let stderr_bytes = if output_stream == "stdout" { stderr.read_bytes()? } else { b"" }
  if output_stream == "stderr" { stdout.remove() }
  if output_stream == "stdout" { stderr.remove() }
  let termination = if status.signaled() {
    ul.Signaled(status.signal_number()?)
  } else {
    ul.Exited(status.exit_code()?)
  }
  Ok({termination: termination, stdout: stdout_bytes, stderr: stderr_bytes})
}

proc closed_reader(s: uu.Scene, util: Str, args: List[Path], is_reference: Bool, pipe_action: Str) [fs, process, env, net, error] -> Result[OutputFailure, Error] {
  let invocation = f"{pipe_action}-{if is_reference { "reference" } else { "candidate" }}"
  let stdout = uu.at(s, f".{util}-{invocation}-stdout-fifo")
  let ready = uu.at(s, f".{util}-{invocation}-ready-fifo")
  let release = uu.at(s, f".{util}-{invocation}-release-fifo")
  let stderr = uu.at(s, f".{util}-{invocation}-pipe-stderr")
  fs.mkfifo(stdout, 0o600)
  fs.mkfifo(ready, 0o600)
  fs.mkfifo(release, 0o600)
  let stdout_reader = unix.open_fd(stdout, write: false)?
  var stdout_reader_open = true
  defer { if stdout_reader_open { unix.close_fd(stdout_reader) } }
  let ready_channel = unix.open_fd(ready, write: true)?
  defer unix.close_fd(ready_channel)
  let release_channel = unix.open_fd(release, write: true)?
  defer unix.close_fd(release_channel)

  let wrapper = uu.at(s, "closed-reader.xsh")
  uu.write(s, "closed-reader.xsh",
    "let ready_fd = unix.open_fd(Path(args[0]), write: true)?\n" +
    "defer unix.close_fd(ready_fd)\n" +
    "let release_fd = unix.open_fd(Path(args[1]))?\n" +
    "defer unix.close_fd(release_fd)\n" +
    "assert unix.write_fd(ready_fd, b\"R\")? == 1\n" +
    "let events = unix.poll_fd(release_fd, [\"readable\"], timeout_ms: 3000)?\n" +
    "assert \"readable\" in events\n" +
    "assert unix.read_fd(release_fd, 1)? == b\"R\"\n" +
    "assert args[2] in [\"default\", \"ignore\"]\n" +
    "let words = [Path(word) for word in args[3..]]\n" +
    "process.set_signal_action(\"PIPE\", args[2])?\n" +
    "unix.exec(process.command_argv(words[0], words))?\n")?

  let vars = {LC_ALL: "C", TZ: "UTC"}
  let command_words = if is_reference {
    ul.reference_argv(s, util, args, vars: vars)?
  } else {
    uu.argv(s, util, args, vars: vars)?
  }
  let wrapper_words = [s.ctx.xsh_bin, wrapper, ready, release, Path.parse_bytes(bytes.from_text(pipe_action))?].extend(command_words)
  let plan = process.command_argv(
    s.ctx.xsh_bin,
    wrapper_words,
    s.root,
    vars,
    b"",
    stdout,
    stderr,
    timeout: 5s,
  )
  let child = spawn plan?
  defer child.cancel(signal: "TERM", kill_after: 1s)
  let ready_events = unix.poll_fd(ready_channel, ["readable"], timeout_ms: 3000)?
  assert "readable" in ready_events, f"{util} did not reach the close barrier"
  assert unix.read_fd(ready_channel, 1)? == b"R"
  unix.close_fd(stdout_reader)?
  stdout_reader_open = false
  assert unix.write_fd(release_channel, b"R")? == 1
  guard let completed = process.wait_timeout([child], 3s)? else {
    return Err(error.failure(f"{util} did not finish after stdout closed"))
  }
  let stderr_bytes = stderr.read_bytes()?
  let termination = if completed.status.signaled() {
    ul.Signaled(completed.status.signal_number()?)
  } else {
    ul.Exited(completed.status.exit_code()?)
  }
  Ok({termination: termination, stdout: b"", stderr: stderr_bytes})
}

proc assert_output_failure_parity(s: uu.Scene, util: Str, args: List[Path], output_stream: Str) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = output_failure(s, util, args, true, output_stream)?
  let candidate = output_failure(s, util, args, false, output_stream)?
  assert reference.termination != ul.Exited(0), util
  if output_stream == "stderr" {
    assert reference.stdout.is_empty(), util
  }
  assert candidate.termination == reference.termination, util
  assert candidate.stdout == reference.stdout, util
  assert candidate.stderr == reference.stderr, util
  Ok()
}

test test_util_linux_stdout_and_stderr_write_failures_match_reference { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  guard p"/dev/full".exists() else { test.skip("requires /dev/full"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let memory_root = uu.at(s, "memory")
  memory_root.mkdir()
  let _ = fs.copy_tree(
    fp"{ctx.core_dir}/tests/fixtures/util-linux/memory/sparse",
    memory_root,
    overwrite: true,
    follow_symlinks: false,
  )?
  let commands: List[CommandCase] = [
    {util: "lslocks", args: [p"--list-columns"], error_args: [p"--output", p"NO_SUCH_COLUMN"]},
    {util: "lsipc", args: [p"--global"], error_args: [p"--time-format=invalid"]},
    {util: "lsmem", args: [p"--sysroot", memory_root], error_args: [p"--summary=invalid"]},
    {util: "mountpoint", args: [p"--show", p"/proc"], error_args: [p"--not-an-option"]},
    {util: "namei", args: [p"--", s.root], error_args: [p"-Z"]},
  ]
  for command in commands {
    assert_output_failure_parity(s, command.util, command.args, "stdout")?
    let reference = ul.reference_paths(s, command.util, command.error_args, timeout: 5s, vars: {LC_ALL: "C", TZ: "UTC"})?
    let candidate = uu.invoke_paths(s, command.util, command.error_args, timeout: 5s, vars: {LC_ALL: "C", TZ: "UTC"})?
    ul.compare(reference, candidate)
    assert reference.status != 0 and reference.stdout.is_empty() and ! reference.stderr.is_empty(), command.util
    assert_output_failure_parity(s, command.util, command.error_args, "stderr")?
  }
}

test test_util_linux_closed_reader_termination_matches_reference { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let memory_root = uu.at(s, "memory")
  memory_root.mkdir()
  let _ = fs.copy_tree(
    fp"{ctx.core_dir}/tests/fixtures/util-linux/memory/sparse",
    memory_root,
    overwrite: true,
    follow_symlinks: false,
  )?
  let commands: List[CommandCase] = [
    {util: "lslocks", args: [p"--list-columns"], error_args: []},
    {util: "lsipc", args: [p"--global"], error_args: []},
    {util: "lsmem", args: [p"--sysroot", memory_root], error_args: []},
    {util: "mountpoint", args: [p"--show", p"/proc"], error_args: []},
    {util: "namei", args: [p"--", s.root], error_args: []},
  ]
  var mismatches: List[Str] = []
  for pipe_action in ["default", "ignore"] {
    for command in commands {
      let reference = closed_reader(s, command.util, command.args, true, pipe_action)?
      let candidate = closed_reader(s, command.util, command.args, false, pipe_action)?
      assert reference.termination != ul.Exited(0), f"{command.util} {pipe_action}"
      if reference.termination != candidate.termination {
        mismatches += [f"{command.util} {pipe_action}: reference {termination_label(reference.termination)} stderr={reference.stderr.utf8()?}; candidate {termination_label(candidate.termination)} stderr={candidate.stderr.utf8()?}"]
      }
      if reference.stderr != candidate.stderr {
        mismatches += [f"{command.util} {pipe_action}: stderr differs: reference {reference.stderr.utf8()?}; candidate {candidate.stderr.utf8()?}"]
      }
    }
  }
  assert mismatches.is_empty(), mismatches.join("\n")
}

test test_util_linux_lsmem_preserves_raw_sysroot_argv { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let memory_root = uu.at_bytes(s, b"memory-\xff")?
  memory_root.mkdir()
  let _ = fs.copy_tree(
    fp"{ctx.core_dir}/tests/fixtures/util-linux/memory/sparse",
    memory_root,
    overwrite: true,
    follow_symlinks: false,
  )?
  let args = [p"--sysroot", memory_root]
  let reference = ul.reference_paths(s, "lsmem", args, timeout: 5s, vars: {LC_ALL: "C", TZ: "UTC"})?
  let candidate = uu.invoke_paths(s, "lsmem", args, timeout: 5s, vars: {LC_ALL: "C", TZ: "UTC"})?
  ul.compare(reference, candidate)
}

test test_util_linux_lsipc_preserves_raw_option_error_bytes { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let option = Path.parse_bytes(b"--time-format=\xff")?
  let reference = ul.reference_paths(s, "lsipc", [option], timeout: 5s, vars: {LC_ALL: "C", TZ: "UTC"})?
  let candidate = uu.invoke_paths(s, "lsipc", [option], timeout: 5s, vars: {LC_ALL: "C", TZ: "UTC"})?
  ul.compare(reference, candidate)
  assert reference.status != 0
  assert reference.stdout.is_empty()
  assert ! reference.stderr.is_empty()
}
