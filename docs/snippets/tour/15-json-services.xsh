type Service = {name: Str, port: Int, tags: List[Str]}

const raw = """
  [{"name": "api", "port": 8080, "tags": ["web", "public"]},
   {"name": "db", "port": 5432, "tags": []}]
  """

let services = json.decode(raw)?.require(List[Service])?
for svc in services {
  print f"${svc.name} -> ${svc.port} [${svc.tags.join(",")}]"
}

let wrong = json.decode("""{"name": "cache", "port": "6379", "tags": []}""")?
match wrong.require(Service) {
  Ok(svc) => print f"unexpected: ${svc.name}"
  Err(error) => print f"rejected: ${error.message}"
}

let report = {count: services.len(), public: [s.name for s in services if "public" in s.tags]}
print json.encode(report)?
print json.encode(report, pretty: true)?
