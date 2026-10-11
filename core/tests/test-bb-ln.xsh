use support.uu as uu

# origin: busybox ln/ln-creates-hard-links
test test_bb_ln_ln_creates_hard_links_eb1c1963 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  let r = uu.invoke(s, "ln", ["file1", "link1"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "file1")?
  assert uu.file_exists(s, "link1")?
}

# origin: busybox ln/ln-creates-soft-links
test test_bb_ln_ln_creates_soft_links_df61cc0d { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  let r = uu.invoke(s, "ln", ["-s", "file1", "link1"])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "link1")?
  assert uu.read_link(s, "link1")? == "file1"
}

# origin: busybox ln/ln-force-creates-hard-links
test test_bb_ln_ln_force_creates_hard_links_f00350b3 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  uu.write(s, "link1", "file number two\n")?
  let r = uu.invoke(s, "ln", ["-f", "file1", "link1"])?
  uu.succeeds(r)
  assert uu.file_exists(s, "file1")?
  assert uu.file_exists(s, "link1")?
}

# origin: busybox ln/ln-force-creates-soft-links
test test_bb_ln_ln_force_creates_soft_links_5fef09ec { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  uu.write(s, "link1", "file number two\n")?
  let r = uu.invoke(s, "ln", ["-f", "-s", "file1", "link1"])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "link1")?
  assert uu.read_link(s, "link1")? == "file1"
}

# origin: busybox ln/ln-preserves-hard-links
test test_bb_ln_ln_preserves_hard_links_45d6d692 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  uu.write(s, "link1", "file number two\n")?
  let r = uu.invoke(s, "ln", ["file1", "link1"])?
  uu.fails(r)
}

# origin: busybox ln/ln-preserves-soft-links
test test_bb_ln_ln_preserves_soft_links_9e7c5311 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "file number one\n")?
  uu.write(s, "link1", "file number two\n")?
  let r = uu.invoke(s, "ln", ["-s", "file1", "link1"])?
  uu.fails(r)
}

