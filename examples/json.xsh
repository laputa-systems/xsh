type Metadata = {name: Str, root: Str, digest: Str, ok: Bool, error: Str}

type Event = {service: Str, event: Str}

let root_handle = fs.tempdir()?
defer fs.close_root(root_handle)?
let root = fs.root_path(root_handle)?
let out = fp"${root}/metadata.json"
let lines = fp"${root}/events.jsonl"
let status = run.status false
let error_message = "shown"
let metadata = {name: "demo", root: p"src".display(), digest: b"abc".base64(), ok: status.ok, error: error_message}
json.write(out, metadata, pretty: true)?
let decoded = json.read(out)?.require(Metadata)?
let events = [{service: decoded.name, event: "start"}, {service: decoded.name, event: "stop"}]
json.write_lines(lines, events)?
let decoded_events = lines.read_text()? |> json.lines
let first = decoded_events[0].require(Event)?
print $decoded.name $decoded.root $decoded.digest $decoded.ok $decoded.error
print $first.service decoded_events.len()
