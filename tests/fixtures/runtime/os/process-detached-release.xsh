let marker = fp"${args[0]}"
let helper = fp"${args[1]}"

proc scoped(marker: Path, helper: Path) [process, error] {
  let command = process.command_argv(helper, ["os-probe", "delayed-marker", marker.display(), "100"], detach: true)
  let _h = spawn command?
}

scoped(marker, helper)?
print "done"
