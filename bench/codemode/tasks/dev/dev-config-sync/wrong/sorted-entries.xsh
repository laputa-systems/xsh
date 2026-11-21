use svc

type Desired = {service: Str, values: Map[Str], drop: List[Str]}

pure merged(entries: List[svc.Entry], desired: Desired) -> List[svc.Entry] {
  var values: Map[Str] = {}
  for entry in entries {
    if entry.key not in desired.drop {
      values[entry.key] = entry.value
    }
  }

  for {key, value} in desired.values {
    values[key] = value
  }

  [svc.Entry(key: key, value: value) for {key, value} in values]
}

let desired = json.read(p"/work/desired.json")?.require(Desired)?
var doc = svc.config_get(desired.service)?
var wanted = merged(doc.entries, desired)

if wanted == doc.entries {
  print f"unchanged {doc.version}"
  exit 0
}

for attempt in range(2) {
  let stored = svc.config_put(desired.service, doc.version, wanted)
  if stored is Err(svc.ServiceError.Conflict {..}) {
    doc = svc.config_get(desired.service)?
    wanted = merged(doc.entries, desired)
    continue
  }

  let version = match stored {
    Ok(saved) => saved.version
    Err(_) => doc.version + 1
  }

  print f"updated {version}"
  exit 0
}

eprint "gave up after two conflicts"
exit 4
