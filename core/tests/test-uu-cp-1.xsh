use support.uu as uu

let FILE_NAME = "foo"
let SYMLINK_NAME = "symlink"
let CONTENTS = "abcd"


# Upstream scene symlinks use absolute targets; link assertions remove the scene prefix.
proc absolute_link(s: uu.Scene, target: Str, name: Str) [fs, error] -> Result[Unit, Error] {
  uu.symlink(s, uu.at(s, target).display(), name)
}

proc resolve_link(s: uu.Scene, name: Str) [fs, error] -> Result[Str, Error] {
  let target = uu.read_link(s, name)?
  let prefix = f"{s.root}/"
  Ok(if target.starts_with(prefix) { target.replace(prefix, with: "") } else { target })
}

proc file_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  if !uu.exists(s, name)? { return Ok(false) }
  uu.file_exists(s, name)
}

proc setup_link(s: uu.Scene, source: Str) [fs, error] -> Result[Unit, Error] {
  match source {
    "file_link" => { uu.touch(s, "file")?; absolute_link(s, "file", "file_link")? },
    "dir_link" => { uu.mkdir(s, "dir")?; absolute_link(s, "dir", "dir_link")? },
    "dang_link" => absolute_link(s, "nowhere", "dang_link")?,
    _ => {},
  }
  Ok()
}

# origin: uutils test_cp::same_file::test_hardlink_of_symlink_to_hardlink_of_same_symlink_with_option_no_deref
test test_uu_cp_same_file_hardlink_of_symlink_to_hardlink_of_same_symlink_with_option_no_deref { |ctx|
  let s = uu.scene(ctx)?
  let hardlink1 = "hardlink_to_symlink_1"
  let hardlink2 = "hardlink_to_symlink_2"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink1)?
  uu.hard_link(s, SYMLINK_NAME, hardlink2)?
  let ino = fs.stat(uu.at(s, hardlink1))?.ino
  assert ino == fs.stat(uu.at(s, hardlink2))?.ino
  let r = uu.invoke(s, "cp", ["-P", hardlink1, hardlink2])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert ino == fs.stat(uu.at(s, hardlink1))?.ino
  assert ino == fs.stat(uu.at(s, hardlink2))?.ino
}

# origin: uutils test_cp::same_file::test_same_dangling_symlink_to_itself_no_dereference
test test_uu_cp_same_file_same_dangling_symlink_to_itself_no_dereference { |ctx|
  let s = uu.scene(ctx)?
  absolute_link(s, "nonexistent_file", SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", ["-P", SYMLINK_NAME, SYMLINK_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "are the same file")
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_file
test test_uu_cp_same_file_same_file_from_file_to_file { |ctx|
  for option in ["-d", "-f", "-df", "--rem"] {
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, FILE_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "'foo' and 'foo' are the same file")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_file_with_backup
test test_uu_cp_same_file_same_file_from_file_to_file_with_backup { |ctx|
  for option in ["-b", "-bd"] {
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, FILE_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "'foo' and 'foo' are the same file")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_file_with_options_backup_and_no_deref
test test_uu_cp_same_file_same_file_from_file_to_file_with_options_backup_and_no_deref { |ctx|
  for option in ["-bf", "-bdf"] {
  let s = uu.scene(ctx)?
  let backup = "foo~"
  uu.write(s, FILE_NAME, CONTENTS)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, FILE_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, backup)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert uu.read_text(s, backup)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_file_with_options_link
test test_uu_cp_same_file_same_file_from_file_to_file_with_options_link { |ctx|
  for option in ["-l", "-dl", "-fl", "-dfl", "-bl", "-bdl"] {
  let s = uu.scene(ctx)?
  let backup = "foo~"
  uu.write(s, FILE_NAME, CONTENTS)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, FILE_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert !file_exists(s, backup)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_file_with_options_link_and_backup_and_force
test test_uu_cp_same_file_same_file_from_file_to_file_with_options_link_and_backup_and_force { |ctx|
  for option in ["-bfl", "-bdfl"] {
  let s = uu.scene(ctx)?
  let backup = "foo~"
  uu.write(s, FILE_NAME, CONTENTS)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, FILE_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, backup)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert uu.read_text(s, backup)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_file_with_options_symlink
test test_uu_cp_same_file_same_file_from_file_to_file_with_options_symlink { |ctx|
  for option in ["-s", "-sf"] {
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, FILE_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "'foo' and 'foo' are the same file")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_hardlink
test test_uu_cp_same_file_same_file_from_file_to_hardlink { |ctx|
  for option in ["-d", "-f", "-df"] {
  let s = uu.scene(ctx)?
  let hardlink = "hardlink"
  uu.write(s, FILE_NAME, CONTENTS)?
  uu.hard_link(s, FILE_NAME, hardlink)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, hardlink])?
  uu.fails(r)
  uu.stderr_contains(r, "'foo' and 'hardlink' are the same file")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, hardlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_hardlink_with_option_backup
test test_uu_cp_same_file_same_file_from_file_to_hardlink_with_option_backup { |ctx|
  for option in ["-b", "-bd", "-bf", "-bdf"] {
  let s = uu.scene(ctx)?
  let hardlink = "hardlink"
  let backup = "hardlink~"
  uu.write(s, FILE_NAME, CONTENTS)?
  uu.hard_link(s, FILE_NAME, hardlink)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, hardlink])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, hardlink)?
  assert uu.file_exists(s, backup)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_hardlink_with_option_link
test test_uu_cp_same_file_same_file_from_file_to_hardlink_with_option_link { |ctx|
  for option in ["-l", "-dl", "-fl", "-dfl", "-bl", "-bdl", "-bfl", "-bdfl"] {
  let s = uu.scene(ctx)?
  let hardlink = "hardlink"
  uu.write(s, FILE_NAME, CONTENTS)?
  uu.hard_link(s, FILE_NAME, hardlink)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, hardlink])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, hardlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_hardlink_with_option_rem
test test_uu_cp_same_file_same_file_from_file_to_hardlink_with_option_rem { |ctx|
  let s = uu.scene(ctx)?
  let hardlink = "hardlink"
  uu.write(s, FILE_NAME, CONTENTS)?
  uu.hard_link(s, FILE_NAME, hardlink)?
  let r = uu.invoke(s, "cp", ["--rem", FILE_NAME, hardlink])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, hardlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_hardlink_with_option_symlink
test test_uu_cp_same_file_same_file_from_file_to_hardlink_with_option_symlink { |ctx|
  for option in ["-s", "-sf"] {
  let s = uu.scene(ctx)?
  let hardlink = "hardlink"
  uu.write(s, FILE_NAME, CONTENTS)?
  uu.hard_link(s, FILE_NAME, hardlink)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, hardlink])?
  uu.fails(r)
  uu.stderr_contains(r, "'foo' and 'hardlink' are the same file")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, hardlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_symlink
test test_uu_cp_same_file_same_file_from_file_to_symlink { |ctx|
  for option in ["-d", "-f", "-df"] {
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, SYMLINK_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "'foo' and 'symlink' are the same file")
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_symlink_with_backup_option
test test_uu_cp_same_file_same_file_from_file_to_symlink_with_backup_option { |ctx|
  for option in ["-b", "-bd", "-bf", "-bdf"] {
  let s = uu.scene(ctx)?
  let backup = "symlink~"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.is_symlink(s, backup)?
  assert FILE_NAME == resolve_link(s, backup)?
  assert uu.file_exists(s, SYMLINK_NAME)?
  assert uu.read_text(s, SYMLINK_NAME)? == CONTENTS
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_symlink_with_link_option
test test_uu_cp_same_file_same_file_from_file_to_symlink_with_link_option { |ctx|
  for option in ["-l", "-dl"] {
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, SYMLINK_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "cp: cannot create hard link 'symlink' to 'foo'")
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_symlink_with_options_backup_and_link
test test_uu_cp_same_file_same_file_from_file_to_symlink_with_options_backup_and_link { |ctx|
  for option in ["-bl", "-bdl", "-bfl", "-bdfl"] {
  let s = uu.scene(ctx)?
  let backup = "symlink~"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, backup)?
  assert FILE_NAME == resolve_link(s, backup)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_symlink_with_options_link_and_force
test test_uu_cp_same_file_same_file_from_file_to_symlink_with_options_link_and_force { |ctx|
  for option in ["-fl", "-dfl"] {
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", [option, FILE_NAME, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, SYMLINK_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_symlink_with_options_symlink
test test_uu_cp_same_file_same_file_from_file_to_symlink_with_options_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", ["-s", FILE_NAME, SYMLINK_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "cp: cannot create symbolic link 'symlink' to 'foo'")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_symlink_with_options_symlink_and_force
test test_uu_cp_same_file_same_file_from_file_to_symlink_with_options_symlink_and_force { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", ["-sf", FILE_NAME, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_file_to_symlink_with_rem_option
test test_uu_cp_same_file_same_file_from_file_to_symlink_with_rem_option { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", ["--rem", FILE_NAME, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, SYMLINK_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink { |ctx|
  let s = uu.scene(ctx)?
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", [hardlink_to_symlink, SYMLINK_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "cp: 'hlsl' and 'symlink' are the same file")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_option_backup
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_option_backup { |ctx|
  for option in ["-b", "-bf"] {
  let s = uu.scene(ctx)?
  let backup = "symlink~"
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", [option, hardlink_to_symlink, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, SYMLINK_NAME)?
  assert !uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.is_symlink(s, backup)?
  assert FILE_NAME == resolve_link(s, backup)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert uu.read_text(s, SYMLINK_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_option_backup_and_no_deref
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_option_backup_and_no_deref { |ctx|
  for option in ["-bd", "-bdf"] {
  let s = uu.scene(ctx)?
  let backup = "symlink~"
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", [option, hardlink_to_symlink, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.is_symlink(s, backup)?
  assert FILE_NAME == resolve_link(s, backup)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_option_force
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_option_force { |ctx|
  let s = uu.scene(ctx)?
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", ["-f", hardlink_to_symlink, SYMLINK_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "cp: 'hlsl' and 'symlink' are the same file")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_option_link
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_option_link { |ctx|
  let s = uu.scene(ctx)?
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", ["-l", hardlink_to_symlink, SYMLINK_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot create hard link 'symlink' to 'hlsl'")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_option_link_and_backup
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_option_link_and_backup { |ctx|
  for option in ["-bl", "-bfl"] {
  let s = uu.scene(ctx)?
  let backup = "symlink~"
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", [option, hardlink_to_symlink, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, SYMLINK_NAME)?
  assert !uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.is_symlink(s, backup)?
  assert FILE_NAME == resolve_link(s, backup)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_option_link_and_force
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_option_link_and_force { |ctx|
  let s = uu.scene(ctx)?
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", ["-fl", hardlink_to_symlink, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, SYMLINK_NAME)?
  assert !uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_option_link_and_no_deref
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_option_link_and_no_deref { |ctx|
  for option in ["-dl", "-dfl"] {
  let s = uu.scene(ctx)?
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", [option, hardlink_to_symlink, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_option_no_deref
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_option_no_deref { |ctx|
  for option in ["-d", "-df"] {
  let s = uu.scene(ctx)?
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", [option, hardlink_to_symlink, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_option_rem
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_option_rem { |ctx|
  let s = uu.scene(ctx)?
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", ["--rem", hardlink_to_symlink, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.file_exists(s, SYMLINK_NAME)?
  assert !uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert uu.read_text(s, SYMLINK_NAME)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_option_symlink
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_option_symlink { |ctx|
  let s = uu.scene(ctx)?
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", ["-s", hardlink_to_symlink, SYMLINK_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot create symbolic link 'symlink' to 'hlsl'")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_option_symlink_and_force
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_option_symlink_and_force { |ctx|
  let s = uu.scene(ctx)?
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", ["-sf", hardlink_to_symlink, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert hardlink_to_symlink == resolve_link(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_hard_link_of_symlink_to_symlink_with_options_backup_link_no_deref
test test_uu_cp_same_file_same_file_from_hard_link_of_symlink_to_symlink_with_options_backup_link_no_deref { |ctx|
  for option in ["-bdl", "-bdfl"] {
  let s = uu.scene(ctx)?
  let hardlink_to_symlink = "hlsl"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  uu.hard_link(s, SYMLINK_NAME, hardlink_to_symlink)?
  let r = uu.invoke(s, "cp", [option, hardlink_to_symlink, SYMLINK_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert uu.is_symlink(s, hardlink_to_symlink)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, hardlink_to_symlink)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_file
test test_uu_cp_same_file_same_file_from_symlink_to_file { |ctx|
  for option in ["-d", "-f", "-df", "--rem"] {
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", [option, SYMLINK_NAME, FILE_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "'symlink' and 'foo' are the same file")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_file_with_option_backup
test test_uu_cp_same_file_same_file_from_symlink_to_file_with_option_backup { |ctx|
  for option in ["-b", "-bf"] {
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", [option, SYMLINK_NAME, FILE_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "'symlink' and 'foo' are the same file")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_file_with_option_backup_without_deref
test test_uu_cp_same_file_same_file_from_symlink_to_file_with_option_backup_without_deref { |ctx|
  for option in ["-bd", "-bdf"] {
  let s = uu.scene(ctx)?
  let backup = "foo~"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", [option, SYMLINK_NAME, FILE_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, backup)?
  assert uu.is_symlink(s, FILE_NAME)?
  assert FILE_NAME == resolve_link(s, FILE_NAME)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert uu.read_text(s, backup)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_file_with_option_symlink
test test_uu_cp_same_file_same_file_from_symlink_to_file_with_option_symlink { |ctx|
  for option in ["-s", "-sf"] {
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", [option, SYMLINK_NAME, FILE_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "'symlink' and 'foo' are the same file")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_file_with_options_link
test test_uu_cp_same_file_same_file_from_symlink_to_file_with_options_link { |ctx|
  for option in ["-l", "-dl", "-fl", "-bl", "-bfl"] {
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", [option, SYMLINK_NAME, FILE_NAME])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.is_symlink(s, SYMLINK_NAME)?
  assert FILE_NAME == resolve_link(s, SYMLINK_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_symlink_with_option_backup
test test_uu_cp_same_file_same_file_from_symlink_to_symlink_with_option_backup { |ctx|
  for option in ["-b", "-bf"] {
  let s = uu.scene(ctx)?
  let symlink1 = "sl1"
  let symlink2 = "sl2"
  let backup = "sl2~"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, symlink1)?
  absolute_link(s, FILE_NAME, symlink2)?
  let r = uu.invoke(s, "cp", [option, symlink1, symlink2])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert FILE_NAME == resolve_link(s, symlink1)?
  assert uu.file_exists(s, symlink2)?
  assert uu.read_text(s, symlink2)? == CONTENTS
  assert FILE_NAME == resolve_link(s, backup)?
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_symlink_with_option_backup_and_link
test test_uu_cp_same_file_same_file_from_symlink_to_symlink_with_option_backup_and_link { |ctx|
  for option in ["-bl", "-bfl"] {
  let s = uu.scene(ctx)?
  let symlink1 = "sl1"
  let symlink2 = "sl2"
  let backup = "sl2~"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, symlink1)?
  absolute_link(s, FILE_NAME, symlink2)?
  let r = uu.invoke(s, "cp", [option, symlink1, symlink2])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert FILE_NAME == resolve_link(s, symlink1)?
  assert uu.file_exists(s, symlink2)?
  assert uu.read_text(s, symlink2)? == CONTENTS
  assert FILE_NAME == resolve_link(s, backup)?
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_symlink_with_option_backup_and_no_deref
test test_uu_cp_same_file_same_file_from_symlink_to_symlink_with_option_backup_and_no_deref { |ctx|
  for option in ["-bd", "-bdf"] {
  let s = uu.scene(ctx)?
  let symlink1 = "sl1"
  let symlink2 = "sl2"
  let backup = "sl2~"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, symlink1)?
  absolute_link(s, FILE_NAME, symlink2)?
  let r = uu.invoke(s, "cp", [option, symlink1, symlink2])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert FILE_NAME == resolve_link(s, symlink1)?
  assert FILE_NAME == resolve_link(s, symlink2)?
  assert FILE_NAME == resolve_link(s, backup)?
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_symlink_with_option_force
test test_uu_cp_same_file_same_file_from_symlink_to_symlink_with_option_force { |ctx|
  let s = uu.scene(ctx)?
  let symlink1 = "sl1"
  let symlink2 = "sl2"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, symlink1)?
  absolute_link(s, FILE_NAME, symlink2)?
  let r = uu.invoke(s, "cp", ["-f", symlink1, symlink2])?
  uu.fails(r)
  uu.stderr_contains(r, "'sl1' and 'sl2' are the same file")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert FILE_NAME == resolve_link(s, symlink1)?
  assert FILE_NAME == resolve_link(s, symlink2)?
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_symlink_with_option_force_link
test test_uu_cp_same_file_same_file_from_symlink_to_symlink_with_option_force_link { |ctx|
  let s = uu.scene(ctx)?
  let symlink1 = "sl1"
  let symlink2 = "sl2"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, symlink1)?
  absolute_link(s, FILE_NAME, symlink2)?
  let r = uu.invoke(s, "cp", ["-fl", symlink1, symlink2])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert FILE_NAME == resolve_link(s, symlink1)?
  assert uu.file_exists(s, symlink2)?
  assert uu.read_text(s, symlink2)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_symlink_with_option_link
test test_uu_cp_same_file_same_file_from_symlink_to_symlink_with_option_link { |ctx|
  let s = uu.scene(ctx)?
  let symlink1 = "sl1"
  let symlink2 = "sl2"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, symlink1)?
  absolute_link(s, FILE_NAME, symlink2)?
  let r = uu.invoke(s, "cp", ["-l", symlink1, symlink2])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot create hard link 'sl2' to 'sl1'")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert FILE_NAME == resolve_link(s, symlink1)?
  assert FILE_NAME == resolve_link(s, symlink2)?
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_symlink_with_option_no_deref
test test_uu_cp_same_file_same_file_from_symlink_to_symlink_with_option_no_deref { |ctx|
  for option in ["-d", "-df"] {
  let s = uu.scene(ctx)?
  let symlink1 = "sl1"
  let symlink2 = "sl2"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, symlink1)?
  absolute_link(s, FILE_NAME, symlink2)?
  let r = uu.invoke(s, "cp", [option, symlink1, symlink2])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert FILE_NAME == resolve_link(s, symlink1)?
  assert FILE_NAME == resolve_link(s, symlink2)?
  }
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_symlink_with_option_rem
test test_uu_cp_same_file_same_file_from_symlink_to_symlink_with_option_rem { |ctx|
  let s = uu.scene(ctx)?
  let symlink1 = "sl1"
  let symlink2 = "sl2"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, symlink1)?
  absolute_link(s, FILE_NAME, symlink2)?
  let r = uu.invoke(s, "cp", ["--rem", symlink1, symlink2])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert FILE_NAME == resolve_link(s, symlink1)?
  assert uu.file_exists(s, symlink2)?
  assert uu.read_text(s, symlink2)? == CONTENTS
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_symlink_with_option_symlink
test test_uu_cp_same_file_same_file_from_symlink_to_symlink_with_option_symlink { |ctx|
  let s = uu.scene(ctx)?
  let symlink1 = "sl1"
  let symlink2 = "sl2"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, symlink1)?
  absolute_link(s, FILE_NAME, symlink2)?
  let r = uu.invoke(s, "cp", ["-s", symlink1, symlink2])?
  uu.fails(r)
  uu.stderr_contains(r, "cannot create symbolic link 'sl2' to 'sl1'")
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert FILE_NAME == resolve_link(s, symlink1)?
  assert FILE_NAME == resolve_link(s, symlink2)?
}

# origin: uutils test_cp::same_file::test_same_file_from_symlink_to_symlink_with_option_symlink_and_force
test test_uu_cp_same_file_same_file_from_symlink_to_symlink_with_option_symlink_and_force { |ctx|
  let s = uu.scene(ctx)?
  let symlink1 = "sl1"
  let symlink2 = "sl2"
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, symlink1)?
  absolute_link(s, FILE_NAME, symlink2)?
  let r = uu.invoke(s, "cp", ["-sf", symlink1, symlink2])?
  uu.succeeds(r)
  assert uu.file_exists(s, FILE_NAME)?
  assert uu.read_text(s, FILE_NAME)? == CONTENTS
  assert FILE_NAME == resolve_link(s, symlink1)?
  assert symlink1 == resolve_link(s, symlink2)?
}

# origin: uutils test_cp::same_file::test_same_symlink_to_itself_no_dereference
test test_uu_cp_same_file_same_symlink_to_itself_no_dereference { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, FILE_NAME, CONTENTS)?
  absolute_link(s, FILE_NAME, SYMLINK_NAME)?
  let r = uu.invoke(s, "cp", ["-P", SYMLINK_NAME, SYMLINK_NAME])?
  uu.fails(r)
  uu.stderr_contains(r, "are the same file")
}

# origin: uutils test_cp::link_deref::test_cp_dang_link_as_source_with_link
test test_uu_cp_link_deref_cp_dang_link_as_source_with_link { |ctx|
  for option in ["", "-L", "-H"] {
  for recursive in [false, true] {
  let s = uu.scene(ctx)?
  setup_link(s, "dang_link")?
  let args = ["--link", "dang_link", "dst"]
  let recursive_args = if recursive { args.extend(["-R"]) } else { args }
  let invocation_args = if option != "" { recursive_args.extend([option]) } else { recursive_args }
  let r = uu.invoke(s, "cp", invocation_args)?
  uu.fails(r)
  uu.stderr_contains(r, "No such file or directory")
  }
  }
}

# origin: uutils test_cp::link_deref::test_cp_dir_link_as_source_with_link
test test_uu_cp_link_deref_cp_dir_link_as_source_with_link { |ctx|
  for option in ["", "-L", "-H"] {
  let s = uu.scene(ctx)?
  setup_link(s, "dir_link")?
  let args = ["--link", "dir_link", "dst"]
  let invocation_args = if option != "" { args.extend([option]) } else { args }
  let r = uu.invoke(s, "cp", invocation_args)?
  uu.fails(r)
  uu.stderr_contains(r, "cp: -r not specified; omitting directory")
  }
}

# origin: uutils test_cp::link_deref::test_cp_dir_link_as_source_with_link_and_r
test test_uu_cp_link_deref_cp_dir_link_as_source_with_link_and_r { |ctx|
  for option in ["", "-L", "-H"] {
  let s = uu.scene(ctx)?
  setup_link(s, "dir_link")?
  let args = ["--link", "-R", "dir_link", "dst"]
  let invocation_args = if option != "" { args.extend([option]) } else { args }
  let r = uu.invoke(s, "cp", invocation_args)?
  uu.succeeds(r)
  }
}

# origin: uutils test_cp::link_deref::test_cp_file_link_as_source_with_link
test test_uu_cp_link_deref_cp_file_link_as_source_with_link { |ctx|
  for option in ["", "-L", "-H"] {
  for recursive in [false, true] {
  let s = uu.scene(ctx)?
  setup_link(s, "file_link")?
  let args = ["--link", "-R", "file_link", "dst"]
  let option_args = if option != "" { args.extend([option]) } else { args }
  let invocation_args = if recursive { option_args.extend(["-R"]) } else { option_args }
  let r = uu.invoke(s, "cp", invocation_args)?
  uu.succeeds(r)
  assert fs.stat(uu.at(s, "file"))?.ino == fs.stat(uu.at(s, "dst"))?.ino
  }
  }
}

# origin: uutils test_cp::link_deref::test_cp_symlink_as_source_with_link_and_no_deref
test test_uu_cp_link_deref_cp_symlink_as_source_with_link_and_no_deref { |ctx|
  for src in ["file_link", "dir_link", "dang_link"] {
    for recursive in [false, true] {
      let s = uu.scene(ctx)?
      setup_link(s, src)?
      let args = if recursive { ["--link", "-P", src, "dst", "-R"] } else { ["--link", "-P", src, "dst"] }
      let r = uu.invoke(s, "cp", args)?
      uu.succeeds(r)
      uu.no_stderr(r)
      assert fs.stat(uu.at(s, src))?.ino == fs.stat(uu.at(s, "dst"))?.ino
    }
  }
}

# origin: uutils test_cp::test_abuse_existing
test test_uu_cp_abuse_existing { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b")?
  uu.mkdir(s, "c")?
  uu.symlink(s, "../t", "a/1")?
  uu.write(s, "b/1", "hello")?
  uu.symlink(s, "../t", "c/1")?
  uu.write(s, "t", "i")?
  let r = uu.invoke(s, "cp", ["-dR", "a/1", "b/1", "c"])?
  uu.fails(r)
  uu.stderr_contains(r, "will not copy 'b/1' through just-created symlink 'c/1'")
  uu.file_is(s, "t", "i")
}

# origin: uutils test_cp::test_acl_preserve
test test_uu_cp_acl_preserve { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b")?
  uu.touch(s, "a/file")?
  let r = match uu.invoke(s, "setfacl", ["-m", "group::rwx", uu.at(s, "a").display()]) {
    Ok(result) => result,
    Err(_) => { test.skip("setfacl unavailable"); return },
  }
  if r.status != 0 { test.skip("setfacl failed"); return }
  let second = uu.invoke(s, "cp", ["-p", uu.at(s, "a/file").display(), "b"])?
  uu.succeeds(second)
  let names1 = fs.xattr_list(uu.at(s, "a/file"))?
  let names2 = fs.xattr_list(uu.at(s, "b/file"))?
  assert names1.len() == names2.len()
  for name in names1 { assert name in names2 }
}

# origin: uutils test_cp::test_canonicalize_symlink
test test_uu_cp_canonicalize_symlink { |ctx|
    let s = uu.scene(ctx)?
    uu.mkdir(s, "dir")?
    uu.touch(s, "dir/file")?
    uu.symlink(s, "../dir/file", "dir/file-ln")?
    let r = uu.invoke(s, "cp", ["dir/file-ln", "."])?
    uu.succeeds(r)
    uu.no_output(r)
}

# origin: uutils test_cp::test_copy_contents_fifo
test test_uu_cp_copy_contents_fifo { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  let plan = uu.command(s, "cp", ["--copy-contents", "fifo", "outfile"], timeout: 10s)?
  let child = spawn plan?
  uu.write(s, "fifo", "foo")?
  let status = wait child?
  assert status.exit_code()? == 0
  assert uu.read(s, ".uu-stdout")? == b""
  assert uu.read(s, ".uu-stderr")? == b""
  uu.file_is(s, "outfile", "foo")
}

# origin: uutils test_cp::test_copy_dir_symlink
test test_uu_cp_copy_dir_symlink { |ctx|
    let s = uu.scene(ctx)?
    uu.mkdir(s, "dir")?
    absolute_link(s, "dir", "dir-link")?
    let r = uu.invoke(s, "cp", ["-r", "dir-link", "copy"])?
    uu.succeeds(r)
    assert resolve_link(s, "copy")? == "dir"
}

# origin: uutils test_cp::test_copy_dir_with_symlinks
test test_uu_cp_copy_dir_with_symlinks { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.touch(s, "dir/file")?
  let r = uu.invoke(s, "ln", ["-sr", uu.at(s, "dir/file").display(), uu.at(s, "dir/file-link").display()])?
  uu.succeeds(r)
  let second = uu.invoke(s, "cp", ["-r", "dir", "copy"])?
  uu.succeeds(second)
  assert uu.read_link(s, "copy/file-link")? == "file"
}

# origin: uutils test_cp::test_copy_directory_to_itself_disallowed
test test_uu_cp_copy_directory_to_itself_disallowed { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  let r = uu.invoke(s, "cp", ["-R", "d", "d"])?
  uu.fails(r)
  uu.stderr_only(r, "cp: cannot copy a directory, 'd', into itself, 'd/d'\n")
}

# origin: uutils test_cp::test_copy_nested_directory_to_itself_disallowed
test test_uu_cp_copy_nested_directory_to_itself_disallowed { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "a/b")?
  uu.mkdir(s, "a/b/c")?
  let r = uu.invoke(s, "cp", ["-R", "a/b", "a/b/c"])?
  uu.fails(r)
  uu.stderr_only(r, "cp: cannot copy a directory, 'a/b', into itself, 'a/b/c/b'\n")
}

# origin: uutils test_cp::test_copy_no_dereference_1
test test_uu_cp_copy_no_dereference_1 { |ctx|
    let s = uu.scene(ctx)?
    uu.mkdir(s, "a")?
    uu.mkdir(s, "b")?
    uu.write(s, "a/foo", "bar")?
    uu.symlink(s, "../a/foo", "b/foo")?
    let r = uu.invoke(s, "cp", ["-P", "a/foo", "b"])?
    uu.fails(r)
}

# origin: uutils test_cp::test_copy_same_symlink_no_dereference
test test_uu_cp_copy_same_symlink_no_dereference { |ctx|
    let s = uu.scene(ctx)?
    uu.symlink(s, "t", "a")?
    uu.symlink(s, "t", "b")?
    uu.touch(s, "t")?
    let r = uu.invoke(s, "cp", ["-d", "a", "b"])?
    uu.succeeds(r)
}

# origin: uutils test_cp::test_copy_same_symlink_no_dereference_dangling
test test_uu_cp_copy_same_symlink_no_dereference_dangling { |ctx|
    let s = uu.scene(ctx)?
    uu.symlink(s, "t", "a")?
    uu.symlink(s, "t", "b")?
    let r = uu.invoke(s, "cp", ["-d", "a", "b"])?
    uu.succeeds(r)
}

# origin: uutils test_cp::test_copy_symlink_force
test test_uu_cp_copy_symlink_force { |ctx|
    let s = uu.scene(ctx)?
    uu.touch(s, "file")?
    absolute_link(s, "file", "file-link")?
    uu.touch(s, "copy")?
    let r = uu.invoke(s, "cp", ["file-link", "copy", "-f", "--no-dereference"])?
    uu.succeeds(r)
    assert resolve_link(s, "copy")? == "file"
}

# origin: uutils test_cp::test_copy_symlink_overwrite
test test_uu_cp_copy_symlink_overwrite { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  uu.mkdir(s, "b")?
  uu.mkdir(s, "c")?
  uu.write(s, "t", "hello")?
  uu.symlink(s, "../t", "a/1")?
  uu.symlink(s, "../t", "b/1")?
  let r = uu.invoke(s, "cp", ["--no-dereference", "a/1", "b/1", "c"])?
  uu.fails(r)
  uu.stderr_only(r, "cp: will not overwrite just-created 'c/1' with 'b/1'\n")
}

# origin: uutils test_cp::test_copy_through_dangling_symlink
test test_uu_cp_copy_through_dangling_symlink { |ctx|
    let s = uu.scene(ctx)?
    uu.touch(s, "file")?
    absolute_link(s, "nonexistent", "target")?
    let r = uu.invoke(s, "cp", ["file", "target"])?
    uu.fails(r)
    uu.stderr_only(r, "cp: not writing through dangling symlink 'target'\n")
}

# origin: uutils test_cp::test_copy_through_dangling_symlink_force
test test_uu_cp_copy_through_dangling_symlink_force { |ctx|
    let s = uu.scene(ctx)?
    uu.touch(s, "src")?
    absolute_link(s, "no-such-file", "dest")?
    let r = uu.invoke(s, "cp", ["--force", "src", "dest"])?
    uu.fails(r)
    uu.stderr_only(r, "cp: not writing through dangling symlink 'dest'\n")
    assert !file_exists(s, "dest")?
}

# origin: uutils test_cp::test_copy_through_dangling_symlink_no_dereference
test test_uu_cp_copy_through_dangling_symlink_no_dereference { |ctx|
    let s = uu.scene(ctx)?
    absolute_link(s, "no-such-file", "dangle")?
    let r = uu.invoke(s, "cp", ["-P", "dangle", "d2"])?
    uu.succeeds(r)
    uu.no_output(r)
}

# origin: uutils test_cp::test_copy_through_dangling_symlink_no_dereference_2
test test_uu_cp_copy_through_dangling_symlink_no_dereference_2 { |ctx|
    let s = uu.scene(ctx)?
    uu.touch(s, "file")?
    absolute_link(s, "nonexistent", "target")?
    let r = uu.invoke(s, "cp", ["-P", "file", "target"])?
    uu.fails(r)
    uu.stderr_only(r, "cp: not writing through dangling symlink 'target'\n")
}

# origin: uutils test_cp::test_copy_through_dangling_symlink_no_dereference_permissions
test test_uu_cp_copy_through_dangling_symlink_no_dereference_permissions { |ctx|
  let s = uu.scene(ctx)?
  absolute_link(s, "no-such-file", "dangle")?
  time.sleep(100ms)
  # Reading the source link advances its access time; preserve the pre-copy value.
  let original = fs.stat(uu.at(s, "dangle"))?
  let r = uu.invoke(s, "cp", ["-P", "-p", "dangle", "d2"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.is_symlink(s, "d2")?
  let metadata1 = fs.stat(uu.at(s, "dangle"))?
  let metadata2 = fs.stat(uu.at(s, "d2"))?
  assert metadata1.mode == metadata2.mode
  assert metadata1.uid == metadata2.uid
  assert original.atime_ns == metadata2.atime_ns
  assert metadata1.mtime_ns == metadata2.mtime_ns
}

# origin: uutils test_cp::test_copy_through_dangling_symlink_posixly_correct
test test_uu_cp_copy_through_dangling_symlink_posixly_correct { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.write(s, "file", "content")?
  absolute_link(s, "nonexistent", "target")?
  let r = uu.invoke(s, "cp", ["file", "target"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r)
  assert uu.file_exists(s, "nonexistent")?
  uu.file_is(s, "nonexistent", "content")
}

# origin: uutils test_cp::test_copy_through_just_created_symlink
test test_uu_cp_copy_through_just_created_symlink { |ctx|
  for create_t in [true, false] {
    let s = uu.scene(ctx)?
    uu.mkdir(s, "a")?
    uu.mkdir(s, "b")?
    uu.mkdir(s, "c")?
    uu.symlink(s, "../t", "a/1")?
    uu.write(s, "b/1", "hello")?
    if create_t { uu.write(s, "t", "world")? }
    let r = uu.invoke(s, "cp", ["--no-dereference", "a/1", "b/1", "c"])?
    uu.fails(r)
    uu.stderr_only(r, "cp: will not copy 'b/1' through just-created symlink 'c/1'\n")
    if create_t { uu.file_is(s, "a/1", "world") }
  }
}

# origin: uutils test_cp::test_cp_archive
test test_uu_cp_cp_archive { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cp", "hello_world.txt", "hello_world.txt")?
  uu.fixture(s, "cp", "how_are_you.txt", "how_are_you.txt")?
  let previous = time.now() * 1000000 - 3600 * 1000000000
  fs.set_times(uu.at(s, "hello_world.txt"), atime_ns: previous, mtime_ns: previous)?
  let r = uu.invoke(s, "cp", ["hello_world.txt", "--archive", "how_are_you.txt"])?
  uu.succeeds(r)
  uu.file_is(s, "how_are_you.txt", "Hello, World!\n")
  let creation = fs.stat(uu.at(s, "hello_world.txt"))?.mtime_ns
  let creation2 = fs.stat(uu.at(s, "how_are_you.txt"))?.mtime_ns
  let second = uu.invoke(s, "ls", ["-al", s.root.display()])?
  uu.succeeds(second)
  assert creation == creation2
}

# origin: uutils test_cp::test_cp_archive_cli_deref_inner_preserved
test test_uu_cp_cp_archive_cli_deref_inner_preserved { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "srcdir")?
  uu.touch(s, "srcdir/real.txt")?
  uu.symlink(s, "real.txt", "srcdir/link.txt")?
  let r = uu.invoke(s, "cp", ["-aH", "srcdir", "dest_aH"])?
  uu.succeeds(r)
  assert uu.is_symlink(s, "dest_aH/link.txt")?
  let second = uu.invoke(s, "cp", ["-Ha", "srcdir", "dest_Ha"])?
  uu.succeeds(second)
  assert uu.is_symlink(s, "dest_Ha/link.txt")?
}
