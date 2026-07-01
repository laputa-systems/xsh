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
  let mounts = fs.mounts()?
  test.ok(mounts.len() > 0)?
  test.ok(mounts |> any .mounted_on.display() == "/")?
  let root_mount = fs.mount_for(root)?
  test.ok(root_mount.blocks_1k > 0)?
  test.ok(root_mount.available_1k >= 0)?
  test.ok(root_mount.capacity_percent >= 0)?
  test.ok(root_mount.fstype != "")?
  test.ok(fs.cwd()?.display() != "")?
  let gitroot = fs.gitroot()?
  test.ok(fp"${gitroot}/docs/STDLIB.md".exists()?)?
  let children = fs.children(nested)? |> sort-by .name
  let listed = fs.ls(nested)? |> sort-by .name
  test.eq(children.len(), listed.len())?
  test.ok(fs.children(nested, stat: false, ordered: false)? |> any .name == "data.txt")?
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
  fs.root_mkdir(project, p"cap-directories-check", parents: true)?
  test.ok(fs.root_exists(project, p"cap-directories-check")?)
  fs.root_remove(project, p"cap-directories-check", dir: true)?
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
  test.error_kind(fs.root_read_text(root, p"nested/internal-link"), "fs-root-read")?
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
