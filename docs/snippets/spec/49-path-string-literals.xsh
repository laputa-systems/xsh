proc install(source: Path, dest: Path = "/usr/local/bin") [fs, error] {
  source.copy(fp"{dest}/{source.name()}")?
}

proc classify(config: Path, seen: List[Path]) [fs, error] -> Result[Str] {
  # begin example
  let fallback: Path = "/etc/xsh/config.ini"
  install("build/xsh")? # a user parameter
  let relative = config.strip_prefix("/etc")? # a standard method parameter
  let default_config = config == "/etc/xsh/config.ini"
  let repeated = "/etc/hosts" in seen
  let kind = match relative {
    "hosts" | "resolv.conf" => "network",
    else => "other",
  }
  # end example
  if default_config or repeated { fallback.display() } else { kind }
}
