let plan = process.command {
  cwd = p"/srv/app"
  env = {RUST_LOG: "info"}
  timeout = 30s
  run /srv/app/server --port 8080
}
let status = process.run(plan)?
