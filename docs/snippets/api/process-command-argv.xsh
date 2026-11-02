let command = process.command_argv("cat", ["cat"], stdin: b"hello\n", accept: [0])
let status = process.run(command)?
