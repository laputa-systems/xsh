use svc

type Line = {id: Str, title: Str, priority: Int}

type Reject = {line: Int, reason: Str}

proc parse(text: Str) -> Line? {
  guard let value = json.decode(text) else {
    return null
  }

  guard let line = value.require(Line) else {
    return null
  }

  if line.id.is_empty() or line.priority < 1 or line.priority > 4 {
    return null
  }

  line
}

var rejects: List[Reject] = []
var seen: List[Str] = []
var created = 0
var number = 0

for text in p"/work/tickets.jsonl".lines()? {
  number += 1
  if text.trim().is_empty() {
    continue
  }

  let line = parse(text)
  if line == null {
    rejects += [Reject(line: number, reason: "invalid")]
    continue
  }

  if line.id in seen {
    rejects += [Reject(line: number, reason: "duplicate")]
    continue
  }

  seen += [line.id]
  match svc.tracker_create(line.id, line.title, line.priority) {
    Ok(_) => created += 1
    Err(_) => rejects += [Reject(line: number, reason: "failed")]
  }
}

var out = ""
for reject in rejects {
  out += json.encode(reject)? + "\n"
}

p"/work/rejects.jsonl".write(out)
print (json.encode({created: created, rejected: rejects.len()})?)
