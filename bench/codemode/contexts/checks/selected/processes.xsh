let words = ["two words", "", "*.txt"]
let shown = run.text printf "<%s>\n" @words
print (shown.trim())
let argv = ["sh", "-c", "echo out; echo err >&2; exit 3"]
let out = run.capture --text @argv
print f"{out.status.ok} {out.status.exit_code()?} {out.stdout.trim()} {out.stderr.trim()}"
let strict = try run.text sh -c "exit 5"
print f"run.text fails: {strict is Err(_)}"
let missing = try run.capture --text not-a-real-program
print f"cannot start: {missing is Err(_)}"
let count = run.text --timeout=5s --accept=[0, 1] grep -c nothing /dev/null
print (count.trim())
let slow = try run.capture --text --timeout=100ms sleep 5
print f"timeout: {slow is Err(ProcessError.Timeout {..})}"
let name = "inner"
let quoted = run.text printf "%s" "x$name"
print $quoted
tempdir dir {
  cd (dir) {
    p"a".write("1")
    let listing = run.text ls
    print (listing.trim())
  }

  let vars: Map[Str] = {GREETING: "hi there"}
  env (vars) {
    let seen = run.text printenv GREETING
    print (seen.trim())
  }

  print f"{dir.exists()?}"
}
