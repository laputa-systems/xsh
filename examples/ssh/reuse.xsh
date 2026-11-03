##! Run several commands over one authenticated connection.
use session

let target = session.target("A")?

with connection = ssh.connect(target)? {
  let info = ssh.info(connection)?
  print f"connected to {info.host_key} with {info.kex}"
  for command in ["hostname", "uname -s", "id -un"] {
    let result = ssh.exec(connection, command, {})?
    print f"{command}: {result.stdout.utf8()?.trim()}"
  }
}
