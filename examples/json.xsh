type Metadata = {name: Str, root: Str, digest: Str, ok: Bool, error: Str}

type Event = {service: Str, event: Str}

type Summary = {name: Str, events: Int, complete: Bool}

let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let out = fp"${root}/metadata.json"
let lines = fp"${root}/events.jsonl"
let summary_path = fp"${root}/summary.json"
let status = run.status false
let error_message = "shown"
let metadata = {
  name: "demo",
  root: p"src".display(),
  digest: b"abc".base64(),
  ok: status.ok,
  error: error_message,
}
json.write(out, metadata, pretty: true)?
let decoded = json.read(out)?.require(Metadata)?
let events = [{service: decoded.name, event: "start"}, {service: decoded.name, event: "stop"}]
json.write_lines(lines, events)?
let decoded_events = lines.read_text()? |> json.lines
let first = decoded_events[0].require(Event)?
let second = decoded_events[1].require(Event)?
let summary = {name: decoded.name, events: decoded_events.len(), complete: second.event == "stop"}
json.write(summary_path, summary, pretty: true)?
let checked_summary = json.read(summary_path)?.require(Summary)?

print f"metadata ${decoded.name} ${decoded.root} ${decoded.digest} ${decoded.ok} ${decoded.error}"
print f"events ${first.event},${second.event} ${decoded_events.len()}"
print f"summary ${checked_summary.name} ${checked_summary.events} ${checked_summary.complete}"
