use core.lib.fat as vfat

test test_fatlabel_cli_reads_and_changes_volume_label { |ctx|
  let root = test.temp_dir(ctx, name: "fatlabel")?
  let image = fp"{root}/disk.img"
  let _ = vfat.format(image, 1474560, bits: 12, label: "BEFORE")?
  let script = fp"{ctx.core_dir}/fatlabel.xsh"
  let read = test.expect(ctx, script.read_text()?, status: 0, env: {XSH_MODULE_PATH: ctx.core_dir}, args: [image])?
  assert read.stdout == "BEFORE\n"
  test.expect(ctx, script.read_text()?, status: 0, env: {XSH_MODULE_PATH: ctx.core_dir}, args: [image, "AFTER"])?
  assert vfat.inspect(image)?.label == "AFTER"
  assert vfat.check(image)?.issues.is_empty()
}
