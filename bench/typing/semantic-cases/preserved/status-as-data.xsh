let status = run.status --accept=[1] /usr/bin/false
print ${status.exit_code()?}
let _ = status
print done
