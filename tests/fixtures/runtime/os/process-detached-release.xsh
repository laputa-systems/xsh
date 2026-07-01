let marker = fp"${ARGV[0]}"
let helper = fp"${ARGV[1]}"

proc scoped(marker: Path, helper: Path) [process, error] {
  let command = process.command_argv(helper, ["os-probe", "delayed-marker", marker.display(), "100"], detach: true)
  let _h = spawn command?
}

scoped(marker, helper)?
print "done"
