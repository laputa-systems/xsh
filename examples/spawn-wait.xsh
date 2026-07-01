let ok = spawn run true ?
let expected_failure = spawn run false ?
let statuses = wait [ok, expected_failure]?
print statuses[0].ok statuses[1].exited_with(1)
