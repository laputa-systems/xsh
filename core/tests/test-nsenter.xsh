use core.lib.proc_target as target_files

type Ran = {status: Int, signaled: Bool, stdout: Str, stderr: Str}

# A process that holds a set of namespaces open while a test enters them.
type Holder = {pid: Int, release: Path, handle: ProcessHandle}

proc run_applet(ctx: TestContext, tool: Str, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: tool)?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/{tool}.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err, timeout: 20s)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, signaled: status.signaled(), stdout: out.read_text()?, stderr: err.read_text()?})
}

proc nsenter(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  run_applet(ctx, "nsenter", args)
}

proc require_user_namespaces(ctx: TestContext) [fs, process, error] -> Result[Bool] {
  let probe = run_applet(ctx, "unshare", ["-r", "true"])?
  if probe.status != 0 {
    test.skip(f"the kernel refuses unprivileged user namespaces: {probe.stderr.trim()}")
    return false
  }
  true
}

proc program(ctx: TestContext, name: Str, source: Str) [fs, error] -> Path {
  test.temp_file(ctx, name:, contents: bytes.from_text(source))?
}

# Starts `unshare FLAGS` around a program that signals readiness and waits for
# the release file, and returns the host pid of that program.
proc hold(ctx: TestContext, flags: List[Str]) [fs, process, time, error] -> Result[Holder] {
  let root = test.temp_dir(ctx, name: "holder")?
  let ready = fp"{root}/ready"
  let release = fp"{root}/release"
  let source = f"""cli main(ready: Path, release: Path) {{
  ready.write("up")
  for _ in range(0, 1500) {{
    break when release.exists()?
    time.sleep(20ms)
  }}
}}
"""
  let holder = program(ctx, f"holder-{root.basename()}.xsh", source)
  let words = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/unshare.xsh".display(), @flags, ctx.xsh_bin.display(), holder.display(), ready.display(), release.display()]
  let handle = spawn process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C"})?
  for _ in range(0, 500) {
    break when ready.exists()
    time.sleep(10ms)
  }
  if !ready.exists() {
    handle.cancel(signal: "KILL", kill_after: 0ms)
    return Err(error.failure("the namespace holder did not start"))
  }
  # The holder is the process whose command line names its script; its
  # ancestors are the unshare applet and, with --fork, the relaying child.
  var found = null
  for entry in process.list()? {
    if entry.argv.find(holder.display()) != null and entry.pid != handle.pid {
      found = entry.pid
    }
  }
  guard let pid = found else {
    handle.cancel(signal: "KILL", kill_after: 0ms)
    return Err(error.failure("the namespace holder is not running"))
  }
  Ok({pid: pid, release: release, handle: handle})
}

proc link(pid: Int, name: Str) [fs, error] -> Result[Str] {
  Ok(fp"/proc/{pid}/ns/{name}".readlink()?.display())
}

test test_nsenter_enters_the_user_and_network_namespaces_of_a_process { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let holder = hold(ctx, ["-U", "-r", "-n"])?
  defer holder.release.write("")
  let probe = program(ctx, "probe.xsh", "print fp\"/proc/self/ns/net\".readlink()?.display()\nprint unix.id()?.uid\n")
  let ran = nsenter(ctx, ["-t", f"{holder.pid}", "-U", "--preserve-credentials", "-n", ctx.xsh_bin.display(), probe.display()])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout == f"{link(holder.pid, "net")?}\n0\n", ran.stdout
  assert link(holder.pid, "net")? != fp"/proc/self/ns/net".readlink()?.display()
}

test test_nsenter_takes_namespace_files_by_name { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let holder = hold(ctx, ["-U", "-r", "-u"])?
  defer holder.release.write("")
  let probe = program(ctx, "uts.xsh", "print fp\"/proc/self/ns/uts\".readlink()?.display()\n")
  let user_file = fp"/proc/{holder.pid}/ns/user"
  let uts = fp"/proc/{holder.pid}/ns/uts"
  # The long form takes `=FILE`; the short form attaches the file directly.
  let long = nsenter(ctx, [f"--user={user_file}", "--preserve-credentials", f"--uts={uts}", ctx.xsh_bin.display(), probe.display()])?
  assert long.status == 0, long.stderr
  assert long.stdout == f"{link(holder.pid, "uts")?}\n", long.stdout
  let short = nsenter(ctx, [f"-U{user_file}", "--preserve-credentials", f"-u{uts}", ctx.xsh_bin.display(), probe.display()])?
  assert short.status == 0, short.stderr
  assert short.stdout == f"{link(holder.pid, "uts")?}\n", short.stdout
}

test test_nsenter_fork_puts_the_command_in_the_entered_pid_namespace { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let holder = hold(ctx, ["-U", "-r", "-p", "-f"])?
  defer holder.release.write("")
  let probe = program(ctx, "pid.xsh", "print fp\"/proc/self/ns/pid\".readlink()?.display()\nprint fp\"/proc/self/ns/pid_for_children\".readlink()?.display()\n")
  let inside = link(holder.pid, "pid")?
  let forked = nsenter(ctx, ["-t", f"{holder.pid}", "-U", "--preserve-credentials", "-p", ctx.xsh_bin.display(), probe.display()])?
  assert forked.status == 0, forked.stderr
  assert forked.stdout == f"{inside}\n{inside}\n", forked.stdout
  # --no-fork leaves the command in its own pid namespace; only the
  # namespace its children would start in has changed.
  # The kernel gives a process whose children will start in another pid
  # namespace no threads, so the unforked command is a shell, not xsh.
  let kept = nsenter(ctx, ["-t", f"{holder.pid}", "-U", "--preserve-credentials", "-p", "-F", "sh", "-c", "readlink /proc/$$/ns/pid; readlink /proc/$$/ns/pid_for_children"])?
  assert kept.status == 0, kept.stderr
  let lines = kept.stdout.lines()
  assert lines[0] == fp"/proc/self/ns/pid".readlink()?.display()
  assert lines[1] == inside
}

test test_nsenter_resets_credentials_when_entering_a_user_namespace { |ctx|
  guard require_user_namespaces(ctx) else { return }
  # The holder's namespace denies setgroups, which dropping the groups needs.
  let holder = hold(ctx, ["-U", "-r"])?
  defer holder.release.write("")
  let ran = nsenter(ctx, ["-t", f"{holder.pid}", "-U", "true"])?
  assert ran.status == 1
  assert ran.stderr == "nsenter: setgroups failed: Operation not permitted\n"
  let kept = nsenter(ctx, ["-t", f"{holder.pid}", "-U", "--preserve-credentials", "-S", "0", "true"])?
  assert kept.status == 0, kept.stderr
  let gid = nsenter(ctx, ["-t", f"{holder.pid}", "-U", "--preserve-credentials", "-S", "5", "-G", "6", "true"])?
  assert gid.status == 1
  assert gid.stderr == "nsenter: setgid() failed: Invalid argument\n"
}

test test_nsenter_sets_the_root_and_working_directories { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let holder = hold(ctx, ["-U", "-r"])?
  defer holder.release.write("")
  let dir = test.temp_dir(ctx, name: "wd")?
  let probe = program(ctx, "cwd.xsh", "print fs.cwd()?.display()\n")
  let ran = nsenter(ctx, ["-t", f"{holder.pid}", "-U", "--preserve-credentials", f"-w{dir}", ctx.xsh_bin.display(), probe.display()])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout == f"{dir}\n", ran.stdout
  # A bare -w takes the working directory of the target process.
  let target = fp"/proc/{holder.pid}/cwd".readlink()?.display()
  let bare = nsenter(ctx, ["-t", f"{holder.pid}", "-U", "--preserve-credentials", "-w", ctx.xsh_bin.display(), probe.display()])?
  assert bare.status == 0, bare.stderr
  assert bare.stdout == f"{target}\n", bare.stdout
  let missing = nsenter(ctx, ["-t", f"{holder.pid}", "-U", "--preserve-credentials", "-w/xsh-no-such-dir", "true"])?
  assert missing.status == 1
  assert missing.stderr == "nsenter: cannot open /xsh-no-such-dir: No such file or directory\n"
}

test test_nsenter_relays_exit_status_and_signal { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let holder = hold(ctx, ["-U", "-r"])?
  defer holder.release.write("")
  let base = ["-t", f"{holder.pid}", "-U", "--preserve-credentials"]
  assert nsenter(ctx, base.extend(["sh", "-c", "exit 9"]))?.status == 9
  let signaled = nsenter(ctx, base.extend(["sh", "-c", "kill -TERM $$"]))?
  assert signaled.signaled
  assert signaled.status == 143
}

test test_nsenter_reports_what_it_cannot_do { |ctx|
  let none = nsenter(ctx, ["-t", "1", "true"])?
  assert none.status == 1
  assert none.stderr == "nsenter: no namespace specified\n"
  let target = nsenter(ctx, ["-U", "true"])?
  assert target.status == 1
  assert target.stderr == "nsenter: no target PID specified\n"
  let invalid = nsenter(ctx, ["-t", "abc", "-U", "true"])?
  assert invalid.status == 1
  assert invalid.stderr == "nsenter: invalid PID argument\n"
  let zero = nsenter(ctx, ["-t", "0", "-U", "true"])?
  assert zero.stderr == "nsenter: invalid PID argument\n"
  let gone = nsenter(ctx, ["-t", "2147483646", "-U", "true"])?
  assert gone.status == 1
  assert gone.stderr == "nsenter: cannot open /proc/2147483646/ns/user: No such file or directory\n"
  let uid = nsenter(ctx, ["-t", "1", "-U", "-S", "abc", "true"])?
  assert uid.status == 1
  assert uid.stderr == "nsenter: failed to parse uid: 'abc'\n"
  let unknown = nsenter(ctx, ["-x"])?
  assert unknown.status == 1
  assert unknown.stderr == "nsenter: invalid option -- 'x'\nTry 'nsenter --help' for more information.\n"
}

test test_nsenter_reports_commands_it_cannot_start { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let holder = hold(ctx, ["-U", "-r"])?
  defer holder.release.write("")
  let base = ["-t", f"{holder.pid}", "-U", "--preserve-credentials"]
  let missing = nsenter(ctx, base.extend(["xsh-no-such-command"]))?
  assert missing.status == 127
  assert missing.stderr == "nsenter: failed to execute xsh-no-such-command: No such file or directory\n"
  let directory = nsenter(ctx, base.extend(["/"]))?
  assert directory.status == 126
  assert directory.stderr == "nsenter: failed to execute /: Permission denied\n"
}

test test_target_files_name_the_namespaces_root_and_cwd_of_a_process {
  let own = process.current_pid()?
  assert target_files.namespace_file(own, "net") == fp"/proc/{own}/ns/net"
  # The handle is the one the kernel exposes for this very process, so it
  # resolves to the same namespace as the self link.
  assert target_files.namespace_file(own, "net").readlink()? == fp"/proc/self/ns/net".readlink()?
  assert fs.stat(target_files.root_dir(own), follow_symlinks: true)?.kind == "dir"
  assert fs.stat(target_files.cwd_dir(own), follow_symlinks: true)?.kind == "dir"
}
