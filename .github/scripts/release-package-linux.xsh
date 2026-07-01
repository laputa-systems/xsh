#!/usr/bin/env -S xsh --
proc checksum_line(target_path: Path) [fs, error] -> Result[Str] {
  return f"""${hash.sha256(target_path)?.hex()}  ${target_path.display()}
"""
}

proc main(version: Str, target: Str, arch: Str) [fs, error] {
  let bin_dir = fp"target/${target}/dist"
  let dist = p"dist"
  dist.mkdir()?
  let multicall_xz = fp"dist/xsh-multicall-${version}-${arch}-linux-musl.xz"
  archive.compress(fp"${bin_dir}/xsh-multicall", multicall_xz, "xz", 6, true)?
  fp"dist/xsh-multicall-${version}-${arch}-linux-musl.sha256".write(checksum_line(multicall_xz)?)?
}

main(@args)?
