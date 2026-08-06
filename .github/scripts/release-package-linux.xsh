#!/usr/bin/env -S xsh --
proc checksum_line(target_path: Path) [fs, error] -> Result[Str] {
  return f"""${hash.sha256(target_path)?.hex()}  ${target_path.display()}
"""
}

proc main(version: Str, target: Str, arch: Str) [fs, error] {
  let bin_dir = fp"target/${target}/dist"
  let dist = p"dist"
  dist.mkdir()?
  for bin in ["xsh", "xsht", "xshi"] {
    let artifact = fp"dist/${bin}-${version}-${arch}-linux-musl"
    fs.install(fp"${bin_dir}/${bin}", artifact, 0o755, overwrite: true)?
    fp"dist/${bin}-${version}-${arch}-linux-musl.sha256".write(checksum_line(artifact)?)?
  }
}
