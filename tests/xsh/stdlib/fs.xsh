proc test_missing_file_read_propagates_structured_error(ctx: TestContext) [fs, error] {
  let missing = test.temp_path(ctx, name: "missing-read")
  let output = test.run_script(
    ctx,
    f"""let _ = p"${missing.display()}".read_bytes()?
""",
  )?

  output.status == 3
  "fs-read" in output.stderr
}

proc test_fs_walk_and_files_take_any_break_and_count(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "fs-walk-stage")?
  var index = 0
  while index < 50 {
    fs.write(fp"${root}/f${index}.txt", "x")?
    index = index + 1
  }

  let first3 = fs.files(root)
    |> take(3)
    |> map .name
  first3.len() == 3
  fs.walk(root) |> any .kind == "file"

  var visited: List[Str] = []
  for entry in fs.files(root) {
    visited = visited.push(entry.name)
    break when visited.len() >= 2
  }

  visited.len() == 2
  fs.files(root) |> count() == 50
}

proc test_fs_walk_dynamic_stat_flag_preserves_metadata_boundary(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "fs-walk-dynamic-stat")?
  fs.write(fp"${root}/file.txt", "data")?
  let output = test.run_script(
    ctx,
    f"""
let root = p"${root.display()}"
let use_stat = false
let entry = (fs.walk(root, stat: use_stat) |> first())?
print \${entry.size}
""",
  )?
  output.status == 3
  "metadata-unavailable" in output.stderr
}

proc test_fs_walk_stat_true_matches_direct_record_and_snapshots_metadata(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "fs-walk-stat-record")?
  let file = fp"${root}/entry.txt"
  file.write("old")?

  let walked = (fs.files(root, gitignore: false) |> first())?
  let direct = (fs.children(root)? |> first())?
  walked == direct
  walked.keys() == direct.keys()
  walked.size == 3

  file.write("new longer content")?
  walked.size == 3
  test.eq(walked.get("size")?, 3)?
}

proc test_fs_files_dynamic_walk_flags_are_evaluated(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "fs-files-dynamic-flags")?
  fs.write(fp"${root}/normal.txt", "data")?
  fs.write(fp"${root}/ignored.txt", "ignored")?
  fs.write(fp"${root}/.hidden.txt", "hidden")?
  fs.write(
    fp"${root}/.gitignore",
    """ignored.txt
""",
  )?

  let include_hidden = true
  let exclude_hidden = false
  fs.files(root, gitignore: false, hidden: include_hidden) |> any .name == ".hidden.txt"
  ! (fs.files(root, gitignore: false, hidden: exclude_hidden) |> any .name == ".hidden.txt")

  let use_gitignore = true
  let skip_gitignore = false
  ! (fs.files(root, gitignore: use_gitignore) |> any .name == "ignored.txt")
  fs.files(root, gitignore: skip_gitignore) |> any .name == "ignored.txt"

  let use_stat = true
  let normal = (fs.files(root, stat: use_stat)
    |> where .name == "normal.txt"
    |> first())?
  normal.size == 4

  let unstat = test.run_script(
    ctx,
    f"""
let root = p"${root.display()}"
let use_stat = false
let entry = (fs.files(root, stat: use_stat) |> where .name == "normal.txt" |> first())?
print \${entry.size}
""",
  )?
  unstat.status == 3
  "metadata-unavailable" in unstat.stderr
}

proc test_fs_tree_metadata_install_and_locking(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "fs")?
  let src = fp"${root}/src"
  let nested = fp"${src}/nested"
  fs.mkdir(nested)?
  let file = fp"${nested}/data.txt"
  fs.write(file, "hello")?
  fs.write(fp"${nested}/bytes.bin", b"bytes")?
  fs.write_atomic(fp"${nested}/atomic.txt", "atomic")?
  fs.write_atomic(fp"${nested}/atomic.bin", b"atomic-bytes")?
  fs.chmod(file, 0o755)?
  (fs.read_text(file)?) == "hello"
  (fs.exists(file)?)
  (fs.executable(file)?)
  let file_meta = fs.metadata(file)?
  file_meta.name == "data.txt"
  file_meta.executable
  file_meta.owner_executable
  file_meta.group_executable
  file_meta.other_executable
  fs.executable(file_meta.mode)
  fs.owner_executable(file_meta.mode)
  fs.group_executable(file_meta.mode)
  fs.other_executable(file_meta.mode)
  ! fs.world_writable(file_meta.mode)
  fs.setuid(0o4755)
  fs.setgid(0o2755)
  fs.sticky(0o1777)
  ! fs.setuid(0o0755)
  ! fs.setgid(0o0755)
  ! fs.sticky(0o0755)
  ! file_meta.world_writable
  (fs.filesystem_stats(root)?.blocks_1k > 0)
  let mounts = fs.mounts()?.collect()
  (mounts.len() > 0)
  mounts |> any .mounted_on.display() == "/"
  let root_mount = fs.mount_for(root)?
  (root_mount.blocks_1k > 0)
  (root_mount.available_1k >= 0)
  (root_mount.capacity_percent >= 0)
  (root_mount.fstype != "")
  (fs.cwd()?.display() != "")
  let gitroot = fs.gitroot()?
  (fp"${gitroot}/docs/SPEC.md".exists()?)
  let children = fs.children(nested)? |> sort-by .name
  let listed = fs.ls(nested)? |> sort-by .name
  children.len() == listed.len()
  fs.children(nested, stat: false, ordered: false)? |> any .name == "data.txt"
  let unstat_children = test.run_script(
    ctx,
    f"""
let entry = (fs.children(fp"${nested}", stat: false, ordered: false)? |> first())?
print \$entry.size
""",
  )?
  unstat_children.status == 3
  "metadata-unavailable" in unstat_children.stderr
  fs.walk(src)? |> any .name == "data.txt"
  fs.files(src)? |> any .name == "data.txt"
  fs.dirs(src)? |> any .name == "nested"
  let cache = fp"${root}/remote-cache"
  fs.mkdir(fp"${cache}/packages")?
  let tarball = fp"${cache}/packages/pkg.tar"
  fs.write(tarball, "package")?
  fs.mkdir(fp"${root}/old-build")?
  fs.write(fp"${root}/old-file", "stale")?

  for entry in fs.children(root)? {
    if entry.name != "remote-cache" and entry.name != "src" {
      fs.remove(entry.path, missing_ok: true)?
    }
  }

  (fs.exists(cache)?)
  (cache.exists()?)
  (fs.exists(tarball)?)
  let copied = fp"${root}/copied.txt"
  fs.copy(file, copied)?
  (fs.read_text(copied)?) == "hello"
  let renamed = fp"${root}/renamed.txt"
  fs.rename(copied, renamed)?
  ! fs.exists(copied)?
  (fs.read_text(renamed)?) == "hello"
  let tree = fp"${root}/tree-copy"
  let tree_result = fs.copy_tree(src, tree)?
  (tree_result.files >= 4)
  (fp"${tree}/nested/data.txt".read_text()?) == "hello"
  let install_dest = fp"${root}/install/bin/data.txt"
  fs.install(file, install_dest, 0o600)?
  (fs.metadata(install_dest)?.mode % 512) == 0o600
  let current_user = user.current()?
  let current_group = group.current()?
  fs.install_as(file, fp"${root}/install-as/data.txt", 0o600, current_user, current_group)?
  fs.chmod(install_dest, 0o644)?
  fs.chown(install_dest, current_user)?
  fs.chgrp(install_dest, current_group)?
  (fs.metadata(install_dest)?.mode % 512) == 0o644
  let fifo = fp"${root}/fifo"
  fs.mkfifo(fifo, 0o600)?
  (fs.exists(fifo)?)
  fs.fsync(file)?
  fs.sync()?
  let link = fp"${root}/link"
  fs.symlink(file, link)?
  link.readlink()?.display() == file.display()
  let lock_file = fp"${root}/lock"
  let lock = fs.lock(lock_file, shared: true)?
  lock.path == lock_file
  lock.shared
  fs.unlock(lock)?
  let manifest_result = fs.remove_manifest(root, [p"renamed.txt", p"missing.txt"], missing_ok: true, prune_dirs: false)?
  manifest_result.removed == 1
  manifest_result.missing == 1
  fs.remove(fp"${root}/missing-again", missing_ok: true)?
  fs.remove(tree, missing_ok: false)?
  let temp_file = fs.tempfile()?
  (fs.root_exists(temp_file.root, temp_file.path)?)
  fs.root_write(temp_file.root, temp_file.path, "temp")?
  (fs.root_read_text(temp_file.root, temp_file.path)?) == "temp"
  fs.close_root(temp_file.root)?
  let temp_dir = fs.tempdir()?
  fs.root_mkdir(temp_dir, p"child")?
  fs.root_metadata(temp_dir, p"child")?.kind == "dir"
  let temp_path = fs.root_path(temp_dir)?
  fp"${temp_path}/host-path.txt".write("host")?
  (fs.root_read_text(temp_dir, p"host-path.txt")?) == "host"
  fs.close_root(temp_dir)?
  test.error_kind(fs.root_path(temp_dir), "fs-root")?
  let home = fs.user_root("home")?
  (fs.root_exists(home, p".")?)
  fs.close_root(home)?
  let project = fs.project_root("cache", "dev", "LaputaSystems", "xsh-test")?
  fs.root_mkdir(project, p"project-directories-check", parents: true)?
  (fs.root_exists(project, p"project-directories-check")?)
  fs.root_remove(project, p"project-directories-check", dir: true)?
  fs.close_root(project)?
  test.error_kind(fs.user_root("bogus"), "fs-dir")?
  test.error_kind(fs.project_root("bogus", "dev", "LaputaSystems", "xsh-test"), "fs-dir")?
}

proc test_fs_root_operations_reject_traversal(ctx: TestContext) [fs, error] {
  let root_dir = test.temp_dir(ctx, name: "fs-root")?
  let outside = test.temp_dir(ctx, name: "fs-root-outside")?
  fp"${outside}/secret.txt".write("secret")?
  let root = fs.open_root(root_dir)?
  fs.root_mkdir(root, p"nested")?
  fs.root_mkdir(root, p"restricted", mode: 0o700)?
  (fs.root_metadata(root, p"restricted")?.mode % 512) == 0o700
  fs.root_mkdir(root, p"parents/child", parents: true)?
  (fs.root_exists(root, p"parents/child")?)
  fs.root_write(root, p"nested/data.txt", "rooted")?
  (fs.root_read_text(root, p"nested/data.txt")?) == "rooted"
  let observed = fs.root_read_result(root, p"nested/data.txt")?
  observed.state == "observed"
  observed.data == b"rooted"
  observed.errno == null
  ! observed.truncated
  let filesystem = fs.root_filesystem_stats(root, p".")?
  filesystem.state == "observed"
  (filesystem.total_bytes != null and (filesystem.total_bytes ?? 0) > 0)
  (filesystem.used_bytes != null and (filesystem.used_bytes ?? -1) >= 0)
  (filesystem.available_bytes != null and (filesystem.available_bytes ?? -1) >= 0)
  (filesystem.block_size_bytes != null and (filesystem.block_size_bytes ?? 0) > 0)
  let nested_filesystem = fs.root_filesystem_stats(root, p"nested")?
  nested_filesystem.state == "observed"
  let file_filesystem = fs.root_filesystem_stats(root, p"nested/data.txt")?
  file_filesystem.state == "observed"
  file_filesystem.total_bytes == nested_filesystem.total_bytes
  let absent_filesystem = fs.root_filesystem_stats(root, p"nested/missing")?
  absent_filesystem.state == "absent"
  absent_filesystem.error_kind == "not_found"
  test.error_kind(
    fs.root_filesystem_stats(root, /tmp),
    "fs-root-filesystem-stats",
  )?
  let limited = fs.root_read_result(root, p"nested/data.txt", max_bytes: 2)?
  limited.data == b"ro"
  limited.truncated
  let missing = fs.root_read_result(root, p"nested/missing.txt")?
  missing.state == "absent"
  missing.error_kind == "not_found"
  (missing.errno != null)
  test.error_kind(
    fs.root_read_result(root, p"nested/data.txt", max_bytes: -1),
    "fs-root-read-result",
  )?
  let empty_directory = fs.root_children(root, p"parents/child")?
  empty_directory.state == "complete"
  empty_directory.enumeration_succeeded
  empty_directory.children == []
  let absent_directory = fs.root_children(root, p"absent")?
  absent_directory.state == "absent"
  absent_directory.error_kind == "not_found"
  ! absent_directory.enumeration_succeeded
  fs.root_children(root, p"nested")?.children == [p"nested/data.txt"]
  fs.root_write(root, p"nested/data.bin", b"rooted\0bytes")?
  fs.root_children(root, p"nested")?.children == [p"nested/data.bin", p"nested/data.txt"]
  let truncated_directory = fs.root_children(root, p"nested", max_entries: 1)?
  truncated_directory.state == "truncated"
  ! truncated_directory.enumeration_succeeded
  truncated_directory.children == [p"nested/data.bin"]
  (fs.root_read(root, p"nested/data.bin")?) == b"rooted\0bytes"
  fs.root_write_atomic(root, p"nested/data.txt", "atomic")?
  (fs.root_read_text(root, p"nested/data.txt")?) == "atomic"
  fs.root_chmod(root, p"nested/data.txt", 0o700)?
  (fs.root_metadata(root, p"nested/data.txt")?.mode % 512) == 0o700
  (fs.root_exists(root, p"nested/data.txt")?)
  ! fs.root_exists(root, p"nested/missing.txt")?
  fs.root_metadata(root, p"nested/data.txt")?.kind == "file"
  let nested_root = fs.root(root, p"nested")?
  (fs.root_read_text(nested_root, p"data.txt")?) == "atomic"
  fs.root_symlink(root, p"data.txt", p"nested/internal-link")?
  fs.root_readlink(root, p"nested/internal-link")?.display() == "data.txt"
  (fs.root_read_text(root, p"nested/internal-link")?) == "atomic"
  (fs.root_read_text(root, p"nested/../nested/data.txt")?) == "atomic"
  let source_root = fs.open_root(outside)?
  fs.root_install_file(source_root, p"secret.txt", root, p"installed/secret.txt", 0o600)?
  (fs.root_read_text(root, p"installed/secret.txt")?) == "secret"
  (fs.root_metadata(root, p"installed/secret.txt")?.mode % 512) == 0o600
  fs.root_write(source_root, p"secret.txt", "changed")?

  test.error_kind(
    fs.root_install_file(source_root, p"secret.txt", root, p"installed/secret.txt", 0o600),
    "fs-root-install",
  )?

  fs.root_install_file(source_root, p"secret.txt", root, p"installed/secret.txt", 0o600, overwrite: true)?
  (fs.root_read_text(root, p"installed/secret.txt")?) == "changed"
  fs.symlink(fp"${outside}/secret.txt", fp"${root_dir}/nested/link")?
  test.error_kind(fs.root_read_text(root, p"nested/link"), "fs-root-read")?
  let escaped_directory = fs.root_children(root, p"nested/link")?
  ! escaped_directory.enumeration_succeeded
  let escaped_path = fs.root_children(root, ../outside)?
  ! escaped_path.enumeration_succeeded
  test.error_kind(fs.root_read_text(root, ../secret.txt), "fs-root-read")?
  test.error_kind(fs.root_symlink(root, p"target", ../escape), "fs-root-symlink")?
  test.error_kind(fs.root_write_atomic(root, p"missing/parent.txt", "x"), "fs-root-write")?
  test.error_kind(fs.root_install_file(source_root, ../secret.txt, root, p"escape.txt", 0o600), "fs-root-install")?
  fs.root_remove(root, p"nested/data.txt")?
  ! fs.root_exists(root, p"nested/data.txt")?
  fs.close_root(source_root)?
  fs.close_root(nested_root)?
  fs.close_root(root)?
}

proc test_fs_root_and_children_preserve_non_utf8_name(ctx: TestContext) [fs, env, error] {
  if system.uname()?.sysname == "Darwin" {
    test.skip("macOS filesystems reject non-UTF-8 filenames")
    return
  }

  let dir = test.temp_dir(ctx, name: "fs-raw-name")?
  let root = fs.open_root(dir)?
  let raw_name = Path.parse_bytes(b"raw\xfffile")?
  fs.root_write(root, raw_name, b"ok")?
  (fs.root_read(root, raw_name)?) == b"ok"
  fs.root_children(root, p".")?.children == [raw_name]

  let entries = fs.children(dir)?.collect()
  test.eq(entries.len(), 1)?
  test.eq(entries[0].path.relative_to(dir), raw_name)?
  test.eq(entries[0].path.read_bytes()?, b"ok")?
  fs.close_root(root)?
}

proc test_fs_root_symlink_preserves_default_parents_with_named_overwrite(ctx: TestContext) [fs, error] {
  let root_dir = test.temp_dir(ctx, name: "root-symlink-overwrite-defaults")?
  let root = fs.open_root(root_dir)?
  let overwrite = false
  fs.root_symlink(root, p"target", p"nested/link", overwrite: overwrite)?
  fs.root_readlink(root, p"nested/link")?.display() == "target"
  fs.close_root(root)?
}

proc test_fs_walk_filters_large_flat_directory(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "fs-walk-flat")?
  let sub = fp"${root}/sub"
  let ignored = fp"${root}/ignored"
  sub.mkdir()?
  ignored.mkdir()?

  fp"${root}/.gitignore".write("""ignored/
*.log
""")?

  for index in [0] |> range(0, 200) {
    fp"${sub}/f${index}.txt".write("x")?
    fp"${sub}/f${index}.log".write("x")?
  }

  fp"${ignored}/hidden.txt".write("x")?

  let paths = fs.walk(root)
    |> map .path.display()
    |> sort-by .

  let file_count = fs.files(root) |> count()
  let has_hidden = paths |> any "hidden" in .

  # 200 .txt files survive; plus root and sub directories.
  paths.len() == 202
  file_count == 200
  has_hidden == false
}

proc test_fs_walk_honors_gitignore_by_default_and_can_disable_it(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "fs-walk-gitignore")?
  fp"${root}/ignored".mkdir()?
  fp"${root}/nested".mkdir()?
  fp"${root}/build".mkdir()?
  fp"${root}/.git".mkdir()?
  fp"${root}/.cache".mkdir()?

  fp"${root}/.gitignore".write("""ignored/
*.log
!keep.log
/build
""")?

  fp"${root}/visible.txt".write("visible")?
  fp"${root}/a.log".write("ignored")?
  fp"${root}/keep.log".write("kept")?
  fp"${root}/ignored/hidden.txt".write("ignored")?
  fp"${root}/nested/a.log".write("ignored")?
  fp"${root}/build/output.txt".write("ignored")?
  fp"${root}/.git/config".write("ignored")?
  fp"${root}/.cache/secret.txt".write("hidden")?
  fp"${root}/.env".write("hidden")?

  let filtered = fs.files(root)
    |> sort-by .path
    |> map { |entry|
      entry.path.strip_prefix(root)?.display()
    }

  let raw = fs.files(root, gitignore: false)
    |> sort-by .path
    |> map { |entry|
      entry.path.strip_prefix(root)?.display()
    }

  let raw_hidden = fs.files(root, gitignore: false, hidden: true)
    |> sort-by .path
    |> map { |entry|
      entry.path.strip_prefix(root)?.display()
    }

  ("visible.txt" in filtered)
  ("keep.log" in filtered)
  ! ("a.log" in filtered)
  ! ("ignored/hidden.txt" in filtered)
  ! ("nested/a.log" in filtered)
  ! ("build/output.txt" in filtered)
  ! (".git/config" in filtered)
  ! (".cache/secret.txt" in filtered)
  ! (".env" in filtered)
  ("a.log" in raw)
  ("ignored/hidden.txt" in raw)
  ("nested/a.log" in raw)
  ("build/output.txt" in raw)
  ! (".git/config" in raw)
  ! (".cache/secret.txt" in raw)
  ! (".env" in raw)
  (".gitignore" in raw_hidden)
  (".git/config" in raw_hidden)
  (".cache/secret.txt" in raw_hidden)
  (".env" in raw_hidden)
}

proc test_fs_optional_arguments_accept_positional_forms(ctx: TestContext) [fs, error] {
  # Positional optional arguments must compile and behave identically to the
  # equivalent named form (regression for compact-runtime fs.files/fs.walk).
  let root = test.temp_dir(ctx, name: "fs-positional-optional")?
  fp"${root}/nested".mkdir()?
  fp"${root}/.git".mkdir()?
  fp"${root}/.gitignore".write(""".git/
*.log
""")?
  fp"${root}/a.txt".write("text")?
  fp"${root}/b.log".write("ignored")?
  fp"${root}/nested/c.txt".write("text")?
  fp"${root}/.git/config".write("ignored")?

  let by_name = fs.files(root, gitignore: false)
    |> sort-by .path
    |> map { |e|
      e.path.display()
    }
    |> collect()
  let by_position = fs.files(root, false)
    |> sort-by .path
    |> map { |e|
      e.path.display()
    }
    |> collect()
  by_position.join(",") == by_name.join(",")

  let walk_by_name = fs.walk(root, gitignore: false)
    |> sort-by .path
    |> map { |e|
      e.path.display()
    }
    |> collect()
  let walk_by_position = fs.walk(root, false)
    |> sort-by .path
    |> map { |e|
      e.path.display()
    }
    |> collect()
  walk_by_position.join(",") == walk_by_name.join(",")
  ("b.log" in by_name.join(","))
  ("nested/c.txt" in by_name.join(","))
}

proc test_fs_files_recurses_with_raw_walk_and_preserves_entry_ext(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "fs-files-recursive")?
  fp"${root}/include/bits".mkdir()?
  fp"${root}/include/sys".mkdir()?
  fp"${root}/src".mkdir()?
  fp"${root}/obj".mkdir()?

  fp"${root}/.gitignore".write("""*.lo
*.so
*.a
/obj/
""")?

  fp"${root}/include/top.h".write("top")?
  fp"${root}/include/bits/alltypes.h".write("bits")?
  fp"${root}/include/sys/stat.h".write("sys")?
  fp"${root}/src/main.c".write("main")?
  fp"${root}/src/Makefile".write("all:")?
  fp"${root}/src/skip.lo".write("obj")?
  fp"${root}/obj/hidden.h".write("hidden")?

  let raw_headers = fs.files(fp"${root}/include", gitignore: false)
    |> sort-by .path
    |> map { |entry|
      entry.path.strip_prefix(root)?.display()
    }

  let filtered = fs.files(root)
    |> sort-by .path
    |> map { |entry|
      entry.path.strip_prefix(root)?.display()
    }

  let c_files = fs.files(fp"${root}/src", gitignore: false) |> where .ext == "c"
  let dot_c_files = fs.files(fp"${root}/src", gitignore: false) |> where .ext == ".c"

  let source_headers = fs.files(root, exts: ["h", "c"])
    |> sort-by .path
    |> map { |entry|
      entry.path.strip_prefix(root)?.display()
    }

  let extensionless = fs.files(root, exts: [""])
    |> sort-by .path
    |> map { |entry|
      entry.path.strip_prefix(root)?.display()
    }

  let cheap_c = (fs.files(root, gitignore: false, stat: false, exts: ["c"]) |> first())?
  raw_headers.len() == 3
  ("include/top.h" in raw_headers)
  ("include/bits/alltypes.h" in raw_headers)
  ("include/sys/stat.h" in raw_headers)
  ("include/top.h" in filtered)
  ("include/bits/alltypes.h" in filtered)
  ("include/sys/stat.h" in filtered)
  ("src/main.c" in filtered)
  ! ("src/skip.lo" in filtered)
  ! ("obj/hidden.h" in filtered)
  c_files.len() == 1
  c_files[0].name == "main.c"
  c_files[0].ext == "c"
  dot_c_files.len() == 0
  source_headers.len() == 4
  ("include/top.h" in source_headers)
  ("src/main.c" in source_headers)
  ! ("src/skip.lo" in source_headers)
  fs.files(root, exts: [".c"]) |> count() == 0
  extensionless.len() == 1
  ("src/Makefile" in extensionless)
  cheap_c.name == "main.c"
  cheap_c.ext == "c"
  cheap_c.kind == "file"
  let unstat_files = test.run_script(
    ctx,
    f"""
let entry = (fs.files(fp"${root}", false, false, [], true) |> first())?
print \$entry.size
""",
  )?
  unstat_files.status == 3
  "metadata-unavailable" in unstat_files.stderr
  cheap_c.path.strip_prefix(root)?.display() == "src/main.c"
}

proc test_filesystem_path_and_install_apis(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "fs-path-install")?
  let note = fp"${root}/note.txt"
  note.write_atomic("old")?

  note.write_atomic("""hello
""")?

  let note_text = note.read_text()?
  note.chmod(0o600)?
  let link = fp"${root}/note.link"
  fs.symlink(note, link)?
  let entries = fs.ls(root) |> sort-by .name
  let files = fs.children(root) |> where .kind == "file"
  let usage = root.du()?
  let renamed = note.with_ext("log")
  let stripped = note.strip_prefix(root)?
  let resolved = root.resolve()?
  let cwd = fs.cwd()?
  let scratch = fs.tempdir()?
  let temp = fs.tempfile()?
  entries[0].name == "note.link"
  entries[1].name == "note.txt"
  (files[0].mode % 512) == 0o600
  (files[0].uid >= 0)
  (files[0].modified > 0)
  renamed.name == "note.log"
  renamed.ext == "log"
  note.parent().name() == root.name()
  stripped.display() == "note.txt"
  (usage >= 6)
  resolved.name() == root.name()

  note_text == """hello
"""

  (fs.root_exists(scratch, p".")?)
  (fs.root_exists(temp.root, temp.path)?)
  let copy = fp"${root}/copy.txt"
  let moved = fp"${root}/moved.txt"
  let hard = fp"${root}/hard.txt"
  let empty = fp"${root}/empty"
  fs.copy(note, copy)?
  let refused = fs.copy(note, copy)
  copy.rename(moved)?
  moved.truncate(4)?
  let moved_text = moved.read_text()?
  let moved_meta = moved.metadata()?
  let installed = fp"${root}/bin/tool"
  fs.install(moved, installed, 0o755)?
  let install_refused = fs.install(moved, installed, 0o755)
  fs.install(moved, installed, 0o700, parents: false, overwrite: true)?
  let installed_meta = installed.metadata()?
  fs.fsync(installed)?
  let fifo = fp"${root}/control"
  fs.mkfifo(fifo, 0o600)?
  let fifo_meta = fifo.metadata()?
  let install_link = fp"${root}/installed.link"
  fs.symlink(installed, install_link)?
  let symlink_refused = fs.install(moved, install_link, 0o755)
  fp"${root}/stamp".touch()?
  empty.mkdir()?
  empty.remove_dir()?
  moved.hardlink(hard)?
  hard.unlink()?
  let link_target = link.readlink()?
  moved_text == "hell"
  moved_meta.size == 4
  link_target.display() == note.display()
  (cwd.name() != "")
  (installed_meta.mode % 512) == 0o700
  (installed.read_text()?) == moved_text
  fifo_meta.kind == "other"
  test.error_kind(refused, "fs-copy")?
  test.error_kind(install_refused, "fs-install")?
  test.error_kind(symlink_refused, "fs-install")?
  fs.close_root(temp.root)?
  fs.close_root(scratch)?
}

proc test_filesystem_package_policy_apis(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "package-policy-fs")?
  let src = fp"${root}/src"
  fp"${src}/dir".mkdir()?
  let tool = fp"${src}/dir/tool"

  tool.write("""tool
""")?

  tool.chmod(0o755)?
  fs.symlink(p"dir/tool", fp"${src}/tool.link")?
  let copied = fs.copy_tree(src, fp"${root}/copy")?
  let me = user.current()?
  let grp = group.current()?
  let copied_tool = fp"${root}/copy/dir/tool"
  fs.chown(copied_tool, me)?
  fs.chgrp(copied_tool, grp)?
  let lock = fs.lock(fp"${root}/pm.lock")?
  (lock.id > 0)
  ! lock.shared
  fs.unlock(lock)?
  let installed = fp"${root}/image/usr/bin/tool"
  fs.install_as(copied_tool, installed, 0o755, me, grp)?
  let installed_meta = installed.metadata()?
  let removed = fs.remove_manifest(fp"${root}/image", [p"usr/bin/tool"])?
  copied.files == 1
  copied.dirs == 2
  copied.symlinks == 1
  (installed_meta.mode % 512) == 0o755
  removed.removed == 1
  removed.pruned_dirs == 2
  ! fs.exists(installed)?
  test.error_kind(fs.remove_manifest(fp"${root}/image", [../escape], missing_ok: true), "fs-remove-manifest")?
  test.error_kind(fs.copy_tree(src, fp"${root}/copy"), "fs-copy-tree")?
}

proc test_stable_tables_sort_files_and_process_records(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "table-sort-process")?
  fp"${root}/small".write("a")?
  fp"${root}/large".write("abcd")?
  let entries = fs.ls(root) |> sort-by .size
  entries[0].name == "small"
  entries[0].size == 1
  entries[1].name == "large"
  entries[1].size == 4
  ((process.list() |> count()) > 0)
}
