let files = ["two words.txt", "", "*.log"]
let shown = run.text printf "<%s>\n" @files
print (shown.trim())

let probe = run.capture --text sh -c "echo found; echo warned >&2; exit 3"
print f"ok={probe.status.ok} exit={probe.status.exit_code()?} out={probe.stdout.trim()} err={probe.stderr.trim()}"

let missing = try run.capture --text no-such-program
print f"could not start: {missing is Err(_)}"

let slow = try run.capture --text --timeout=100ms sleep 5
print f"timed out: {slow is Err(ProcessError.Timeout {..})}"

tempdir dir {
  cd (dir) {
    p"notes.txt".write("hi\n")
    let listing = run.text ls
    print f"listed: {listing.trim()}"
  }
}
