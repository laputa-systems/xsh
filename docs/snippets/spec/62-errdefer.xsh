proc render(output: Path) {
  output.write("rendered\n")
}

# begin example
proc publish(output: Path) {
  let partial = fp"{output}.partial"
  errdefer partial.remove()
  render(partial)
  partial.rename(to: output)
}

# end example

let scratch = fs.tempdir()?
defer scratch.close()?
let output = fp"{scratch.host_path()?}/report"
publish(output)
print output.read_text()?.trim()
