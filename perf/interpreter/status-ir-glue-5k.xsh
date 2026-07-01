pure status_score(status: Status, expected: Int) -> Result[Int] {
  var total = status.kind.count_chars()

  if status.ok {
    total += 3
  }

  if status.success {
    total += 5
  }

  if status.exited() {
    let code = status.exit_code()?

    if status.exited_with(expected) {
      total += code + 11
    } else {
      total += code + 17
    }
  } else if status.signaled() {
    total += status.signal_number()? + 23
  }

  total
}

let ok_status = run.status true
let fail_status = run.status false
var i = 0
var total = 0

while i < 5000 {
  let status = if i % 4 == 0 { ok_status } else { fail_status }
  total += status_score(status, i % 2)?
  i += 1
}

print $total % 256
