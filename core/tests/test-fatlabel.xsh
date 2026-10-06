use core.lib.fat as vfat

test test_fatlabel_cli_reads_and_changes_volume_label { |ctx|
  let root = test.temp_dir(ctx, name: "fatlabel")?
  let image = fp"{root}/disk.img"
  let _ = vfat.format(image, 1474560, bits: 12, label: "BEFORE")?
  let script = fp"{ctx.core_dir}/fatlabel.xsh"
  let read = test.run_script(ctx, script.read_text()?, env: {XSH_MODULE_PATH: ctx.core_dir.display()}, args: [image.display()])?
  assert read.status == 0, read.stderr
  assert read.stdout == "BEFORE\n"
  let changed = test.run_script(ctx, script.read_text()?, env: {XSH_MODULE_PATH: ctx.core_dir.display()}, args: [image.display(), "AFTER"])?
  assert changed.status == 0, changed.stderr
  assert vfat.inspect(image)?.label == "AFTER"
  assert vfat.check(image)?.issues.is_empty()
}
