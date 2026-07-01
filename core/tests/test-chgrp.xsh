pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_chgrp_current_group(ctx: TestContext) [fs, process, error] {
  let target = test.temp_file(ctx, name: "grouped.txt", contents: b"payload")?
  let current = group.current()?
  let name = current.name
  let output = run.text xsh_bin() chgrp.xsh -- $name $target ?
  test.eq(output, "")?
  test.eq(target.metadata()?.gid, current.gid)?
  let root = test.temp_dir(ctx, name: "grouped-tree")?
  let child = fp"${root}/child.txt"
  child.write("payload")?
  let recursive = run.text xsh_bin() chgrp.xsh -- -R $name $root ?
  test.eq(recursive, "")?
  test.eq(child.metadata()?.gid, current.gid)?
}
