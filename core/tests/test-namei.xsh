use lib.gnu as gnu
use support.uu
use support.util_linux as ul

proc parity(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference(s, "namei", args, timeout: 5s, vars:)?
  let candidate = uu.invoke(s, "namei", args, vars:, timeout: 5s)?
  ul.compare(reference, candidate)
  Ok()
}

proc parity_paths(s: uu.Scene, args: List[Path], vars: Record = {}) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference_paths(s, "namei", args, timeout: 5s, vars:)?
  let candidate = uu.invoke_paths(s, "namei", args, vars:, timeout: 5s)?
  ul.compare(reference, candidate)
  Ok()
}

test test_namei_upstream_logic_plain_relative_paths { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.mkdir(s, "namei1/namei2")
  uu.write(s, "namei1/namei2/a", "a\n")
  uu.write(s, "namei1/namei2/b", "b\n")
  parity(s, ["namei1/namei2/a"])?
  parity(s, ["namei1/namei2/b"])?
  assert uu.read_text(s, "namei1/namei2/a")? == "a\n"
  assert uu.read_text(s, "namei1/namei2/b")? == "b\n"
}

test test_namei_root_and_repeated_path_separators { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tree/inner")
  uu.write(s, "tree/inner/file", "file\n")
  parity(s, ["/"])?
  parity(s, ["//"])?
  parity(s, ["///"])?
  parity(s, ["--", fp"{s.root}//tree///inner//file".display()])?
  parity(s, ["--", fp"{s.root}/tree/inner/".display()])?
}

test test_namei_each_output_option_and_long_combination { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tree/inner")
  uu.write(s, "tree/inner/file", "file\n")
  let operand = fp"{s.root}/tree/inner/file".display()
  for options in [
    ["-x"], ["--mountpoints"], ["-m"], ["--modes"], ["-o"], ["--owners"],
    ["-l"], ["--long"], ["-n"], ["--nosymlinks"], ["-v"], ["--vertical"],
    ["-Z"], ["--context"], ["-xmo"], ["-l", "-n"], ["-m", "-o", "-v"]
  ] {
    parity(s, options.extend(["--", operand]))?
  }
  for args in [["-Z", "--", operand], ["--context", "--", operand]] {
    let reference = ul.reference(s, "namei", args, timeout: 5s)?
    let option = if args[0] == "-Z" { "Z" } else { "context" }
    assert reference.status == 1
    assert reference.stdout == b""
    assert reference.stderr == bytes.from_text(f"namei: unrecognized option: {option}\nTry 'namei --help' for more information.\n")
  }
}

test test_namei_component_walk_follows_relative_and_absolute_symlinks { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tree/inner")
  uu.write(s, "tree/inner/file", "file\n")
  uu.mkfifo(s, "tree/pipe")
  uu.symlink(s, "inner/file", "tree/relative")
  uu.symlink(s, fp"{s.root}/tree/inner/file".display(), "tree/absolute")
  parity(s, ["-l", "--", fp"{s.root}/tree/relative".display()])?
  parity(s, ["-l", "--", fp"{s.root}/tree/absolute".display()])?
  parity(s, ["--nosymlinks", "--", fp"{s.root}/tree/relative".display()])?
  parity(s, ["-l", "--", fp"{s.root}/tree/pipe".display()])?
  parity(s, ["-l", "--", "/dev/null"])?
  let quoted = fp"{s.root}/tree/space and quote'"
  quoted.write("quoted path\n")
  parity_paths(s, [p"--", quoted])?
  assert uu.read_text(s, "tree/inner/file")? == "file\n"
  assert uu.at(s, "tree/relative").readlink()? == p"inner/file"
  assert uu.at(s, "tree/absolute").readlink()? == fp"{s.root}/tree/inner/file"
  assert quoted.read_text()? == "quoted path\n"
}

test test_namei_component_walk_reports_socket_and_block_device { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let socket = uu.at(s, "socket")
  let block = uu.at(s, "block")
  fs.mknod(socket, "socket", 0o600)?
  fs.mknod(block, "block", 0o600, major: 0, minor: 0)?
  parity(s, ["--modes", "--", socket.display()])?
  parity(s, ["-m", "--", block.display()])?
  assert fs.stat(socket)?.kind == "socket"
  assert fs.stat(block)?.kind == "block"
}

test test_namei_modes_and_owner_columns_cover_special_bits { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let setuid = uu.at(s, "setuid-file")
  let sticky = uu.at(s, "sticky-directory")
  let owned = uu.at(s, "wide-owner")
  setuid.write("fixture\n")
  setuid.chmod(0o4755)
  sticky.mkdir()
  sticky.chmod(0o3777)
  owned.write("fixture\n")
  fs.chown(owned, {uid: 100000, gid: 100001, name: "", home: p"/", shell: "/"})?
  fs.chgrp(owned, {gid: 100001, members: [], name: ""})?
  owned.chmod(0o6755)
  parity(s, ["--modes", "--", setuid.display()])?
  parity(s, ["-m", "--", sticky.display()])?
  parity(s, ["-v", "-o", "--", owned.display()])?
  parity(s, ["-l", "--", owned.display()])?
  let modes = ul.reference(s, "namei", ["-m", "--", setuid.display()], timeout: 5s)?
  let mode_text = modes.stdout.utf8()?
  assert "-rwsr-xr-x" in mode_text, mode_text
  let long_modes = ul.reference(s, "namei", ["-m", "--", sticky.display()], timeout: 5s)?
  let long_mode_text = long_modes.stdout.utf8()?
  assert "drwxrwsrwt" in long_mode_text, long_mode_text
  let owners = ul.reference(s, "namei", ["-o", "--", owned.display()], timeout: 5s)?
  let owner_text = owners.stdout.utf8()?
  assert "root   root" in owner_text
  assert "100000 100001" in owner_text
}

test test_namei_committed_path_fixture_links_and_components { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let tree = fp"{ctx.core_dir}/tests/fixtures/util-linux/paths/tree"
  for name in ["regular", "inner/entry", "relative", "dangling", "loop-a"] {
    parity(s, ["-l", "--", fp"{tree}/{name}".display()])?
  }
  uu.symlink(s, "/", "root-link")?
  let root_link = uu.at(s, "root-link")
  assert root_link.readlink()? == p"/"
  parity(s, ["-l", "--", root_link.display()])?
}

test test_namei_mountpoint_markers_follow_a_real_mount_boundary { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let mount = fs.mount_for(p"/proc/1")?
  assert mount.mounted_on == p"/proc"
  parity(s, ["--mountpoints", "--", mount.mounted_on.display()])?
  parity(s, ["-x", "--", p"/proc/1".display()])?
  let reference = ul.reference(s, "namei", ["-x", "--", p"/proc/1".display()], timeout: 5s)?
  let output = reference.stdout.utf8()?
  assert "D proc\n" in output, output
}

test test_namei_private_same_device_bind_mount_matches_reference { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let fixture = fp"{ctx.core_dir}/tests/fixtures/util-linux/paths/bind"
  uu.mkdir(s, "bind/source")
  uu.mkdir(s, "bind/target")
  uu.write(s, "bind/source/entry", fp"{fixture}/source/entry".read_text()?)
  uu.write(s, "bind/target/sentinel", fp"{fixture}/target/sentinel".read_text()?)
  let source = fp"{s.root}/bind/source"
  let target = fp"{s.root}/bind/target"
  assert fs.stat(source)?.dev == fs.stat(target)?.dev
  var mounted = false
  defer {
    if mounted { run.status umount -- $target }
  }
  let bound = run.capture --text mount --bind $source $target
  assert bound.status.exited_with(0), bound.stderr
  mounted = true
  assert fp"{target}/entry".exists()?
  assert ! fp"{target}/sentinel".exists()?
  let args = ["-x", "--", target.display()]
  let reference = ul.reference(s, "namei", args, timeout: 5s)?
  let candidate = uu.invoke(s, "namei", args, timeout: 5s)?
  let output = reference.stdout.utf8()?
  assert output.ends_with("d target\n"), output
  ul.compare(reference, candidate)
  parity(s, ["-l", "--", fp"{target}/entry".display()])?
  let unmounted = run.capture --text umount -- $target
  assert unmounted.status.exited_with(0), unmounted.stderr
  mounted = false
  assert fp"{target}/sentinel".exists()?
  assert ! fp"{target}/entry".exists()?
}

test test_namei_dangling_missing_and_looping_components { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tree")
  uu.symlink(s, "missing", "tree/dangling")
  uu.symlink(s, "loop-b", "tree/loop-a")
  uu.symlink(s, "loop-a", "tree/loop-b")
  parity(s, ["tree/dangling"])?
  parity(s, ["tree/missing/child"])?
  parity(s, ["tree/loop-a"])?
  parity(s, ["-n", "tree/loop-a"])?
}

test test_namei_multiple_operands_and_usage_errors { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.write(s, "first", "first\n")
  uu.mkdir(s, "second")
  parity(s, [])?
  parity(s, ["--not-an-option"])?
  parity(s, ["first", "missing", "second"])?
  parity(s, ["--help"])?
  parity(s, ["-h"])?
}

test test_namei_path_arguments_preserve_non_utf8_components { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let directory = uu.at_bytes(s, b"directory-\xfe")?
  directory.mkdir()
  let name = Path.parse_bytes(bytes.concat([directory.bytes(), b"/leaf-\xff"]))?
  name.write("owned raw path\n")
  parity_paths(s, [p"--", name])?
  parity_paths(s, [p"-m", p"--", name])?
  for locale in ["C", "C.UTF-8"] {
    parity_paths(s, [p"--", name], vars: {LC_ALL: locale})?
  }
  assert name.read_text()? == "owned raw path\n"
}

test test_namei_permission_denied_from_unprivileged_runner { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  s.root.chmod(0o755)
  uu.mkdir(s, "inaccessible")
  uu.write(s, "inaccessible/secret", "owned path\n")
  let directory = fp"{s.root}/inaccessible"
  defer directory.chmod(0o700)
  directory.chmod(0)
  let operand = fp"{directory}/secret".display()
  for uid in [10001, 10002] {
    for args in [["-l", "--", operand], ["--nosymlinks", "--", operand]] {
      let reference = ul.reference_as_user(s, "namei", args, uid:, timeout: 5s)?
      let candidate = ul.candidate_as_user(s, "namei", args, uid:, timeout: 5s)?
      ul.compare(reference, candidate)
    }
  }
  assert fs.stat(directory)?.mode.bit_and(0o777) == 0
  assert fp"{directory}/secret".read_text()? == "owned path\n"
}

test test_namei_version_provenance { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let banner = gnu.version_text("namei")
  for args in [["--version"], ["-V"]] {
    let reference = ul.reference(s, "namei", args, timeout: 5s)?
    let candidate = uu.invoke(s, "namei", args, timeout: 5s)?
    assert reference.status == 0
    assert reference.stdout == b"namei from util-linux 2.42.4\n"
    assert reference.stderr == b""
    assert candidate.status == reference.status
    assert candidate.stdout == bytes.from_text(f"{banner}\n")
    assert candidate.stderr == b""
  }
}

test test_namei_mount_parent_stat_failure { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  s.root.chmod(0o755)
  uu.mkdir(s, "locked")
  uu.symlink(s, "locked", "link")
  let locked = uu.at(s, "locked")
  locked.chmod(0o000)
  defer locked.chmod(0o755)
  let args = ["-x", "link"]
  for uid in [10001, 10002] {
    let reference = ul.reference_as_user(s, "namei", args, uid:, timeout: 5s)?
    assert reference.status == 1 and reference.stdout == b""
    assert reference.stderr == b"namei: stat of locked/.. failed: Permission denied\n"
    ul.compare(reference, ul.candidate_as_user(s, "namei", args, uid:, timeout: 5s)?)
  }
}

test test_namei_loop_errno_text { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "loop", "loop")
  let args = ["-n", "loop/x"]
  let reference = ul.reference(s, "namei", args)?
  assert reference.status == 1 and reference.stderr == b""
  assert reference.stdout == b"f: loop/x\n l loop -> loop\n    x - Symbolic link loop\n"
  ul.compare(reference, uu.invoke(s, "namei", args, timeout: 5s)?)
}

test test_namei_owner_width_includes_components_after_link_limit { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  var operand = ""
  for n in range(1, 21) {
    let link = f"c{n}"
    uu.symlink(s, ".", link)
    operand += link + "/"
  }
  uu.symlink(s, "/", "r21")
  parity(s, ["-o", operand + "r21/etc/shadow"])?
}

test test_namei_invalid_utf8_short_option { |ctx|
  let s = uu.scene(ctx)?
  let result = uu.invoke_paths(s, "namei", [b"-q\xff" as Path, p"/"])?
  assert result.status == 1
  assert result.stdout == b""
  for index in range(result.stderr.len()) { assert result.stderr.byte_at(index) != 0 }
  assert result.stderr == b"namei: unrecognized option: q\nTry 'namei --help' for more information.\n"
}

type TerminalCapture = {status: Int, output: Bytes}

proc merged_terminal(s: uu.Scene, words: List[Path]) [fs, process, net, error] -> Result[TerminalCapture, Error] {
  let pair = unix.open_pty()?
  defer unix.close_fd(pair.master)
  defer unix.close_fd(pair.replica)
  let plan = process.command_argv(words[0], words, s.root, {}, b"", fp"{pair.name}", fp"{pair.name}", timeout: 5s)
  let status = process.run(plan)?
  var output = b""
  while "readable" in unix.poll_fd(pair.master, ["readable"], timeout_ms: 0)? {
    let chunk = unix.read_fd(pair.master, 8192)?
    break when chunk.is_empty()
    output = bytes.concat([output, chunk])
  }
  Ok({status: status.exit_code()?, output: output})
}

proc merged_pipe(s: uu.Scene, words: List[Path]) [fs, process, error] -> Result[TerminalCapture, Error] {
  let output = uu.at(s, "capture")
  let command = [p"/bin/sh", p"-c", p"exec \"$@\" 2>&1", p"namei-merge"].extend(words)
  let plan = process.command_argv(command[0], command, s.root, {}, b"", output, p"/dev/null", timeout: 5s)
  let status = process.run(plan)?
  let captured = output.read_bytes()?
  output.remove()
  Ok({status: status.exit_code()?, output: captured})
}

test test_namei_loop_terminal_interleaving { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.symlink(s, "loop2", "loop1")
  uu.symlink(s, "loop1", "loop2")
  uu.write(s, "plain", "x")
  for args in [[p"loop1"], [p"plain", p"loop1", p"plain"], [p"-l", p"loop1"], [p"-m", p"loop1"], [p"-o", p"loop1"], [p"-v", p"loop1"], [p"-x", p"loop1"], [p"-n", p"loop1"]] {
    let reference = merged_terminal(s, ul.reference_argv(s, "namei", args)?)?
    let candidate = merged_terminal(s, uu.argv(s, "namei", args)?)?
    assert reference.status == 1
    assert reference == candidate
    let pipe_reference = merged_pipe(s, ul.reference_argv(s, "namei", args)?)?
    let pipe_candidate = merged_pipe(s, uu.argv(s, "namei", args)?)?
    assert pipe_reference == pipe_candidate
  }
}

test test_namei_error_with_os_error_in_path { |ctx|
  let s = uu.scene(ctx)?
  parity(s, ["e (os error 5)/x"])?
}

test test_namei_fatal_readlink_after_link_limit_and_repeated_slashes { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  for n in range(0, 25) { uu.symlink(s, f"p{n + 1}", f"p{n}")? }
  uu.symlink(s, "/proc/1", "p25")?
  parity(s, ["p0/cwd"])?
}

test test_namei_fatal_readlink_preserves_repeated_slashes { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  parity(s, ["/proc//1/cwd"])?
}

test test_namei_owner_display_cell_width { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  if env.get_or("XSH_NAMEI_OWNER_WIDTH_ISOLATED", "")? != "1" {
    let xsht = fp"{ctx.xsh_bin.parent()}/xsht"
    let source = "export XSH_UTIL_LINUX_PARENT_mnt=$(readlink /proc/self/ns/mnt); exec unshare --mount --propagation private env XSH_NAMEI_OWNER_WIDTH_ISOLATED=1 \"$1\" test --exact-id core/tests/test-namei.xsh::test_namei_owner_display_cell_width"
    let isolated = run.capture --text sh -ec $source test $xsht
    assert isolated.status.exited_with(0), f"{isolated.stdout}\n{isolated.stderr}"
    return
  }
  let s = uu.scene(ctx)?
  let passwd = uu.at(s, "passwd")
  let groups = uu.at(s, "group")
  passwd.write("jösé:x:0:0::/:/bin/sh\nabcd:x:65534:65534::/:/bin/sh\n")
  groups.write("ab:x:0:\nxy:x:65534:\n")
  var mounted_passwd = false
  var mounted_group = false
  defer {
    if mounted_group { run.status umount /etc/group }
    if mounted_passwd { run.status umount /etc/passwd }
  }
  assert (run.capture --text mount --bind $passwd /etc/passwd).status.exited_with(0)
  mounted_passwd = true
  assert (run.capture --text mount --bind $groups /etc/group).status.exited_with(0)
  mounted_group = true
  for locale in ["C", "C.UTF-8"] {
    parity(s, ["-l", s.root.display()], vars: {LC_ALL: locale})?
  }
  passwd.write("ééééééé:x:0:0::/:/bin/sh\nnobody:x:65534:65534::/:/bin/sh\n")
  groups.write("äääääääää:x:0:\nnogroup:x:65534:\n")
  for vars in [{LC_ALL: "", LANG: ""}, {LC_ALL: "", LANG: "C"}] {
    parity(s, ["-l", s.root.display()], vars:)?
  }
  let long_name = ["a"] |> repeat(300) |> join("")
  passwd.write(f"{long_name}:x:0:0::/:/bin/sh\nnobody:x:65534:65534::/:/bin/sh\n")
  groups.write("root:x:0:\nnogroup:x:65534:\n")
  parity(s, ["-l", s.root.display()], vars: {LC_ALL: "C.UTF-8"})?
  let wide_name = (["a"] |> repeat(255) |> join("")) + "中中中"
  passwd.write(f"{wide_name}:x:0:0::/:/bin/sh\nnobody:x:65534:65534::/:/bin/sh\n")
  parity(s, ["-l", s.root.display()], vars: {LC_ALL: "C.UTF-8"})?
  passwd.write("一:x:0:0::/:/bin/sh\nabcd:x:65534:65534::/:/bin/sh\n")
  parity(s, ["-l", s.root.display()], vars: {LC_ALL: "C.UTF-8"})?
}

test test_namei_nonprintable_owner_falls_back_to_id { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  if env.get_or("XSH_NAMEI_NONPRINTABLE_OWNER_ISOLATED", "")? != "1" {
    let xsht = fp"{ctx.xsh_bin.parent()}/xsht"
    let source = "export XSH_UTIL_LINUX_PARENT_mnt=$(readlink /proc/self/ns/mnt); exec unshare --mount --propagation private env XSH_NAMEI_NONPRINTABLE_OWNER_ISOLATED=1 \"$1\" test --exact-id core/tests/test-namei.xsh::test_namei_nonprintable_owner_falls_back_to_id"
    let isolated = run.capture --text sh -ec $source test $xsht
    assert isolated.status.exited_with(0), f"{isolated.stdout}\n{isolated.stderr}"
    return
  }
  let s = uu.scene(ctx)?
  let passwd = uu.at(s, "passwd")
  let groups = uu.at(s, "group")
  passwd.write("a\x01b:x:0:0::/:/bin/sh\n")
  groups.write("c\x01d:x:0:\n")
  var mounted_passwd = false
  var mounted_group = false
  defer {
    if mounted_group { run.status umount /etc/group }
    if mounted_passwd { run.status umount /etc/passwd }
  }
  assert (run.capture --text mount --bind $passwd /etc/passwd).status.exited_with(0)
  mounted_passwd = true
  assert (run.capture --text mount --bind $groups /etc/group).status.exited_with(0)
  mounted_group = true
  parity(s, ["-l", s.root.display()], vars: {LC_ALL: "C.UTF-8"})?
  for name in ["a\u{85}b", "a\u{fffe}b", "\u{200b}", "\u{301}", "\u{1160}", "\u{feff}", "\u{e0001}", "a\u{fff9}b", "a\u{fffb}b", "a\u{1fffe}b", "a\u{3fffe}b", "a\u{effff}b", "a\u{10fffe}b"] {
    passwd.write(f"{name}:x:0:0::/:/bin/sh\n")
    parity(s, ["-l", s.root.display()], vars: {LC_ALL: "C.UTF-8"})?
  }
}

test test_namei_multibyte_short_option_c_locale { |ctx|
  let s = uu.scene(ctx)?
  parity(s, ["-é"], vars: {LC_ALL: "C"})?
  parity(s, ["-é"], vars: {LC_ALL: "C.UTF-8"})?
}
