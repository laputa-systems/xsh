const text = "21"
# begin example
let parsed = try {
  let n = text.parse_int()?
  n * 2
}
# end example
