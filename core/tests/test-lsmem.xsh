use support.uu
use support.util_linux as ul

proc install_lsmem_fixture(s: uu.Scene, name: Str) [fs, error] -> Result[Unit, Error] {
  let source = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/memory/{name}"
  let _ = fs.copy_tree(source, s.root, overwrite: true, follow_symlinks: false)?
  Ok()
}

proc compare_lsmem(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference(s, "lsmem", args, timeout: 5s, vars: vars)?
  let candidate = uu.invoke(s, "lsmem", args, timeout: 5s, vars: vars)?
  ul.compare(reference, candidate)
  Ok()
}

proc compare_lsmem_rooted(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[Unit, Error] {
  compare_lsmem(s, args.extend(["--sysroot", s.root.display()]), vars)
}

proc compare_lsmem_upstream_dump(ctx: TestContext, name: Str) [fs, process, env, error] -> Result[Unit, Error] {
  let s = uu.scene(ctx)?
  let dump_archive = fp"{ctx.core_dir}/tests/fixtures/util-linux/memory/upstream/{name}.tar.bz2"
  uu.succeeds(uu.invoke(s, "tar", ["-C", s.root.display(), "-jxf", dump_archive.display()])?)
  let dump_root = fp"{s.root}/{name}".display()
  let columns = "RANGE,SIZE,STATE,REMOVABLE,BLOCK,NODE,ZONES"
  let cases = [
    ["--split=STATE,REMOVABLE"],
    ["--output", "RANGE,SIZE", "--split", "none"],
    ["--output", "RANGE,SIZE,STATE", "--split", "STATE"],
    ["--all", "--output", columns],
    ["--raw", "--output", columns, "--split", columns],
    ["--json", "--output", columns, "--split", columns],
    ["-o", "+ZONES"],
    ["--output-all"],
    [],
  ]
  for options in cases {
    compare_lsmem(s, options.extend(["--sysroot", dump_root]))?
  }
  Ok()
}

test test_lsmem_util_linux_2_42_4_upstream_s390_zvm_6g { |ctx|
  compare_lsmem_upstream_dump(ctx, "s390-zvm-6g")?
}

test test_lsmem_util_linux_2_42_4_upstream_x86_64_16g { |ctx|
  compare_lsmem_upstream_dump(ctx, "x86_64-16g")?
}

test test_lsmem_util_linux_2_42_4_version_identity { |ctx|
  let s = uu.scene(ctx)?
  for args in [["--version"], ["-V"]] {
    let reference = ul.reference(s, "lsmem", args, timeout: 5s)?
    let candidate = uu.invoke(s, "lsmem", args, timeout: 5s)?
    ul.version_identity(reference, candidate)
  }
}

test test_lsmem_util_linux_2_42_4_help_matches_reference { |ctx|
  let s = uu.scene(ctx)?
  for args in [["--help"], ["-h"]] { compare_lsmem(s, args)? }
}

test test_lsmem_rooted_sparse_memory_attributes_and_output_modes { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let columns = "RANGE,SIZE,STATE,REMOVABLE,BLOCK,NODE,ZONES"
  let cases = [
    [],
    ["--all", "--output", columns],
    ["--output", "RANGE,SIZE", "--split", "none"],
    ["--output", "RANGE,SIZE,STATE", "--split", "STATE"],
    ["--output", "BLOCK,STATE", "--split", "STATE"],
    ["--output", "BLOCK,NODE", "--split", "NODE"],
    ["--output", "BLOCK,ZONES", "--split", "ZONES"],
    ["--raw", "--output", columns, "--split", columns],
    ["--pairs", "--bytes", "--output", columns],
    ["--json", "--bytes", "--output", columns],
    ["-o", "+ZONES"],
    ["--output-all"],
    ["--noheadings", "--bytes"],
    ["--summary=never"],
    ["--summary=always"],
    ["--summary=only"],
    ["--summary"],
  ]
  for args in cases { compare_lsmem_rooted(s, args)? }
}

test test_lsmem_rooted_columns_environment_and_explicit_output_precedence { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  compare_lsmem_rooted(s, [], {LSMEM_COLUMNS: "RANGE,SIZE,STATE"})?
  compare_lsmem_rooted(s, ["--output", "RANGE,BLOCK"], {LSMEM_COLUMNS: "NODE,ZONES"})?
  compare_lsmem_rooted(s, ["--output-all"], {LSMEM_COLUMNS: "NODE"})?
  compare_lsmem_rooted(s, [], {LSMEM_COLUMNS: "NOT-A-COLUMN"})?
}

test test_lsmem_rooted_short_option_aliases { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let columns = "RANGE,SIZE,STATE,REMOVABLE,BLOCK,NODE,ZONES"
  let cases = [
    ["-a", "-o", columns],
    ["-b", "-n", "-o", "RANGE,SIZE,BLOCK"],
    ["-J", "-o", columns],
    ["-P", "-o", columns],
    ["-r", "-o", columns],
    ["-S", "none", "-o", "RANGE,BLOCK"],
  ]
  for args in cases { compare_lsmem_rooted(s, args)? }
  compare_lsmem(s, ["-s", s.root.display(), "-o", columns])?
}

test test_lsmem_rooted_configured_memory_and_memmap_summary { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "configured")?
  compare_lsmem_rooted(s, ["--output-all"])?
  compare_lsmem_rooted(s, ["--json", "--bytes", "--output-all"])?
  compare_lsmem_rooted(s, ["--pairs", "--output-all"])?
  compare_lsmem_rooted(s, ["--raw", "--output-all"])?
  compare_lsmem_rooted(s, ["--output", "BLOCK,CONFIGURED,MEMMAP-ON-MEMORY", "--split", "CONFIGURED,MEMMAP-ON-MEMORY"])?
  for value in ["force", "Y", "N", "Y-extra", "Nonsense", "forceful", "unknown", " Y", " N", " force"] {
    uu.write(s, "sys/module/memory_hotplug/parameters/memmap_on_memory", f"{value}\n")?
    compare_lsmem_rooted(s, ["--bytes", "--summary=only"])?
  }
}

test test_lsmem_rooted_malformed_block_attributes { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "malformed-block-size")?
  compare_lsmem_rooted(s, ["--all", "--output-all"])?
  compare_lsmem_rooted(s, ["--bytes", "--summary=only"])?
}

test test_lsmem_rooted_numeric_and_state_parsing_matches_reference { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let block_size = "sys/devices/system/memory/block_size_bytes"
  for value in ["0\n", "1000\n", "0x1000 trailing\n", "+1000\n", "-1\n", "ffffffffffffffff\n", "10000000000000000\n", "not-hex\n"] {
    uu.write(s, block_size, value)?
    compare_lsmem_rooted(s, ["--bytes", "--output", "BLOCK,SIZE", "--summary=never"])?
  }
  let state = "sys/devices/system/memory/memory0/state"
  for value in ["online\n", " online\n", "online \n", "not-a-kernel-memory-state\n"] {
    uu.write(s, state, value)?
    compare_lsmem_rooted(s, ["--all", "--output", "BLOCK,STATE"])?
  }
  uu.write(s, state, "online\n")?
  let removable = "sys/devices/system/memory/memory0/removable"
  for value in ["1\n", "+1\n", "1trailing\n", "not-a-number\n"] {
    uu.write(s, removable, value)?
    compare_lsmem_rooted(s, ["--all", "--output", "BLOCK,REMOVABLE"])?
  }
}

test test_lsmem_rooted_zone_tokenization_and_invalid_bytes_match_reference { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let zones = "sys/devices/system/memory/memory0/valid_zones"
  for value in ["DMA Normal\n", "DMA  Normal\n", "DMA\tNormal\n", "\tDMA\n", "dma Normal\n"] {
    uu.write(s, zones, value)?
    compare_lsmem_rooted(s, ["--all", "--output", "BLOCK,ZONES"])?
  }
  uu.write_bytes(s, zones, b"DMA \xff\n")?
  compare_lsmem_rooted(s, ["--all", "--output", "BLOCK,ZONES"])?
}

test test_lsmem_rooted_dangling_and_cyclic_attribute_symlinks_match_reference { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let state = uu.at(s, "sys/devices/system/memory/memory1/state")
  state.remove()?
  state.symlink(to: uu.at(s, "missing-state"))?
  compare_lsmem_rooted(s, ["--all", "--output", "BLOCK,STATE"])?
  state.remove()?
  state.symlink(to: state)?
  compare_lsmem_rooted(s, ["--all", "--output", "BLOCK,STATE"])?
  state.remove()?
  state.mkdir()
  compare_lsmem_rooted(s, ["--all", "--output", "BLOCK,STATE"])?
  state.remove()?
  state.write("online\n")
  let removable = uu.at(s, "sys/devices/system/memory/memory1/removable")
  removable.remove()?
  removable.symlink(to: removable)?
  compare_lsmem_rooted(s, ["--all", "--output", "BLOCK,REMOVABLE"])?
  removable.remove()?
  removable.mkdir()
  compare_lsmem_rooted(s, ["--all", "--output", "BLOCK,REMOVABLE"])?
  let zones = uu.at(s, "sys/devices/system/memory/memory1/valid_zones")
  zones.remove()?
  zones.symlink(to: zones)?
  compare_lsmem_rooted(s, ["--all", "--output", "BLOCK,ZONES"])?
  zones.remove()?
  zones.mkdir()
  compare_lsmem_rooted(s, ["--all", "--output", "BLOCK,ZONES"])?
}

test test_lsmem_rooted_required_block_size_wrong_types_match_reference { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let block_size = uu.at(s, "sys/devices/system/memory/block_size_bytes")
  block_size.remove()?
  block_size.mkdir()?
  compare_lsmem_rooted(s, [])?
  compare_lsmem_rooted(s, ["--summary=only"])?
  block_size.remove()?
  block_size.symlink(to: uu.at(s, "missing-block-size"))?
  compare_lsmem_rooted(s, [])?
  compare_lsmem_rooted(s, ["--summary=only"])?
}

test test_lsmem_rooted_firmware_inventory_can_differ_from_sysfs_blocks { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "configured")?
  uu.mkdir(s, "sys/firmware/memory/memory12")?
  uu.write(s, "sys/firmware/memory/memory12/config", "1\n")?
  uu.write(s, "sys/firmware/memory/memory12/memmap_on_memory", "1\n")?
  compare_lsmem_rooted(s, ["--all", "--output-all"])?
  compare_lsmem_rooted(s, ["--json", "--bytes", "--output-all"])?
  compare_lsmem_rooted(s, ["--summary=only"])?
}

test test_lsmem_rooted_firmware_integer_parsing_matches_reference { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "configured")?
  let columns = "BLOCK,CONFIGURED,MEMMAP-ON-MEMORY"
  let config = "sys/firmware/memory/memory0/config"
  let memmap = "sys/firmware/memory/memory0/memmap_on_memory"
  for value in ["1\n", "+1\n", "1 trailing\n", "not-a-number\n"] {
    uu.write(s, config, value)?
    compare_lsmem_rooted(s, ["--all", "--output", columns])?
  }
  uu.write(s, config, "1\n")?
  for value in ["1\n", "+1\n", "1 trailing\n", "not-a-number\n"] {
    uu.write(s, memmap, value)?
    compare_lsmem_rooted(s, ["--all", "--output", columns])?
  }
}

test test_lsmem_rooted_leading_zero_block_ids_follow_reference_order { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  uu.mkdir(s, "sys/devices/system/memory/memory01")?
  uu.write(s, "sys/devices/system/memory/memory01/state", "online\n")?
  uu.write(s, "sys/devices/system/memory/memory01/removable", "1\n")?
  compare_lsmem_rooted(s, ["--all", "--output", "RANGE,BLOCK,STATE,REMOVABLE"])?
}

test test_lsmem_rooted_missing_block_size_is_an_error { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "missing-block-size")?
  compare_lsmem_rooted(s, [])?
}

test test_lsmem_rooted_nonnumeric_memory_entries_match_reference { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "no-numeric-blocks")?
  compare_lsmem_rooted(s, [])?
  compare_lsmem_rooted(s, ["--summary=only"])?
}

test test_lsmem_rooted_missing_sysfs_tree_does_not_use_host_memory { |ctx|
  let s = uu.scene(ctx)?
  compare_lsmem_rooted(s, [])?
}

test test_lsmem_rooted_option_errors_match_reference { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let cases = [
    ["--all", "--split=STATE"],
    ["--json", "--pairs"],
    ["--json", "--raw"],
    ["--pairs", "--raw"],
    ["--json", "--summary=only"],
    ["--pairs", "--summary=only"],
    ["--raw", "--summary=only"],
    ["--summary=invalid"],
    ["--output", "NOT-A-COLUMN"],
    ["--split", "NOT-A-COLUMN"],
    ["--unknown-option"],
    ["unexpected-operand"],
  ]
  for args in cases { compare_lsmem_rooted(s, args)? }
}
