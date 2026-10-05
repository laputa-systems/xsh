# begin example
type Port = Int range 1..=65535

# `..` stops before its upper bound, as a slice does: 0 to 255.
type Byte = UInt range 0..256

type Listener = {host: Str, port: Port, backlog: Byte}

pure address(target: Listener) -> Str {
  f"{target.host}:{target.port}"
}

# Arithmetic returns an `Int`; the result is validated to be a `Port` again.
# `.require()` takes its target, `Port`, from the return type.
pure after(port: Port) -> Result[Port] {
  (port + 1).require()
}

proc listener(host: Str, text: Str) [error] -> Result[Listener] {
  # A literal is checked where it is written.
  let backlog: Byte = 128

  # Any other integer is validated once, at an explicit boundary.
  let number = text as Int
  let port = number as Port
  Ok(Listener(host:, port:, backlog:))
}

# end example

let web = listener("localhost", "8080")?
let next = after(web.port)?
let rejected = try { listener("localhost", "70000")? }
print address(web) $next ${web.port < next} ${rejected is Err(_)}
