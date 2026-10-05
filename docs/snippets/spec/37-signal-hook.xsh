on SIGINT --pre-cancel=150ms [fs, process, error] {
  p"/tmp/build.interrupted".write("interrupted\n")?
  exit 130
}
