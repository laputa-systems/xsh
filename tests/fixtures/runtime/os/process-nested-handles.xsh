let ready1 = fp"${ARGV[0]}"
let leaked1 = fp"${ARGV[1]}"
let ready2 = fp"${ARGV[2]}"
let leaked2 = fp"${ARGV[3]}"
let helper = fp"${ARGV[4]}"

proc inner(ready: Path, leaked: Path, helper: Path) [fs, process, time, error] {
  let _h = spawn process.command_argv(helper, ["os-probe", "group-leak", ready.display(), leaked.display()])?

  while ! ready.exists()? {
    time.sleep(10ms)?
  }
}

proc outer(ready1: Path, leaked1: Path, ready2: Path, leaked2: Path, helper: Path) [fs, process, time, error] {
  let _h = spawn process.command_argv(helper, ["os-probe", "group-leak", ready1.display(), leaked1.display()])?

  while ! ready1.exists()? {
    time.sleep(10ms)?
  }

  inner(ready2, leaked2, helper)?
}

outer(ready1, leaked1, ready2, leaked2, helper)?
print "done"
