let build = spawn run make all ?
let tests = spawn run make test ?
let statuses = wait [build, tests]?

let server = spawn run /srv/app/server --port 8080 ?
server.cancel(signal: "TERM", kill_after: 2s)
