type State = {transfer: Str?}

proc lookup() -> Result[State] {
  State(transfer: "t-1")
}

var sent: List[Str] = []
match lookup() {
  Ok(state) => {
    if state.transfer != null {
      sent += [state.transfer]
    }
  }
  Err(_) => {}
}

print f"{sent.len()}"
