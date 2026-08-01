proc test_mdev_wrapper_preserves_platform_boundary(ctx: TestContext) [fs, process, env, error] {
  if system.uname()?.sysname == "Linux" {
    test.skip("mdev scan behavior is covered by tests/xsh/stdlib/auth.xsh")
    return
  }

  let err = test.temp_path(ctx, name: "mdev.err")
  let result = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/mdev.xsh" -- --help 2> $err
  test.ok(! result.exited_with(0))?
  test.contains(err.read_text()?, "mdev is only available on Linux")?
}
