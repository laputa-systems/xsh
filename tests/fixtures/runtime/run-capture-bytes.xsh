let out = run.bytes head -c 1 /dev/zero ?

if out == b"\0" {
  print "ok"
}
