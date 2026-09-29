proc test_showcase_df_root() [fs, process, error] {
  let root_mount = fs.mount_for(/)?
  let output = run.text "xsh" "showcase/df.xsh" -- / ?
  "Filesystem" in output
  "Mounted on" in output
  root_mount.filesystem in output
}

proc test_showcase_df_kp_path(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "showcase-df")?
  let mount = fs.mount_for(root)?
  let output = run.text "xsh" "showcase/df.xsh" -- -kP $root ?
  "1024-blocks" in output
  f" ${mount.blocks_1k} " in output
}
