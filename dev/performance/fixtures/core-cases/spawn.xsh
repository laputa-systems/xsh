cli main(count: Int) {
  let first = spawn run sh -c "exit 0" ?
  defer first.cancel(signal: "KILL", kill_after: 0ms)
  var handles: List[ProcessHandle] = [first]
  var index = 1
  while index < count {
    let child = spawn run sh -c "exit 0" ?
    handles += [child]
    index += 1
  }
  let statuses = wait handles?
  for status in statuses {
    assert status.exit_code()? == 0
  }
  print statuses.len()
}
