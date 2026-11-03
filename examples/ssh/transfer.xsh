##! Send a file over stdin and read it back from stdout without loading it into memory.
use session

let target = session.target("A")?
let scratch = fs.tempdir()?
let source = fp"{scratch.host_path()?}/payload.bin"
let echoed = fp"{scratch.host_path()?}/echoed.bin"
let _ = bytes.zero_at(source, 0, 8 * 1024 * 1024, create: true)?

with connection = ssh.connect(target)? {
  let result = ssh.exec(connection, "cat", {stdin_path: source, stdout_path: echoed, timeout_ms: 60000})?
  assert result.exit_code == 0
  print f"sent and received {echoed.metadata()?.size} bytes, equal: {hash.sha256(source)? == hash.sha256(echoed)?}"
}
