let command = process.command {
  stdin = b"hello\n"
  accept = [0]
  run cat
}
let status = process.run(command)?
