let status = run sh -c "kill -TERM $$"
let _ = status.exit_code()?
