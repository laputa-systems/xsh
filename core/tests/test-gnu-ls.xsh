use support.uu

pure has_bytes(data: Bytes, needle: Bytes) -> Bool {
  for i in range(data.len()) { if data.slice(i, needle.len()) == needle { return true } }
  false
}

# origin: gnu ls/a-option.log
test test_gnu_ls_a_option_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  let r = uu.invoke(s, "ls", ["-aA", "d"])?
  uu.succeeds(r)
  uu.no_stdout(r)
}

# origin: gnu ls/birthtime.log
test test_gnu_ls_birthtime_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  for args in [["--time=birth", "-l", "a"], ["--time=creation", "-t", "a"]] {
    uu.succeeds(uu.invoke(s, "ls", args)?)
  }
}

# origin: gnu ls/hex-option.log
test test_gnu_ls_hex_option_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "ls", ["-x", "-T0x10", "-w010"])?)
}

# origin: gnu ls/x-option.log
test test_gnu_ls_x_option_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "subdir")?
  uu.touch(s, "subdir/b")?
  uu.touch(s, "subdir/a")?
  for row in [{args: ["-x", "subdir"], out: "a  b\n"}, {args: ["-rx", "subdir"], out: "b  a\n"}] {
    let r = uu.invoke(s, "ls", row.args)?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
}

# origin: gnu ls/sort-width-option.log
test test_gnu_ls_sort_width_option_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "subdir")?
  for name in ["aaaaa", "bbb", "cccc", "d", "zz"] { uu.touch(s, f"subdir/{name}")? }
  let r = uu.invoke(s, "ls", ["--sort=width", "subdir"])?
  uu.succeeds(r)
  uu.stdout_is(r, "d\nzz\nbbb\ncccc\naaaaa\n")
}

# origin: gnu ls/symlink-loop.log
test test_gnu_ls_symlink_loop_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "loop", "loop")?
  for args in [["loop"], ["-l", "loop"], ["-l", "--color=always", "loop"]] {
    uu.succeeds(uu.invoke(s, "ls", args)?)
  }
  for flag in ["-H", "-L"] { uu.fails_with_code(uu.invoke(s, "ls", [flag, "loop"])?, 2) }
}

# origin: gnu ls/symlink-quote.log
test test_gnu_ls_symlink_quote_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "needs quoting", "symlink")?
  let r = uu.invoke(s, "ls", ["-l", "--quoting-style=shell-escape", "symlink"])?
  uu.succeeds(r)
  assert r.stdout.utf8()?.ends_with("symlink -> 'needs quoting'\n")
}

# origin: gnu ls/symlink-slash.log
test test_gnu_ls_symlink_slash_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.symlink(s, "dir", "symlink")?
  for name in ["symlink/", "symlink/."] {
    let r = uu.invoke(s, "ls", ["-l", name])?
    assert r.stdout.utf8()?.words().join(" ") == "total 0"
  }
}

# origin: gnu ls/color-symlink-target.log
test test_gnu_ls_color_symlink_target_log { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "ls", ["--color=always", "."], vars: {LS_COLORS: "ln=target:x"})?
  uu.succeeds(r)
  uu.stderr_is(r, "ls: unparsable value for LS_COLORS environment variable\n")
}

# origin: gnu ls/color-term.log
test test_gnu_ls_color_term_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "exe")?
  uu.set_mode(s, "exe", 0o744)?
  for row in [{term: "", color: "nonempty", out: "\x1b[0m\x1b[01;32mexe\x1b[0m\n"}, {term: "xterm", color: "", out: "\x1b[0m\x1b[01;32mexe\x1b[0m\n"}, {term: "dumb", color: "", out: "exe\n"}, {term: "", color: "", out: "exe\n"}] {
    let r = uu.invoke(s, "ls", ["--color=always", "exe"], vars: {TIME_STYLE: "+norm", LS_COLORS: "", COLORTERM: row.color, TERM: row.term})?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
}

# origin: gnu ls/infloop.log
test test_gnu_ls_infloop_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "loop")?
  uu.symlink(s, "../loop", "loop/sub")?
  let r = uu.invoke(s, "ls", ["-RL", "loop"], timeout: 10s)?
  uu.fails_with_code(r, 2)
  uu.stdout_is(r, "loop:\nsub\n")
  uu.stderr_is(r, "ls: loop/sub: not listing already-listed directory\n")
}

# origin: gnu ls/dangle.log
test test_gnu_ls_dangle_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "no-such-file", "dangle")?
  uu.mkdir(s, "dir/sub")?
  uu.symlink(s, "dir", "slink-to-dir")?
  uu.mkdir(s, "d")?
  uu.symlink(s, "no-such", "d/dangle")?
  for flag in ["-L", "-H"] { uu.fails_with_code(uu.invoke(s, "ls", [flag, "dangle"])?, 2) }
  let plain = uu.invoke(s, "ls", ["dangle"])?
  uu.succeeds(plain)
  uu.stdout_is(plain, "dangle\n")
  for flags in [[], ["-H"], ["-L"]] {
    let r = uu.invoke(s, "ls", flags.extend(["slink-to-dir"]))?
    uu.succeeds(r)
    assert bytes.concat([r.stdout, r.stderr]) == b"sub\n"
  }
  for row in [{flag: "-Li", out: "? dangle\n"}, {flag: "-Ls", out: "total 0\n? dangle\n"}] {
    let r = uu.invoke(s, "ls", [row.flag, "d"])?
    uu.fails_with_code(r, 1)
    uu.stdout_is(r, row.out)
  }
}

# origin: gnu ls/group-dirs.log
test test_gnu_ls_group_dirs_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir/b")?
  uu.touch(s, "dir/a")?
  uu.symlink(s, "b", "dir/bl")?
  for row in [{args: ["--group", "dir"], out: "b\nbl\na\n"}, {args: ["--group", "-d", "dir/a", "dir/b", "dir/bl"], out: "dir/b\ndir/bl\ndir/a\n"}] {
    let r = uu.invoke(s, "ls", row.args)?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
  uu.mkdir(s, "dir2")?
  for name in ["dir_b", "dir_a", "dir_c"] { uu.mkdir(s, f"dir2/{name}")? }
  for name in ["file_c", "file_a", "file_b"] { uu.touch(s, f"dir2/{name}")? }
  for flags in [["--group-directories-first"], ["--group-directories-first", "--sort=size"]] {
    let r = uu.invoke(s, "ls", flags.extend(["dir2"]))?
    uu.succeeds(r)
    uu.stdout_is(r, "dir_a\ndir_b\ndir_c\nfile_a\nfile_b\nfile_c\n")
  }
}

# origin: gnu ls/follow-slink.log
test test_gnu_ls_follow_slink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir/sub")?
  uu.mkdir(s, "dir1")?
  uu.symlink(s, "link", "dir/link")?
  uu.symlink(s, "../../dir1", "dir/sub/link-to-dir")?
  let nested = {ctx: s.ctx, root: uu.at(s, "dir")}
  uu.succeeds(uu.invoke(nested, "ls", ["-F", "link"])?)
  uu.fails_with_code(uu.invoke(nested, "ls", ["-L", "link"])?, 2)
  let direct = uu.invoke(nested, "ls", ["-L"])?
  uu.succeeds(direct)
  uu.stdout_is(direct, "link\nsub\n")
  let recursive = uu.invoke(nested, "ls", ["-FLR", "sub"])?
  uu.succeeds(recursive)
  uu.stdout_is(recursive, "sub:\nlink-to-dir/\n\nsub/link-to-dir:\n")
}

# origin: gnu ls/zero-option.log
test test_gnu_ls_zero_option_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  for name in ["a", "b", "cc"] { uu.touch(s, f"dir/{name}")? }
  let long = uu.invoke(s, "ls", ["-l", "--zero", "dir"])?
  uu.succeeds(long)
  assert long.stdout.utf8()?.starts_with("total")
  uu.fails_with_code(uu.invoke(s, "ls", ["-l", "--dired", "--zero", "dir"])?, 2)
  for name in ["com,ma", "n\nl"] { uu.touch(s, f"dir/{name}")? }
  let r = uu.invoke(s, "ls", ["--color=always", "-x", "-m", "-C", "-Q", "-q", "--zero", "dir"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"a\x00b\x00cc\x00com,ma\x00n\nl\x00")
}

# origin: gnu ls/time-style-diag.log
test test_gnu_ls_time_style_diag_log { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "ls", ["-l", "--time-style=XX"])?
  uu.fails_with_code(r, 2)
  uu.no_stdout(r)
  uu.stderr_is_bytes(r, fp"{ctx.core_dir}/tests/data/gnu/ls/time-style-diag.stderr".read_bytes()?)
}

# origin: gnu ls/non-utf8-hidden.log
test test_gnu_ls_non_utf8_hidden_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.write(s, "d/visible", "content\n")?
  uu.touch(s, "d/.hidden_valid")?
  uu.at_bytes(s, b"d/.hidden_invalid\xff|\xed\xba\xad|\xc2\x89|\xed\xa6\xbf\xed\xbf\xbf")?.write(b"content\n")?
  for locale in ["C", "fr_FR", "fr_FR.UTF-8"] {
    let r = uu.invoke(s, "ls", ["-U", "d"], vars: {LC_ALL: locale})?
    uu.succeeds(r)
    uu.stdout_is(r, "visible\n")
    let all = uu.invoke(s, "ls", ["-a", "-U", "d"], vars: {LC_ALL: locale})?
    uu.succeeds(all)
    assert all.stdout.count_lines() >= 5
    for name in [b"visible", b".hidden_valid", b".hidden_invalid"] { assert has_bytes(all.stdout, name) }
  }
}

# origin: gnu ls/no-arg.log
test test_gnu_ls_no_arg_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir/subdir")?
  uu.touch(s, "dir/subdir/file2")?
  uu.symlink(s, "f", "symlink")?
  uu.touch(s, "exp")?
  uu.touch(s, "out")?
  let plain = uu.invoke(s, "ls", ["-1"])?
  uu.succeeds(plain)
  uu.stdout_is(plain, "dir\nexp\nout\nsymlink\n")
  let tree = uu.invoke(s, "ls", ["-R1"])?
  uu.succeeds(tree)
  uu.stdout_is(tree, ".:\ndir\nexp\nout\nsymlink\n\n./dir:\nsubdir\n\n./dir/subdir:\nfile2\n")
}

# origin: gnu ls/inode.log
test test_gnu_ls_inode_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.symlink(s, "f", "slink")?
  for row in [{flag: "-Ci", explicit: true, same: false}, {flag: "-CLi", explicit: true, same: true}, {flag: "-CHi", explicit: true, same: true}, {flag: "-Ci", explicit: false, same: false}, {flag: "-CLi", explicit: false, same: true}, {flag: "-CHi", explicit: false, same: false}] {
    let args = if row.explicit { [row.flag, "f", "slink"] } else { [row.flag] }
    let r = uu.invoke(s, "ls", args)?
    let fields = r.stdout.utf8()?.words()
    assert fields.len() == 4
    let equal = fields[0] == fields[2]
    assert equal == row.same
  }
}

# origin: gnu ls/rt-1.log
test test_gnu_ls_rt_1_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["c", "a", "b"] { uu.succeeds(uu.invoke(s, "touch", ["-d", "1998-01-15", name])?) }
  for row in [{flag: "-1t", out: "a\nb\nc\n"}, {flag: "-1rt", out: "c\nb\na\n"}] {
    let r = uu.invoke(s, "ls", [row.flag, "a", "b", "c"])?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
}

# origin: gnu ls/size-align.log
test test_gnu_ls_size_align_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "small")?
  uu.touch(s, "large")?
  uu.truncate(s, "large", 123456)?
  uu.write(s, "alloc", "\n")?
  let r = uu.invoke(s, "ls", ["-s", "-l", "small", "alloc", "large"])?
  uu.succeeds(r)
  let lines = r.stdout.utf8()?.lines()
  assert lines.len() == 3
  assert lines[0].byte_len() == lines[1].byte_len() and lines[1].byte_len() == lines[2].byte_len()
}

# origin: gnu ls/selinux-segfault.log
test test_gnu_ls_selinux_segfault_log { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "ls", ["-l", "/proc/sys"])?)
  uu.mkdir(s, "sedir")?
  uu.symlink(s, "missing", "sedir/broken")?
  uu.fails_with_code(uu.invoke(s, "ls", ["-L", "-R", "-Z", "-m", "sedir"])?, 1)
  uu.succeeds(uu.invoke(s, "ls", ["-Z", "."])?)
}

# origin: gnu ls/stat-dtype.log
test test_gnu_ls_stat_dtype_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "c/d")?
  uu.set_mode(s, "c", 0o644)?
  defer { uu.set_mode(s, "c", 0o755)? }
  let probe = uu.invoke(s, "ls", ["-p", "c"])?
  if bytes.concat([probe.stdout, probe.stderr]) != b"d/\n" { test.skip("directory entry types unavailable") }
  uu.mkdir(s, "d")?
  uu.symlink(s, "/", "d/s")?
  uu.set_mode(s, "d", 0o600)?
  defer { uu.set_mode(s, "d", 0o700)? }
  uu.mkdir(s, "e/a2345")?
  uu.mkdir(s, "e/b")?
  uu.set_mode(s, "e", 0o600)?
  defer { uu.set_mode(s, "e", 0o700)? }
  let types = uu.invoke(s, "ls", ["--file-type", "d"])?
  uu.succeeds(types)
  uu.stdout_is(types, "s@\n")
  let columns = uu.invoke(s, "ls", ["-CF", "e"])?
  uu.succeeds(columns)
  uu.stdout_is(columns, "a2345/\tb/\n")
}

# origin: gnu ls/stat-failed.log
test test_gnu_ls_stat_failed_log { |ctx|
  let s = uu.scene(ctx)?
  if user.current()?.uid == 0 { test.skip("directory search permission requires unprivileged user") }
  uu.mkdir(s, "d")?
  uu.symlink(s, "/", "d/s")?
  uu.set_mode(s, "d", 0o600)?
  defer { uu.set_mode(s, "d", 0o700)? }
  let r = uu.invoke(s, "ls", ["-Log", "d"])?
  uu.fails_with_code(r, 1)
  assert r.stdout.utf8()?.replace("\nl", with: "\n?") == "total 0\n?????????? ? ?            ? s\n"
  let dired = uu.invoke(s, "ls", ["--dired", "-l", "d"])?
  uu.fails_with_code(dired, 1)
  assert dired.stdout.utf8()?.replace("  l", with: "  ?") == "  total 0\n  ?????????? ? ? ? ?            ? s\n//DIRED// 44 45\n//DIRED-OPTIONS// --quoting-style=literal\n"
}

# origin: gnu ls/w-option.log
test test_gnu_ls_w_option_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "b"] { uu.touch(s, name)? }
  uu.set_mode(s, "a", 0o755)?
  for flag in ["-w-1", "-w08"] { uu.fails_with_code(uu.invoke(s, "ls", [flag])?, 2) }
  uu.succeeds(uu.invoke(s, "ls", ["-w18446744073709551616"])?)
  for args in [["-w0", "-x", "-T1", "a", "b"], ["-w4", "-x", "-T0", "a", "b"]] {
    let r = uu.invoke(s, "ls", args)?
    uu.succeeds(r)
    uu.stdout_is(r, "a  b\n")
  }
  uu.succeeds(uu.invoke(s, "ls", ["-w0", "-x", "--color=always"], vars: {TERM: "xterm"})?)
  for name in ["aa", "c"] { uu.touch(s, name)? }
  for row in [{flag: "-x", out: "aa  b\nc\n"}, {flag: "-C", out: "aa  c\nb\n"}] {
    let r = uu.invoke(s, "ls", ["-w5", row.flag, "-T0", "aa", "b", "c"])?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
  uu.mkdir(s, "subdir2")?
  for name in ["Desktop", "Documents", "Downloads", "Music", "Pictures", "Public", "Templates", "Videos", "code"] { uu.touch(s, f"subdir2/{name}")? }
  let fit = uu.invoke(s, "ls", ["-x", "-T0", "-w79", "subdir2"])?
  uu.succeeds(fit)
  uu.stdout_is(fit, "Desktop  Documents  Downloads  Music  Pictures  Public  Templates  Videos  code\n")
  let wrap = uu.invoke(s, "ls", ["-x", "-T0", "-w78", "subdir2"])?
  uu.succeeds(wrap)
  assert wrap.stdout.utf8()?.lines().len() > 1
}

# origin: gnu ls/color-ext.log
test test_gnu_ls_color_ext_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["img1.jpg", "IMG2.JPG", "img3.JpG", "file1.z", "file2.Z"] { uu.touch(s, name)? }
  for row in [
    {colors: "*.jpg=01;35:*.Z=01;31", names: ["img1.jpg", "IMG2.JPG", "file1.z", "file2.Z"], codes: ["01;35", "01;35", "01;31", "01;31"]},
    {colors: "*.jpg=01;35:*.JPG=01;35;46", names: ["img1.jpg", "IMG2.JPG", "img3.JpG"], codes: ["01;35", "01;35;46", ""]},
    {colors: "*.jpg=01;35:*.JPG=01;35", names: ["img1.jpg", "IMG2.JPG", "img3.JpG"], codes: ["01;35", "01;35", "01;35"]},
    {colors: "*.jpg=01;35:*.jpg=01;35;46:*.JPG=01;35;46", names: ["img1.jpg", "IMG2.JPG", "img3.JpG"], codes: ["01;35;46", "01;35;46", "01;35;46"]},
    {colors: "*.jpg=01;35;46:*.jpg=01;35:*.JPG=01;35;46", names: ["img1.jpg", "IMG2.JPG", "img3.JpG"], codes: ["01;35", "01;35;46", ""]}
  ] {
    let r = uu.invoke(s, "ls", ["-U1", "--color=always"].extend(row.names), vars: {LS_COLORS: row.colors})?
    uu.succeeds(r)
    var expected = "\x1b[0m"
    for i in range(row.names.len()) {
      expected += if row.codes[i] == "" { f"{row.names[i]}\n" } else { f"\x1b[{row.codes[i]}m{row.names[i]}\x1b[0m\n" }
    }
    uu.stdout_is(r, expected)
  }
}

# origin: gnu ls/multihardlink.log
test test_gnu_ls_multihardlink_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["file", "file1"] { uu.touch(s, name)? }
  uu.hard_link(s, "file1", "file2")?
  let plain = uu.invoke(s, "ls", ["-U1", "--color=always", "file"], vars: {LS_COLORS: "mh=37;44"})?
  uu.succeeds(plain)
  uu.stdout_is(plain, "file\n")
  let linked = uu.invoke(s, "ls", ["-U1", "--color=always", "file1", "file2"], vars: {LS_COLORS: "mh=37;44"})?
  uu.succeeds(linked)
  uu.stdout_is(linked, "\x1b[0m\x1b[37;44mfile1\x1b[0m\n\x1b[37;44mfile2\x1b[0m\n")
  uu.rename(s, "file2", "file2.png")?
  let suffix = uu.invoke(s, "ls", ["-U1", "--color=always", "file1", "file2.png"], vars: {LS_COLORS: "mh=37;44:*.png=01;35"})?
  uu.succeeds(suffix)
  uu.stdout_is(suffix, "\x1b[0m\x1b[37;44mfile1\x1b[0m\n\x1b[37;44mfile2.png\x1b[0m\n")
  uu.set_mode(s, "file2.png", 0o755)?
  let executable = uu.invoke(s, "ls", ["-U1", "--color=always", "file1", "file2.png"], vars: {LS_COLORS: "mh=37;44:*.png=01;35:ex=01;32"})?
  uu.succeeds(executable)
  uu.stdout_is(executable, "\x1b[0m\x1b[01;32mfile1\x1b[0m\n\x1b[01;32mfile2.png\x1b[0m\n")
  uu.set_mode(s, "file2.png", 0o644)?
  for colors in ["mh=00:*.png=01;35", "*.png=01;35"] {
    let r = uu.invoke(s, "ls", ["-U1", "--color=always", "file1", "file2.png"], vars: {LS_COLORS: colors})?
    uu.succeeds(r)
    uu.stdout_is(r, "file1\n\x1b[0m\x1b[01;35mfile2.png\x1b[0m\n")
  }
}

# origin: gnu ls/color-dtype-dir.log
test test_gnu_ls_color_dtype_dir_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["d", "other-writable", "sticky"] { uu.mkdir(s, name)? }
  uu.set_mode(s, "d", 0o755)?
  uu.set_mode(s, "other-writable", 0o757)?
  uu.set_mode(s, "sticky", 0o1755)?
  uu.touch(s, "out")?
  for row in [{colors: "", other: "34;42"}, {colors: "ow=:", other: "01;34"}] {
    let r = uu.invoke(s, "ls", ["--color=always"], vars: {TERM: "xterm", LS_COLORS: row.colors}, umask: 0o022)?
    uu.succeeds(r)
    uu.stdout_is(r, f"\x1b[0m\x1b[01;34md\x1b[0m\n\x1b[{row.other}mother-writable\x1b[0m\nout\n\x1b[37;44msticky\x1b[0m\n")
  }
}

# origin: gnu ls/color-clear-to-eol.log
test test_gnu_ls_color_clear_to_eol_log { |ctx|
  let s = uu.scene(ctx)?
  let name = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz.foo"
  uu.touch(s, name)?
  let r = uu.invoke(s, "ls", ["-og", "--color=always", name], vars: {TERM: "xterm", COLUMNS: "80", LS_COLORS: "*.foo=31;42", TIME_STYLE: "+T"})?
  uu.succeeds(r)
  let suffix = r.stdout.utf8()?.split("T ")[1]
  assert suffix == f"\x1b[0m\x1b[31;42m{name}\x1b[0m\x1b[K\n"
}

# origin: gnu ls/quote-align.log
test test_gnu_ls_quote_align_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir:name")?
  for name in ["a b", "c.foo"] { uu.touch(s, f"dir:name/{name}")? }
  let colored = "\x1b[0m\x1b[31;42mc.foo\x1b[0m"
  for row in [{flags: ["-w0", "-x"], out: f"'dir:name':\n'a b'  {colored}\n"}, {flags: ["-x"], out: f"'dir:name':\n'a b'   {colored}\n"}, {flags: ["-og"], out: f"'dir:name':\n'a b'\n {colored}\n"}, {flags: ["-1"], out: f"'dir:name':\n'a b'\n{colored}\n"}, {flags: ["-m"], out: f"'dir:name':\n'a b', {colored}\n"}, {flags: ["-C"], out: f"'dir:name':\n'a b'   {colored}\n"}] {
    let r = uu.invoke(s, "ls", row.flags.extend(["-R", "--quoting=shell-escape", "--color=always", "dir:name"]), vars: {TERM: "xterm", LS_COLORS: "*.foo=31;42", TIME_STYLE: "+T"})?
    uu.succeeds(r)
    var lines: List[Str] = []
    for line in r.stdout.utf8()?.lines() {
      if line.starts_with("total") { continue }
      let parts = line.split("T ")
      lines = lines.extend([parts[parts.len() - 1]])
    }
    assert lines.join("\n") + "\n" == row.out
  }
}

proc types_scene(s: uu.Scene, name: Str) [fs, process, env, error] -> Result[Str, Error] {
  uu.mkdir(s, name)?
  uu.mkdir(s, f"{name}/dir")?
  for file in ["regular", "executable"] { uu.touch(s, f"{name}/{file}")? }
  uu.set_mode(s, f"{name}/executable", 0o755)?
  for row in [{target: "regular", name: "slink-reg"}, {target: "dir", name: "slink-dir"}, {target: "nowhere", name: "slink-dangle"}] { uu.symlink(s, row.target, f"{name}/{row.name}")? }
  var prefix = ""
  for row in [{file: "block", kind: "b", major: "20", minor: "20"}, {file: "char", kind: "c", major: "10", minor: "10"}] {
    let r = uu.invoke(s, "mknod", [f"{name}/{row.file}", row.kind, row.major, row.minor])?
    if r.status == 0 { prefix += f"{row.file}\n" }
  }
  uu.mkfifo(s, f"{name}/fifo")?
  Ok(prefix + "dir/\nexecutable*\nfifo|\nregular\nslink-dangle@\nslink-dir@\nslink-reg@\n")
}

# origin: gnu ls/classify.log
test test_gnu_ls_classify_log { |ctx|
  let s = uu.scene(ctx)?
  let classified = types_scene(s, "testdir")?
  let plain = classified.replace("/", with: "").replace("*", with: "").replace("|", with: "").replace("@", with: "")
  for row in [{flag: "--classify", out: classified}, {flag: "--classify=always", out: classified}, {flag: "--classify=auto", out: plain}, {flag: "--classify=never", out: plain}] {
    let r = uu.invoke(s, "ls", [row.flag, "testdir"])?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
  uu.fails_with_code(uu.invoke(s, "ls", ["--classify=invalid"])?, 1)
}

# origin: gnu ls/file-type.log
test test_gnu_ls_file_type_log { |ctx|
  let s = uu.scene(ctx)?
  let classified = types_scene(s, "sub")?
  let typed = classified.replace("*", with: "")
  let slash = typed.replace("@", with: "").replace("|", with: "")
  for row in [{flags: ["-F"], out: classified}, {flags: ["--indicator-style=file-type"], out: typed}, {flags: ["-p"], out: slash}, {flags: ["--color=auto", "-F"], out: classified}] {
    let r = uu.invoke(s, "ls", row.flags.extend(["sub"]))?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
}

# origin: gnu ls/m-option.log
test test_gnu_ls_m_option_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.write(s, "b", [f"{i}\n" for i in range(1, 2001)].join(""))?
  let narrow = uu.invoke(s, "ls", ["-w2", "-m", "a", "b"])?
  uu.succeeds(narrow)
  uu.stdout_is(narrow, "a,\nb\n")
  let sized = uu.invoke(s, "ls", ["-sm", "a", "b"])?
  uu.succeeds(sized)
  let leading = rx"^[0-9]".replace(sized.stdout.utf8()?, with: "0")
  let normalized = rx", [0-9][0-9]* b".replace(leading, with: ", 12 b")
  assert normalized == "0 a, 12 b\n"
  for name in ["bb", "c"] { uu.touch(s, name)? }
  for row in [{names: ["a", "bb", "c"], out: "a,\nbb, c\n"}, {names: ["a", "bb"], out: "a, bb\n"}] {
    let r = uu.invoke(s, "ls", ["-w5", "-m"].extend(row.names))?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
  uu.touch(s, "com,ma")?
  for row in [{style: "literal", out: "com,ma\n"}, {style: "shell", out: "'com,ma'\n"}, {style: "shell-always", out: "'com,ma'\n"}, {style: "shell-escape", out: "'com,ma'\n"}, {style: "shell-escape-always", out: "'com,ma'\n"}, {style: "c", out: "\"com,ma\"\n"}, {style: "c-maybe", out: "\"com,ma\"\n"}, {style: "escape", out: "com\\,ma\n"}, {style: "locale", out: "'com,ma'\n"}, {style: "clocale", out: "\"com,ma\"\n"}] {
    let r = uu.invoke(s, "ls", ["-m", f"--quoting-style={row.style}", "com,ma"])?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
  uu.touch(s, "n\nl")?
  for row in [{flags: ["-m", "-w0"], out: "n\nl\n"}, {flags: ["-m"], out: "n?l\n"}] {
    let r = uu.invoke(s, "ls", row.flags.extend(["n\nl"]))?
    uu.succeeds(r)
    uu.stdout_is(r, row.out)
  }
}

# origin: gnu ls/dired.log
test test_gnu_ls_dired_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  for flag in [["-l"], [], ["--hyperlink"], ["-x"]] {
    let r = uu.invoke(s, "ls", flag.extend(["-R", "--dired", "dir"]), vars: {LC_MESSAGES: "C"})?
    uu.succeeds(r)
    uu.stdout_is(r, "  dir:\n  total 0\n//SUBDIRED// 2 5\n//DIRED-OPTIONS// --quoting-style=literal\n")
  }
  for name in ["1a", "2æ", "aaa"] { uu.touch(s, f"dir/{name}")? }
  uu.mkdir(s, "dir/3dir")?
  uu.symlink(s, "target", "dir/0aaa_link")?
  let r = uu.invoke(s, "ls", ["-l", "--dired", "dir"])?
  uu.succeeds(r)
  let lines = r.stdout.utf8()?.lines()
  let offsets = [line for line in lines if line.starts_with("//DIRED//")][0].words()
  let names = ["0aaa_link", "1a", "2æ", "3dir", "aaa"]
  assert offsets.len() == 11
  for i in range(names.len()) {
    let start = offsets[1 + i * 2].parse_int()?
    let end = offsets[2 + i * 2].parse_int()?
    assert r.stdout.slice(start, end - start).utf8()?.split(" -> ")[0] == names[i]
  }
  uu.mkdir(s, "newline-dir")?
  uu.touch(s, "newline-dir/n\nl")?
  let newline = uu.invoke(s, "ls", ["-l", "--dired", "--quoting-style=literal", "newline-dir"])?
  uu.succeeds(newline)
  let pair = [line for line in newline.stdout.utf8()?.lines() if line.starts_with("//DIRED//")][0].words()
  assert pair.len() == 3
  let start = pair[1].parse_int()?
  let end = pair[2].parse_int()?
  assert newline.stdout.slice(start, end - start) == b"n\nl"
}

# origin: gnu ls/abmon-align.log
test test_gnu_ls_abmon_align_log { |ctx|
  let s = uu.scene(ctx)?
  let current = uu.invoke(s, "date", ["+%Y-%m-15"])?
  uu.succeeds(current)
  let mid = current.stdout.utf8()?.trim()
  var names: List[Str] = []
  for month in range(1, 13) {
    let padded = if month < 10 { f"0{month}" } else { f"{month}" }
    names += [f"{padded}.ts"]
    uu.succeeds(uu.invoke(s, "touch", ["-d", f"{mid} +{padded} month", f"{padded}.ts"])?)
  }
  for format in ["%b", "[%b", "%b]", "[%b]"] {
    for locale in ["C", "gv_GB", "ga_IE", "fi_FI.utf8", "zh_CN", "ar_SY", "fr_FR.UTF-8"] {
      let r = uu.invoke(s, "ls", ["-lgG"].extend(names), vars: {LC_ALL: locale, TIME_STYLE: f"+{format}"})?
      let months = [bytes.from_text(line).slice(15, line.byte_len() - 15 - 6).utf8()?.replace(" ", with: ".") for line in r.stdout.utf8()?.lines()]
      assert months.len() == 12
      var widths: List[Str] = []
      for month in months {
        let width = uu.invoke(s, "wc", ["-L"], stdin: bytes.from_text(month + "\n"), vars: {LC_ALL: locale})?
        widths += [width.stdout.utf8()?.trim()]
      }
      assert widths.to_set().len() == 1
      assert months.to_set().len() == 12
    }
  }
}

# origin: gnu ls/block-size.log
test test_gnu_ls_block_size_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "sub")?
  let sub = {ctx: s.ctx, root: uu.at(s, "sub")}
  let names = ["file1024", "file262144", "file4096"]
  let sizes = [1024, 262144, 4096]
  for i in range(names.len()) {
    uu.write_bytes(sub, names[i], bytes.concat([b"foo\n", bytes.zero(sizes[i] - 4)?]))?
    uu.succeeds(uu.invoke(sub, "touch", ["-d", "2001-01-01 00:00", names[i]])?)
  }
  for row in [{flags: ["-og"], vars: {}}, {flags: ["-og"], vars: {POSIXLY_CORRECT: "1"}}, {flags: ["-k", "-og"], vars: {POSIXLY_CORRECT: "1"}}] {
    let r = uu.invoke(sub, "ls", row.flags.extend(names), vars: row.vars)?
    uu.succeeds(r)
    let normalized = [size_tail(line) for line in r.stdout.utf8()?.lines()].join("\n") + "\n"
    assert normalized == "1024 Jan  1  2001 file1024\n262144 Jan  1  2001 file262144\n4096 Jan  1  2001 file4096\n"
  }
  for variable in ["BLOCKSIZE", "BLOCK_SIZE", "LS_BLOCK_SIZE"] {
    for block in ["1", "512", "1K", "1KiB"] {
      let vars: Record = if variable == "BLOCKSIZE" { {BLOCKSIZE: block} } else if variable == "BLOCK_SIZE" { {BLOCK_SIZE: block} } else { {LS_BLOCK_SIZE: block} }
      for mode in range(3) {
        let divisor = if block == "1" or (variable == "BLOCKSIZE" and mode != 2) { 1 } else if block == "512" { 512 } else { 1024 }
        let flags = if mode == 0 { ["-og"] } else if mode == 1 { ["-og", "-k"] } else { ["-og", "-k", f"--block-size={block}"] }
        let r = uu.invoke(sub, "ls", flags.extend(names), vars: vars)?
        uu.succeeds(r)
        let normalized = [size_tail(line) for line in r.stdout.utf8()?.lines()]
        for i in range(names.len()) { assert normalized[i] == f"{sizes[i] / divisor} Jan  1  2001 {names[i]}" }
      }
    }
  }
  for size in [1, 10] { uu.write_bytes(sub, f"file{size}M", bytes.concat([b"foo\n", bytes.zero(size * 1048576 - 4)?]))? }
  for locale in ["sv_SE.UTF-8", "fr_FR.UTF-8"] {
    let probe = uu.invoke(sub, "ls", ["-s1", "--block-size='k", "file1M"], vars: {LC_ALL: locale})?
    let prefix = probe.stdout.utf8()?.split("K")[0]
    let width = uu.invoke(sub, "wc", ["-L"], stdin: bytes.from_text(prefix), vars: {LC_ALL: locale})?
    if width.stdout.utf8()?.trim() != "5" { continue }
    let all = uu.invoke(sub, "ls", ["-s1", "--block-size='k"], vars: {LC_ALL: locale})?
    var widths: List[Str] = []
    for line in [all.stdout.utf8()?.lines()[i] for i in range(1, all.stdout.utf8()?.lines().len())] {
      let measured = uu.invoke(sub, "wc", ["-L"], stdin: bytes.from_text(line.split("K")[0] + "\n"), vars: {LC_ALL: locale})?
      widths += [measured.stdout.utf8()?.trim()]
    }
    assert widths.to_set().len() == 1
  }
}

pure size_tail(line: Str) -> Str {
  var offset = 0
  for _ in range(2) {
    while offset < line.byte_len() and line.byte_at(offset) != 32 { offset += 1 }
    while offset < line.byte_len() and line.byte_at(offset) == 32 { offset += 1 }
  }
  line.byte_slice(offset)
}

# origin: gnu ls/ls-time.log
test test_gnu_ls_ls_time_log { |ctx|
  let s = uu.scene(ctx)?
  for row in [{name: "a", hour: "23"}, {name: "B", hour: "22"}, {name: "c", hour: "21"}] {
    uu.succeeds(uu.invoke(s, "touch", ["-m", "-d", f"1998-01-15 {row.hour}:00", row.name])?)
  }
  for flags in [[], ["--sort=name"], ["-U", "--sort=name"], ["-t", "--sort=name"]] {
    let r = uu.invoke(s, "ls", flags.extend(["a", "B", "c"]))?
    assert r.stdout.utf8()?.words().join(" ") == "B a c"
  }
  for row in [{name: "c", hour: "13"}, {name: "B", hour: "12"}] {
    uu.succeeds(uu.invoke(s, "touch", ["-a", "-d", f"1998-01-14 {row.hour}:00", row.name])?)
  }
  time.sleep(2s)
  uu.succeeds(uu.invoke(s, "touch", ["-a", "-d", "1998-01-14 11:00", "a"])?)
  uu.hard_link(s, "a", "a-ctime")?
  uu.remove(s, "a-ctime")?
  for flags in [["-t", "-c"], ["-c"]] { assert uu.invoke(s, "ls", flags.extend(["a", "c"]))?.stdout.utf8()?.words().join(" ") == "a c" }
  time.sleep(2s)
  uu.hard_link(s, "c", "d")?
  let mtime = uu.invoke(s, "ls", ["--full", "-l", "--time=mtime", "a"])?
  assert mtime.stdout.utf8()?.words().join(" ").ends_with("1998-01-15 23:00:00.000000000 +0000 a")
  let atime = uu.invoke(s, "ls", ["--full", "-lu", "a"])?
  assert atime.stdout.utf8()?.words().join(" ").ends_with("1998-01-14 11:00:00.000000000 +0000 a")
  for flags in [["-ut"], ["-u"]] { assert uu.invoke(s, "ls", flags.extend(["a", "B", "c"]))?.stdout.utf8()?.words().join(" ") == "c B a" }
  for flags in [["-t"], ["--time=mtime"]] { assert uu.invoke(s, "ls", flags.extend(["a", "B", "c"]))?.stdout.utf8()?.words().join(" ") == "a B c" }
  assert uu.invoke(s, "ls", ["-ct", "a", "c"])?.stdout.utf8()?.words().join(" ") == "c a"
  let english = uu.invoke(s, "ls", ["-l", "c"], vars: {LC_ALL: "en_US"})?
  let iso = uu.invoke(s, "ls", ["-l", "--time-style=long-iso", "c"])?
  assert english.stdout != iso.stdout
  uu.succeeds(uu.invoke(s, "touch", ["-m", "recent"])?)
  let year = uu.invoke(s, "date", ["-r", "recent", "+%Y"])?
  uu.succeeds(year)
  let styled = uu.invoke(s, "ls", ["-l", "--time-style=+old-%Y\nnew-%Y", "a", "recent"])?
  uu.succeeds(styled)
  assert styled.stdout.utf8()?.lines()[0].ends_with("old-1998 a")
  assert styled.stdout.utf8()?.lines()[1].ends_with(f"new-{year.stdout.utf8()?.trim()} recent")
}

# origin: gnu ls/recursive.log
test test_gnu_ls_recursive_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["x", "y", "a/1", "a/2", "a/3", "b", "c"] { uu.mkdir(s, name)? }
  for name in ["f", "a/1/I", "a/1/II"] { uu.touch(s, name)? }
  let ordered = uu.invoke(s, "ls", ["-R1", "a", "b", "c"])?
  uu.succeeds(ordered)
  uu.stdout_is(ordered, "a:\n1\n2\n3\n\na/1:\nI\nII\n\na/2:\n\na/3:\n\nb:\n\nc:\n")
  let mixed = uu.invoke(s, "ls", ["-R1", "x", "y", "f"])?
  uu.succeeds(mixed)
  uu.stdout_is(mixed, "f\n\nx:\n\ny:\n")
  uu.mkdir(s, [f"{i}" for i in range(1, 31)].join("/"))?
  let argv = uu.argv(s, "ls", [p"-R", p"1"])?
  let shell = process.which("sh")?
  let plan = process.command_argv(s.ctx.xsh_bin, argv[..3].extend([shell, p"-c", p"ulimit -n 20; exec \"$@\"", p"ls-depth"]).extend(argv[3..]), s.root, {}, b"", uu.at(s, "deep-out"), uu.at(s, "deep-err"), timeout: 10s)
  assert process.run(plan)?.exit_code()? == 0
  assert uu.read(s, "deep-out")?.count_lines() == 88
  assert uu.read(s, "deep-err")? == b""
}

# origin: gnu ls/removed-directory.log
test test_gnu_ls_removed_directory_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  let argv = uu.argv(s, "ls", [])?
  let shell = process.which("sh")?
  let plan = process.command_argv(shell, [shell, p"-c", p"rmdir ../d || exit 77; exec \"$@\"", p"removed-cwd"].extend(argv), uu.at(s, "d"), {}, b"", uu.at(s, "out"), uu.at(s, "err"), timeout: 10s)
  let status = process.run(plan)?.exit_code()?
  if status == 77 { test.skip("host cannot remove current directory") }
  assert status == 0
  assert uu.read(s, "out")? == b""
  assert uu.read(s, "err")? == b""
}

# origin: gnu ls/stat-vs-dirent.log
test test_gnu_ls_stat_vs_dirent_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "tmp")?
  let root = uu.invoke(s, "stat", ["--format=%d-%i", "/"])?
  var entry_path = s.root
  while true {
    let listing = uu.invoke(s, "ls", ["-i1", entry_path.display()])?
    if listing.status == 0 and listing.stdout.len() > 0 {
      let first = listing.stdout.utf8()?.lines()[0].trim()
      let fields = first.split(" ", maxsplit: 1)
      assert fields.len() == 2
      let actual = uu.invoke(s, "stat", ["--format=%i", f"{entry_path}/{fields[1]}"])?
      if actual.status == 0 { assert fields[0] == actual.stdout.utf8()?.trim() }
    }
    entry_path = entry_path.parent()
    let current = uu.invoke(s, "stat", ["--format=%d-%i", entry_path.display()])?
    if current.stdout == root.stdout { break }
  }
}

# origin: gnu ls/readdir-mountpoint-inode.log
test test_gnu_ls_readdir_mountpoint_inode_log { |ctx|
  let s = uu.scene(ctx)?
  let mounts = uu.invoke(s, "df", ["--local", "--out=target"])?
  uu.succeeds(mounts)
  let points = [line for line in mounts.stdout.utf8()?.lines() if line.starts_with("/") and line != "/"]
  if points.is_empty() { test.skip("no nonroot local mount point") }
  for index in range(if points.len() < 64 { points.len() } else { 64 }) {
    let entry_path = Path(points[index])
    let name = entry_path.basename()
    if name.starts_with(".") or "*" in name or "?" in name { continue }
    var options = ["-i", "-I", ".?*", "-I", f"{name}?*"]
    let chars = [name.byte_slice(i, 1) for i in range(name.byte_len())]
    for length in range(1, chars.len() + 1) {
      let prefix = chars[..length - 1].join("")
      options += ["-I", f"{prefix}[^{chars[length - 1]}]*"]
      if length > 1 { options += ["-I", ["?" for _ in range(length - 1)].join("")] }
    }
    let listing = uu.invoke(s, "ls", options.extend([entry_path.parent().display()]))?
    if listing.status != 0 { continue }
    let inode = mount_inode(s, entry_path.display())?
    if inode == b"0\n" or inode == b"" { continue }
    assert listing.stdout.utf8()?.words()[0] == inode.utf8()?.trim()
  }
}

# The one-second deadline starts after isolation setup. An expired stat may
# still have written an inode, so its captured output is preserved.
proc mount_inode(s: uu.Scene, name: Str) [fs, process, env, error] -> Result[Bytes, Error] {
  let output = uu.at(s, "mount-inode")
  let errors = uu.at(s, "mount-inode-errors")
  let timeout = process.which("timeout")?
  let argv = uu.argv(s, "stat", [p"--format=%i", Path(name)])?
  let words = argv[..3].extend([timeout, p"1"]).extend(argv[3..])
  let plan = process.command_argv(s.ctx.xsh_bin, words, s.root, {}, b"", output, errors)
  let status = process.run(plan)?.shell_code()?
  assert status in [0, 1, 124], f"mount stat wrapper status {status}: {errors.read_text()?}"
  Ok(output.read_bytes()?)
}

proc traced_stats(s: uu.Scene, args: List[Str], colors: Str, name: Str) [fs, process, env, error] -> Result[Int, Error] {
  let tracer = process.which("strace")?
  let argv = uu.argv(s, "ls", [Path(arg) for arg in args], vars: {LS_COLORS: colors})?
  let plan = process.command_argv(tracer, [tracer, p"-f", p"-q", p"-o", uu.at(s, f"{name}-trace"), p"-e", p"trace=stat,lstat,newfstatat,statx"].extend(argv), s.root, {LS_COLORS: colors}, b"", uu.at(s, f"{name}-out"), uu.at(s, f"{name}-err"), timeout: 10s)
  assert process.run(plan)?.exit_code()? == 0
  let lines = uu.read_text(s, f"{name}-trace")?.lines()
  Ok([line for line in lines if "+++" not in line and "ENOSYS" not in line and "NOTSUP" not in line].len())
}

# origin: gnu ls/stat-free-color.log
test test_gnu_ls_stat_free_color_log { |ctx|
  let s = uu.scene(ctx)?
  let colors = "rs=0:di=01;34:ln=01;36:pi=40;33:so=01;35:do=01;35:bd=40;33;01:cd=40;33;01:or=00:su=00:sg=00:ca=00:tw=00:ow=00:st=00:ex=00:mh=00:"
  uu.mkdir(s, "d")?
  let baseline = traced_stats(s, ["-a", "--color=always", "d"], colors, "empty")?
  if baseline == 0 { test.skip("stat syscalls unavailable") }
  uu.mkdir(s, "d/subdir")?
  uu.touch(s, "d/regf")?
  uu.hard_link(s, "d/regf", "d/hlink")?
  uu.symlink(s, "regf", "d/slink")?
  uu.symlink(s, "nowhere", "d/dangle")?
  let populated = traced_stats(s, ["--color=always", "d"], colors, "populated")?
  assert baseline >= populated, f"empty listing {baseline} stat calls, populated listing {populated}"
}

# origin: gnu ls/ls-misc.log
test test_gnu_ls_ls_misc_log { |ctx|
  let catalog = uu.scene(ctx)?
  let generated = uu.invoke(catalog, "dircolors", ["-b"], vars: {TERM: "xterm"})?
  uu.succeeds(generated)
  let database = generated.stdout.utf8()?.split("'")[1]
  for defaults in ["", database] {
    let s = uu.scene(ctx)?
    let base = {LS_COLORS: defaults, TERM: "xterm"}
    uu.touch(s, "q\x07")?
    for row in [
      {flags: [], out: "q\x07\n"}, {flags: ["-N"], out: "q\x07\n"}, {flags: ["-q"], out: "q?\n"}, {flags: ["-Q"], out: "\"q\\a\"\n"},
      {flags: ["--quoting=literal"], out: "q\x07\n"}, {flags: ["--quoting=shell"], out: "q\x07\n"}, {flags: ["--quoting=shell-always"], out: "'q\x07'\n"}, {flags: ["--quoting=shell-escape"], out: "'q'$'\\a'\n"}, {flags: ["--quoting=c"], out: "\"q\\a\"\n"}, {flags: ["--quoting=escape"], out: "q\\a\n"}, {flags: ["--quoting=locale"], out: "'q\\a'\n"}, {flags: ["--quoting=clocale"], out: "\"q\\a\"\n"},
      {flags: ["--quoting=literal", "-q"], out: "q?\n"}, {flags: ["--quoting=shell", "-q"], out: "q?\n"}, {flags: ["--quoting=shell-al", "-q"], out: "'q?'\n"}, {flags: ["--quoting=shell-escape", "-q"], out: "'q'$'\\a'\n"}, {flags: ["--quoting=c", "-q"], out: "\"q\\a\"\n"}, {flags: ["--quoting=escape", "-q"], out: "q\\a\n"}, {flags: ["--quoting=locale", "-q"], out: "'q\\a'\n"}, {flags: ["--quoting=clocale", "-q"], out: "\"q\\a\"\n"}
    ] {
      let r = uu.invoke(s, "ls", row.flags.extend(["q\x07"]), vars: base)?
      uu.succeeds(r)
      uu.stdout_only(r, row.out)
    }
    uu.remove(s, "q\x07")?
    uu.touch(s, "t\x04")?
    let control = uu.invoke(s, "ls", ["--quoting=c", "t\x04"], vars: base)?
    uu.succeeds(control)
    uu.stdout_only(control, "\"t\\004\"\n")
    uu.remove(s, "t\x04")?
    uu.mkdir(s, "d")?
    for row in [{args: ["d"], out: ""}, {args: ["d", "d"], out: "d:\n\nd:\n"}, {args: ["-R", "d"], out: "d:\n"}, {args: ["--ignore=[a-ce-zA-Z]*", "-R", "."], out: ".:\nd\n\n./d:\n"}, {args: ["--color=always", "d"], out: ""}] {
      let r = uu.invoke(s, "ls", row.args, vars: base)?
      uu.succeeds(r)
      uu.stdout_only(r, row.out)
    }
    uu.touch(s, "d/f")?
    let regular = uu.invoke(s, "ls", ["--color=always", "d"], vars: base)?
    uu.succeeds(regular)
    uu.stdout_only(regular, "f\n")
    let absent = uu.invoke(s, "ls", ["-U1", "d", "no-such"], vars: base)?
    uu.fails_with_code(absent, 2)
    uu.stdout_is(absent, "d:\nf\n")
    uu.stderr_is(absent, "ls: cannot access 'no-such': No such file or directory\n")
    uu.remove(s, "d/f")?
    let mixed = uu.invoke(s, "ls", ["no-dir", "d"], vars: base)?
    uu.fails_with_code(mixed, 2)
    uu.stdout_is(mixed, "d:\n")
    uu.stderr_is(mixed, "ls: cannot access 'no-dir': No such file or directory\n")
    uu.mkdir(s, "d/e")?
    let recurse = uu.invoke(s, "ls", ["-R", "d"], vars: base)?
    uu.succeeds(recurse)
    uu.stdout_only(recurse, "d:\ne\n\nd/e:\n")
    uu.remove(s, "d")?
    uu.symlink(s, "/", "d")?
    for row in [{flags: ["-F"], out: "d@\n"}, {flags: ["-dF"], out: "d@\n"}, {flags: ["-dFH"], out: "d/\n"}, {flags: ["-dFL"], out: "d/\n"}, {flags: ["-F", "--color=always"], out: "\x1b[0m\x1b[01;36md\x1b[0m@\n"}, {flags: ["-dF", "--color=always"], out: "\x1b[0m\x1b[01;36md\x1b[0m@\n"}] {
      let r = uu.invoke(s, "ls", row.flags.extend(["d"]), vars: {TERM: "xterm", LS_COLORS: "ln=01;36:di=01;34:or=40;31;01"})?
      uu.succeeds(r)
      uu.stdout_only(r, row.out)
    }
    uu.remove(s, "d")?
    uu.mkdir(s, "d")?
    uu.symlink(s, ".", "d/X")?
    let target = uu.invoke(s, "ls", ["--color=always", "d"], vars: {TERM: "xterm", LS_COLORS: "ln=target"})?
    uu.succeeds(target)
    uu.stdout_only(target, "\x1b[0m\x1b[01;34mX\x1b[0m\n")
    uu.remove(s, "d/X")?
    uu.symlink(s, "non-existent", "d/X")?
    let orphan = uu.invoke(s, "ls", ["--color=always", "d"], vars: {TERM: "xterm", LS_COLORS: "or=40;31;01"})?
    uu.succeeds(orphan)
    uu.stdout_only(orphan, "\x1b[0m\x1b[40;31;01mX\x1b[0m\n")
    uu.remove(s, "d")?
    uu.symlink(s, "nowhere", "l")?
    for row in [
      {colors: "ln=target", out: "l -> nowhere\n"},
      {colors: "ln=target:or=40:mi=34:", out: "\x1b[0m\x1b[40ml\x1b[0m -> \x1b[34mnowhere\x1b[0m\n"},
      {colors: "ln=34:mi=35:or=36:", out: "\x1b[0m\x1b[36ml\x1b[0m -> \x1b[35mnowhere\x1b[0m\n"},
      {colors: "ln=34:mi=35:", out: "\x1b[0m\x1b[34ml\x1b[0m -> \x1b[35mnowhere\x1b[0m\n"}
    ] {
      let r = uu.invoke(s, "ls", ["-o", "--time-style=+:TIME:", "--color=always", "l"], vars: {TERM: "xterm", LS_COLORS: row.colors})?
      uu.succeeds(r)
      assert r.stdout.utf8()?.split(":TIME: ")[1] == row.out
      uu.no_stderr(r)
    }
    uu.remove(s, "l")?
    uu.mkdir(s, "d")?
    uu.symlink(s, "dangle", "d/s")?
    let followed = uu.invoke(s, "ls", ["-L", "--color=always", "d"], vars: {TERM: "xterm", LS_COLORS: "ln=target"})?
    uu.fails_with_code(followed, 1)
    uu.stdout_is(followed, "s\n")
    uu.stderr_is(followed, "ls: cannot access 'd/s': No such file or directory\n")
    for row in [{colors: "ln=target:or=:ex=:", out: "\x1b[0m\x1b[ms\x1b[0m\n"}, {colors: "ln=1;36:or=:", out: "\x1b[0m\x1b[1;36ms\x1b[0m\n"}] {
      let r = uu.invoke(s, "ls", ["--color=always", "d"], vars: {TERM: "xterm", LS_COLORS: row.colors})?
      uu.succeeds(r)
      uu.stdout_only(r, row.out)
    }
    uu.remove(s, "d")?
    uu.symlink(s, "dangle", "s")?
    let explicit = uu.invoke(s, "ls", ["--color=always", "s"], vars: {TERM: "xterm", LS_COLORS: "ln=1;36:or=:"})?
    uu.succeeds(explicit)
    uu.stdout_only(explicit, "\x1b[0m\x1b[1;36ms\x1b[0m\n")
    uu.remove(s, "s")?
    uu.mkdir(s, "j")?
    uu.set_mode(s, "j", 0o700)?
    uu.touch(s, "j/d")?
    uu.set_mode(s, "j/d", 0o555)?
    let executable = uu.invoke(s, "ls", ["--color=always", "j"], vars: {TERM: "xterm", LS_COLORS: "ex=01;32"})?
    uu.succeeds(executable)
    uu.stdout_only(executable, "\x1b[0m\x1b[01;32md\x1b[0m\n")
    uu.remove(s, "j")?
    for row in [{name: "setuid", mode: 0o4644}, {name: "setgid", mode: 0o2644}] { uu.touch(s, row.name)?; uu.set_mode(s, row.name, row.mode)? }
    for row in [{name: "sticky", mode: 0o1755}, {name: "owt", mode: 0o1757}, {name: "owr", mode: 0o757}] { uu.mkdir(s, row.name)?; uu.set_mode(s, row.name, row.mode)? }
    let permissions = uu.invoke(s, "ls", ["-1", "-d", "--color=always", "owr", "owt", "setgid", "setuid", "sticky"], vars: {TERM: "xterm", LS_COLORS: "ow=34;42:tw=30;42:sg=30;43:su=37;41:st=37;44"})?
    uu.succeeds(permissions)
    uu.stdout_only(permissions, "\x1b[0m\x1b[34;42mowr\x1b[0m\n\x1b[30;42mowt\x1b[0m\n\x1b[30;43msetgid\x1b[0m\n\x1b[37;41msetuid\x1b[0m\n\x1b[37;44msticky\x1b[0m\n")
    for name in ["setuid", "setgid", "sticky", "owt", "owr"] { uu.remove(s, name)? }
    uu.mkdir(s, "d")?
    uu.symlink(s, "/", "d/s")?
    let file_type = uu.invoke(s, "ls", ["--file-type", "d"], vars: base)?
    uu.succeeds(file_type)
    uu.stdout_only(file_type, "s@\n")
    uu.remove(s, "d")?
    let versions = [".0", ".9", ".A", ".Z", ".a", ".z", ".zz~", ".zz", ".zz.~1~", ".zz.0", "0", "9", "A", "Z", "a", "z", "zz~", "zz", "zz.~1~", "zz.0"]
    for name in versions { uu.touch(s, name)? }
    let version = uu.invoke(s, "ls", ["-v", "-A"].extend(versions), vars: base)?
    uu.succeeds(version)
    uu.stdout_only(version, versions.join("\n") + "\n")
    for value in ["-9", "zz", "0.5"] {
      let r = uu.invoke(s, "ls", [f"--tabsize={value}"], vars: base)?
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, f"ls: invalid tab size: '{value}'\n")
    }
  }
}
