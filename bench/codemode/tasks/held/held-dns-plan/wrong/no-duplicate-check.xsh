use svc

type Desired = {name: Str, kind: Str, value: Str, ttl: Int}

type Identity = {name: Str, kind: Str}

type Action = {action: Str, name: Str, kind: Str, value: Str, ttl: Int}

type Line = {action: Str, name: Str, kind: Str, result: Str}

proc load_desired(file: Path) -> Result[List[Desired]] {
  let records = json.read(file)?.require(List[Desired])?
  var seen: List[Identity] = []
  for record in records {
    let identity = Identity(name: record.name, kind: record.kind)
    seen += [identity]
  }

  records
}

pure plan(desired: List[Desired], current: List[svc.DnsRecord]) -> List[Action] {
  var actions: List[Action] = []
  for wanted in desired {
    let present = [r for r in current if r.name == wanted.name and r.kind == wanted.kind]
    if present.is_empty() {
      actions += [Action(action: "create", name: wanted.name, kind: wanted.kind, value: wanted.value, ttl: wanted.ttl)]
    } else if present[0].value != wanted.value or present[0].ttl != wanted.ttl {
      let action = if present[0].owner == "dnsplan" { "update" } else { "conflict" }
      actions += [Action(action:, name: wanted.name, kind: wanted.kind, value: wanted.value, ttl: wanted.ttl)]
    }
  }

  for record in current {
    let desired_here = [w for w in desired if w.name == record.name and w.kind == record.kind]
    if record.owner == "dnsplan" and desired_here.is_empty() {
      actions += [Action(action: "delete", name: record.name, kind: record.kind, value: record.value, ttl: record.ttl)]
    }
  }

  actions |> sort-by { |a| {first: a.name, second: a.kind} }
}

proc perform(zone: Str, action: Action, apply: Bool) -> Str {
  if action.action == "conflict" {
    return "skipped"
  }

  if ! apply {
    return "planned"
  }

  let written = if action.action == "delete" {
    svc.dns_delete(zone, action.name, action.kind)
  } else {
    svc.dns_put(zone, action.name, action.kind, action.value, action.ttl)
  }

  if written is Ok(_) { "applied" } else { "failed" }
}

## Bring a DNS zone in line with a desired-state file.
cli main(zone: Str, desired: Path, apply = false) {
  let wanted = match load_desired(desired) {
    Ok(records) => records
    Err(failure) => {
      eprint f"{desired}: {failure.message}"
      exit 2
    }
  }

  let current = svc.dns_records(zone)?
  var clean = true
  for action in plan(wanted, current) {
    let result = perform(zone, action, apply)
    if result != "planned" and result != "applied" {
      clean = false
    }

    print (json.encode(Line(action: action.action, name: action.name, kind: action.kind, result:))?)
  }

  if ! clean {
    exit 4
  }
}
