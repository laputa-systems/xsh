var retry_events: List[Str] = []

proc spin() {
  loop {
    let attempt = retry [] {
      defer { retry_events += ["close"] }
      retry_events += ["attempt"]
      assert false
    }
    retry_events += ["after"]
    break
  }
}

proc retry_no_completion() -> Result[Unit] {
  spin()
  assert retry_events == ["attempt", "close", "after"]
  let typed: Result[Int, Error] = retry [] { assert false }
  match typed {
    Err(_) => {}
    else => assert false
  }
}
