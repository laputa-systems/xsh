proc test_showcase_df_root() [fs, process, error] {
  let root_mount = fs.mount_for(/)?
  let output = run.text "xsh" "showcase/df.xsh" -- / ?
  test.contains(output, "Filesystem")?
  test.contains(output, "Mounted on")?
  test.contains(output, root_mount.filesystem)?
}

proc test_showcase_df_kp_path(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "showcase-df")?
  let mount = fs.mount_for(root)?
  let output = run.text "xsh" "showcase/df.xsh" -- -kP $root ?
  test.contains(output, "1024-blocks")?
  test.contains(output, f" ${mount.blocks_1k} ")?
}
