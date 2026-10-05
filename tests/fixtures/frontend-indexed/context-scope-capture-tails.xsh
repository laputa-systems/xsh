proc text_tail() [env, process, error] -> Result[Str] {
  cd (p".") { run.text sh -c "printf text" ? }
}

proc bytes_tail() [env, process, error] -> Result[Bytes] {
  env ({XSH_CAPTURE_TAIL: "bytes"}) {
    defer { print ${env.get("XSH_CAPTURE_TAIL")?} }
    run.bytes sh -c "printf bytes" ?
  }
}

proc record_tail() [env, process, error] -> Result[Str] {
  let captured = cd (p".") { run.capture --text sh -c "printf record" ? }?
  captured.stdout
}

proc nested_tail() [env, process, error] -> Result[Result[Str, ProcessError]] {
  cd (p".") { try run.text sh -c "printf nested" }
}

proc failed_tail() [env, process, error] -> Result[Str] {
  let ignored = env ({XSH_CAPTURE_TAIL: "failed"}) { run.text sh -c "exit 7" ? }
  "unreachable"
}

proc discarded_tail() [env, process, error] -> Result[Unit] {
  let discarded: Unit = cd (p".") { run.text sh -c "printf discarded" ? }?
  discarded
}
