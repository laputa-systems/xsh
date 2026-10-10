type Ran = {status: Int, signaled: Bool, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], stdin = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "unshare")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/unshare.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: "", SHELL: "/bin/sh"}, stdin, out, err, timeout: 20s)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, signaled: status.signaled(), stdout: out.read_text()?, stderr: err.read_text()?})
}

# Skips the calling test when the kernel refuses an unprivileged user
# namespace, which every namespace test below needs.
proc require_user_namespaces(ctx: TestContext) [fs, process, error] -> Result[Bool] {
  let probe = invoke(ctx, ["-r", "true"])?
  if probe.status != 0 {
    test.skip(f"the kernel refuses unprivileged user namespaces: {probe.stderr.trim()}")
    return false
  }
  true
}

# A child xsh program, so no test depends on which shell the host provides.
proc program(ctx: TestContext, name: Str, source: Str) [fs, error] -> Path {
  test.temp_file(ctx, name:, contents: bytes.from_text(source))?
}

test test_unshare_map_root_user_makes_the_caller_root { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let probe = program(ctx, "root.xsh", "print unix.id()?.uid\nprint fp\"/proc/self/uid_map\".read_text()?.fields().join(\" \")\nprint fp\"/proc/self/setgroups\".read_text()?.trim()\n")
  let ran = invoke(ctx, ["-r", ctx.xsh_bin.display(), probe.display()])?
  assert ran.status == 0, ran.stderr
  let outer = unix.id()?.uid
  assert ran.stdout == f"0\n0 {outer} 1\ndeny\n", ran.stdout
}

test test_unshare_user_namespace_without_a_map_has_the_overflow_uid { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let probe = program(ctx, "overflow.xsh", "print unix.id()?.uid\nprint fp\"/proc/self/uid_map\".read_text()?.trim()\nprint fp\"/proc/self/setgroups\".read_text()?.trim()\n")
  let ran = invoke(ctx, ["-U", ctx.xsh_bin.display(), probe.display()])?
  assert ran.status == 0, ran.stderr
  let overflow = fp"/proc/sys/kernel/overflowuid".read_text()?.trim()
  assert ran.stdout == f"{overflow}\n\nallow\n", ran.stdout
}

test test_unshare_creates_the_namespace_each_flag_names { |ctx|
  guard require_user_namespaces(ctx) else { return }
  # -p and -T change the namespaces of children, which the kernel shows as
  # the *_for_children links; every other flag changes the process itself.
  let cases = [
    {flag: "-m", link: "mnt"},
    {flag: "-u", link: "uts"},
    {flag: "-i", link: "ipc"},
    {flag: "-n", link: "net"},
    {flag: "-C", link: "cgroup"},
    {flag: "-p -f", link: "pid_for_children"},
    {flag: "-T", link: "time_for_children"},
  ]
  for entry in cases {
    let here = fp"/proc/self/ns/{entry.link}".readlink()?
    let probe = program(ctx, f"link-{entry.link}.xsh", f"print fp\"/proc/self/ns/{entry.link}\".readlink()?.display()\nprint fp\"/proc/self/ns/user\".readlink()?.display()\n")
    let ran = invoke(ctx, ["-r", @entry.flag.split(" "), ctx.xsh_bin.display(), probe.display()])?
    # A kernel without time namespaces refuses -T; that is not a failure here.
    if ran.status != 0 and entry.flag == "-T" and ran.stderr.find("unshare failed") != null {
      continue
    }
    assert ran.status == 0, f"{entry.flag}: {ran.stderr}"
    let lines = ran.stdout.lines()
    assert lines[0] != here.display(), f"{entry.flag} left {entry.link} unchanged"
    assert lines[1] != p"/proc/self/ns/user".readlink()?.display(), "-r creates a user namespace"
  }
}

test test_unshare_leaves_other_namespaces_alone { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let probe = program(ctx, "others.xsh", "print p\"/proc/self/ns/net\".readlink()?.display()\nprint p\"/proc/self/ns/uts\".readlink()?.display()\n")
  let ran = invoke(ctx, ["-r", "-u", ctx.xsh_bin.display(), probe.display()])?
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.lines()
  assert lines[0] == p"/proc/self/ns/net".readlink()?.display()
  assert lines[1] != p"/proc/self/ns/uts".readlink()?.display()
}

test test_unshare_fork_makes_the_command_the_first_process_of_a_pid_namespace { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let probe = program(ctx, "pid.xsh", "print process.current_pid()?\n")
  let forked = invoke(ctx, ["-r", "-p", "-f", ctx.xsh_bin.display(), probe.display()])?
  assert forked.status == 0, forked.stderr
  assert forked.stdout == "1\n", forked.stdout
  # Without -f the program is not in the new namespace; its children would be.
  let plain = invoke(ctx, ["-r", "-p", "sh", "-c", "echo $$"])?
  assert plain.status == 0, plain.stderr
  assert plain.stdout != "1\n"
}

test test_unshare_relays_the_exit_status_of_the_command { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let exits = program(ctx, "exit.xsh", "exit 7\n")
  assert invoke(ctx, ["-r", ctx.xsh_bin.display(), exits.display()])?.status == 7
  assert invoke(ctx, ["-r", "-f", ctx.xsh_bin.display(), exits.display()])?.status == 7
  assert invoke(ctx, ["-r", "true"])?.status == 0
}

test test_unshare_repeats_a_signal_death_of_the_command { |ctx|
  guard require_user_namespaces(ctx) else { return }
  # With -f the command is a grandchild, so the signal has to be relayed.
  let ran = invoke(ctx, ["-r", "-f", "sh", "-c", "kill -TERM $$"])?
  assert ran.signaled, f"expected a signal death, got status {ran.status}"
  assert ran.status == 143
}

test test_unshare_reports_commands_it_cannot_start { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let missing = invoke(ctx, ["-r", "xsh-no-such-command"])?
  assert missing.status == 127
  assert missing.stderr == "unshare: failed to execute xsh-no-such-command: No such file or directory\n"
  let directory = invoke(ctx, ["-r", "/"])?
  assert directory.status == 126
  assert directory.stderr == "unshare: failed to execute /: Permission denied\n"
}

test test_unshare_rejects_invalid_arguments { |ctx|
  let propagation = invoke(ctx, ["--propagation", "bogus", "true"])?
  assert propagation.status == 1
  assert propagation.stderr == "unshare: unsupported propagation mode: bogus\n"
  let unknown = invoke(ctx, ["-x"])?
  assert unknown.status == 1
  assert unknown.stderr == "unshare: invalid option -- 'x'\nTry 'unshare --help' for more information.\n"
  let persistent = invoke(ctx, ["--uts=/tmp/xsh-never-created", "true"])?
  assert persistent.status == 1
  assert persistent.stderr == "unshare: --uts=/tmp/xsh-never-created: persistent namespace files are not supported\n"
}

test test_unshare_fails_when_the_kernel_refuses { |ctx|
  if user.current()?.uid == 0 { test.skip("a privileged caller is not refused"); return }
  # A mount namespace needs CAP_SYS_ADMIN unless a user namespace comes with it.
  let ran = invoke(ctx, ["-m", "true"])?
  assert ran.status == 1
  assert ran.stderr == "unshare: unshare failed: Operation not permitted\n"
}

test test_unshare_propagation_marks_the_new_mount_tree { |ctx|
  guard require_user_namespaces(ctx) else { return }
  # The root mount's optional fields name its peer group when it is shared.
  let probe = program(ctx, "propagation.xsh", """for line in fp"/proc/self/mountinfo".read_text()?.lines() {
  let fields = line.split(" ")
  if fields.len() > 6 and fields[4] == "/" {
    let shared = fields[6].starts_with("shared:")
    let word = if shared { "shared" } else { "private" }
    print $word
    break
  }
}
""")
  let private = invoke(ctx, ["-r", "-m", ctx.xsh_bin.display(), probe.display()])?
  assert private.status == 0, private.stderr
  assert private.stdout == "private\n", private.stdout
  let shared = invoke(ctx, ["-r", "-m", "--propagation", "shared", ctx.xsh_bin.display(), probe.display()])?
  assert shared.status == 0, shared.stderr
  assert shared.stdout == "shared\n", shared.stdout
  let explicit = invoke(ctx, ["-r", "-m", "--propagation", "private", ctx.xsh_bin.display(), probe.display()])?
  assert explicit.stdout == "private\n"
}

test test_unshare_mount_proc_shows_the_new_pid_namespace { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let probe = program(ctx, "proc.xsh", "print fp\"/proc/self/stat\".read_text()?.fields()[0]\n")
  let ran = invoke(ctx, ["-r", "-p", "-f", "--mount-proc", ctx.xsh_bin.display(), probe.display()])?
  if ran.status != 0 and ran.stderr.find("mount /proc failed") != null {
    test.skip(f"procfs cannot be mounted in this environment: {ran.stderr.trim()}")
    return
  }
  assert ran.status == 0, ran.stderr
  assert ran.stdout == "1\n", ran.stdout
}

test test_unshare_mount_proc_accepts_a_directory { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let dir = test.temp_dir(ctx, name: "procdir")?
  let probe = program(ctx, "procdir.xsh", f"print fp\"{dir}/self/stat\".read_text()?.fields()[0]\n")
  let missing = invoke(ctx, ["-r", "-m", f"--mount-proc={dir}/absent", "true"])?
  assert missing.status == 1
  assert missing.stderr == f"unshare: mount {dir}/absent failed: No such file or directory\n"
  let ran = invoke(ctx, ["-r", "-p", "-f", f"--mount-proc={dir}", ctx.xsh_bin.display(), probe.display()])?
  if ran.status != 0 and ran.stderr.find("failed: Operation not permitted") != null {
    test.skip(f"procfs cannot be mounted in this environment: {ran.stderr.trim()}")
    return
  }
  assert ran.status == 0, ran.stderr
  assert ran.stdout == "1\n", ran.stdout
}

test test_unshare_runs_the_login_shell_without_a_command { |ctx|
  guard require_user_namespaces(ctx) else { return }
  if ! p"/bin/sh".exists() { test.skip("requires /bin/sh"); return }
  let ran = invoke(ctx, ["-r"], b"echo from-shell\n")?
  assert ran.status == 0, ran.stderr
  assert ran.stdout == "from-shell\n"
}

test test_unshare_without_namespace_flags_runs_the_command { |ctx|
  let probe = program(ctx, "plain.xsh", "print \"ran\"\n")
  let ran = invoke(ctx, [ctx.xsh_bin.display(), probe.display()])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout == "ran\n"
}

test test_unshare_command_arguments_stay_separate { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let probe = program(ctx, "argv.xsh", "proc main(...args: List[Str]) [io] { for word in args { print $word } }\n")
  let ran = invoke(ctx, ["-r", "--", ctx.xsh_bin.display(), probe.display(), "--", "-r", "a b"])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout == "-r\na b\n", ran.stdout
}
