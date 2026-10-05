error PortError = Missing(file: Path) | Invalid(text: Str)

proc read_port(file: Path) -> Result[Int] {
  guard file.exists() else {
    return Err(PortError.Missing(file:))
  }

  let text = file.read_text()?.trim()
  text.parse_int() ?? { |_| Err(PortError.Invalid(text:))? }
}

let scratch = fs.tempdir()?
defer scratch.close()
let dir = scratch.host_path()?
fp"{dir}/good".write("8080\n")
fp"{dir}/bad".write("eighty\n")

print f"good: {read_port(fp"{dir}/good")?}"
print f"missing, with default: {read_port(fp"{dir}/none") ?? 80}"

match read_port(fp"{dir}/bad") {
  Ok(port) => print f"port {port}"
  Err(PortError.Invalid {text}) => print f"not a port: {text}"
  Err(error) => print f"unreadable: {error.message}"
}
