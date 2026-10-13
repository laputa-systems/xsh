##! Directory listings shared by the patch applet tests and the generator
##! that records GNU patch's results.

## One line per entry below `root`, sorted by path: kind (`f` file, `d`
## directory, `l` symbolic link), the owner permission bits, the path, and a
## link target after ` -> `. Paths in `mtimes` also carry ` mtime=SECONDS`.
## Group and other bits are left out because they depend on the umask.
export proc describe(root: Path, mtimes: List[Str]) [fs, error] -> Result[List[Str], Error] {
  let rows = collect {
    for entry in fs.walk(root, stat: true, hidden: true)? {
      let relative = entry.path.strip_prefix(root)?.display()
      continue when relative == "." or relative == ""
      let info = fs.stat(entry.path, follow_symlinks: false)?
      let owner = info.mode / 64 % 8
      let line = if info.kind == "symlink" {
        f"l 0 {relative} -> {entry.path.readlink()?.display()}"
      } else if info.kind == "dir" {
        f"d {owner} {relative}"
      } else {
        let mtime = if relative in mtimes { f" mtime={info.mtime_ns / 1000000000}" } else { "" }
        f"f {owner} {relative}{mtime}"
      }
      yield line
    }
  }
  Ok(rows |> sort)
}
