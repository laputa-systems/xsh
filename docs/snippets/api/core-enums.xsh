enum Mode { Fast, Custom(Int) }
type SelectedMode = Mode

pure jobs(mode: SelectedMode) -> Int {
  match mode {
    Fast => 1
    Custom(count) => count
  }
}

enum Token { Present(Str) }
let token = Present("ready")
match token {
  Present(text) => print $text
}
print ${jobs(Custom(4))}


enum State: Str { Ready = "ready", Empty = "" }
pure decode_state(text: Str) -> Result[State] {
  return text.require(State)
}
pure encode_state(state: State) -> Result[Str] {
  return json.encode(state)
}
