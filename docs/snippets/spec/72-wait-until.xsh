proc await_socket(socket: Path) [fs, time, error] {
  # begin example
  wait until socket.exists() within 30s every 250ms
  # end example
}

proc await_lease(lease: Path, patience: Duration) [fs, time, error] {
  wait until ! lease.exists() within patience backoff 100ms..5s
}

let root = fs.tempdir()?
defer root.close()
let marker = fp"{root.host_path()?}/ready"
marker.write("")
await_socket(marker)
let late = try { await_lease(marker, 20ms)? }
print f"{late is Err(is Timeout)}"
