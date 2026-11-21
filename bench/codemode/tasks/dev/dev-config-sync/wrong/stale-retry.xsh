use svc

type Desired = {service: Str, values: Map[Str], drop: List[Str]}

pure merged(entries: List[svc.Entry], desired: Desired) -> List[svc.Entry] {
  var out: List[svc.Entry] = []
  var present: List[Str] = []
  for entry in entries {
    present += [entry.key]
    if entry.key not in desired.drop {
      out += [svc.Entry(key: entry.key, value: desired.values.get(entry.key) ?? entry.value)]
    }
  }

  for {key, value} in desired.values {
    if key not in present {
      out += [svc.Entry(key: key, value: value)]
    }
  }

  out
}

let desired = json.read(p"/work/desired.json")?.require(Desired)?
var doc = svc.config_get(desired.service)?
let wanted = merged(doc.entries, desired)

if wanted == doc.entries {
  print f"unchanged {doc.version}"
  exit 0
}

for attempt in range(2) {
  let stored = svc.config_put(desired.service, doc.version, wanted)
  if stored is Err(svc.ServiceError.Conflict {..}) {
    doc = svc.config_get(desired.service)?
    continue
  }

  print f"updated {stored?.version}"
  exit 0
}

eprint "gave up after two conflicts"
exit 4
