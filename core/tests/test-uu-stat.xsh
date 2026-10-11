##! Transcribed file metadata and formatting tests from the uutils suite.

use support.uu as uu

const NORMAL = "%a %A %b %B %d %D %f %F %g %G %h %i %m %n %o %s %u %U %x %X %y %Y %z %Z"
const DEVICE = "%a %A %b %B %d %D %f %F %g %G %h %i %m %n %o %s (%t/%T) %u %U %w %W %x %X %y %Y %z %Z"
const FILESYSTEM = "%b %c %i %l %n %s %S %t %T"

# The metadata API transports native unsigned fields through signed Int bits.
proc unsigned(value: Int) [error] -> Result[Str, Error] {
  if value >= 0 { return Ok(f"{value}") }
  let inverse = f"{-(value + 1)}"
  var carry = 0
  var digits: List[Str] = []
  for offset in range(20) {
    let maximum_digit = "18446744073709551615".byte_slice(19 - offset, length: 1).parse_int()?
    let index = inverse.byte_len() - 1 - offset
    let subtract = if index < 0 { 0 } else { inverse.byte_slice(index, length: 1).parse_int()? }
    let digit = maximum_digit - subtract - carry
    digits = [f"{if digit < 0 { digit + 10 } else { digit }}"].extend(digits)
    carry = if digit < 0 { 1 } else { 0 }
  }
  Ok(digits.join(""))
}

proc radix(value: Int, base: Int) [error] -> Result[Str, Error] {
  var remaining = if value < 0 { -(value + 1) } else { value }
  var digits: List[Str] = []
  if value < 0 {
    assert base == 16
    for offset in range(16) {
      digits = ["0123456789abcdef".byte_slice(15 - remaining % 16, length: 1)].extend(digits)
      remaining /= 16
    }
  } else {
    loop {
      digits = ["0123456789abcdef".byte_slice(remaining % base, length: 1)].extend(digits)
      remaining /= base
      break when remaining == 0
    }
  }
  Ok(digits.join(""))
}

proc canonical_number(word: Str, hex: Bool = false) [error] -> Result[Str, Error] {
  var digits = if word.starts_with("+") { word.byte_slice(1) } else { word }
  assert if hex { rx"^[0-9a-fA-F]+$".matches(digits) } else { rx"^[0-9]+$".matches(digits) }
  while digits.byte_len() > 1 and digits.starts_with("0") { digits = digits.byte_slice(1) }
  Ok(digits.lower())
}

# GNU renders the kernel's two fsid words in their array order; statvfs
# transports the first word in the low half of its native scalar.
proc filesystem_id(value: Int) [error] -> Result[Str, Error] {
  let raw = radix(value, 16)?
  let padding = ["0" for index in range(16 - raw.byte_len())].join("")
  let full = padding + raw
  Ok(canonical_number(full.byte_slice(8, length: 8) + full.byte_slice(0, length: 8), hex: true)?)
}

pure symbolic(meta: FsStat) -> Str {
  let triples = ["---", "--x", "-w-", "-wx", "r--", "r-x", "rw-", "rwx"]
  var owner = triples[meta.mode / 64 % 8]
  var owners_group = triples[meta.mode / 8 % 8]
  var other = triples[meta.mode % 8]
  if meta.mode / 0o4000 % 2 == 1 { owner = owner.byte_slice(0, length: 2) + (if owner.ends_with("x") { "s" } else { "S" }) }
  if meta.mode / 0o2000 % 2 == 1 { owners_group = owners_group.byte_slice(0, length: 2) + (if owners_group.ends_with("x") { "s" } else { "S" }) }
  if meta.mode / 0o1000 % 2 == 1 { other = other.byte_slice(0, length: 2) + (if other.ends_with("x") { "t" } else { "T" }) }
  let prefix = match meta.kind { "dir" => "d", "symlink" => "l", "char" => "c", "block" => "b", "fifo" => "p", "socket" => "s", else => "-" }
  prefix + owner + owners_group + other
}

pure file_kind(meta: FsStat) -> Str {
  match meta.kind {
    "file" => if meta.size == 0 { "regular empty file" } else { "regular file" },
    "dir" => "directory", "symlink" => "symbolic link", "char" => "character special file", "block" => "block special file", else => meta.kind,
  }
}

proc date_text(nanos: Int) [time, error] -> Result[Str, Error] {
  time.format(nanos, "%Y-%m-%d %H:%M:%S.%N %z", utc: true)
}

# Serialize the fixed full-format contract directly from native metadata.
proc normal_output(target: Path, name: Str, device: Bool = false, follow: Bool = false) [fs, process, time, error] -> Result[Str, Error] {
  let mount = fs.mount_for(target.resolve()?)?
  let meta = fs.stat(target, follow_symlinks: follow)?
  let owner = user.by_uid(meta.uid)?.name
  let owners_group = group.by_gid(meta.gid)?.name
  var values = [radix(meta.mode % 4096, 8)?, symbolic(meta), f"{meta.blocks_512}", "512", f"{unsigned(meta.dev)?}", radix(meta.dev, 16)?, radix(meta.mode, 16)?, file_kind(meta), f"{meta.gid}", owners_group, f"{meta.nlink}", f"{unsigned(meta.ino)?}", mount.mounted_on.display(), name, f"{meta.blksize}", f"{unsigned(meta.size)?}"]
  if device { values += [f"({radix(fs.dev_major(meta.rdev), 16)?}/{radix(fs.dev_minor(meta.rdev), 16)?})"] }
  values += [f"{meta.uid}", owner]
  if device {
    if let born = meta.birth_ns { values += [date_text(born)?, f"{born / 1000000000}"] } else { values += ["-", "0"] }
  }
  for nanos in [meta.atime_ns, meta.mtime_ns, meta.ctime_ns] { values += [date_text(nanos)?, f"{nanos / 1000000000}"] }
  Ok(values.join(" ") + "\n")
}

proc filesystem_output(target: Path, name: Str) [fs, error] -> Result[Str, Error] {
  let data = fs.statvfs(target)?
  let mount = fs.mount_for(target.resolve()?)?
  guard let magic = data.type_magic else { assert false, "Linux statfs has no type magic"; return Ok("") }
  Ok(f"{data.blocks} {data.files} {filesystem_id(data.fsid)?} {data.name_max} {name} {data.block_size} {data.fragment_size} {radix(magic, 16)?} {mount.fstype}\n")
}

# Capture a real pipe rather than changing the diagnostic descriptor to a file.
proc pipe_stderr(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  uu.mkfifo(s, "stderr-pipe")?
  let captured = uu.at(s, "pipe-captured")
  let reader_plan = uu.command(s, "cat", ["stderr-pipe"], stdout: captured, stderr: uu.at(s, "reader-error"))?
  let reader = spawn reader_plan?
  let r = uu.invoke(s, "stat", args, stderr: uu.at(s, "stderr-pipe"))?
  let status = wait reader?
  assert status.exited_with(0)
  assert uu.at(s, "reader-error").read_bytes()? == b""
  Ok({util: r.util, args: r.args, status: r.status, stdout: r.stdout, stderr: captured.read_bytes()?})
}

# origin: uutils test_stat::test_invalid_arg
test test_uu_stat_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stat", ["--definitely-invalid"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_stat::test_invalid_option
test test_uu_stat_invalid_option { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stat", ["-w", "-q", "/"])?
  uu.fails(r0)
}

# origin: uutils test_stat::test_format_hyphen_leading_as_separate_arg
test test_uu_stat_format_hyphen_leading_as_separate_arg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stat", ["--format", "-%n", "/"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "-/\n")
  let r1 = uu.invoke(s, "stat", ["--printf", "-%n", "/"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "-/")
  let r2 = uu.invoke(s, "stat", ["-c", "-%n", "/"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "-/\n")
}

# origin: uutils test_stat::test_fs_default_format_block_size_label
test test_uu_stat_fs_default_format_block_size_label { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stat", ["-f", "/"])?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "Block size:")
}

# origin: uutils test_stat::test_without_argument
test test_uu_stat_without_argument { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stat", [])?
  uu.fails(r0)
  uu.stderr_contains(r0, "missing operand")
}

# origin: uutils test_stat::test_no_such_directory_message
test test_uu_stat_no_such_directory_message { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stat", ["a"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_is(r0, "stat: cannot statx 'a': No such file or directory\n")
}

# origin: uutils test_stat::test_printf_invalid_directive
test test_uu_stat_printf_invalid_directive { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stat", ["--printf=%9", "."])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "'%9': invalid directive")
  let r1 = uu.invoke(s, "stat", ["--printf=%9%", "."])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "'%9%': invalid directive")
}

# origin: uutils test_stat::test_fs_default_format_quotes_name
test test_uu_stat_fs_default_format_quotes_name { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a b")?
  let r = uu.invoke(s, "stat", ["-f", "a b"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "  File: 'a b'\n")
  let terse = uu.invoke(s, "stat", ["-f", "-t", "a b"])?
  uu.succeeds(terse)
  uu.stdout_str_starts_with(terse, "'a b' ")
}

# origin: uutils test_stat::test_fs_format
test test_uu_stat_fs_format { |ctx|
  let s = uu.scene(ctx)?
  let expected = filesystem_output(p"/dev/shm", "/dev/shm")?
  let r = uu.invoke(s, "stat", ["-f", "-c", FILESYSTEM, "/dev/shm"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
}

# origin: uutils test_stat::test_terse_fs_format
test test_uu_stat_terse_fs_format { |ctx|
  let s = uu.scene(ctx)?
  let data = fs.statvfs(p"/proc")?
  guard let magic = data.type_magic else { assert false; return }
  let expected = f"/proc {filesystem_id(data.fsid)?} {data.name_max} {radix(magic, 16)?} {data.block_size} {data.fragment_size} {data.blocks} {data.blocks_free} {data.blocks_available} {data.files} {data.files_free}\n"
  let r = uu.invoke(s, "stat", ["-f", "-t", "/proc"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
}

# origin: uutils test_stat::test_normal_format
test test_uu_stat_normal_format { |ctx|
  let s = uu.scene(ctx)?
  let expected = normal_output(p"/bin", "/bin")?
  let r = uu.invoke(s, "stat", ["-c", NORMAL, "/bin"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
}

# origin: uutils test_stat::test_char
test test_uu_stat_char { |ctx|
  let s = uu.scene(ctx)?
  let expected = normal_output(p"/dev/pts/ptmx", "/dev/pts/ptmx", device: true)?
  let r = uu.invoke(s, "stat", ["-c", DEVICE, "/dev/pts/ptmx"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
}

# origin: uutils test_stat::test_date
test test_uu_stat_date { |ctx|
  let s = uu.scene(ctx)?
  let expected0 = date_text(fs.stat(p"/bin/sh")?.ctime_ns)? + "\n"
  let r0 = uu.invoke(s, "stat", ["-c", "%z", "/bin/sh"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, expected0)
  let expected1 = date_text(fs.stat(p"/dev/ptmx")?.ctime_ns)? + "\n"
  let r1 = uu.invoke(s, "stat", ["-c", "%z", "/dev/ptmx"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, expected1)
}

# origin: uutils test_stat::test_multi_files
test test_uu_stat_multi_files { |ctx|
  let s = uu.scene(ctx)?
  let targets = ["/dev", "/usr/lib", "/etc/fstab", "/var"]
  var expected = ""
  for name in targets { expected += normal_output(Path(name), name)? }
  let r = uu.invoke(s, "stat", ["-c", NORMAL].extend(targets))?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
}

# origin: uutils test_stat::test_symlinks
test test_uu_stat_symlinks { |ctx|
  let s = uu.scene(ctx)?
  var tested = false
  for name in ["/bin/sh", "/data/data/com.termux/files/usr/bin/sh", "/bin/sudoedit", "/usr/bin/ex", "/etc/localtime", "/etc/aliases"] {
    let target = Path(name)
    let regular = match fs.stat(target, follow_symlinks: true) { Ok(meta) => meta.kind == "file", Err(failure) => { if failure.errno == 2 { false } else { return Err(failure) } } }
    if regular and fs.stat(target)?.kind == "symlink" {
      tested = true
      let expected = normal_output(target, name)?
      let r = uu.invoke(s, "stat", ["-c", NORMAL, name])?
      uu.succeeds(r)
      uu.stdout_is(r, expected)
      let followed = normal_output(target, name, follow: true)?
      let r_followed = uu.invoke(s, "stat", ["-L", "-c", NORMAL, name])?
      uu.succeeds(r_followed)
      uu.stdout_is(r_followed, followed)
    }
  }
  assert tested, "no symlink is available for the format checks"
}

# origin: uutils test_stat::test_terse_normal_format
test test_uu_stat_terse_normal_format { |ctx|
  let s = uu.scene(ctx)?
  let meta = fs.stat(p"/")?
  let expected = ["/", f"{unsigned(meta.size)?}", f"{meta.blocks_512}", radix(meta.mode, 16)?, f"{meta.uid}", f"{meta.gid}", radix(meta.dev, 16)?, f"{unsigned(meta.ino)?}", f"{meta.nlink}", radix(fs.dev_major(meta.rdev), 16)?, radix(fs.dev_minor(meta.rdev), 16)?, f"{meta.atime_ns / 1000000000}", f"{meta.mtime_ns / 1000000000}", f"{meta.ctime_ns / 1000000000}", f"{(meta.birth_ns ?? 0) / 1000000000}", f"{meta.blksize}"]
  let r = uu.invoke(s, "stat", ["-t", "/"])?
  uu.succeeds(r)

  let actual = r.stdout.utf8()?.trim().split(" ")
  assert ! expected.is_empty()
  for index in range(if actual.len() < expected.len() { actual.len() } else { expected.len() }) { assert actual[index] == expected[index] or expected[index] == "0" }
}

# origin: uutils test_stat::test_format_created_time
test test_uu_stat_format_created_time { |ctx|
  let s = uu.scene(ctx)?
  let meta = fs.stat(p"/bin")?
  let expected = if let born = meta.birth_ns { date_text(born)? } else { "-" }
  let r = uu.invoke(s, "stat", ["-c", "%w", "/bin"])?
  uu.succeeds(r)

  let actual_fields = rx"\s".replace(r.stdout.utf8()?, with: "\u{0}").split("\u{0}")
  let expected_fields = rx"\s".replace(expected + "\n", with: "\u{0}").split("\u{0}")
  assert ! expected_fields.is_empty()
  if expected != "-" {
    for index in range(if actual_fields.len() < expected_fields.len() { actual_fields.len() } else { expected_fields.len() }) {
      assert actual_fields[index] == expected_fields[index] or expected_fields[index] == "-"
    }
  }
}

# origin: uutils test_stat::test_format_created_seconds
test test_uu_stat_format_created_seconds { |ctx|
  let s = uu.scene(ctx)?
  let meta = fs.stat(p"/bin")?
  let expected = if let born = meta.birth_ns { f"{born / 1000000000}" } else { "0" }
  let r = uu.invoke(s, "stat", ["-c", "%W", "/bin"])?
  uu.succeeds(r)

  let actual_fields = rx"\s".replace(r.stdout.utf8()?, with: "\u{0}").split("\u{0}")
  let expected_fields = rx"\s".replace(expected + "\n", with: "\u{0}").split("\u{0}")
  assert ! expected_fields.is_empty()
  if expected != "0" {
    for index in range(if actual_fields.len() < expected_fields.len() { actual_fields.len() } else { expected_fields.len() }) {
      assert actual_fields[index] == expected_fields[index] or expected_fields[index] == "0"
    }
  }
}

# origin: uutils test_stat::test_printf_atime_ctime_mtime_precision
test test_uu_stat_printf_atime_ctime_mtime_precision { |ctx|
  let s = uu.scene(ctx)?
  let meta = fs.stat(p"/dev/pts/ptmx")?
  let expected = f"{meta.mtime_ns / 1000000000} {meta.mtime_ns / 1000000000}.{time.format(meta.mtime_ns, "%1N", utc: true)?} {meta.atime_ns / 1000000000}.{time.format(meta.atime_ns, "%2N", utc: true)?} {meta.mtime_ns / 1000000000}.{time.format(meta.mtime_ns, "%2N", utc: true)?} {meta.ctime_ns / 1000000000}.{time.format(meta.ctime_ns, "%2N", utc: true)?}\n"
  let r = uu.invoke(s, "stat", ["-c", "%.0Y %.1Y %.2X %.2Y %.2Z", "/dev/pts/ptmx"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
}

# origin: uutils test_stat::test_printf
test test_uu_stat_printf { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stat", ["--printf=123%-# 15q\\r\\\"\\\\\\a\\b\\x1B\\f\\x0B%+020.23m\\x12\\167\\132\\112\\n", "/"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.concat([b"123?\r\"\\\x07\x08\x1b\x0c\x0b", b"                   /\x12wZJ\n"]))
}

# origin: uutils test_stat::test_pipe_fifo
test test_uu_stat_pipe_fifo { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "FIFO")?
  let r = uu.invoke(s, "stat", ["FIFO"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_contains(r, "fifo")
  uu.stdout_contains(r, "File: FIFO")
}

# origin: uutils test_stat::test_stdin_pipe_fifo2
test test_uu_stat_stdin_pipe_fifo2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_from_path(s, "stat", ["-"], stdin: p"/dev/null")?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_contains(r, "character special file")
  uu.stdout_contains(r, "File: -")
}

# origin: uutils test_stat::test_stdin_redirect
test test_uu_stat_stdin_redirect { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r = uu.invoke_from_path(s, "stat", ["-"], stdin: uu.at(s, "f"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_contains(r, "regular empty file")
  uu.stdout_contains(r, "File: -")
}

# origin: uutils test_stat::test_stdin_pipe_fifo1
test test_uu_stat_stdin_pipe_fifo1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stat", ["-"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_contains(r, "fifo")
  uu.stdout_contains(r, "File: -")
  let followed = uu.invoke(s, "stat", ["-L", "-"])?
  uu.succeeds(followed)
  uu.no_stderr(followed)
  uu.stdout_contains(followed, "fifo")
  uu.stdout_contains(followed, "File: -")
}

# origin: uutils test_stat::test_stdin_with_fs_option
test test_uu_stat_stdin_with_fs_option { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_from_path(s, "stat", ["-f", "-"], stdin: p"/dev/null")?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "using '-' to denote standard input does not work in file system mode")
}

# origin: uutils test_stat::test_printf_octal_1
test test_uu_stat_printf_octal_1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stat", ["--printf=\\012\\377", "."])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"\x0a\xff")
}

# origin: uutils test_stat::test_printf_octal_2
test test_uu_stat_printf_octal_2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stat", ["--printf=.\\012a\\377b", "."])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"\x2e\x0a\x61\xff\x62")
}

# origin: uutils test_stat::test_printf_octal_out_of_range
test test_uu_stat_printf_octal_out_of_range { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stat", ["--printf=\\400\\777", "."])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"\x00\xff")
}

# origin: uutils test_stat::test_printf_bel_etc
test test_uu_stat_printf_bel_etc { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stat", ["--printf=\\a\\b\\f\\n\\r\\t", "."])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"\x07\x08\x0c\x0a\x0d\x09")
}

# origin: uutils test_stat::test_invalid_directive_after_multibyte_char
test test_uu_stat_invalid_directive_after_multibyte_char { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stat", ["-c", "€%-", "."])?
  uu.fails_with_code(r0, 1)
  uu.stdout_is(r0, "€")
  uu.stderr_is(r0, "stat: '%-': invalid directive\n")
  let r1 = uu.invoke(s, "stat", ["-c", "ä%0", "."])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is(r1, "ä")
  uu.stderr_is(r1, "stat: '%0': invalid directive\n")
  let r2 = uu.invoke(s, "stat", ["-c", "€%.", "."])?
  uu.fails_with_code(r2, 1)
  uu.stdout_is(r2, "€")
  uu.stderr_is(r2, "stat: '%.': invalid directive\n")
}

# origin: uutils test_stat::test_precision_splits_multibyte_char_in_value
test test_uu_stat_precision_splits_multibyte_char_in_value { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "é")?
  let r = uu.invoke(s, "stat", ["-c", "%.1n", "é"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"\xc3\n")
}

# origin: uutils test_stat::test_mount_point_basic
test test_uu_stat_mount_point_basic { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stat", ["-c", "%m", "/"])?
  uu.succeeds(r)
  let output = r.stdout.utf8()?.trim()
  assert ! output.is_empty()
  assert output == "/"
}

# origin: uutils test_stat::test_mount_point_combined_with_other_specifiers
test test_uu_stat_mount_point_combined_with_other_specifiers { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stat", ["-c", "%m %n %s", "/bin/sh"])?
  uu.succeeds(r)
  assert r.stdout.utf8()?.fields().len() >= 3
}

# origin: uutils test_stat::test_percent_escaping
test test_uu_stat_percent_escaping { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stat", ["--printf", "%%%m%%m%m%%%", "/bin/sh"])?
  uu.succeeds(r)
  uu.stdout_is(r, "%/%m/%%")
}

# origin: uutils test_stat::test_mount_point_width_and_alignment
test test_uu_stat_mount_point_width_and_alignment { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "stat", ["-c", "%15m", "/"])?
  uu.succeeds(r0)
  let right = r0.stdout.utf8()?
  assert right.trim().byte_len() <= 15 and right.byte_len() >= 15
  let r1 = uu.invoke(s, "stat", ["-c", "%-15m", "/"])?
  uu.succeeds(r1)
  let left = r1.stdout.utf8()?
  assert left.trim().byte_len() <= 15 and left.byte_len() >= 15
}

# origin: uutils test_stat::test_timestamp_format
test test_uu_stat_timestamp_format { |ctx|
  let s = uu.scene(ctx)?
  let touched = uu.invoke(s, "touch", ["-d", "1970-01-01 18:43:33.023456789", "k"])?
  uu.succeeds(touched)
  uu.no_stderr(touched)
  let r0 = uu.invoke(s, "stat", ["-c", "%Y", "k"])?
  uu.succeeds(r0)
  uu.stdout_is(r0, "67413\n")

  let r1 = uu.invoke(s, "stat", ["-c", "%.Y", "k"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "67413.023456789\n")

  let r2 = uu.invoke(s, "stat", ["-c", "%.1Y", "k"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "67413.0\n")

  let r3 = uu.invoke(s, "stat", ["-c", "%.3Y", "k"])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "67413.023\n")

  let r4 = uu.invoke(s, "stat", ["-c", "%.6Y", "k"])?
  uu.succeeds(r4)
  uu.stdout_is(r4, "67413.023456\n")

  let r5 = uu.invoke(s, "stat", ["-c", "%.9Y", "k"])?
  uu.succeeds(r5)
  uu.stdout_is(r5, "67413.023456789\n")

  let r6 = uu.invoke(s, "stat", ["-c", "%13.6Y", "k"])?
  uu.succeeds(r6)
  uu.stdout_is(r6, " 67413.023456\n")

  let r7 = uu.invoke(s, "stat", ["-c", "%013.6Y", "k"])?
  uu.succeeds(r7)
  uu.stdout_is(r7, "067413.023456\n")

  let r8 = uu.invoke(s, "stat", ["-c", "%-13.6Y", "k"])?
  uu.succeeds(r8)
  uu.stdout_is(r8, "67413.023456 \n")

  let r9 = uu.invoke(s, "stat", ["-c", "%18.10Y", "k"])?
  uu.succeeds(r9)
  uu.stdout_is(r9, "  67413.0234567890\n")

  let r10 = uu.invoke(s, "stat", ["-c", "%I18.10Y", "k"])?
  uu.succeeds(r10)
  uu.stdout_is(r10, "  67413.0234567890\n")

  let r11 = uu.invoke(s, "stat", ["-c", "%018.10Y", "k"])?
  uu.succeeds(r11)
  uu.stdout_is(r11, "0067413.0234567890\n")

  let r12 = uu.invoke(s, "stat", ["-c", "%-18.10Y", "k"])?
  uu.succeeds(r12)
  uu.stdout_is(r12, "67413.0234567890  \n")

}

# origin: uutils test_stat::test_timestamp_format_preserves_nanoseconds
test test_uu_stat_timestamp_format_preserves_nanoseconds { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "timestamp")?
  fs.set_times(uu.at(s, "timestamp"), atime_ns: 1755300000123456789, mtime_ns: 1755300000123456789)?
  let meta = fs.stat(uu.at(s, "timestamp"))?
  let expected = "1755300000.123456789 1755300000.123456789 " + time.format(meta.ctime_ns, "%s.%N", utc: true)? + "\n"
  let r = uu.invoke(s, "stat", ["-c", "%.9X %.9Y %.9Z", "timestamp"])?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
}

# origin: uutils test_stat::test_timestamp_format_before_epoch
test test_uu_stat_timestamp_format_before_epoch { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "timestamp")?
  fs.set_times(uu.at(s, "timestamp"), atime_ns: -876543211, mtime_ns: -876543211)?
  let r = uu.invoke(s, "stat", ["-c", "%.1X %.3X %.9X %.1Y %.3Y %.9Y", "timestamp"])?
  uu.succeeds(r)
  uu.stdout_is(r, "-0.8 -0.876 -0.876543211 -0.8 -0.876 -0.876543211\n")

}

# origin: uutils test_stat::test_timestamp_format_before_epoch_truncation
test test_uu_stat_timestamp_format_before_epoch_truncation { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "fractional-time")?
  fs.set_times(uu.at(s, "fractional-time"), atime_ns: -2234567891, mtime_ns: -2234567891)?
  let rX = uu.invoke(s, "stat", ["-c", "%X|%.0X|%.X|%.2X|%.6X|%.12X", "fractional-time"])?
  uu.succeeds(rX)
  uu.stdout_is(rX, "-3|-3|-2.234567891|-2.23|-2.234567|-2.234567891000\n")

  let rY = uu.invoke(s, "stat", ["-c", "%Y|%.0Y|%.Y|%.2Y|%.6Y|%.12Y", "fractional-time"])?
  uu.succeeds(rY)
  uu.stdout_is(rY, "-3|-3|-2.234567891|-2.23|-2.234567|-2.234567891000\n")

}

# origin: uutils test_stat::test_quoting_style_default
test test_uu_stat_quoting_style_default { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "plain")?
  uu.touch(s, "a b")?
  uu.symlink(s, "plain", "link")?
  let r = uu.invoke(s, "stat", ["-c", "%N", "plain", "a b", "link"])?
  uu.succeeds(r)
  uu.stdout_only(r, "plain\n'a b'\nlink -> plain\n")
  let normal = uu.invoke(s, "stat", ["a b"])?
  uu.succeeds(normal)
  uu.stdout_contains(normal, "  File: 'a b'\n")
}

# origin: uutils test_stat::test_quoted_name_directive
test test_uu_stat_quoted_name_directive { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a b")?
  uu.symlink(s, "a b", "link")?
  let r = uu.invoke(s, "stat", ["-c", "%Qn|%-6Qn|", "a b", "link"])?
  uu.succeeds(r)
  uu.stdout_only(r, "'a b'|'a b' |\nlink|link  |\n")
  let quoted = uu.invoke(s, "stat", ["-f", "-c", "%Qn", "a b"], vars: {QUOTING_STYLE: "c"})?
  uu.succeeds(quoted)
  uu.stdout_only(quoted, "\"a b\"\n")
  let terse = uu.invoke(s, "stat", ["-t", "a b"])?
  uu.succeeds(terse)
  uu.stdout_str_starts_with(terse, "'a b' ")
}

# origin: uutils test_stat::test_quoting_style_env
test test_uu_stat_quoting_style_env { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "it's")?
  uu.touch(s, "tab\there")?
  let r0 = uu.invoke(s, "stat", ["-c", "%N", "it's", "tab\there"], vars: {QUOTING_STYLE: "literal"})?
  uu.succeeds(r0)
  uu.stdout_only(r0, "it's\ntab\there\n")
  let r1 = uu.invoke(s, "stat", ["-c", "%N", "it's", "tab\there"], vars: {QUOTING_STYLE: "shell"})?
  uu.succeeds(r1)
  uu.stdout_only(r1, "\"it's\"\n'tab\there'\n")
  let r2 = uu.invoke(s, "stat", ["-c", "%N", "it's", "tab\there"], vars: {QUOTING_STYLE: "shell-always"})?
  uu.succeeds(r2)
  uu.stdout_only(r2, "\"it's\"\n'tab\there'\n")
  let r3 = uu.invoke(s, "stat", ["-c", "%N", "it's", "tab\there"], vars: {QUOTING_STYLE: "shell-escape"})?
  uu.succeeds(r3)
  uu.stdout_only(r3, "\"it's\"\n'tab'$'\\t''here'\n")
  let r4 = uu.invoke(s, "stat", ["-c", "%N", "it's", "tab\there"], vars: {QUOTING_STYLE: "shell-escape-always"})?
  uu.succeeds(r4)
  uu.stdout_only(r4, "\"it's\"\n'tab'$'\\t''here'\n")
  let r5 = uu.invoke(s, "stat", ["-c", "%N", "it's", "tab\there"], vars: {QUOTING_STYLE: "c"})?
  uu.succeeds(r5)
  uu.stdout_only(r5, "\"it's\"\n\"tab\\there\"\n")
  let r6 = uu.invoke(s, "stat", ["-c", "%N", "it's", "tab\there"], vars: {QUOTING_STYLE: "escape"})?
  uu.succeeds(r6)
  uu.stdout_only(r6, "it's\ntab\\there\n")
  let r7 = uu.invoke(s, "stat", ["-c", "%N", "it's", "tab\there"], vars: {QUOTING_STYLE: "locale"})?
  uu.succeeds(r7)
  uu.stdout_only(r7, "'it\\'s'\n'tab\\there'\n")
  let r8 = uu.invoke(s, "stat", ["-c", "%N", "it's", "tab\there"], vars: {QUOTING_STYLE: "clocale"})?
  uu.succeeds(r8)
  uu.stdout_only(r8, "\"it's\"\n\"tab\\there\"\n")
}

# origin: uutils test_stat::test_quoting_style_locale
test test_uu_stat_quoting_style_locale { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "'")?
  let r = uu.invoke(s, "stat", ["-c", "%N", "'"], vars: {QUOTING_STYLE: "locale"})?
  uu.succeeds(r)
  uu.stdout_only(r, "'\\''\n")
  let default = uu.invoke(s, "stat", ["-c", "%N", "'"])?
  uu.succeeds(default)
  uu.stdout_only(default, "\"'\"\n")
  uu.touch(s, "\"")?
  let double = uu.invoke(s, "stat", ["-c", "%N", "\""])?
  uu.succeeds(double)
  uu.stdout_only(double, "'\"'\n")
}

# origin: uutils test_stat::test_quoting_newline_in_filename
test test_uu_stat_quoting_newline_in_filename { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "test\nnewline")?
  let r0 = uu.invoke(s, "stat", ["-c", "{\"name\":\"%N\"}", "test\nnewline"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "{\"name\":\"'test'$'\\n''newline'\"}\n")
  uu.touch(s, "contiguous\n\nescape_characters")?
  let r1 = uu.invoke(s, "stat", ["-c", "%N", "contiguous\n\nescape_characters"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "'contiguous'$'\\n\\n''escape_characters'\n")
  uu.touch(s, "multiple\nescape\ncharacters")?
  let r2 = uu.invoke(s, "stat", ["-c", "%N", "multiple\nescape\ncharacters"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "'multiple'$'\\n''escape'$'\\n''characters'\n")
  uu.touch(s, "\t \n \r \u{0001}")?
  let r3 = uu.invoke(s, "stat", ["-c", "%N", "\t \n \r \u{0001}"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "''$'\\t'' '$'\\n'' '$'\\r'' '$'\\001'\n")
}

# origin: uutils test_stat::test_quoting_style_invalid_env
test test_uu_stat_quoting_style_invalid_env { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "baguette")?
  uu.touch(s, "Croissant")?
  uu.touch(s, "Escargot")?
  let r = uu.invoke(s, "stat", ["-c", "nom=[%N]", "baguette", "Croissant", "Escargot"], vars: {QUOTING_STYLE: "fromage"})?
  uu.succeeds(r)
  uu.stdout_is(r, "nom=[baguette]\nnom=[Croissant]\nnom=[Escargot]\n")

  assert r.stderr.utf8()?.split("ignoring invalid value of environment variable QUOTING_STYLE").len() - 1 == 1
  let empty = uu.invoke(s, "stat", ["-c", "%N", "baguette"], vars: {QUOTING_STYLE: ""})?
  uu.succeeds(empty)
  uu.stdout_is(empty, "baguette\n")
  uu.stderr_is(empty, "stat: ignoring invalid value of environment variable QUOTING_STYLE: ''\n")
  let multibyte = uu.invoke(s, "stat", ["-c", "%%%N", "baguette"], vars: {QUOTING_STYLE: "soufflé"})?
  uu.succeeds(multibyte)
  uu.stdout_is(multibyte, "%baguette\n")
  uu.stderr_is(multibyte, "stat: ignoring invalid value of environment variable QUOTING_STYLE: 'souffl\\303\\251'\n")
  let unused = uu.invoke(s, "stat", ["-c", "taille=%s genre:%F brut=%n", "baguette"], vars: {QUOTING_STYLE: "crème-brûlée"})?
  uu.succeeds(unused)
  uu.no_stderr(unused)
}

# origin: uutils test_stat::test_error_message_preserves_non_utf8_filename
test test_uu_stat_error_message_preserves_non_utf8_filename { |ctx|
  let s = uu.scene(ctx)?
  for value in [{name: b"missing-\xff", quoted: "'missing-'$'\\377'"}, {name: b"missing-\xc3\xa9", quoted: "'missing-'$'\\303\\251'"}] {
    for options in [[], [p"-L"], [p"-f"]] {
      let r = uu.invoke_paths(s, "stat", options.extend([Path.parse_bytes(value.name)?]), vars: {LC_ALL: "C"})?
      uu.fails_with_code(r, 1)
      uu.stderr_contains(r, value.quoted)
    }
  }
}

# origin: uutils test_stat::test_correct_metadata
test test_uu_stat_correct_metadata { |ctx|
  let s = uu.scene(ctx)?
  for name in ["/", "/dev/null"] {
    let meta = fs.stat(Path(name), follow_symlinks: true)?
    let expected = [meta.uid, meta.gid, meta.mode, meta.blocks_512, meta.size, meta.nlink, meta.ino, meta.dev, fs.dev_major(meta.dev), fs.dev_minor(meta.dev), meta.dev, meta.rdev, fs.dev_major(meta.rdev), fs.dev_minor(meta.rdev), meta.rdev, fs.dev_major(meta.rdev), fs.dev_minor(meta.rdev)]
    let r = uu.invoke(s, "stat", ["--printf", "%u %g %f %b %s %h %i %d %Hd %Ld %D %r %Hr %Lr %R %t %T", name])?
    uu.succeeds(r)
    let actual = r.stdout.utf8()?.split(" ")
    assert actual.len() == expected.len()
    for index in range(expected.len()) {
      if index in [2, 10, 14, 15, 16] { assert canonical_number(actual[index], hex: true)? == radix(expected[index], 16)? } else { assert canonical_number(actual[index])? == unsigned(expected[index])? }
    }
  }
}

# origin: uutils test_stat::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_stat_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = pipe_stderr(s, ["-c", "%d%.3", "/dev/null"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "stat: '%.3': invalid directive\n")
}
