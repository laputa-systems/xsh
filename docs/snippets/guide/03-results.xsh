error LookupError = Missing(key: Str) | Malformed(text: Str)

pure port_of(settings: Map[Str], key: Str) -> Result[Int, LookupError] {
  let text = settings.get(key) ?? ""
  fail .Missing(key:) when text == ""

  text.parse_int() ?? Err(LookupError.Malformed(text:))?
}

let settings: Map[Str] = {http: "8080", admin: "eighty"}
print f"http: {port_of(settings, "http")?}"
print f"https, with a default: {port_of(settings, "https") ?? 443}"

for key in ["admin", "metrics"] {
  match port_of(settings, key) {
    Ok(port) => print f"{key}: {port}"
    Err(.Malformed {text}) => print f"{key}: not a number: {text}"
    Err(.Missing {key: _}) => print f"{key}: not set"
  }
}
