let shell = process.which("sh")?
let out = run.text sh -c "printf '%s\n' probe" ?
let status = run.status false
print (shell.name != "") out.trim() status.exited_with(1)
