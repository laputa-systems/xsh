pure nested(values: List[List[Int]]) -> Int {
  match values {
    [[first, ..tail], [99]] => first + tail.len()
    else => 0
  }
}
