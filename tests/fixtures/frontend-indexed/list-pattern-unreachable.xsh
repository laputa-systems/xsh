pure select(values: List[Int]) -> Int {
  match values {
    [..] if false => 1
    [] => 2
    [_, ..] => 3
    _ => 4
  }
}
