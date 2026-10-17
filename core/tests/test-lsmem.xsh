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
  let archive = fp"{ctx.core_dir}/tests/fixtures/util-linux/memory/upstream/{name}.tar.bz2"
  uu.succeeds(uu.invoke(s, "tar", ["-C", s.root.display(), "-jxf", archive.display()])?)
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
  for value in ["force", "Y", "N"] {
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

test test_lsmem_rooted_missing_block_size_is_an_error { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "missing-block-size")?
  compare_lsmem_rooted(s, [])?
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
    ["--raw", "--summary=only"],
    ["--summary=invalid"],
    ["--output", "NOT-A-COLUMN"],
    ["--split", "NOT-A-COLUMN"],
    ["--unknown-option"],
    ["unexpected-operand"],
  ]
  for args in cases { compare_lsmem_rooted(s, args)? }
}
