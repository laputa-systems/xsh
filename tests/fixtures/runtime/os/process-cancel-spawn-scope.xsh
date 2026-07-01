let ready = fp"${ARGV[0]}"
let leaked = fp"${ARGV[1]}"
let helper = fp"${ARGV[2]}"

proc scoped(ready: Path, leaked: Path, helper: Path) [fs, process, time, error] {
  let _h = spawn process.command_argv(helper, ["os-probe", "group-leak", ready.display(), leaked.display()])?

  while ! ready.exists()? {
    time.sleep(10ms)?
  }
}

scoped(ready, leaked, helper)?
time.sleep(100ms)?
print "done"
