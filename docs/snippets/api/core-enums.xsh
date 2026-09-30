enum Mode { Fast, Custom(Int) }
type SelectedMode = Mode

pure jobs(mode: SelectedMode) -> Int {
  match mode {
    Fast => 1
    Custom(count) => count
  }
}

enum Token { Present(Str) }
pure token_text(token: Token) -> Str {
  match token {
    Present(text) => text
  }
}
print token_text(Present("ready"))
print jobs(Custom(4))


enum State: Str { Ready = "ready", Empty = "" }
pure decode_state(text: Str) -> Result[State] {
  text.require()
}
pure encode_state(state: State) -> Result[Str] {
  json.encode(state)
}
