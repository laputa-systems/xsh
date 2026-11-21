use svc

type Line = {id: Str, title: Str, priority: Int}

type Reject = {line: Int, reason: Str}

var rejects: List[Reject] = []
var seen: List[Str] = []
var created = 0
var number = 0

for text in p"/work/tickets.jsonl".lines()? {
  number += 1
  if text.trim().is_empty() {
    continue
  }

  let parsed = try { json.decode(text)?.require(Line)? }
  guard let line = parsed else {
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
