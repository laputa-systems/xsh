
test test_tail_hash_table_held_truncate_probe { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file", "same data\n")
  let original = fs.stat(uu.at(s, "file"), follow_symlinks: true)?
  let tail_plan = follow(s, ["--follow=name", "file"])?
  let child = spawn tail_plan?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(300ms)
  alive(child)
  let initial = streams(s)?
  assert initial.stdout == bytes.from_text("same data\n")
  assert initial.stderr.is_empty()
  overwrite_in_place(s, "file", "same data\n")
  time.sleep(300ms)
  let stable = streams(s)?
  assert stable.stdout == bytes.from_text("same data\n")
  assert stable.stderr.is_empty()
  let stable_stat = fs.stat(uu.at(s, "file"), follow_symlinks: true)?
  assert stable_stat.dev == original.dev and stable_stat.ino == original.ino and stable_stat.size == original.size
  let writer_plan = process.command_argv(
    p"/bin/sh",
    [p"/bin/sh", p"-c", r"""exec 3> file; sleep 1; printf 'same data\n' >&3""" as Path],
    s.root,
    {},
    b"",
    uu.at(s, ".truncate-out"),
    uu.at(s, ".truncate-err"),
  )
  let writer = spawn writer_plan?
  defer writer.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(300ms)
  let held = fs.stat(uu.at(s, "file"), follow_symlinks: true)?
  assert held.dev == original.dev and held.ino == original.ino and held.size == 0
  let window = streams(s)?
  uu.stderr_is(window, "tail: file: file truncated\n")
  let stable_err = stable.stderr.utf8()?
  let window_err = window.stderr.utf8()?
  print f"in_place_identity={original.dev}:{original.ino}:{original.size}->{stable_stat.dev}:{stable_stat.ino}:{stable_stat.size}; stderr={stable_err}"
  print f"held_identity={held.dev}:{held.ino}:{held.size}; stderr={window_err}"
  time.sleep(800ms)
  assert process.wait_timeout([writer], 0ms)? != null
  let restored = fs.stat(uu.at(s, "file"), follow_symlinks: true)?
  assert restored.dev == original.dev and restored.ino == original.ino and restored.size == original.size
  assert uu.at(s, "file").read_text()? == "same data\n"
  child.cancel(signal: "KILL", kill_after: 0ms)
}
