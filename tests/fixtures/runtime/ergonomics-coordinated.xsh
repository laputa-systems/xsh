pure label(code: Int) -> Str {
  match code {
    0 => "ok"
    _ => {
      let detail = f"exit ${code}"
      detail
    }
  }
}

pure join_words(words: List[Str], separator: Str) -> Str {
  words.join(separator:)
}

pure configured_name(name: Str?) -> Str {
  return name.trim() when name != null
  "default"
}

let config = {root: "src", build: {jobs: [1, 2, 3], target: "native"}}
let {root, build: {jobs, target: target_name, ..}, ..} = config
let rows = [
  f"${root}:${target_name}:${job}:${part}"
  for job in jobs
  if 1 < job <= 3
  for part in ["a", "b"]
]
var words: List[Str] = []
let previous = words
words += rows
words += [label(0)]
let separator = ","
print (join_words(words:, separator:))
print (previous.len())
print (configured_name(null))
let data: Bytes? = b"abcdef"
print ((data?[1..4] ?? b"").base64())
let outcome: Result[Int] = Ok(1)
print (outcome is Ok(_))
let absent: Str? = null
print (absent?.trim() ?? "absent")
