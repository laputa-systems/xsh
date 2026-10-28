##! The kernel state `syslogd` reads: the boot identity and what its standard
##! output is.

## What descriptor 1 is when it is a regular file: its target, device, inode and
## whether any link to it remains.
export type Output = {target: Str, dev: Int, ino: Int, linked: Bool}

## The boot identity the kernel reports.
export proc boot_id() [fs, error] -> Result[Str, Error] {
  Ok(p"/proc/sys/kernel/random/boot_id".read_text()?.trim())
}

## Describes standard output, or null when it is not a regular file.
export proc output() [fs, error] -> Result[Output?, Error] {
  let info = fs.stat(p"/proc/self/fd/1", follow_symlinks: true)?
  if info.kind != "file" { return Ok(null) }
  Ok({target: p"/proc/self/fd/1".readlink()?.display(), dev: info.dev, ino: info.ino, linked: info.nlink > 0})
}
