proc settle(marker: Path) [fs, time, error] -> Bool {
  # begin example
  let settled = wait until marker.exists() within 5s # error: parse.expected-expression
  # end example
  true
}
