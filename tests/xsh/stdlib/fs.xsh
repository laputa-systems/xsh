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
  test.eq(fs.read_text(file)?, "hello")?
  test.ok(fs.exists(file)?)?
  test.ok(fs.executable(file)?)?
  let file_meta = fs.metadata(file)?
  test.eq(file_meta.name, "data.txt")?
  test.ok(file_meta.executable)?
  test.ok(file_meta.owner_executable)?
  test.ok(file_meta.group_executable)?
  test.ok(file_meta.other_executable)?
  test.ok(fs.executable(file_meta.mode))?
  test.ok(fs.owner_executable(file_meta.mode))?
  test.ok(fs.group_executable(file_meta.mode))?
  test.ok(fs.other_executable(file_meta.mode))?
  test.ok(! fs.world_writable(file_meta.mode))?
  test.ok(fs.setuid(0o4755))?
  test.ok(fs.setgid(0o2755))?
  test.ok(fs.sticky(0o1777))?
  test.ok(! fs.setuid(0o0755))?
  test.ok(! fs.setgid(0o0755))?
  test.ok(! fs.sticky(0o0755))?
  test.ok(! file_meta.world_writable)?
  test.ok(fs.filesystem_stats(root)?.blocks_1k > 0)?
  let mounts = fs.mounts()?.collect()
  test.ok(mounts.len() > 0)?
  test.ok(mounts |> any .mounted_on.display() == "/")?
  let root_mount = fs.mount_for(root)?
  test.ok(root_mount.blocks_1k > 0)?
  test.ok(root_mount.available_1k >= 0)?
  test.ok(root_mount.capacity_percent >= 0)?
  test.ok(root_mount.fstype != "")?
  test.ok(fs.cwd()?.display() != "")?
  let gitroot = fs.gitroot()?
  test.ok(fp"${gitroot}/docs/SPEC.md".exists()?)?
  let children = fs.children(nested)? |> sort-by .name
  let listed = fs.ls(nested)? |> sort-by .name
  test.eq(children.len(), listed.len())?
  test.ok(fs.children(nested, stat: false, ordered: false)? |> any .name == "data.txt")?
  let unstat_children = test.run_script(
    ctx,
    f"""
let entry = (fs.children(fp"${nested}", stat: false, ordered: false)? |> first())?
print \$entry.size
""",
  )?
  test.eq(unstat_children.status, 3)?
  test.contains(unstat_children.stderr, "metadata-unavailable")?
  test.ok(fs.walk(src)? |> any .name == "data.txt")?
  test.ok(fs.files(src)? |> any .name == "data.txt")?
  test.ok(fs.dirs(src)? |> any .name == "nested")?
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

  test.ok(fs.exists(cache)?)?
  test.ok(cache.exists()?)?
  test.ok(fs.exists(tarball)?)?
  let copied = fp"${root}/copied.txt"
  fs.copy(file, copied)?
  test.eq(fs.read_text(copied)?, "hello")?
  let renamed = fp"${root}/renamed.txt"
  fs.rename(copied, renamed)?
  test.ok(! fs.exists(copied)?)?
  test.eq(fs.read_text(renamed)?, "hello")?
  let tree = fp"${root}/tree-copy"
  let tree_result = fs.copy_tree(src, tree)?
  test.ok(tree_result.files >= 4)?
  test.eq(fp"${tree}/nested/data.txt".read_text()?, "hello")?
  let install_dest = fp"${root}/install/bin/data.txt"
  fs.install(file, install_dest, 0o600)?
  test.eq(fs.metadata(install_dest)?.mode % 512, 0o600)?
  let current_user = user.current()?
  let current_group = group.current()?
  fs.install_as(file, fp"${root}/install-as/data.txt", 0o600, current_user, current_group)?
  fs.chmod(install_dest, 0o644)?
  fs.chown(install_dest, current_user)?
  fs.chgrp(install_dest, current_group)?
  test.eq(fs.metadata(install_dest)?.mode % 512, 0o644)?
  let fifo = fp"${root}/fifo"
  fs.mkfifo(fifo, 0o600)?
  test.ok(fs.exists(fifo)?)?
  fs.fsync(file)?
  fs.sync()?
  let link = fp"${root}/link"
  fs.symlink(file, link)?
  test.eq(link.readlink()?.display(), file.display())?
  let lock_file = fp"${root}/lock"
  let lock = fs.lock(lock_file, shared: true)?
  test.eq(lock.path, lock_file)?
  test.ok(lock.shared)?
  fs.unlock(lock)?
  let manifest_result = fs.remove_manifest(root, [p"renamed.txt", p"missing.txt"], missing_ok: true, prune_dirs: false)?
  test.eq(manifest_result.removed, 1)?
  test.eq(manifest_result.missing, 1)?
  fs.remove(fp"${root}/missing-again", missing_ok: true)?
  fs.remove(tree, missing_ok: false)?
  let temp_file = fs.tempfile()?
  test.ok(fs.root_exists(temp_file.root, temp_file.path)?)?
  fs.root_write(temp_file.root, temp_file.path, "temp")?
  test.eq(fs.root_read_text(temp_file.root, temp_file.path)?, "temp")?
  fs.close_root(temp_file.root)?
  let temp_dir = fs.tempdir()?
  fs.root_mkdir(temp_dir, p"child")?
  test.eq(fs.root_metadata(temp_dir, p"child")?.kind, "dir")?
  let temp_path = fs.root_path(temp_dir)?
  fp"${temp_path}/host-path.txt".write("host")?
  test.eq(fs.root_read_text(temp_dir, p"host-path.txt")?, "host")?
  fs.close_root(temp_dir)?
  test.error_kind(fs.root_path(temp_dir), "fs-root")?
  let home = fs.user_root("home")?
  test.ok(fs.root_exists(home, p".")?)
  fs.close_root(home)?
  let project = fs.project_root("cache", "dev", "LaputaSystems", "xsh-test")?
  fs.root_mkdir(project, p"project-directories-check", parents: true)?
  test.ok(fs.root_exists(project, p"project-directories-check")?)
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
  fs.root_mkdir(root, p"parents/child", parents: true)?
  test.ok(fs.root_exists(root, p"parents/child")?)
  fs.root_write(root, p"nested/data.txt", "rooted")?
  test.eq(fs.root_read_text(root, p"nested/data.txt")?, "rooted")?
  fs.root_write(root, p"nested/data.bin", b"rooted\0bytes")?
  test.eq(fs.root_read(root, p"nested/data.bin")?, b"rooted\0bytes")?
  fs.root_write_atomic(root, p"nested/data.txt", "atomic")?
  test.eq(fs.root_read_text(root, p"nested/data.txt")?, "atomic")?
  fs.root_chmod(root, p"nested/data.txt", 0o700)?
  test.eq(fs.root_metadata(root, p"nested/data.txt")?.mode % 512, 0o700)?
  test.ok(fs.root_exists(root, p"nested/data.txt")?)
  test.ok(! fs.root_exists(root, p"nested/missing.txt")?)
  test.eq(fs.root_metadata(root, p"nested/data.txt")?.kind, "file")?
  let nested_root = fs.root(root, p"nested")?
  test.eq(fs.root_read_text(nested_root, p"data.txt")?, "atomic")?
  fs.root_symlink(root, p"data.txt", p"nested/internal-link")?
  test.eq(fs.root_readlink(root, p"nested/internal-link")?.display(), "data.txt")?
  test.eq(fs.root_read_text(root, p"nested/internal-link")?, "atomic")?
  test.eq(fs.root_read_text(root, p"nested/../nested/data.txt")?, "atomic")?
  let source_root = fs.open_root(outside)?
  fs.root_install_file(source_root, p"secret.txt", root, p"installed/secret.txt", 0o600)?
  test.eq(fs.root_read_text(root, p"installed/secret.txt")?, "secret")?
  test.eq(fs.root_metadata(root, p"installed/secret.txt")?.mode % 512, 0o600)?
  fs.root_write(source_root, p"secret.txt", "changed")?

  test.error_kind(
    fs.root_install_file(source_root, p"secret.txt", root, p"installed/secret.txt", 0o600),
    "fs-root-install",
  )?

  fs.root_install_file(source_root, p"secret.txt", root, p"installed/secret.txt", 0o600, overwrite: true)?
  test.eq(fs.root_read_text(root, p"installed/secret.txt")?, "changed")?
  fs.symlink(fp"${outside}/secret.txt", fp"${root_dir}/nested/link")?
  test.error_kind(fs.root_read_text(root, p"nested/link"), "fs-root-read")?
  test.error_kind(fs.root_read_text(root, ../secret.txt), "fs-root-read")?
  test.error_kind(fs.root_symlink(root, p"target", ../escape), "fs-root-symlink")?
  test.error_kind(fs.root_write_atomic(root, p"missing/parent.txt", "x"), "fs-root-write")?
  test.error_kind(fs.root_install_file(source_root, ../secret.txt, root, p"escape.txt", 0o600), "fs-root-install")?
  fs.root_remove(root, p"nested/data.txt")?
  test.ok(! fs.root_exists(root, p"nested/data.txt")?)
  fs.close_root(source_root)?
  fs.close_root(nested_root)?
  fs.close_root(root)?
}

proc test_fs_root_symlink_preserves_default_parents_with_named_overwrite(ctx: TestContext) [fs, error] {
  let root_dir = test.temp_dir(ctx, name: "root-symlink-overwrite-defaults")?
  let root = fs.open_root(root_dir)?
  let overwrite = false
  fs.root_symlink(root, p"target", p"nested/link", overwrite: overwrite)?
  test.eq(fs.root_readlink(root, p"nested/link")?.display(), "target")?
  fs.close_root(root)?
}

proc test_fs_walk_is_parallel_unordered_and_honors_gitignore(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "fs-walk-parallel")?
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

  let par = fs.walk(root)
    |> map .path.display()
    |> sort-by .

  let par_files = fs.files(root) |> count()
  let has_hidden = par |> any "hidden" in .

  # 200 .txt files survive; plus root and sub directories.
  test.eq(par.len(), 202)?
  test.eq(par_files, 200)?
  test.eq(has_hidden, false)?
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

  test.ok("visible.txt" in filtered)?
  test.ok("keep.log" in filtered)?
  test.ok(! ("a.log" in filtered))?
  test.ok(! ("ignored/hidden.txt" in filtered))?
  test.ok(! ("nested/a.log" in filtered))?
  test.ok(! ("build/output.txt" in filtered))?
  test.ok(! (".git/config" in filtered))?
  test.ok(! (".cache/secret.txt" in filtered))?
  test.ok(! (".env" in filtered))?
  test.ok("a.log" in raw)?
  test.ok("ignored/hidden.txt" in raw)?
  test.ok("nested/a.log" in raw)?
  test.ok("build/output.txt" in raw)?
  test.ok(! (".git/config" in raw))?
  test.ok(! (".cache/secret.txt" in raw))?
  test.ok(! (".env" in raw))?
  test.ok(".gitignore" in raw_hidden)?
  test.ok(".git/config" in raw_hidden)?
  test.ok(".cache/secret.txt" in raw_hidden)?
  test.ok(".env" in raw_hidden)?
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
  test.eq(by_position.join(","), by_name.join(","))?

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
  test.eq(walk_by_position.join(","), walk_by_name.join(","))?
  test.ok("b.log" in by_name.join(","))?
  test.ok("nested/c.txt" in by_name.join(","))?
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
  test.eq(raw_headers.len(), 3)?
  test.ok("include/top.h" in raw_headers)?
  test.ok("include/bits/alltypes.h" in raw_headers)?
  test.ok("include/sys/stat.h" in raw_headers)?
  test.ok("include/top.h" in filtered)?
  test.ok("include/bits/alltypes.h" in filtered)?
  test.ok("include/sys/stat.h" in filtered)?
  test.ok("src/main.c" in filtered)?
  test.ok(! ("src/skip.lo" in filtered))?
  test.ok(! ("obj/hidden.h" in filtered))?
  test.eq(c_files.len(), 1)?
  test.eq(c_files[0].name, "main.c")?
  test.eq(c_files[0].ext, "c")?
  test.eq(dot_c_files.len(), 0)?
  test.eq(source_headers.len(), 4)?
  test.ok("include/top.h" in source_headers)?
  test.ok("src/main.c" in source_headers)?
  test.ok(! ("src/skip.lo" in source_headers))?
  test.eq(fs.files(root, exts: [".c"]) |> count(), 0)?
  test.eq(extensionless.len(), 1)?
  test.ok("src/Makefile" in extensionless)?
  test.eq(cheap_c.name, "main.c")?
  test.eq(cheap_c.ext, "c")?
  test.eq(cheap_c.kind, "file")?
  let unstat_files = test.run_script(
    ctx,
    f"""
let entry = (fs.files(fp"${root}", false, false, [], true) |> first())?
print \$entry.size
""",
  )?
  test.eq(unstat_files.status, 3)?
  test.contains(unstat_files.stderr, "metadata-unavailable")?
  test.eq(cheap_c.path.strip_prefix(root)?.display(), "src/main.c")?
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
  test.eq(entries[0].name, "note.link")?
  test.eq(entries[1].name, "note.txt")?
  test.eq(files[0].mode % 512, 0o600)?
  test.ok(files[0].uid >= 0)?
  test.ok(files[0].modified > 0)?
  test.eq(renamed.name, "note.log")?
  test.eq(renamed.ext, "log")?
  test.eq(note.parent().name(), root.name())?
  test.eq(stripped.display(), "note.txt")?
  test.ok(usage >= 6)?
  test.eq(resolved.name(), root.name())?

  test.eq(
    note_text,
    """hello
""",
  )?

  test.ok(fs.root_exists(scratch, p".")?)?
  test.ok(fs.root_exists(temp.root, temp.path)?)?
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
  test.eq(moved_text, "hell")?
  test.eq(moved_meta.size, 4)?
  test.eq(link_target.display(), note.display())?
  test.ok(cwd.name() != "")?
  test.eq(installed_meta.mode % 512, 0o700)?
  test.eq(installed.read_text()?, moved_text)?
  test.eq(fifo_meta.kind, "other")?
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
  test.ok(lock.id > 0)?
  test.ok(! lock.shared)?
  fs.unlock(lock)?
  let installed = fp"${root}/image/usr/bin/tool"
  fs.install_as(copied_tool, installed, 0o755, me, grp)?
  let installed_meta = installed.metadata()?
  let removed = fs.remove_manifest(fp"${root}/image", [p"usr/bin/tool"])?
  test.eq(copied.files, 1)?
  test.eq(copied.dirs, 2)?
  test.eq(copied.symlinks, 1)?
  test.eq(installed_meta.mode % 512, 0o755)?
  test.eq(removed.removed, 1)?
  test.eq(removed.pruned_dirs, 2)?
  test.ok(! fs.exists(installed)?)?
  test.error_kind(fs.remove_manifest(fp"${root}/image", [../escape], missing_ok: true), "fs-remove-manifest")?
  test.error_kind(fs.copy_tree(src, fp"${root}/copy"), "fs-copy-tree")?
}

proc test_stable_tables_sort_files_and_process_records(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "table-sort-process")?
  fp"${root}/small".write("a")?
  fp"${root}/large".write("abcd")?
  let entries = fs.ls(root) |> sort-by .size
  test.eq(entries[0].name, "small")?
  test.eq(entries[0].size, 1)?
  test.eq(entries[1].name, "large")?
  test.eq(entries[1].size, 4)?
  test.ok((process.list() |> count()) > 0)?
}
