test test_failed_logger_spawn_reaps_service { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("child inventory uses Linux procfs")
    return
  }
  let root = test.temp_dir(ctx, name: "failed-logger-spawn")?
  let logger = fp"{root}/logger"
  logger.write("#!/xsh-missing-interpreter\n")?
  logger.chmod(0o755)?
  test.expect(ctx, r"""let logger = Path(args[0])
let service = process.command_argv("sleep", ["sleep", "30"])
let broken = process.command_argv(logger, [logger])
assert unix.spawn_logged_process_group(service, broken) is Err(_)
for task in fs.children(p"/proc/self/task")? {
  let children = fp"/proc/self/task/{task.name}/children".read_text()?
  assert children.trim() == "", children
}
""", status: 0, args: [logger])?
}

test test_spawn_setup_rejects_discarded_child_status { |ctx|
  let root = test.temp_dir(ctx, name: "spawn-discarded-status")?
  let marker = fp"{root}/started"
  test.expect(ctx, r"""let marker = Path(args[0])
let words = ["sh", "-c", f"printf started > '{marker}'"]
let limited = process.command_argv("sh", words, cpu_max: 80)
let service = process.command_argv("sh", words)
let logger = process.command_argv("cat", ["cat"])
process.set_signal_action("CHLD", "ignore")?
assert unix.spawn_process_group(limited, notify: true) is Err(_)
assert unix.spawn_process_group_log(limited, Path(args[1])) is Err(_)
assert unix.spawn_logged_process_group(service, logger) is Err(_)
assert process.signal_action("CHLD")? == "ignore"
process.set_signal_action("CHLD", "default")?
assert ! marker.exists()?
""", status: 0, args: [marker, fp"{root}/log"], env: {XSH_CGROUP_ROOT: root})?
}
