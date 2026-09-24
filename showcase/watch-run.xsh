#!/usr/bin/env -S xsh --
# Watch Run
# Watch files for timestamp changes and rerun a command.
# In --once mode, report the child command's failure to the caller.
# Commands have no implicit deadline; SIGINT/SIGTERM cancel their process group.
# Usage: xsh showcase/watch-run.xsh -- [--root DIR] [--ext EXT] COMMAND [ARGS...]
# Example: xsh showcase/watch-run.xsh -- --root src --ext rs cargo test
type Opts = {root: Path, ext: List[Str], interval: Int, once: Bool, cmd: List[Str]}

on SIGINT [error] {
  abort(3)
}

on SIGTERM [error] {
  abort(3)
}

proc stamp(root: Path, exts: List[Str]) [fs] -> Int {
  let ext_set = set.from(exts)

  fs.files(root)
    |> where exts.len() == 0 or set.has(ext_set, .path.ext())
    |> map .modified
    |> sum
}

proc main(...argv: List[Str]) [fs, process, time, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      root: {
        form: "--root DIR",
        default: p".",
      },
      ext: {
        form: "--ext EXT",
        repeated: true,
      },
      interval: {
        form: "--interval N",
        default: 1,
      },
      once: {
        form: "--once",
        default: false,
      },
      cmd: {
        form: "...COMMAND",
        repeated: true,
        required: true,
      },
    },
  )?

  let root = opts.root.resolve()?
  let exts = opts.ext
  let interval = if opts.interval < 1 { 1 } else { opts.interval }
  print f"watching ${root.display()}  interval=${interval}s  command=${opts.cmd.join(" ")}"
  var last_stamp = stamp(root, exts)
  var run_count = 0
  var trigger = true

  while true {
    if trigger {
      run_count += 1
      print f"[run ${run_count}]"
      let cmd = process.command_argv(opts.cmd[0], opts.cmd)
      let status = process.run(cmd)?

      if ! status.exited_with(0) {
        if status.exited() {
          print f"  exit ${status.exit_code()?}"
        } else if status.signaled() {
          print f"  signal ${status.signal_number()?}"
        } else {
          print "  child failed"
        }

        if opts.once {
          abort(1)
        }
      }

      # Re-snapshot so the command's own output doesn't trigger a false re-run
      last_stamp = stamp(root, exts)
      trigger = false

      if opts.once {
        return
      }
    }

    for _ in range(interval) {
      time.sleep(1s)?
    }

    let current_stamp = stamp(root, exts)

    if current_stamp != last_stamp {
      last_stamp = current_stamp
      trigger = true
      print "change detected"
    }
  }
}
