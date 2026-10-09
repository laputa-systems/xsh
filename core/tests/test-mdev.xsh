test test_mdev_wrapper_preserves_platform_boundary { |ctx|
  if system.uname()?.sysname == "Linux" {
    test.skip("mdev scan behavior is covered by tests/xsh/stdlib/auth.xsh")
    return
  }

  let err = test.temp_path(ctx, name: "mdev.err")
  let result = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/mdev.xsh" --help 2> $err
  assert ! result.exited_with(0)
  assert "mdev is only available on Linux" in err.read_text()?
}
