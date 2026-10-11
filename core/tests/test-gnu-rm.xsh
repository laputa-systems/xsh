use support.uu

proc removed(s: uu.Scene, names: List[Str]) [fs, error] {
  for name in names { assert ! uu.exists(s, name)?, f"{name} remains" }
}

proc present(s: uu.Scene, names: List[Str]) [fs, error] {
  for name in names { assert uu.exists(s, name)?, f"{name} disappeared" }
}

proc shell_run(s: uu.Scene, args: List[Str], setup: Str, input: Bytes = b"", timeout: Duration = 120s) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let out = uu.at(s, ".wrapper-out")
  let err = uu.at(s, ".wrapper-err")
  let words = [p"/bin/sh", p"-c", Path(setup), p"rm-wrapper"].extend(uu.argv(s, "rm", [Path(arg) for arg in args])?)
  let plan = process.command_argv(p"/bin/sh", words, s.root, {LC_ALL: "C"}, input, out, err, timeout: timeout)
  let status = process.run(plan)?.shell_code()?
  Ok({util: "rm", args: args, status: status, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: gnu rm/cycle.log
test test_gnu_rm_cycle_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.touch(s, "a/b/file")?
  uu.set_mode(s, "a/b", 0o555)?
  let r = uu.invoke(s, "rm", ["-rf", "a", "a"])?
  let strip = regex.compile(":[^:\n]*\n")?
  assert strip.replace(bytes.concat([r.stdout, r.stderr]).utf8()?, with: "\n") == "rm: cannot remove 'a/b/file'\nrm: cannot remove 'a/b/file'\n"
}

# origin: gnu rm/d-1.log
test test_gnu_rm_d_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "rm", ["--verbose", "--dir", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_is(r, "removed directory 'a'\nremoved 'b'\n")
  removed(s, ["a", "b"])
}

# origin: gnu rm/d-2.log
test test_gnu_rm_d_2_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.touch(s, "d/a")?
  let r = uu.invoke(s, "rm", ["-d", "d"])?
  uu.fails(r)
  assert r.stderr.utf8()?.replace("File exists", with: "Directory not empty") == "rm: cannot remove 'd': Directory not empty\n"
}

# origin: gnu rm/d-3.log
test test_gnu_rm_d_3_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  let r = uu.invoke(s, "rm", ["-i", "-d", "--verbose", "d"], stdin: b"y\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "removed directory 'd'\n")
  uu.stderr_is(r, "rm: remove directory 'd'? ")
}

# origin: gnu rm/dangling-symlink.log
test test_gnu_rm_dangling_symlink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "no-file", "dangle")?
  uu.symlink(s, "/", "symlink")?
  let r = uu.invoke(s, "rm", ["---presume-input-tty", "dangle", "symlink"], timeout: 6300ms)?
  assert ! uu.exists(s, "dangle")?
  assert ! uu.exists(s, "symlink")?
}

# origin: gnu rm/dash-hint.log
test test_gnu_rm_dash_hint_log { |ctx|
  let s = uu.scene(ctx)?
  let absent = uu.invoke(s, "rm", ["-foo"])?
  uu.fails_with_code(absent, 1)
  uu.no_stdout(absent)
  assert ! ("to remove the file" in absent.stderr.utf8()?)
  for item in [
    {name: "-foo", hint: "Try 'rm ./-foo' to remove the file '-foo'.\nTry 'rm --help' for more information.\n"},
    {name: "-foo\nbar", hint: r"""Try 'rm ./'-foo'$'\n''bar'' to remove the file '-foo'$'\n''bar'.
Try 'rm --help' for more information.
"""},
  ] {
    uu.write(s, item.name, "a\n")?
    let r = uu.invoke(s, "rm", [item.name])?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    assert r.stderr.utf8()?.ends_with(item.hint)
  }
}

# origin: gnu rm/deep-1.log
test test_gnu_rm_deep_1_log { |ctx|
  let s = uu.scene(ctx)?
  let operand = "t" + ["/k" for _ in range(400)].join("")
  uu.mkdir(s, operand)?
  assert uu.dir_exists(s, operand)?
  uu.succeeds(uu.invoke(s, "rm", ["-r", "t"], umask: 0o022)?)
  removed(s, ["t"])
}

# origin: gnu rm/deep-2.log
test test_gnu_rm_deep_2_log { |ctx|
  let s = uu.scene(ctx)?
  let r = shell_run(s, ["---presume-input-tty", "-r", "x"], r"""mkdir x || exit; (cd x || exit; perl -e 'my $remaining=52; my $component=join q{}, map {q{x}} 1..200; while ($remaining > 0) { mkdir($component,448) or die $!; chdir($component) or die $!; --$remaining; }') || exit; exec "$@";""" , b"n\n")?
  uu.succeeds(r)
  uu.no_stdout(r)
  removed(s, ["x"])
}

# origin: gnu rm/dir-no-w.log
test test_gnu_rm_dir_no_w_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "unwritable-dir")?
  uu.set_mode(s, "unwritable-dir", 320)?
  let r = uu.invoke(s, "rm", ["---presume-input-tty", "unwritable-dir"])?
  uu.fails_with_code(r, 1)
  assert bytes.concat([r.stdout, r.stderr]).utf8()?.replace("remove directory", with: "remove") == "rm: cannot remove 'unwritable-dir': Is a directory\n"
  present(s, ["unwritable-dir"])
}

# origin: gnu rm/dir-nonrecur.log
test test_gnu_rm_dir_nonrecur_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.set_mode(s, "d", 493)?
  let r = uu.invoke(s, "rm", ["d"])?
  uu.fails_with_code(r, 1)
  assert bytes.concat([r.stdout, r.stderr]).utf8()?.replace("remove directory", with: "remove") == "rm: cannot remove 'd': Is a directory\n"
  present(s, ["d"])
}

# origin: gnu rm/rm4.log
test test_gnu_rm_rm4_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.set_mode(s, "dir", 493)?
  let r = uu.invoke(s, "rm", ["dir"])?
  uu.fails_with_code(r, 1)
  assert bytes.concat([r.stdout, r.stderr]).utf8()?.replace("remove directory", with: "remove") == "rm: cannot remove 'dir': Is a directory\n"
  present(s, ["dir"])
}

# origin: gnu rm/dot-rel.log
test test_gnu_rm_dot_rel_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a", "b"] { uu.mkdir(s, name)?; uu.touch(s, f"{name}/f")? }
  uu.succeeds(uu.invoke(s, "rm", ["-r", "a", "b"])?)
}

# origin: gnu rm/empty-inacc.log
test test_gnu_rm_empty_inacc_log { |ctx|
  let s = uu.scene(ctx)?
  for item in [{name: "inacc", mode: 0o000}, {name: "a/unreadable", mode: 0o333}] {
    uu.mkdir(s, item.name)?
    uu.set_mode(s, item.name, item.mode)?
    let name = item.name.split("/")[0]
    uu.succeeds(uu.invoke(s, "rm", ["-rf", name])?)
    removed(s, [name])
  }
  for item in [{name: "unreadable2", mode: 0o333}, {name: "inacc2", mode: 0o000}] {
    uu.mkdir(s, item.name)?
    uu.set_mode(s, item.name, item.mode)?
    uu.succeeds(uu.invoke(s, "rm", ["-d", item.name])?)
    removed(s, [item.name])
  }
  uu.mkdir(s, "inacc3")?
  uu.set_mode(s, "inacc3", 0o000)?
  for answer in ["n", "y"] {
    let r = uu.invoke(s, "rm", ["---presume-input-tty", "-di", "inacc3"], stdin: bytes.from_text(answer + "\n"))?
    uu.succeeds(r)
    uu.stderr_only(r, "rm: attempt removal of inaccessible directory 'inacc3'? ")
    assert uu.exists(s, "inacc3")? == (answer == "n")
  }
}

# origin: gnu rm/empty-name.log
test test_gnu_rm_empty_name_log { |ctx|
  let s = uu.scene(ctx)?
  for args in [[""], ["a", "", "b"]] {
    uu.touch(s, "a")?; uu.touch(s, "b")?
    let r = uu.invoke(s, "rm", args)?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, "rm: cannot remove '': No such file or directory\n")
    if args.len() == 3 { removed(s, ["a", "b"]) }
  }
}

# origin: gnu rm/ext3-perf.log
test test_gnu_rm_ext3_perf_log { |ctx|
  let s = uu.scene(ctx)?
  let stats = fs.statvfs(s.root)?
  if stats.type_magic != 61267 { test.skip("requires ext3 or ext4"); return }
  if stats.files_free <= 480000 { test.skip("requires more than 480000 free inodes"); return }
  let r = shell_run(s, ["-rf", "d"], r"""start=$(date +%s); mkdir d || exit; i=1; while [ "$i" -le 400000 ]; do : > "d/$i" || exit; i=$((i+1)); done; test -f d/1 && test -f d/400000 || exit; elapsed=$(($(date +%s)-start)); limit=60; [ "$elapsed" -le "$limit" ] || limit=$elapsed; exec timeout "${limit}s" "$@""" )?
  uu.succeeds(r)
}

# origin: gnu rm/f-1.log
test test_gnu_rm_f_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  uu.succeeds(uu.invoke(s, "rm", ["-f", "d/no-such-file"])?)
}

# origin: gnu rm/fail-eacces.log
test test_gnu_rm_fail_eacces_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?; uu.touch(s, "d/f")?; uu.symlink(s, "f", "d/slink")?; uu.set_mode(s, "d", 0o555)?
  uu.mkdir(s, "e")?; uu.symlink(s, "f", "e/slink")?; uu.set_mode(s, "e", 0o555)?
  for item in [{name: "d/f", failed: "d/f"}, {name: "e", failed: "e/slink"}] {
    let r = uu.invoke(s, "rm", ["-rf", item.name])?
    uu.fails(r)
    uu.stderr_is(r, f"rm: cannot remove '{item.failed}': Permission denied\n")
  }
}

# origin: gnu rm/hash.log
test test_gnu_rm_hash_log { |ctx|
  let s = uu.scene(ctx)?
  let suffix = ["y/" for _ in range(150)].join("")
  for i in range(1, 4) { for letter in "abcdefghijklmnopqrstuvwxyz".split("") { uu.mkdir(s, f"t/{i}/{letter}/{suffix}")? } }
  uu.succeeds(uu.invoke(s, "rm", ["-r", "t"], timeout: 60s)?)
}

# origin: gnu rm/i-1.log
test test_gnu_rm_i_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "t")?; uu.write(s, "t/a", "\n")?
  for answer in ["n", "y"] {
    uu.succeeds(uu.invoke(s, "rm", ["-i", "t/a"], stdin: bytes.from_text(answer + "\n"))?)
    assert uu.exists(s, "t/a")? == (answer == "n")
  }
}

# origin: gnu rm/i-never.log
test test_gnu_rm_i_never_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?; uu.set_mode(s, "f", 0o000)?
  let r = uu.invoke(s, "rm", ["--interactive=never", "f"])?
  uu.succeeds(r); uu.no_stdout(r)
}

# origin: gnu rm/i-no-r.log
test test_gnu_rm_i_no_r_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  let r = uu.invoke(s, "rm", ["-i", "dir"], stdin: b"y\n")?
  uu.fails_with_code(r, 1)
  present(s, ["dir"])
}

# origin: gnu rm/ignorable.log
test test_gnu_rm_ignorable_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "existing-non-dir")?
  uu.succeeds(uu.invoke(s, "rm", ["-f", "existing-non-dir/f"])?)
}

# origin: gnu rm/ir-1.log
test test_gnu_rm_ir_1_log { |ctx|
  let s = uu.scene(ctx)?
  for item in [{dir: "a", file: "a"}, {dir: "b", file: "bb"}, {dir: "c", file: "cc"}] {
    uu.mkdir(s, f"t/{item.dir}")?; uu.touch(s, f"t/{item.dir}/{item.file}")?
  }
  uu.succeeds(uu.invoke(s, "rm", ["--verbose", "-i", "-r", "t"], stdin: b"y\ny\ny\ny\ny\ny\ny\ny\nn\nn\nn\n")?)
  present(s, ["t"])
  let names = fs.children(uu.at(s, "t"))? |> map .name |> collect()
  assert names.len() == 1 and names[0] in ["a", "b", "c"]
}

# origin: gnu rm/isatty.log
test test_gnu_rm_isatty_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?; uu.set_mode(s, "f", 0o000)?
  let r = shell_run(s, ["---presume-input-tty", "f"], r"""mkfifo input || exit; exec 3<>input; "$@" <&3 >prompt 2>&1 & child=$!; trap 'kill "$child" 2>/dev/null; wait "$child" 2>/dev/null' EXIT; sleep 1; test -f f || exit 1; kill "$child"; wait "$child"; cat prompt; printf 'x
'; exit 0""")?
  uu.succeeds(r)
  uu.stdout_is(r, "rm: remove write-protected regular empty file 'f'? x\n")
}

# origin: gnu rm/one-file-system2.log
test test_gnu_rm_one_file_system2_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?
  uu.succeeds(uu.invoke(s, "rm", ["--one-file-system", "-rf", "a"])?)
  removed(s, ["a"])
}

# origin: gnu rm/r-1.log
test test_gnu_rm_r_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/a")?; uu.touch(s, "b")?
  let r = uu.invoke(s, "rm", ["--verbose", "-r", "a", "b"])?
  uu.succeeds(r)
  uu.stdout_is(r, "removed directory 'a/a'\nremoved directory 'a'\nremoved 'b'\n")
}

# origin: gnu rm/r-2.log
test test_gnu_rm_r_2_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "t/a/b")?; uu.touch(s, "t/a/f")?; uu.touch(s, "t/a/b/g")?
  let r = uu.invoke(s, "rm", ["--verbose", "-r", "t/a"])?
  uu.succeeds(r)
  assert (r.stdout.utf8()?.split("\n") |> sort) == (["removed directory 't/a'", "removed directory 't/a/b'", "removed 't/a/b/g'", "removed 't/a/f'", ""] |> sort)
  removed(s, ["t/a"])
}

# origin: gnu rm/r-3.log
test test_gnu_rm_r_3_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "t")?
  for i in "0123456789abcdefghij".split("") { for j in "abcdefghijklmnopqrstuvwxy".split("") { uu.touch(s, f"t/{i}{j}")? } }
  present(s, ["t/0a", "t/by"])
  uu.succeeds(uu.invoke(s, "rm", ["-rf", "t"])?)
  removed(s, ["t"])
}

# origin: gnu rm/r-4.log
test test_gnu_rm_r_4_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?; uu.touch(s, "d/a")?
  for name in ["d/.", "d/./", "d/.////", "d/..", "d/../"] {
    let r = uu.invoke(s, "rm", ["-fr", name])?
    uu.fails(r)
    let diagnostic = regex.compile("^rm: refusing to remove '\\.' or '\\.\\.' directory: skipping '.*'\n$")?
    assert diagnostic.matches(r.stderr.utf8()?)
  }
  present(s, ["d/a"])
}

# origin: gnu rm/readdir-bug.log
test test_gnu_rm_readdir_bug_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "b")?
  for i in range(1, 251) { uu.touch(s, "b/" + ["0" for _ in range(40 - f"{i}".byte_len())].join("") + f"{i}")? }
  uu.succeeds(uu.invoke(s, "rm", ["-rf", "b"])?)
  removed(s, ["b"])
}

# origin: gnu rm/rm1.log
test test_gnu_rm_rm1_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["b/a/p", "b/c", "b/d"] { uu.mkdir(s, name)? }
  uu.set_mode(s, "b/a", 0o555)?
  let r = uu.invoke(s, "rm", ["-rf", "b"])?
  uu.fails(r)
  assert r.stderr == b"rm: cannot remove directory 'b/a/p': Permission denied\n" or r.stderr == b"rm: cannot remove 'b/a/p': Permission denied\n"
  present(s, ["b/a/p"]); removed(s, ["b/c", "b/d"])
}

# origin: gnu rm/rm2.log
test test_gnu_rm_rm2_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a/0", "a/1/2", "b/3", "a/2", "a/3"] { uu.mkdir(s, name)? }
  uu.set_mode(s, "a/1", 0o655)?; uu.set_mode(s, "b", 0o655)?
  let r = uu.invoke(s, "rm", ["-rf", "a", "b"])?
  uu.fails(r)
  uu.no_stdout(r)
  assert r.stderr == b"rm: cannot remove 'a/1': Permission denied\nrm: cannot remove 'b': Permission denied\n" or r.stderr == b"rm: cannot remove 'a/1/2': Permission denied\nrm: cannot remove 'b/3': Permission denied\n"
  removed(s, ["a/0", "a/2", "a/3"]); present(s, ["a/1"])
  uu.set_mode(s, "b", 0o755)?; present(s, ["b/3"])
}

# origin: gnu rm/rm3.log
test test_gnu_rm_rm3_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "z/d")?; uu.mkdir(s, "z/du")?
  uu.touch(s, "z/empty")?; uu.touch(s, "z/empty-u")?; uu.write(s, "z/fu", "not-empty\n")?
  uu.symlink(s, "empty-f", "z/slink")?; uu.symlink(s, ".", "z/slinkdot")?
  uu.set_mode(s, "z/fu", 0o444)?; uu.set_mode(s, "z/empty-u", 0o444)?; uu.set_mode(s, "z/du", 0o555)?
  let r = uu.invoke(s, "rm", ["-ir", "z"], stdin: b"y\ny\ny\ny\ny\ny\ny\ny\ny\n")?
  uu.succeeds(r)
  let prompts = [part for part in r.stderr.utf8()?.split("? ") if part != ""] |> sort
  assert prompts == (["rm: descend into directory 'z'", "rm: remove regular empty file 'z/empty'", "rm: remove write-protected regular file 'z/fu'", "rm: remove write-protected regular empty file 'z/empty-u'", "rm: remove symbolic link 'z/slink'", "rm: remove symbolic link 'z/slinkdot'", "rm: remove directory 'z/d'", "rm: remove write-protected directory 'z/du'", "rm: remove directory 'z'"] |> sort)
  removed(s, ["z"])
}

# origin: gnu rm/rm5.log
test test_gnu_rm_rm5_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d/e")?
  let r = uu.invoke(s, "rm", ["-ir", "d"], stdin: b"y\ny\ny\n")?
  uu.succeeds(r)
  uu.stderr_only(r, "rm: descend into directory 'd'? rm: remove directory 'd/e'? rm: remove directory 'd'? ")
  removed(s, ["d"])
}

# origin: gnu rm/sunos-1.log
test test_gnu_rm_sunos_1_log { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "rm", ["-r", ""])?, 1)
}

# origin: gnu rm/unread2.log
test test_gnu_rm_unread2_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a/b")?; uu.set_mode(s, "a", 0o355)?
  let r = uu.invoke(s, "rm", ["-rf", "a"])?
  uu.fails(r)
  uu.stderr_only(r, "rm: cannot remove 'a': Permission denied\n")
}

# origin: gnu rm/unread3.log
test test_gnu_rm_unread3_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a/1", "b", "c", "d/2", "e/3"] { uu.mkdir(s, name)? }
  uu.set_mode(s, "c", 0o100)?
  for names in [["a", "b"], ["d", "e"]] {
    let r = shell_run(s, ["-r"].extend([uu.at(s, name).display() for name in names]), r"""cd c || exit; exec "$@";""" )?
    uu.succeeds(r)
    removed(s, names)
  }
}

# origin: gnu rm/unreadable.log
test test_gnu_rm_unreadable_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?; uu.set_mode(s, "dir", 0o100)?
  let empty = uu.invoke(s, "rm", ["-rf", "dir"])?
  uu.succeeds(empty); uu.no_output(empty)
  uu.mkdir(s, "dir/x")?; uu.set_mode(s, "dir", 0o100)?
  let nonempty = uu.invoke(s, "rm", ["-rf", "dir"])?
  uu.fails_with_code(nonempty, 1)
  uu.stderr_only(nonempty, "rm: cannot remove 'dir': Permission denied\n")
}

# origin: gnu rm/v-slash.log
test test_gnu_rm_v_slash_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?; uu.touch(s, "a/x")?
  let r = uu.invoke(s, "rm", ["--verbose", "-r", "a///"])?
  uu.succeeds(r)
  uu.stdout_is(r, "removed 'a/x'\nremoved directory 'a/'\n")
}

# origin: gnu rm/interactive-always.log
test test_gnu_rm_interactive_always_log { |ctx|
  let s = uu.scene(ctx)?
  for i in range(1, 5) { for j in range(1, 3) { uu.touch(s, f"file{i}-{j}")? } }
  for item in [
    {args: ["-R", "--interactive", "file1-1", "file1-2"], prompt: "rm: remove regular empty file 'file1-1'? rm: remove regular empty file 'file1-2'? ", kept: ["file1-1"], gone: ["file1-2"]},
    {args: ["-R", "--interactive=never", "file2-1", "file2-2"], prompt: "", kept: [], gone: ["file2-1", "file2-2"]},
    {args: ["-R", "--interactive=once", "file3-1", "file3-2"], prompt: "rm: remove 2 arguments recursively? ", kept: ["file3-1", "file3-2"], gone: []},
    {args: ["-R", "--interactive=always", "file4-1", "file4-2"], prompt: "rm: remove regular empty file 'file4-1'? rm: remove regular empty file 'file4-2'? ", kept: ["file4-1"], gone: ["file4-2"]},
    {args: ["-R", "--interactive=once", "-f", "file1-1"], prompt: "", kept: [], gone: ["file1-1"]},
    {args: ["-R", "-f", "--interactive=once", "file4-1"], prompt: "rm: remove 1 argument recursively? ", kept: ["file4-1"], gone: []},
  ] {
    let r = uu.invoke(s, "rm", item.args, stdin: b"n\ny\n")?
    uu.succeeds(r)
    uu.stderr_only(r, item.prompt)
    present(s, item.kept)
    removed(s, item.gone)
  }
}

# origin: gnu rm/interactive-once.log
test test_gnu_rm_interactive_once_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["dir1-1", "dir2-1", "dir2-2"] { uu.mkdir(s, name)? }
  for name in ["file1-1", "file2-1", "file2-2", "file2-3", "file3-1", "file3-2", "file3-3", "file3-4"] { uu.touch(s, name)? }
  let one = uu.invoke(s, "rm", ["-I", "file1-1"], stdin: b"n\n")?
  uu.succeeds(one); uu.no_output(one); removed(s, ["file1-1"])
  uu.touch(s, "file1-1")?; uu.set_mode(s, "file1-1", 0o444)?
  let protected = uu.invoke(s, "rm", ["---presume-input-tty", "-I", "file1-1"], stdin: b"n\n")?
  uu.succeeds(protected)
  uu.stderr_only(protected, "rm: remove write-protected regular empty file 'file1-1'? ")
  present(s, ["file1-1"])
  let three = uu.invoke(s, "rm", ["-I", "file2-1", "file2-2", "file2-3"], stdin: b"n\n")?
  uu.succeeds(three); uu.no_output(three); removed(s, ["file2-1", "file2-2", "file2-3"])
  let four = ["file3-1", "file3-2", "file3-3", "file3-4"]
  for answer in ["n", "y"] {
    let r = uu.invoke(s, "rm", ["-I"].extend(four), stdin: bytes.from_text(answer + "\n"))?
    uu.succeeds(r); uu.stderr_only(r, "rm: remove 4 arguments? ")
    if answer == "n" { present(s, four) } else { removed(s, four) }
  }
  for name in four { uu.touch(s, name)? }
  uu.write(s, "file3-4", "non_empty\n")?; uu.set_mode(s, "file3-4", 0o444)?
  let mixed = uu.invoke(s, "rm", ["---presume-input-tty", "-I"].extend(four), stdin: b"y\nn\n")?
  uu.succeeds(mixed)
  uu.stderr_only(mixed, "rm: remove 4 arguments? rm: remove write-protected regular file 'file3-4'? ")
  removed(s, ["file3-1", "file3-2", "file3-3"]); present(s, ["file3-4"])
  for names in [["dir1-1"], ["dir2-1", "dir2-2"]] {
    for answer in ["n", "y"] {
      let r = uu.invoke(s, "rm", ["-I", "-R"].extend(names), stdin: bytes.from_text(answer + "\n"))?
      uu.succeeds(r)
      let noun = if names.len() == 1 { "argument" } else { "arguments" }
      uu.stderr_only(r, f"rm: remove {names.len()} {noun} recursively? ")
      if answer == "n" { present(s, names) } else { removed(s, names) }
    }
  }
}
