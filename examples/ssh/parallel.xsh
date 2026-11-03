##! Run one command on two independent sessions concurrently.
use session

let targets = [session.target("A")?, session.target("B")?]
let answers = targets |> par-map(jobs: 2) { |target|
  with connection = ssh.connect(target)? {
    let result = ssh.exec(connection, "hostname", {})?
    result.stdout.utf8()?.trim()
  }
}
for answer in answers {
  print f"host answered: {answer?}"
}
