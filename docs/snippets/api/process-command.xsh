let command = process.command {
  stdin = b"hello\n"
  run cat
}
let status = process.run(command)?
