let scratch = fs.tempdir()?
defer scratch.close()?
let log = fp"{scratch.host_path()?}/app.log"
log.write("ok\nERROR disk full\nok\nERROR link down\n")?

let errors = run.text --accept=[0, 1] grep -c ERROR $log ?
let panics = run.text --accept=[0, 1] grep -c PANIC $log ?
print f"errors={errors.trim()} panics={panics.trim()}"
