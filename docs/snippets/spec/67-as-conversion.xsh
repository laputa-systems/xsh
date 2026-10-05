# begin example
type Endpoint = {host: Str, port: UInt}

pure endpoint(spec: Str) -> Result[Endpoint] {
  let fields = spec.split(":")
  Ok({host: fields[0], port: fields[1] as UInt})
}

let scale = "1.5" as Float
let attempts = try { "many" as Int } ?? 3
let offset = -("12" as Int)
# end example
let parsed = endpoint("localhost:8080")?
print f"{parsed.host} {parsed.port} {scale} {attempts} {offset}"
