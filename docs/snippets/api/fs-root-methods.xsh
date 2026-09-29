let root = fs.tempdir()?
defer root.close()?
root.write(p"data", "payload")?
let snapshot = root.read_result(p"data", max_bytes: 1024)?
print $snapshot.state
