##! Edit sshd-style `Key value` configuration text.

## Set `key` to `value`, replacing the first active or commented-out
## occurrence, or appending the setting when the key is absent.
export pure set_option(text: Str, key: Str, value: Str) -> Str {
  var out = []
  var done = false
  for line in text.lines() {
    let words = line.replace("#", with: " ").fields()
    if ! done and ! words.is_empty() and words[0] == key {
      out += [f"{key} {value}"]
      done = true
    } else {
      out += [line]
    }
  }

  if ! done {
    out += [f"{key} {value}"]
  }

  out.join("\n") + "\n"
}
