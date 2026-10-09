test test_chgrp_current_group { |ctx|
  let target = test.temp_file(ctx, name: "grouped.txt", contents: b"payload")?
  let current = group.current()?
  let name = current.name
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chgrp.xsh" $name $target ?
  assert output == ""
  assert target.metadata()?.gid == current.gid
  let root = test.temp_dir(ctx, name: "grouped-tree")?
  let child = fp"{root}/child.txt"
  child.write("payload")
  let recursive = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chgrp.xsh" -R $name $root ?
  assert recursive == ""
  assert child.metadata()?.gid == current.gid
}
