##! Directory listings shared by the patch applet tests and the generator
##! that records GNU patch's results.

## One line per entry below `root`, sorted by path: kind (`f` file, `d`
## directory, `l` symbolic link), the owner permission bits, the path, and a
## link target after ` -> `. Paths in `mtimes` also carry ` mtime=SECONDS`.
## Group and other bits are left out because they depend on the umask.
export proc describe(root: Path, mtimes: List[Str]) [fs, error] -> Result[List[Str], Error] {
  var rows: List[Str] = []
  for entry in fs.walk(root, stat: true, hidden: true)? {
    let relative = entry.path.strip_prefix(root)?.display()
    if relative == "." or relative == "" { continue }
    let info = fs.stat(entry.path, follow_symlinks: false)?
    let owner = info.mode / 64 % 8
    var line = ""
    if info.kind == "symlink" {
      line = f"l 0 {relative} -> {entry.path.readlink()?.display()}"
    } else if info.kind == "dir" {
      line = f"d {owner} {relative}"
    } else {
      line = f"f {owner} {relative}"
      if relative in mtimes { line += f" mtime={info.mtime_ns / 1000000000}" }
    }
    rows += [line]
  }
  Ok(rows |> sort)
}
