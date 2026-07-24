proc test_chown_current_user(ctx: TestContext) [fs, process, env, error] {
  let target = test.temp_file(ctx, name: "owned.txt", contents: b"payload")?
  let current = user.current()?
  let name = current.name
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/chown.xsh" -- $name $target ?
  test.eq(output, "")?
  test.eq(target.metadata()?.uid, current.uid)?
  let root = test.temp_dir(ctx, name: "owned-tree")?
  let child = fp"${root}/child.txt"
  child.write("payload")?
  let recursive = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/chown.xsh" -- -R $name $root ?
  test.eq(recursive, "")?
  test.eq(child.metadata()?.uid, current.uid)?
}
