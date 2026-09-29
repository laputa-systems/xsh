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
