test test_showcase_df_root {
  let root_mount = fs.mount_for(/)?
  let output = run.text "xsh" "showcase/df.xsh" -- / ?
  assert "Filesystem" in output
  assert "Mounted on" in output
  assert root_mount.filesystem in output
}

test test_showcase_df_kp_path { |ctx|
  let root = test.temp_dir(ctx, name: "showcase-df")?
  let mount = fs.mount_for(root)?
  let output = run.text "xsh" "showcase/df.xsh" -- -kP $root ?
  assert "1024-blocks" in output
  assert f" {mount.blocks_1k} " in output
}
