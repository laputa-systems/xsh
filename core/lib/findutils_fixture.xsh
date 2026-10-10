##! Directory tree fixture for the find and xargs applets' tests.
##!
##! The tree has fixed modification times and permissions so that the same
##! command line gives byte-identical output on any host. Entries that depend
##! on the current time live under `recent/` with ages that sit in the middle
##! of a minute, hour, or day so that a test finishing seconds later still sees
##! the same ages. Nothing here is used by an applet itself.

## Modification time of the fixed-age entries: 2001-09-09T01:46:40Z.
export let EPOCH_NS = 1000000000000000000

const SECOND = 1000000000

proc put(target: Path, text: Str, mode: Int, mtime_ns: Int, atime_ns: Int? = null) [fs, error] {
  target.write(text)?
  target.chmod(mode)?
  fs.set_times(target, atime_ns: atime_ns ?? mtime_ns, mtime_ns: mtime_ns)?
}

## Recreate `recent` below the tree directory with ages at the middle of a
## minute, hour, or day (5.5 minutes, 2.5 hours and a minute is avoided, 3.5
## days, 10.5 days) so that a run finishing seconds later sees the same ages.
export proc build_recent(tree: Path) [fs, time, error] {
  let now = time.now() * 1000000
  let recent = fp"{tree}/recent"
  recent.remove()?
  recent.mkdir()?
  put(fp"{recent}/m5", "r", 0o644, now - 330 * SECOND)?
  put(fp"{recent}/h2", "r", 0o644, now - 9030 * SECOND)?
  put(fp"{recent}/d3", "r", 0o644, now - 302400 * SECOND)?
  put(fp"{recent}/d10", "r", 0o644, now - 907200 * SECOND)?
}

## Build the fixture tree named `name` below `root`.
export proc build(root: Path, name: Str = "tree") [fs, time, error] {
  let tree = fp"{root}/{name}"
  tree.mkdir()?
  let T = EPOCH_NS
  put(fp"{tree}/a.txt", "alpha\n", 0o644, T)?
  put(fp"{tree}/B.TXT", "", 0o644, T + 86400 * SECOND)?
  put(fp"{tree}/.hidden", "h", 0o644, T)?
  put(fp"{tree}/exec.sh", "#!/bin/sh\n", 0o755, T)?
  put(fp"{tree}/ro.txt", "ro", 0o444, T)?
  put(fp"{tree}/secret", "s", 0o600, T)?
  put(fp"{tree}/suid", "u", 0o4755, T)?
  put(fp"{tree}/sgid", "g", 0o2755, T)?
  put(fp"{tree}/hard1", "h\n", 0o644, T)?
  fs.link(fp"{tree}/hard1", fp"{tree}/hard2")?
  fs.mkfifo(fp"{tree}/fifo", 0o644)?
  put(Path.parse_bytes(bytes.concat([tree.bytes(), b"/odd\nname"]))?, "n", 0o644, T)?
  put(Path.parse_bytes(bytes.concat([tree.bytes(), b"/bad\xffname"]))?, "b", 0o644, T)?
  fs.symlink(p"a.txt", fp"{tree}/link_a")?
  fs.symlink(p"sub", fp"{tree}/link_dir")?
  fs.symlink(p"nowhere", fp"{tree}/broken")?
  for link in ["link_a", "link_dir", "broken"] {
    fs.set_times(fp"{tree}/{link}", atime_ns: T, mtime_ns: T, follow_symlinks: false)?
  }
  fs.set_times(fp"{tree}/fifo", atime_ns: T, mtime_ns: T)?

  let sub = fp"{tree}/sub"
  sub.mkdir()?
  put(fp"{sub}/c.log", "log\n", 0o644, T + 1000 * SECOND)?
  fp"{sub}/deep/deeper".mkdir(parents: true)?
  put(fp"{sub}/deep/d.txt", "deep\n", 0o644, T + 2000 * SECOND)?
  put(fp"{sub}/deep/e.c", "int x;\n", 0o644, T + 3000 * SECOND)?
  put(fp"{sub}/deep/deeper/f.txt", "ff", 0o644, T + 4000 * SECOND)?
  fp"{tree}/with space".mkdir()?
  put(fp"{tree}/with space/f g.txt", "fg", 0o644, T)?
  fp"{tree}/empty_dir".mkdir()?
  fp"{tree}/stick".mkdir()?
  fp"{tree}/stick".chmod(0o1777)?
  fp"{tree}/sizes".mkdir()?
  for size in [1, 511, 512, 513, 1024, 1025, 2049] {
    let sized = fp"{tree}/sizes/n{size}"
    sized.write(bytes.zero(size) ?? b"")?
    fs.set_times(sized, atime_ns: T, mtime_ns: T)?
  }
  fp"{tree}/chain/l1/l2/l3".mkdir(parents: true)?
  put(fp"{tree}/chain/l1/l2/l3/leaf", "l", 0o644, T)?
  fp"{tree}/times".mkdir()?
  put(fp"{tree}/times/old", "o", 0o644, T, T + 5000 * SECOND)?
  put(fp"{tree}/times/mid", "m", 0o644, T + 1000 * SECOND, T)?
  put(fp"{tree}/times/new", "n", 0o644, T + 2000 * SECOND, T + 1000 * SECOND)?

  build_recent(tree)?

  for dir in ["sub", "sub/deep/deeper", "sub/deep", "with space", "empty_dir", "stick", "sizes", "chain/l1/l2/l3", "chain/l1/l2", "chain/l1", "chain", "times", "recent"] {
    fs.set_times(fp"{tree}/{dir}", atime_ns: T, mtime_ns: T + 7000 * SECOND)?
  }
  fs.set_times(tree, atime_ns: T, mtime_ns: T + 7000 * SECOND)?
}

## A small mutable copy of the tree for the actions that change it: `-delete`
## and the output files.
export proc build_scratch(root: Path) [fs, error] {
  let scratch = fp"{root}/scratch"
  scratch.remove()?
  scratch.mkdir()?
  let T = EPOCH_NS
  fp"{scratch}/sub/deep/deeper".mkdir(parents: true)?
  put(fp"{scratch}/sub/c.log", "log\n", 0o644, T)?
  put(fp"{scratch}/sub/deep/d.txt", "deep\n", 0o644, T)?
  put(fp"{scratch}/sub/deep/e.c", "int x;\n", 0o644, T)?
  put(fp"{scratch}/sub/deep/deeper/f.txt", "ff", 0o644, T)?
  put(fp"{scratch}/a.txt", "alpha\n", 0o644, T)?
  put(fp"{scratch}/ro.txt", "ro", 0o444, T)?
  put(fp"{scratch}/hard1", "h\n", 0o644, T)?
  fs.link(fp"{scratch}/hard1", fp"{scratch}/hard2")?
  fs.symlink(p"a.txt", fp"{scratch}/link_a")?
  fs.symlink(p"nowhere", fp"{scratch}/broken")?
  fp"{scratch}/empty_dir".mkdir()?
}

## Symlink loops, kept away from the tree so that following links there does
## not report them on every run.
export proc build_loops(root: Path) [fs, error] {
  fp"{root}/loops".mkdir()?
  fp"{root}/loops/file".write("f")?
  fs.symlink(p"loop2", fp"{root}/loops/loop1")?
  fs.symlink(p"loop1", fp"{root}/loops/loop2")?
  fs.symlink(p".", fp"{root}/loops/self")?
}

## An unreadable directory and a readable one that cannot be searched, both
## beside the tree. Call `restore` before the temporary directory is removed.
export proc build_hostile(root: Path) [fs, error] {
  build_loops(root)?
  fp"{root}/locked".mkdir()?
  fp"{root}/locked".chmod(0o000)?
  fp"{root}/noexec".mkdir()?
  fp"{root}/noexec/inside".write("i")?
  fp"{root}/noexec".chmod(0o644)?
}

## Make the hostile directories removable again.
export proc restore(root: Path) [fs, error] {
  fp"{root}/locked".chmod(0o755)?
  fp"{root}/noexec".chmod(0o755)?
}

## One command line of a corpus. `flags` selects how the harness runs it:
## `n` skips the case for a privileged user, who is not refused by file
## permissions and owns what the fixture builds as root, `r` recreates the
## time-relative entries first, `s` and `z` compare newline- or NUL-separated records as sorted sets, `t`
## runs inside the tree, `d` rebuilds `scratch` first and appends what the
## command left behind, `l` blanks the volatile columns of `-ls` lines, `u`
## expects the invoking user's ids where the stored output has `@UID@` and
## `@GID@`.
export type Case = {name: Str, flags: Str, input: Bytes, args: List[Str]}
## What a run printed, with the exit status.
export type Observed = {status: Int, stdout: Bytes, stderr: Bytes}

const DIGITS = "0123456789abcdef"

## Lowercase hexadecimal text of the bytes.
export pure hex(data: Bytes) -> Str {
  var parts: List[Str] = []
  for at in range(data.len()) {
    let byte = data.byte_at(at) ?? 0
    parts += [DIGITS.byte_slice(byte / 16, 1), DIGITS.byte_slice(byte % 16, 1)]
  }
  parts.join("")
}

## The bytes that `hex` encoded.
export pure unhex(text: Str) -> Bytes {
  var values: List[Int] = []
  var at = 0
  while at + 1 < text.byte_len() {
    values += [(DIGITS.find(text.byte_slice(at, 1)) ?? 0) * 16 + (DIGITS.find(text.byte_slice(at + 1, 1)) ?? 0)]
    at += 2
  }
  bytes.from_ints(values) ?? b""
}

## Whether the bytes are valid UTF-8 without NUL, so a JSON string holds them.
export pure textual(data: Bytes) -> Bool {
  if data.utf8() is Err(_) { return false }
  for at in range(data.len()) { if data.byte_at(at) == 0 { return false } }
  true
}

## The stored form of a byte string: text when it round-trips, hex otherwise.
export pure stored_text(data: Bytes) -> Str {
  if textual(data) { data.utf8() ?? "" } else { "" }
}

## Hex form of the bytes when they do not fit in a JSON string, else empty.
export pure stored_hex(data: Bytes) -> Str {
  if textual(data) { "" } else { hex(data) }
}

## Rebuild the bytes from the pair stored by `stored_text` and `stored_hex`.
export pure stored_bytes(text: Str, encoded: Str) -> Bytes {
  if encoded != "" { unhex(encoded) } else { bytes.from_text(text) }
}

## Replace the invoking user's ids in recorded output with portable markers.
export pure mark_ids(data: Bytes, uid: Int, gid: Int) -> Bytes {
  match data.utf8() {
    Ok(text) => bytes.from_text(text.replace(f"{uid}", with: "@UID@").replace(f"{gid}", with: "@GID@"))
    Err(_) => data
  }
}

# Split into records and order them bytewise, so output that depends on
# directory enumeration order can be compared as a set.
pure sorted_records(data: Bytes, separator: Int) -> List[Str] {
  var records: List[Str] = []
  var start = 0
  for at in range(data.len()) {
    if data.byte_at(at) == separator { records += [hex(data[start..at])]; start = at + 1 }
  }
  if start < data.len() { records += [hex(data[start..])] }
  records |> sort() |> collect()
}

## Blank inode, block count, owner and group, which differ between hosts.
export pure normalize_ls(data: Bytes) -> Bytes {
  if data.utf8() is Err(_) { return data }
  let text = data.utf8() ?? ""
  var lines: List[Str] = []
  for line in text.split("\n") {
    var fields: List[Str] = []
    var at = 0
    while fields.len() < 10 {
      while at < line.byte_len() and line.byte_slice(at, 1) == " " { at += 1 }
      let start = at
      while at < line.byte_len() and line.byte_slice(at, 1) != " " { at += 1 }
      if at == start { break }
      fields += [line.byte_slice(start, at - start)]
    }
    if fields.len() < 10 { lines += [line]; continue }
    while at < line.byte_len() and line.byte_slice(at, 1) == " " { at += 1 }
    fields[0] = "-"; fields[1] = "-"; fields[4] = "U"; fields[5] = "G"
    lines += [f"{fields.join(" ")} {line.byte_slice(at)}"]
  }
  bytes.from_text(lines.join("\n"))
}

# Paths below `directory` relative to the directory `strip` bytes up.
proc listing(directory: Path, strip: Int) [fs, error] -> Result[List[Bytes]] {
  var names: List[Bytes] = []
  for entry in fs.children(directory)? {
    let full = entry.path.bytes()
    names += [full[strip..]]
    if entry.kind == "dir" { names += listing(entry.path, strip)? }
  }
  Ok(names)
}

pure with_root_name(data: Bytes, root: Path) -> Bytes {
  match data.utf8() {
    Ok(text) => bytes.from_text(text.replace(root.display(), with: "/fixture"))
    Err(_) => data
  }
}

## The standard input of a case. `@seq:N` stands for the numbers 1 to N one
## per line and `@wide:N` for N long words, so large inputs stay out of the
## stored corpus; anything else is the bytes themselves.
export pure expand_input(data: Bytes) -> Bytes {
  let text = data.utf8() ?? ""
  var lines: List[Str] = []
  if text.starts_with("@seq:") {
    for number in range(1, (text.byte_slice(5).parse_int() ?? 0) + 1) { lines += [f"{number}\n"] }
    return bytes.from_text(lines.join(""))
  }
  if text.starts_with("@wide:") {
    for number in range(1, (text.byte_slice(6).parse_int() ?? 0) + 1) { lines += [f"word{number}-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx "] }
    return bytes.from_text(lines.join(""))
  }
  data
}

## Run one case of a corpus and return what the applet printed.
export proc invoke(xsh_bin: Path, script: Path, root: Path, work: Path, c: Case) [fs, process, time, error] -> Result[Observed, Error] {
  let identity = unix.id()?
  let scratch = fp"{root}/scratch"
  if "d" in c.flags { build_scratch(root)? }
  if "r" in c.flags { build_recent(fp"{root}/tree")? }
  var args: List[Str] = [xsh_bin.display(), script.display(), "--"]
  for arg in c.args { args += [resolve_ids(arg, identity.uid, identity.gid)] }
  let out = fp"{work}/stdout"
  let err = fp"{work}/stderr"
  let cwd = if "t" in c.flags { fp"{root}/tree" } else { root }
  let plan = process.command_argv(xsh_bin, args, cwd, {LC_ALL: "C", TZ: "UTC"}, expand_input(c.input), out, err)
  let status = process.run(plan)?
  var stdout = out.read_bytes()?
  if "d" in c.flags {
    var tail: List[Bytes] = [b"--- after ---\n"]
    let cap = fp"{scratch}/cap"
    if cap.exists()? { tail += [b"cap:\n", cap.read_bytes()?] }
    var names = ["scratch"] |> map { |name| bytes.from_text(name) } |> collect()
    names += listing(scratch, root.bytes().len() + 1)?
    var keyed: List[Str] = []
    for name in names { keyed += [hex(name)] }
    let ordered = keyed |> sort() |> collect()
    for key in ordered { tail += [unhex(key), b"\n"] }
    stdout = bytes.concat([stdout] + tail)
  }
  if "l" in c.flags { stdout = normalize_ls(stdout) }
  Ok({status: status.exit_code() ?? -1, stdout: with_root_name(stdout, root), stderr: with_root_name(err.read_bytes()?, root)})
}

## Diagnostics of a case that sorts its output are compared as a set of lines
## too: the order in which unreadable entries are reported follows the
## directory enumeration order.
export pure same_errors(c: Case, expected: Bytes, actual: Bytes) -> Bool {
  if "s" in c.flags or "z" in c.flags { return sorted_records(expected, 10) == sorted_records(actual, 10) }
  expected == actual
}

## Replace the stored `@UID@` and `@GID@` markers with the given ids.
export pure resolve_ids(text: Str, uid: Int, gid: Int) -> Str {
  text.replace("@UID@", with: f"{uid}").replace("@GID@", with: f"{gid}")
}

## True when the observation equals the stored one, comparing sorted records
## when the case asks for it.
export pure same_output(c: Case, expected: Bytes, actual: Bytes) -> Bool {
  if "s" in c.flags { return sorted_records(expected, 10) == sorted_records(actual, 10) }
  if "z" in c.flags { return sorted_records(expected, 0) == sorted_records(actual, 0) }
  expected == actual
}

# Printable rendering of output for failure messages.
pure show(data: Bytes) -> Str {
  var parts: List[Str] = []
  for at in range(data.len()) {
    let byte = data.byte_at(at) ?? 0
    if byte == 10 { parts += ["\\n"] } else if byte == 9 { parts += ["\\t"] } else if byte >= 32 and byte < 127 {
      parts += [(bytes.from_ints([byte]) ?? b"").utf8() ?? "?"]
    } else { parts += [f"\\x{DIGITS.byte_slice(byte / 16, 1)}{DIGITS.byte_slice(byte % 16, 1)}"] }
  }
  f"\"{parts.join("")}\""
}

## One recorded run, as stored in a corpus file: text fields hold UTF-8 without
## NUL and the matching `_hex` field holds anything else.
export type Stored = {name: Str, flags: Str, args: List[Str], input: Str, input_hex: Str, status: Int, stdout: Str, stdout_hex: Str, stderr: Str, stderr_hex: Str}

## Run the cases of a JSON Lines corpus whose position leaves `part` when
## divided by `parts` (so several tests can share the work) and describe each
## difference from the recorded GNU output; an empty list means the applet
## matches.
export proc run_corpus(xsh_bin: Path, script: Path, corpus: Path, root: Path, work: Path, part: Int, parts: Int) [fs, process, time, error] -> Result[List[Str], Error] {
  let identity = unix.id()?
  var failures: List[Str] = []
  var position = -1
  for line in corpus.read_text()?.split("\n") {
    if line == "" { continue }
    position += 1
    if position % parts != part { continue }
    let stored = json.decode(line)?.require(Stored)?
    let c: Case = {name: stored.name, flags: stored.flags, input: stored_bytes(stored.input, stored.input_hex), args: stored.args}
    if "n" in c.flags and identity.euid == 0 { continue }
    let seen = invoke(xsh_bin, script, root, work, c)?
    let status = stored.status
    let expected_out = stored_bytes(resolve_ids(stored.stdout, identity.uid, identity.gid), stored.stdout_hex)
    let expected_err = stored_bytes(stored.stderr, stored.stderr_hex)
    if seen.status != status { failures += [f"{c.name}: exit {seen.status}, expected {status}"] }
    if ! same_output(c, expected_out, seen.stdout) { failures += [f"{c.name}: stdout {show(seen.stdout)} expected {show(expected_out)}"] }
    if ! same_errors(c, expected_err, seen.stderr) { failures += [f"{c.name}: stderr {show(seen.stderr)} expected {show(expected_err)}"] }
  }
  Ok(failures)
}
