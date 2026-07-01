let cc_raw = run.text CC=cc CFLAGS="-O2 -pipe" printenv CC ?
let cflags_raw = run.text CC=cc CFLAGS="-O2 -pipe" printenv CFLAGS ?
let cc = cc_raw.trim()
let cflags = cflags_raw.trim()
let configured = f"${cc}|${cflags}"
print $configured

env {
  DESTDIR = /tmp/xsh-dest
  XSH_EXAMPLE_FLAG = "yes"
  XSH_EXAMPLE_THREADS = "8"
} {
  let dest = env.path("DESTDIR")?
  let flag = env.bool("XSH_EXAMPLE_FLAG")?
  let threads = env.int("XSH_EXAMPLE_THREADS")?
  let fallback = env.get_or("XSH_EXAMPLE_MISSING", "fallback")?
  print $dest
  print $flag $threads $fallback
}
