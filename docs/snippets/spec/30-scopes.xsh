const repo = p"."
const overlay = {CC: "clang"}

proc collect_report() [process, error] -> Result[Str] {
  run.text cc --version ?
}

# begin example
cd p"build" {
  run make
}

env CC=clang CFLAGS="-O2 -pipe" {
  run make
}

let version = cd (repo) { run.text git describe ? }?
let report = env (overlay) { collect_report()? }?
# end example
