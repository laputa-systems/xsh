# Record reads must not copy all fields.
#
# JSON construction finishes before traversal. The Rust allocation harness
# compares one pass with seventeen passes at each record size.
proc main() [env, error, io] {
  let size = env.int("XSH_RECORD_READ_FIELDS", 256)?
  let passes = env.int("XSH_RECORD_READ_PASSES", 1)?
  var source = "{"
  var index = 0
  while index < size {
    if index > 0 { source = source + "," }
    source = source + f"\"k${index}\":${index}"
    index = index + 1
  }
  source = source + "}"
  let values = json.decode(source)?.require(Record)?

  let keys = values.keys()
  var total = 0
  var pass = 0
  while pass < passes {
    for key in keys {
      total = total + values.get(key)?.require(Int)?
    }
    pass = pass + 1
  }

  print f"${size} ${passes} ${total}"
}
