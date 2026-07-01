error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: ${applet_name} ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

pure validate(raw: Str, index: Int) -> Result[Int] {
  if raw.starts_with("-") {
    return Err(usage_error("tool", raw))
  }

  raw.count_chars() + index % 7
}

var i = 0
var total = 0

while i < 5000 {
  let raw = if i % 5 == 0 { "-bad" } else { "path" }
  total += validate(raw, i) ?? 3
  i += 1
}

print $total % 256
