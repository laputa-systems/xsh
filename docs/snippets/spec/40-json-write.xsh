const log_path = p"events.json"
let status = run.status true ?
# begin example
json.write(log_path, {service: "worker", event: "done", ok: status.ok})
# end example
