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
time.sleep(1ms)

let command = process.command {
  run true
}

let measured = time.measure(command)?
print ${shell.name == "sh"} ${process_count > 0} ${host != ""} ${os.sysname != ""} ${me.uid >= 0}
print ${same_user.uid == me.uid} ${same_group.gid == me.gid} ${term.number > 0} ${time.now() > 0}
print (time.format(0, "%Y", utc: true)?)
print ${me.name != ""} ${me.home.display() != ""} $measured.status.ok ${measured.duration_ms >= 0}
