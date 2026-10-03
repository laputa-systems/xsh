let me = user.current()?

let oldest = process.list()?
  |> where .uid == me.uid
  |> sort-by(desc: true) .runtime_seconds
  |> take(3)

for p in oldest {
  print f"${p.pid:>7} ${p.runtime_seconds:>9}s ${p.command}"
}

for m in fs.mounts()? |> where .capacity_percent >= 90 {
  print f"${m.mounted_on} is ${m.capacity_percent}% full (${m.fstype})"
}

let listeners = process.ports()?
  |> where .state == "LISTEN"
  |> unique-by .local_port
  |> sort-by .local_port

for l in listeners {
  print f"${l.protocol} ${l.local_port:>5} ${l.command} (pid ${l.pid})"
}
