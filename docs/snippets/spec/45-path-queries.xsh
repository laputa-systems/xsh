pure install_mode(relative: Path) -> Int {
  # begin example
  let library = relative.starts_with(p"lib") # lib/x.xsh, not libexec/x
  let static_library = relative.ends_with(p"out/libc.a") # whole trailing components
  let rooted = relative.starts_with(/) # an absolute path
  # end example
  if library or static_library or rooted { 0o644 } else { 0o755 }
}
