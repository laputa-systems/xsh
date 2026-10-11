let root = fs.tempdir()?
root.write(p"data", "payload")
let snapshot = root.read_result(p"data", max_bytes: 1024)?
print $snapshot.state
