error FetchError = Busy | Gone

var calls = 0

proc fetch(fail_with: Str) -> Result[Str] {
  calls += 1
  if fail_with == "busy" {
    return Err(FetchError.Busy("busy"))
  }

  if fail_with == "gone" {
    return Err(FetchError.Gone("gone"))
  }

  "data"
}

proc guarded(dir: Path) -> Result[Int] {
  let lock = fp"{dir}/lock"
  lock.write("1")
  defer {
    lock.remove()
  }

  let n = "x" as Int
  n
}

let busy = retry [10ms, 20ms] on (FetchError.Busy {..} | FetchError.Gone {..}) { fetch("busy")? }
print f"{busy is Err(_)} {calls}"
calls = 0
let gone = retry [10ms, 20ms] on (FetchError.Busy {..}) { fetch("gone")? }
print f"{gone is Err(_)} {calls}"
calls = 0
let any = retry [10ms, 20ms] { fetch("gone")? }
print f"{any is Err(_)} {calls}"
let fine = retry [10ms] { fetch("none")? }
print (fine ?? "none")
tempdir dir {
  print f"{guarded(dir) is Err(_)} {fp"{dir}/lock".exists()?}"
}
