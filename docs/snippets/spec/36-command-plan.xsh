let plan = process.command_argv(
  "/srv/app/server",
  ["/srv/app/server", "--port", "8080"],
  cwd: p"/srv/app",
  env: {RUST_LOG: "info"},
  timeout: 30s,
)
let status = process.run(plan)?
