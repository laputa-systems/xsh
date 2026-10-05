test test_missing_file_read_propagates_structured_error { |ctx|
  let missing = test.temp_path(ctx, name: "missing-read")
  let output = test.run_script(
    ctx,
    f"""let _ = p"{missing}".read_bytes()?
""",
  )?

  assert output.status == 3
  assert "fs-read" in output.stderr
}

test test_fs_walk_and_files_take_any_break_and_count { |ctx|
  let root = test.temp_dir(ctx, name: "fs-walk-stage")?
  var index = 0
  while index < 50 {
    fp"{root}/f{index}.txt".write("x")
    index = index + 1
  }

  let first3 = fs.files(root)
    |> take(3)
    |> map .name
  assert first3.len() == 3
  assert fs.walk(root) |> any .kind == "file"

  var visited = []
  for entry in fs.files(root) {
    visited += [entry.name]
    break when visited.len() >= 2
  }

  assert visited.len() == 2
  assert (fs.files(root) |> count()) == 50
}

test test_fs_walk_dynamic_stat_flag_preserves_metadata_boundary { |ctx|
  let root = test.temp_dir(ctx, name: "fs-walk-dynamic-stat")?
  fp"{root}/file.txt".write("data")
  let output = test.run_script(
    ctx,
    f"""
let root = p"{root}"
let use_stat = false
let entry = fs.walk(root, stat: use_stat) |> first()?
print ${{entry.size}}
""",
  )?
  assert output.status == 3
  assert "metadata-unavailable" in output.stderr
}

test test_fs_walk_stat_true_matches_direct_record_and_snapshots_metadata { |ctx|
  let root = test.temp_dir(ctx, name: "fs-walk-stat-record")?
  let file = fp"{root}/entry.txt"
  file.write("old")

  let walked = fs.files(root, gitignore: false) |> first()?
  let direct = fs.children(root)? |> first()?
  assert walked == direct
  assert walked.keys() == direct.keys()
  assert walked.size == 3

  file.write("new longer content")
  assert walked.size == 3
  assert walked.get("size")? == 3
}

test test_fs_files_dynamic_walk_flags_are_evaluated { |ctx|
  let root = test.temp_dir(ctx, name: "fs-files-dynamic-flags")?
  fp"{root}/normal.txt".write("data")
  fp"{root}/ignored.txt".write("ignored")
  fp"{root}/.hidden.txt".write("hidden")
  fp"{root}/.gitignore".write(
    """ignored.txt
""",
  )

  let include_hidden = true
  let exclude_hidden = false
  assert fs.files(root, gitignore: false, hidden: include_hidden) |> any .name == ".hidden.txt"
  assert ! (fs.files(root, gitignore: false, hidden: exclude_hidden) |> any .name == ".hidden.txt")

  let use_gitignore = true
  let skip_gitignore = false
  assert ! (fs.files(root, gitignore: use_gitignore) |> any .name == "ignored.txt")
  assert fs.files(root, gitignore: skip_gitignore) |> any .name == "ignored.txt"

  let use_stat = true
  let normal = fs.files(root, stat: use_stat)
    |> where .name == "normal.txt"
    |> first()?
  assert normal.size == 4

  let unstat = test.run_script(
    ctx,
    f"""
let root = p"{root}"
let use_stat = false
let entry = fs.files(root, stat: use_stat) |> where .name == "normal.txt" |> first()?
print ${{entry.size}}
""",
  )?
  assert unstat.status == 3
  assert "metadata-unavailable" in unstat.stderr
}

test test_fs_remove_deletes_trees_without_following_symlinks { |ctx|
  let root = test.temp_dir(ctx, name: "fs-remove-tree")?
  let outside = fp"{root}/outside"
  outside.mkdir()
  fp"{outside}/kept.txt".write("kept")
  let tree = fp"{root}/tree"
  fp"{tree}/nested/deeper".mkdir()
  fp"{tree}/nested/deeper/file.txt".write("gone")
  fs.symlink(outside, fp"{tree}/nested/link")

  tree.remove()
  assert ! tree.exists()?
  assert fp"{outside}/kept.txt".read_text()? == "kept"

  let path_tree = fp"{root}/path-tree"
  fp"{path_tree}/child".mkdir()
  fp"{path_tree}/child/file.txt".write("gone")
  path_tree.remove()
  assert ! path_tree.exists()?
}

test test_fs_write_atomic_keeps_plain_write_modes { |ctx|
  let root = test.temp_dir(ctx, name: "fs-atomic-mode")?
  let plain = fp"{root}/plain.txt"
  let atomic = fp"{root}/atomic.txt"
  plain.write("plain")
  atomic.write_atomic("atomic")
  # A new file gets the mode a plain write gives under the same umask, not
  # the temporary file's private 0600.
  assert atomic.metadata()?.mode % 512 == plain.metadata()?.mode % 512

  # Replacing a file keeps its mode, as a plain write over it does.
  atomic.chmod(0o751)
  atomic.write_atomic("replaced")
  assert atomic.read_text()? == "replaced"
  assert atomic.metadata()?.mode % 512 == 0o751
}

test test_fs_tree_metadata_install_and_locking { |ctx|
  let root = test.temp_dir(ctx, name: "fs")?
  let src = fp"{root}/src"
  let nested = fp"{src}/nested"
  nested.mkdir()
  let file = fp"{nested}/data.txt"
  file.write("hello")
  fp"{nested}/bytes.bin".write(b"bytes")
  fp"{nested}/atomic.txt".write_atomic("atomic")
  fp"{nested}/atomic.bin".write_atomic(b"atomic-bytes")
  file.chmod(0o755)
  assert file.read_text()? == "hello"
  assert file.exists()?
  assert file.executable()?
  let file_meta = file.metadata()?
  assert file_meta.name == "data.txt"
  assert file_meta.executable
  assert file_meta.owner_executable
  assert file_meta.group_executable
  assert file_meta.other_executable
  assert fs.executable(file_meta.mode)
  assert fs.owner_executable(file_meta.mode)
  assert fs.group_executable(file_meta.mode)
  assert fs.other_executable(file_meta.mode)
  assert ! fs.world_writable(file_meta.mode)
  assert fs.setuid(0o4755)
  assert fs.setgid(0o2755)
  assert fs.sticky(0o1777)
  assert ! fs.setuid(0o0755)
  assert ! fs.setgid(0o0755)
  assert ! fs.sticky(0o0755)
  assert ! file_meta.world_writable
  assert fs.filesystem_stats(root)?.blocks_1k > 0
  let mounts = fs.mounts()?.collect()
  assert mounts.len() > 0
  assert mounts |> any .mounted_on == "/"
  let root_mount = fs.mount_for(root)?
  assert root_mount.blocks_1k > 0
  assert root_mount.available_1k >= 0
  assert root_mount.capacity_percent >= 0
  assert root_mount.fstype != ""
  assert fs.cwd()?.display() != ""
  let gitroot = fs.gitroot()?
  assert fp"{gitroot}/docs/SPEC.md".exists()?
  let children = fs.children(nested)? |> sort-by .name
  let listed = fs.children(nested, stat: true, ordered: true)? |> sort-by .name
  assert children.len() == listed.len()
  assert fs.children(nested, stat: false, ordered: false)? |> any .name == "data.txt"
  let unstat_children = test.run_script(
    ctx,
    f"""
let entry = fs.children(fp"{nested}", stat: false, ordered: false)? |> first()?
print \$entry.size
""",
  )?
  assert unstat_children.status == 3
  assert "metadata-unavailable" in unstat_children.stderr
  assert fs.walk(src)? |> any .name == "data.txt"
  assert fs.files(src)? |> any .name == "data.txt"
  assert fs.dirs(src)? |> any .name == "nested"
  let cache = fp"{root}/remote-cache"
  fp"{cache}/packages".mkdir()
  let tarball = fp"{cache}/packages/pkg.tar"
  tarball.write("package")
  fp"{root}/old-build".mkdir()
  fp"{root}/old-file".write("stale")

  for entry in fs.children(root)? {
    if entry.name != "remote-cache" and entry.name != "src" {
      entry.path.remove(missing_ok: true)
    }
  }

  assert cache.exists()?
  assert cache.exists()?
  assert tarball.exists()?
  let copied = fp"{root}/copied.txt"
  file.copy(copied)
  assert copied.read_text()? == "hello"
  let renamed = fp"{root}/renamed.txt"
  copied.rename(renamed)
  assert ! copied.exists()?
  assert renamed.read_text()? == "hello"
  let tree = fp"{root}/tree-copy"
  let tree_result = fs.copy_tree(src, tree)?
  assert tree_result.files >= 4
  assert fp"{tree}/nested/data.txt".read_text()? == "hello"
  let install_dest = fp"{root}/install/bin/data.txt"
  fs.install(file, install_dest, 0o600)
  assert install_dest.metadata()?.mode % 512 == 0o600
  let current_user = user.current()?
  let current_group = group.current()?
  fs.install_as(file, fp"{root}/install-as/data.txt", 0o600, current_user, current_group)
  install_dest.chmod(0o644)
  fs.chown(install_dest, current_user)
  fs.chgrp(install_dest, current_group)
  assert install_dest.metadata()?.mode % 512 == 0o644
  let fifo = fp"{root}/fifo"
  fs.mkfifo(fifo, 0o600)
  assert fifo.exists()?
  fs.fsync(file)
  fs.sync()
  let link = fp"{root}/link"
  fs.symlink(file, link)
  assert link.readlink()?.display() == file.display()
  let lock_file = fp"{root}/lock"
  let lock = fs.lock(lock_file, shared: true)?
  assert lock.path == lock_file
  assert lock.shared
  fs.unlock(lock)
  let manifest_result = fs.remove_manifest(root, [p"renamed.txt", p"missing.txt"], missing_ok: true, prune_dirs: false)?
  assert manifest_result.removed == 1
  assert manifest_result.missing == 1
  fp"{root}/missing-again".remove(missing_ok: true)
  tree.remove(missing_ok: false)
  let temp_file = fs.tempfile()?
  assert temp_file.root.exists(temp_file.path)?
  temp_file.root.write(temp_file.path, "temp")
  assert temp_file.root.read_text(temp_file.path)? == "temp"
  temp_file.root.close()
  let temp_dir = fs.tempdir()?
  temp_dir.mkdir(p"child")
  assert temp_dir.metadata(p"child")?.kind == "dir"
  let temp_path = temp_dir.host_path()?
  fp"{temp_path}/host-path.txt".write("host")
  assert temp_dir.read_text(p"host-path.txt")? == "host"
  temp_dir.close()
  test.error_kind(temp_dir.host_path(), "fs-root")
  let home = fs.user_root("home")?
  assert home.exists(p".")?
  home.close()
  let project = fs.project_root("cache", "dev", "LaputaSystems", "xsh-test")?
  project.mkdir(p"project-directories-check", parents: true)
  assert project.exists(p"project-directories-check")?
  project.remove(p"project-directories-check", dir: true)
  project.close()
  test.error_kind(fs.user_root("bogus"), "fs-dir")
  test.error_kind(fs.project_root("bogus", "dev", "LaputaSystems", "xsh-test"), "fs-dir")
}

test test_fs_root_operations_reject_traversal { |ctx|
  let root_dir = test.temp_dir(ctx, name: "fs-root")?
  let outside = test.temp_dir(ctx, name: "fs-root-outside")?
  fp"{outside}/secret.txt".write("secret")
  let root = fs.open_root(root_dir)?
  root.mkdir(p"nested")
  root.mkdir(p"restricted", mode: 0o700)
  assert root.metadata(p"restricted")?.mode % 512 == 0o700
  root.mkdir(p"parents/child", parents: true)
  assert root.exists(p"parents/child")?
  root.write(p"nested/data.txt", "rooted")
  assert root.read_text(p"nested/data.txt")? == "rooted"
  let observed = root.read_result(p"nested/data.txt")?
  assert observed.state == "observed"
  assert observed.data == b"rooted"
  assert observed.errno == null
  assert ! observed.truncated
  let filesystem = root.filesystem_stats(p".")?
  assert filesystem.state == "observed"
  assert filesystem.total_bytes != null and filesystem.total_bytes > 0
  assert filesystem.used_bytes != null and (filesystem.used_bytes ?? -1) >= 0
  assert filesystem.available_bytes != null and (filesystem.available_bytes ?? -1) >= 0
  assert filesystem.block_size_bytes != null and filesystem.block_size_bytes > 0
  let nested_filesystem = root.filesystem_stats(p"nested")?
  assert nested_filesystem.state == "observed"
  let file_filesystem = root.filesystem_stats(p"nested/data.txt")?
  assert file_filesystem.state == "observed"
  assert file_filesystem.total_bytes == nested_filesystem.total_bytes
  let absent_filesystem = root.filesystem_stats(p"nested/missing")?
  assert absent_filesystem.state == "absent"
  assert absent_filesystem.error_kind == "not_found"
  test.error_kind(
    root.filesystem_stats(/tmp),
    "fs-root-filesystem-stats",
  )
  let limited = root.read_result(p"nested/data.txt", max_bytes: 2)?
  assert limited.data == b"ro"
  assert limited.truncated
  let missing = root.read_result(p"nested/missing.txt")?
  assert missing.state == "absent"
  assert missing.error_kind == "not_found"
  assert missing.errno != null
  test.error_kind(
    root.read_result(p"nested/data.txt", max_bytes: -1),
    "fs-root-read-result",
  )
  let empty_directory = root.children(p"parents/child")?
  assert empty_directory.state == "complete"
  assert empty_directory.enumeration_succeeded
  assert empty_directory.children == []
  let absent_directory = root.children(p"absent")?
  assert absent_directory.state == "absent"
  assert absent_directory.error_kind == "not_found"
  assert ! absent_directory.enumeration_succeeded
  assert root.children(p"nested")?.children == [p"nested/data.txt"]
  root.write(p"nested/data.bin", b"rooted\0bytes")
  assert root.children(p"nested")?.children == [p"nested/data.bin", p"nested/data.txt"]
  let truncated_directory = root.children(p"nested", max_entries: 1)?
  assert truncated_directory.state == "truncated"
  assert ! truncated_directory.enumeration_succeeded
  assert truncated_directory.children == [p"nested/data.bin"]
  assert root.read_bytes(p"nested/data.bin")? == b"rooted\0bytes"
  root.write_atomic(p"nested/data.txt", "atomic")
  assert root.read_text(p"nested/data.txt")? == "atomic"
  root.chmod(p"nested/data.txt", 0o700)
  assert root.metadata(p"nested/data.txt")?.mode % 512 == 0o700
  assert root.exists(p"nested/data.txt")?
  assert ! root.exists(p"nested/missing.txt")?
  assert root.metadata(p"nested/data.txt")?.kind == "file"
  let nested_root = root.open_root(p"nested")?
  assert nested_root.read_text(p"data.txt")? == "atomic"
  root.symlink(p"data.txt", p"nested/internal-link")
  assert root.readlink(p"nested/internal-link")?.display() == "data.txt"
  assert root.read_text(p"nested/internal-link")? == "atomic"
  assert root.read_text(p"nested/../nested/data.txt")? == "atomic"
  let source_root = fs.open_root(outside)?
  fs.root_install_file(source_root, p"secret.txt", root, p"installed/secret.txt", 0o600)
  assert root.read_text(p"installed/secret.txt")? == "secret"
  assert root.metadata(p"installed/secret.txt")?.mode % 512 == 0o600
  source_root.write(p"secret.txt", "changed")

  test.error_kind(
    fs.root_install_file(source_root, p"secret.txt", root, p"installed/secret.txt", 0o600),
    "fs-root-install",
  )

  fs.root_install_file(source_root, p"secret.txt", root, p"installed/secret.txt", 0o600, overwrite: true)
  assert root.read_text(p"installed/secret.txt")? == "changed"
  fs.symlink(fp"{outside}/secret.txt", fp"{root_dir}/nested/link")
  test.error_kind(root.read_text(p"nested/link"), "fs-root-read")
  let escaped_directory = root.children(p"nested/link")?
  assert ! escaped_directory.enumeration_succeeded
  let escaped_path = root.children(../outside)?
  assert ! escaped_path.enumeration_succeeded
  test.error_kind(root.read_text(../secret.txt), "fs-root-read")
  test.error_kind(root.symlink(p"target", ../escape), "fs-root-symlink")
  test.error_kind(root.write_atomic(p"missing/parent.txt", "x"), "fs-root-write")
  test.error_kind(fs.root_install_file(source_root, ../secret.txt, root, p"escape.txt", 0o600), "fs-root-install")
  root.remove(p"nested/data.txt")
  assert ! root.exists(p"nested/data.txt")?
  source_root.close()
  nested_root.close()
  root.close()
}

test test_fs_root_and_children_preserve_non_utf8_name { |ctx|
  if system.uname()?.sysname == "Darwin" {
    test.skip("macOS filesystems reject non-UTF-8 filenames")
    return
  }

  let dir = test.temp_dir(ctx, name: "fs-raw-name")?
  let root = fs.open_root(dir)?
  let raw_name = Path.parse_bytes(b"raw\xfffile")?
  root.write(raw_name, b"ok")
  assert root.read_bytes(raw_name)? == b"ok"
  assert root.children(p".")?.children == [raw_name]

  let entries = fs.children(dir)?.collect()
  assert entries.len() == 1
  assert entries[0].path.relative_to(dir) == raw_name
  assert entries[0].path.read_bytes()? == b"ok"
  root.close()
}

test test_fs_root_symlink_preserves_default_parents_with_named_overwrite { |ctx|
  let root_dir = test.temp_dir(ctx, name: "root-symlink-overwrite-defaults")?
  let root = fs.open_root(root_dir)?
  let overwrite = false
  root.symlink(p"target", p"nested/link", overwrite:)
  assert root.readlink(p"nested/link")?.display() == "target"
  root.close()
}

test test_fs_walk_filters_large_flat_directory { |ctx|
  let root = test.temp_dir(ctx, name: "fs-walk-flat")?
  let sub = fp"{root}/sub"
  let ignored = fp"{root}/ignored"
  sub.mkdir()
  ignored.mkdir()

  fp"{root}/.gitignore".write("""ignored/
*.log
""")

  for index in [0] |> range(0, 200) {
    fp"{sub}/f{index}.txt".write("x")
    fp"{sub}/f{index}.log".write("x")
  }

  fp"{ignored}/hidden.txt".write("x")

  let paths = fs.walk(root)
    |> map .path.display()
    |> sort-by .

  let file_count = fs.files(root) |> count()
  let has_hidden = paths |> any "hidden" in .

  # 200 .txt files survive; plus root and sub directories.
  assert paths.len() == 202
  assert file_count == 200
  assert has_hidden == false
}

test test_fs_walk_honors_gitignore_by_default_and_can_disable_it { |ctx|
  let root = test.temp_dir(ctx, name: "fs-walk-gitignore")?
  fp"{root}/ignored".mkdir()
  fp"{root}/nested".mkdir()
  fp"{root}/build".mkdir()
  fp"{root}/.git".mkdir()
  fp"{root}/.cache".mkdir()

  fp"{root}/.gitignore".write("""ignored/
*.log
!keep.log
/build
""")

  fp"{root}/visible.txt".write("visible")
  fp"{root}/a.log".write("ignored")
  fp"{root}/keep.log".write("kept")
  fp"{root}/ignored/hidden.txt".write("ignored")
  fp"{root}/nested/a.log".write("ignored")
  fp"{root}/build/output.txt".write("ignored")
  fp"{root}/.git/config".write("ignored")
  fp"{root}/.cache/secret.txt".write("hidden")
  fp"{root}/.env".write("hidden")

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

  assert "visible.txt" in filtered
  assert "keep.log" in filtered
  assert ! ("a.log" in filtered)
  assert ! ("ignored/hidden.txt" in filtered)
  assert ! ("nested/a.log" in filtered)
  assert ! ("build/output.txt" in filtered)
  assert ! (".git/config" in filtered)
  assert ! (".cache/secret.txt" in filtered)
  assert ! (".env" in filtered)
  assert "a.log" in raw
  assert "ignored/hidden.txt" in raw
  assert "nested/a.log" in raw
  assert "build/output.txt" in raw
  assert ! (".git/config" in raw)
  assert ! (".cache/secret.txt" in raw)
  assert ! (".env" in raw)
  assert ".gitignore" in raw_hidden
  assert ".git/config" in raw_hidden
  assert ".cache/secret.txt" in raw_hidden
  assert ".env" in raw_hidden
}

test test_fs_optional_arguments_accept_positional_forms { |ctx|
  # Positional optional arguments must compile and behave identically to the
  # equivalent named form (regression for compact-runtime fs.files/fs.walk).
  let root = test.temp_dir(ctx, name: "fs-positional-optional")?
  fp"{root}/nested".mkdir()
  fp"{root}/.git".mkdir()
  fp"{root}/.gitignore".write(""".git/
*.log
""")
  fp"{root}/a.txt".write("text")
  fp"{root}/b.log".write("ignored")
  fp"{root}/nested/c.txt".write("text")
  fp"{root}/.git/config".write("ignored")

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
  assert by_position.join(",") == by_name.join(",")

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
  assert walk_by_position.join(",") == walk_by_name.join(",")
  assert "b.log" in by_name.join(",")
  assert "nested/c.txt" in by_name.join(",")
}

test test_fs_files_recurses_with_raw_walk_and_preserves_entry_ext { |ctx|
  let root = test.temp_dir(ctx, name: "fs-files-recursive")?
  fp"{root}/include/bits".mkdir()
  fp"{root}/include/sys".mkdir()
  fp"{root}/src".mkdir()
  fp"{root}/obj".mkdir()

  fp"{root}/.gitignore".write("""*.lo
*.so
*.a
/obj/
""")

  fp"{root}/include/top.h".write("top")
  fp"{root}/include/bits/alltypes.h".write("bits")
  fp"{root}/include/sys/stat.h".write("sys")
  fp"{root}/src/main.c".write("main")
  fp"{root}/src/Makefile".write("all:")
  fp"{root}/src/skip.lo".write("obj")
  fp"{root}/obj/hidden.h".write("hidden")

  let raw_headers = fs.files(fp"{root}/include", gitignore: false)
    |> sort-by .path
    |> map { |entry|
      entry.path.strip_prefix(root)?.display()
    }

  let filtered = fs.files(root)
    |> sort-by .path
    |> map { |entry|
      entry.path.strip_prefix(root)?.display()
    }

  let c_files = fs.files(fp"{root}/src", gitignore: false) |> where .ext == "c"
  let dot_c_files = fs.files(fp"{root}/src", gitignore: false) |> where .ext == ".c"

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

  let cheap_c = fs.files(root, gitignore: false, stat: false, exts: ["c"]) |> first()?
  assert raw_headers.len() == 3
  assert "include/top.h" in raw_headers
  assert "include/bits/alltypes.h" in raw_headers
  assert "include/sys/stat.h" in raw_headers
  assert "include/top.h" in filtered
  assert "include/bits/alltypes.h" in filtered
  assert "include/sys/stat.h" in filtered
  assert "src/main.c" in filtered
  assert ! ("src/skip.lo" in filtered)
  assert ! ("obj/hidden.h" in filtered)
  assert c_files.len() == 1
  assert c_files[0].name == "main.c"
  assert c_files[0].ext == "c"
  assert dot_c_files.len() == 0
  assert source_headers.len() == 4
  assert "include/top.h" in source_headers
  assert "src/main.c" in source_headers
  assert ! ("src/skip.lo" in source_headers)
  assert (fs.files(root, exts: [".c"]) |> count()) == 0
  assert extensionless.len() == 1
  assert "src/Makefile" in extensionless
  assert cheap_c.name == "main.c"
  assert cheap_c.ext == "c"
  assert cheap_c.kind == "file"
  let unstat_files = test.run_script(
    ctx,
    f"""
let entry = fs.files(fp"{root}", false, false, [], true) |> first()?
print \$entry.size
""",
  )?
  assert unstat_files.status == 3
  assert "metadata-unavailable" in unstat_files.stderr
  assert cheap_c.path.strip_prefix(root)?.display() == "src/main.c"
}

test test_filesystem_path_and_install_apis { |ctx|
  let root = test.temp_dir(ctx, name: "fs-path-install")?
  let note = fp"{root}/note.txt"
  note.write_atomic("old")

  note.write_atomic("""hello
""")

  let note_text = note.read_text()?
  note.chmod(0o600)
  let link = fp"{root}/note.link"
  fs.symlink(note, link)
  let entries = fs.children(root) |> sort-by .name
  let files = fs.children(root) |> where .kind == "file"
  let usage = root.du()?
  let renamed = note.with_ext("log")
  let stripped = note.strip_prefix(root)?
  let resolved = root.resolve()?
  let cwd = fs.cwd()?
  let scratch = fs.tempdir()?
  let temp = fs.tempfile()?
  assert entries[0].name == "note.link"
  assert entries[1].name == "note.txt"
  assert files[0].mode % 512 == 0o600
  assert files[0].uid >= 0
  assert files[0].modified > 0
  assert renamed.name == "note.log"
  assert renamed.ext == "log"
  assert note.parent().name() == root.name()
  assert stripped == "note.txt"
  assert usage >= 6
  assert resolved.name() == root.name()

  assert note_text == """hello
"""

  assert scratch.exists(p".")?
  assert temp.root.exists(temp.path)?
  let copy = fp"{root}/copy.txt"
  let moved = fp"{root}/moved.txt"
  let hard = fp"{root}/hard.txt"
  let empty = fp"{root}/empty"
  note.copy(copy)
  let refused = note.copy(copy)
  copy.rename(moved)
  moved.truncate(4)
  let moved_text = moved.read_text()?
  let moved_meta = moved.metadata()?
  let installed = fp"{root}/bin/tool"
  fs.install(moved, installed, 0o755)
  let install_refused = fs.install(moved, installed, 0o755)
  fs.install(moved, installed, 0o700, parents: false, overwrite: true)
  let installed_meta = installed.metadata()?
  fs.fsync(installed)
  let fifo = fp"{root}/control"
  fs.mkfifo(fifo, 0o600)
  let fifo_meta = fifo.metadata()?
  let install_link = fp"{root}/installed.link"
  fs.symlink(installed, install_link)
  let symlink_refused = fs.install(moved, install_link, 0o755)
  fp"{root}/stamp".touch()
  empty.mkdir()
  empty.remove_dir()
  moved.hardlink(hard)
  hard.unlink()
  let link_target = link.readlink()?
  assert moved_text == "hell"
  assert moved_meta.size == 4
  assert link_target.display() == note.display()
  assert cwd.name() != ""
  assert installed_meta.mode % 512 == 0o700
  assert installed.read_text()? == moved_text
  assert fifo_meta.kind == "other"
  test.error_kind(refused, "fs-copy")
  test.error_kind(install_refused, "fs-install")
  test.error_kind(symlink_refused, "fs-install")
  temp.root.close()
  scratch.close()
}

test test_filesystem_package_policy_apis { |ctx|
  let root = test.temp_dir(ctx, name: "package-policy-fs")?
  let src = fp"{root}/src"
  fp"{src}/dir".mkdir()
  let tool = fp"{src}/dir/tool"

  tool.write(
    """tool
""",
    mode: 0o755,
  )
  fs.symlink(p"dir/tool", fp"{src}/tool.link")
  let copied = fs.copy_tree(src, fp"{root}/copy")?
  let me = user.current()?
  let grp = group.current()?
  let copied_tool = fp"{root}/copy/dir/tool"
  fs.chown(copied_tool, me)
  fs.chgrp(copied_tool, grp)
  let lock = fs.lock(fp"{root}/pm.lock")?
  assert lock.id > 0
  assert ! lock.shared
  fs.unlock(lock)
  let installed = fp"{root}/image/usr/bin/tool"
  fs.install_as(copied_tool, installed, 0o755, me, grp)
  let installed_meta = installed.metadata()?
  let removed = fs.remove_manifest(fp"{root}/image", [p"usr/bin/tool"])?
  assert copied.files == 1
  assert copied.dirs == 2
  assert copied.symlinks == 1
  assert installed_meta.mode % 512 == 0o755
  assert removed.removed == 1
  assert removed.pruned_dirs == 2
  assert ! installed.exists()?
  test.error_kind(fs.remove_manifest(fp"{root}/image", [../escape], missing_ok: true), "fs-remove-manifest")
  test.error_kind(fs.copy_tree(src, fp"{root}/copy"), "fs-copy-tree")
}

test test_stable_tables_sort_files_and_process_records { |ctx|
  let root = test.temp_dir(ctx, name: "table-sort-process")?
  fp"{root}/small".write("a")
  fp"{root}/large".write("abcd")
  let entries = fs.children(root) |> sort-by .size
  assert entries[0].name == "small"
  assert entries[0].size == 1
  assert entries[1].name == "large"
  assert entries[1].size == 4
  assert (process.list() |> count()) > 0
}

test test_fs_walk_and_files_iteration_failures_are_catchable { |ctx|
  guard applet.current_euid() != 0 else {
    test.skip("root reads unreadable directories")
    return
  }
  let root = test.temp_dir(ctx, name: "fs-walk-denied")?
  let locked = fp"{root}/a/locked"
  locked.mkdir()
  fp"{locked}/inside.txt".write("x")
  fp"{root}/b.txt".write("x")
  locked.chmod(0o000)
  defer locked.chmod(0o755)?

  let walked: Result[Int] = try {
    var count = 0
    for _ in fs.walk(root)? {
      count += 1
    }

    count
  }
  assert walked is Err(_)
  let counted: Result[Int] = try {
    fs.files(root)? |> count()
  }
  if let Err(failure) = counted {
    assert "locked" in failure.message, failure.message
  } else {
    test.fail("expected the walk to fail")
  }
}

test test_host_filesystem_errors_implement_error_facets [fs, error] { |ctx|
  let missing = test.temp_path(ctx, name: "missing-facet")
  let root = test.temp_dir(ctx, name: "facet-root")?
  let file = fp"{root}/plain.txt"
  file.write("text")
  let read = missing.read_text()
  assert read is Err(is NotFound)
  if let Err(error) = read {
    assert error is NotFound
  } else {
    assert false, "missing file read"
  }

  assert missing.read_bytes() is Err(is NotFound)
  assert missing.read_text() is Err(is NotFound)
  assert fs.files(missing) is Err(is NotFound)
  let below_file = fp"{file}/child".read_text()
  assert below_file is Err(is HostIo)
  assert ! (below_file is Err(is NotFound))
}
