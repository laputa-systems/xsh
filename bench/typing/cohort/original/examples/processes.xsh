proc run_checks() [process, time, error] {
  let first = spawn run true ?
  let second = spawn run sh -c "exit 0" ?
  let statuses = wait [first, second]?
  let command = process.command {
    run true
  }
  let measured = time.measure(command)?
  print f"commands ${statuses[0].ok} ${statuses[1].ok} ${measured.status.ok} ${measured.duration_ms >= 0}"
}

let shell = process.which("sh")?
let process_count = process.list()
  |> where .pid > 0
  |> count()
let host = system.hostname()?
let os = system.uname()?
let me = user.current()?
let same_user = user.by_uid(me.uid)?
let same_group = group.by_gid(me.gid)?
let term = process.signal("TERM")?

print "host" ${shell.name == "sh"} ${process_count > 0} ${host != ""} ${os.sysname != ""}
print "identity" ${same_user.uid == me.uid} ${same_group.gid == me.gid} ${me.name != ""} ${me.home.display() != ""}
print "signal" ${term.number > 0} ${time.now() > 0}
run_checks()?
