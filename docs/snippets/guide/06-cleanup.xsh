error FetchError = Busy | Gone

var attempts = 0

proc fetch() -> Result[Str] {
  attempts += 1
  if attempts < 3 {
    return Err(FetchError.Busy("try again"))
  }

  "payload"
}

proc publish(dir: Path, text: Str) -> Result[Unit] {
  let lock = fp"{dir}/.lock"
  lock.write("held\n")
  defer {
    lock.remove()
  }

  fp"{dir}/current.txt".write_atomic(text)
}

let body = retry [50ms, 100ms, 200ms] on (FetchError.Busy {..}) {
  fetch()?
}?

tempdir dir {
  publish(dir, body)?
  let names = fs.children(dir)? |> map .name |> sort()
  print f"{body} after {attempts} attempts; left: {names.join(" ")}"
}
