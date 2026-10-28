type Scene = {log: Str, err: Str}

proc compiler() [env] -> Str {
  e"CC" ?? "clang-23"
}

proc compiled_client(ctx: TestContext, root: Path) [fs, process, env, error] -> Result[Path] {
  let binary = fp"{root}/syslog_client"
  let source = fp"{ctx.core_dir}/tests/support/syslog_client.c"
  let cc = compiler()
  let status = process.run(process.command_argv(cc, [cc, "-O1", "-o", binary.display(), source.display()], root, {PATH: e"PATH" ?? "/usr/bin:/bin", LC_ALL: "C"}, timeout: 120s))?
  assert status.exited_with(0), f"{cc} could not build the syslog client"
  Ok(binary)
}

# Runs one scene inside new user and mount namespaces whose /dev is a private
# tmpfs, so the C library's hard-coded /dev/log is the service under test and the
# host's own is never touched. Returns null when the kernel refuses the namespaces.
proc run_scene(ctx: TestContext, name: Str, mode: Str, args: List[Str], namespaces: List[Str] = ["user", "mnt"], map_root: Bool = true) [fs, process, env, error] -> Result[Scene?] {
  let root = test.temp_dir(ctx, name: name)?
  let client = compiled_client(ctx, root)?
  let state = fp"{root}/state"
  state.mkdir()
  state.chmod(0o755)
  let log = fp"{root}/scene.log"
  let err = fp"{root}/scene.err"
  let script = fp"{ctx.core_dir}/tests/support/syslog_scene.sh"
  let argv = ["sh", script.display(), ctx.xsh_bin.display(), fp"{ctx.core_dir}/syslogd.xsh".display(), state.display(), log.display(), err.display(), client.display(), mode].extend(args)
  let plan = process.command_argv("sh", argv, root, {PATH: e"PATH" ?? "/usr/bin:/bin", LC_ALL: "C"})
  match linux.run_in_namespaces(plan, unshare: namespaces, map_root_user: map_root) {
    Err(failure) => {
      if "unshare failed" in failure.message { return Ok(null) }
      Err(failure)
    }
    Ok(status) => {
      let text = if err.exists()? { err.read_text()? } else { "" }
      if status.shell_code()? == 90 or ("mount:" in text and "denied" in text) { return Ok(null) }
      assert status.exited_with(0), f"scene {mode} failed: {text}"
      Ok({log: log.read_text()?, err: text})
    }
  }
}

proc records(scene: Scene) -> List[Str] {
  [line for line in scene.log.lines() if line.starts_with("seq=")]
}

test test_a_musl_syslog_client_is_recorded_with_its_tag_pid_facility_and_severity { |ctx|
  guard let scene = run_scene(ctx, "libc-basic", "basic", [])? else {
    test.skip("the kernel refuses unprivileged user and mount namespaces")
    return
  }
  let lines = records(scene)
  assert lines.len() == 2, scene.log
  assert "facility=daemon severity=warning priority=valid tag=\"cclient\"" in lines[0], lines[0]
  assert "msg=\"hello from libc 1\"" in lines[0], lines[0]
  assert "severity=info" in lines[1] and "msg=\"second message\"" in lines[1], lines[1]
  let fields = rx" pid=([0-9]+) uid=0 gid=0 .* tag_pid=([0-9]+) ".captures(lines[0])
  assert ! fields.is_empty() and fields[1] == fields[2], lines[0]
  assert "service-status=0" in scene.err and "socket-left-behind" not in scene.err, scene.err
}

test test_multiline_and_quoted_libc_messages_stay_on_one_escaped_line { |ctx|
  guard let scene = run_scene(ctx, "libc-multiline", "multiline", [])? else {
    test.skip("the kernel refuses unprivileged user and mount namespaces")
    return
  }
  let lines = records(scene)
  assert lines.len() == 1, scene.log
  assert "msg=\"line one\\nline two\\r\\nline three\\ttabbed \\\"quoted\\\" \\\\ backslash\"" in lines[0], lines[0]
}

test test_a_forged_record_in_a_libc_message_cannot_add_a_record { |ctx|
  guard let scene = run_scene(ctx, "libc-forge", "forge", [])? else {
    test.skip("the kernel refuses unprivileged user and mount namespaces")
    return
  }
  let lines = records(scene)
  assert lines.len() == 1 and scene.log.lines().len() == 3, scene.log
  assert lines[0].starts_with("seq=1 ") and "facility=auth severity=alert" in lines[0], lines[0]
  assert "\\nseq=999 pid=1 uid=0" in lines[0], lines[0]
}

test test_a_message_longer_than_the_libc_buffer_arrives_cut_by_the_client_library { |ctx|
  guard let scene = run_scene(ctx, "libc-long", "long", [])? else {
    test.skip("the kernel refuses unprivileged user and mount namespaces")
    return
  }
  let lines = records(scene)
  assert lines.len() == 1, scene.log
  assert " cut=0 " in lines[0], lines[0]
  let payload = rx"msg=\x22(x+)\x22$".captures(lines[0])[1]
  assert payload.byte_len() > 900 and payload.byte_len() < 1024, f"{payload.byte_len()} bytes survived the client library"
}

# The C library's log socket blocks when the receiver's queue is full, so a
# stalled receiver stalls every logging program, and the receiver loses nothing
# to the kernel while the senders wait. This is why the service drains the socket
# before it ever waits on its output.
test test_a_stalled_receiver_blocks_a_musl_client_which_then_loses_nothing { |ctx|
  guard let scene = run_scene(ctx, "libc-stalled", "stalled", ["150"])? else {
    test.skip("the kernel refuses unprivileged user and mount namespaces")
    return
  }
  assert "client-blocked" in scene.err, scene.err
  assert "client-finished" in scene.err, scene.err
  let lines = records(scene)
  assert lines.len() == 150, f"{lines.len()} of 150 arrived"
  let stop = [line for line in scene.log.lines() if line.starts_with("event=stop")]
  assert stop.len() == 1 and "offered=150 accepted=150" in stop[0], scene.log
}

test test_a_burst_of_libc_messages_is_recorded_in_order_without_loss { |ctx|
  guard let scene = run_scene(ctx, "libc-count", "count", ["150"])? else {
    test.skip("the kernel refuses unprivileged user and mount namespaces")
    return
  }
  let lines = records(scene)
  assert lines.len() == 150, f"{lines.len()} of 150 arrived"
  for index in range(150) {
    assert f"msg=\"counted {index}\"" in lines[index], lines[index]
    assert "facility=local3" in lines[index]
  }
}

test test_senders_with_different_uids_are_told_apart_by_the_kernel_credentials { |ctx|
  if unix.id()?.uid != 0 {
    test.skip("running clients as other users needs real root")
    return
  }
  guard let scene = run_scene(ctx, "libc-uids", "uids", [], ["mnt"], false)? else {
    test.skip("the kernel refuses mount namespaces")
    return
  }
  let lines = records(scene)
  assert lines.len() == 30, f"{lines.len()} of 30 arrived: {scene.err}"
  var seen: Map[Str, Int] = {}
  for line in lines {
    let fields = rx" pid=([0-9]+) uid=([0-9]+) gid=([0-9]+) ".captures(line)
    assert ! fields.is_empty(), line
    assert fields[2] == fields[3], line
    seen[fields[2]] = (seen.get(fields[2]) ?? 0) + 1
  }
  assert seen.len() == 3
  assert [seen.get(uid) ?? 0 for uid in ["1001", "1002", "1003"]] == [10, 10, 10]
}
