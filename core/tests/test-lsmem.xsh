use support.uu
use support.util_linux as ul

proc install_lsmem_fixture(s: uu.Scene, name: Str) [fs, error] -> Result[Unit, Error] {
  let source = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/memory/{name}"
  let _ = fs.copy_tree(source, s.root, overwrite: true, follow_symlinks: false)?
  Ok()
}

proc print_lsmem_bytes(label: Str, value: Bytes) [io] {
  print $label
  let encoded = value.base64()
  var offset = 0
  while offset < encoded.byte_len() {
    let remaining = encoded.byte_len() - offset
    let length = if remaining > 100 { 100 } else { remaining }
    let chunk = encoded.byte_slice(offset, length: length)
    print $chunk
    offset += length
  }
}

proc compare_lsmem(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, io, error] -> Result[Unit, Error] {
  let reference = ul.reference(s, "lsmem", args, timeout: 5s, vars: vars)?
  let candidate = uu.invoke(s, "lsmem", args, timeout: 5s, vars: vars)?
  assert reference.util == candidate.util
  assert reference.args == candidate.args
  assert reference.status == candidate.status, f"lsmem status differs: reference={reference.status} stderr_b64={reference.stderr.base64()} candidate={candidate.status} stderr_b64={candidate.stderr.base64()}"
  if reference.stdout != candidate.stdout {
    if reference.stdout.len() <= 1024 and candidate.stdout.len() <= 1024 {
      print_lsmem_bytes("reference stdout base64", reference.stdout)
      print_lsmem_bytes("candidate stdout base64", candidate.stdout)
    }
    var offset = 0
    let limit = if reference.stdout.len() < candidate.stdout.len() { reference.stdout.len() } else { candidate.stdout.len() }
    while offset < limit and reference.stdout.slice(offset, length: 1) == candidate.stdout.slice(offset, length: 1) { offset += 1 }
    let context_start = if offset > 24 { offset - 24 } else { 0 }
    let reference_remaining = reference.stdout.len() - context_start
    let candidate_remaining = candidate.stdout.len() - context_start
    let reference_context_len = if reference_remaining > 48 { 48 } else { reference_remaining }
    let candidate_context_len = if candidate_remaining > 48 { 48 } else { candidate_remaining }
    assert false, f"lsmem stdout differs for {args.join(" ")} at byte {offset}; reference length {reference.stdout.len()} context {reference.stdout.slice(context_start, length: reference_context_len).base64()}; candidate length {candidate.stdout.len()} context {candidate.stdout.slice(context_start, length: candidate_context_len).base64()}"
  }
  if reference.stderr != candidate.stderr {
    assert false, f"lsmem stderr differs for {args.join(" ")}; reference length {reference.stderr.len()} base64 {reference.stderr.base64()}; candidate length {candidate.stderr.len()} base64 {candidate.stderr.base64()}"
  }
  ul.compare(reference, candidate)
  Ok()
}

proc compare_lsmem_rooted(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, io, error] -> Result[Unit, Error] {
  compare_lsmem(s, args.extend(["--sysroot", s.root.display()]), vars)
}

proc compare_lsmem_paths(s: uu.Scene, args: List[Path]) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference_paths(s, "lsmem", args, timeout: 5s)?
  let candidate = uu.invoke_paths(s, "lsmem", args, timeout: 5s)?
  assert reference.status == 0, reference.stderr.utf8() ?? "reference stderr is not UTF-8"
  assert candidate.status == 0, candidate.stderr.utf8() ?? "candidate stderr is not UTF-8"
  assert reference.stderr == b"" and candidate.stderr == b""
  assert ! reference.stdout.is_empty()
  ul.compare(reference, candidate)
  Ok()
}

pure lsmem_zero_entry_suffix_is_one_line(stderr: Bytes, prefix_length: Int) -> Bool {
  if stderr.len() <= prefix_length + 1 or ! stderr.ends_with(b"\n") { return false }
  var offset = prefix_length
  while offset < stderr.len() - 1 {
    if (stderr.byte_at(offset) ?? 10) == 10 { return false }
    offset += 1
  }
  true
}

pure lsmem_zero_entry_error_matches(reference: uu.Ran, candidate: uu.Ran, args: List[Str], memory_path: Str) -> Bool {
  let prefix = bytes.from_text(f"lsmem: Failed to read {memory_path}: ")
  reference.util == "lsmem" and candidate.util == "lsmem" and
    reference.args == args and candidate.args == args and
    reference.status == 1 and candidate.status == 1 and
    reference.stdout == b"" and candidate.stdout == b"" and
    reference.stderr.starts_with(prefix) and
    lsmem_zero_entry_suffix_is_one_line(reference.stderr, prefix.len()) and
    candidate.stderr == bytes.concat([prefix, b"no numeric memory entries\n"])
}

proc compare_lsmem_zero_entry_rooted(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[Unit, Error] {
  let rooted_args = args.extend(["--sysroot", s.root.display()])
  let reference = ul.reference(s, "lsmem", rooted_args)?
  let candidate = uu.invoke(s, "lsmem", rooted_args)?
  let memory_path = f"{s.root}/sys/devices/system/memory"
  assert lsmem_zero_entry_error_matches(reference, candidate, rooted_args, memory_path), f"zero-entry error differs: reference={reference.stderr.base64()} candidate={candidate.stderr.base64()}"
  Ok()
}

proc compare_lsmem_upstream_dump(ctx: TestContext, name: Str) [fs, process, env, io, error] -> Result[Unit, Error] {
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

test test_lsmem_non_utf8_environment_columns { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  compare_lsmem_rooted(s, [], {LSMEM_COLUMNS: b"SIZE\xff" as Path})?
}

test test_lsmem_rooted_column_list_grammar { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  for option in ["-S", "-o"] {
    for value in ["", "+", "+SIZE", "+STATE,NODE", "+none", ",", "SIZE,", "+,SIZE", "+SIZE,", ",SIZE", "SIZE,,NODE"] {
      compare_lsmem_rooted(s, [option, value, "--summary=never"])?
    }
  }
  for value in ["", "+", "+SIZE", "SIZE,"] {
    compare_lsmem_rooted(s, ["--summary=never"], {LSMEM_COLUMNS: value})?
    compare_lsmem_rooted(s, ["-o", "SIZE", "--summary=never"], {LSMEM_COLUMNS: value})?
  }
}

test test_lsmem_rooted_all_columns_hidden { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  for args in [
    ["-o", "CONFIGURED"],
    ["-n", "-o", "CONFIGURED"],
    ["-o", "CONFIGURED,MEMMAP-ON-MEMORY", "--summary=never"],
    ["-J", "-o", "CONFIGURED"],
    ["-P", "-o", "CONFIGURED"],
    ["-r", "-o", "CONFIGURED"],
    ["-r", "-n", "-o", "CONFIGURED"],
  ] { compare_lsmem_rooted(s, args)? }
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

test test_lsmem_separate_values_after_clusters_and_prefixes { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  for args in [["-bo", "SIZE,STATE"], ["-ao", "SIZE"], ["-bS", "STATE"], ["--spl", "STATE"], ["--sysr", s.root.display()]] {
    compare_lsmem_rooted(s, args.extend(["--summary=never"]))?
  }
}

test test_lsmem_rooted_non_utf8_sysroot_arguments_match_reference { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let raw_root = uu.at_bytes(s, b"sysroot-\xff")?
  raw_root.mkdir()?
  let source = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/memory/sparse"
  let _ = fs.copy_tree(source, raw_root, overwrite: true, follow_symlinks: false)?
  let long_attached = Path.parse_bytes(bytes.concat([b"--sysroot=", raw_root.bytes()]))?
  let short_attached = Path.parse_bytes(bytes.concat([b"-s", raw_root.bytes()]))?
  let cases: List[List[Path]] = [
    [long_attached, p"--summary=only"],
    [short_attached, p"--summary=only"],
    [p"--sysroot", raw_root, p"--summary=only"],
    [p"--sysroot", s.root, p"--summary=only"],
  ]
  for args in cases { compare_lsmem_paths(s, args)? }
}

test test_lsmem_sysroot_with_absolute_sys_symlink { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  uu.mkdir(s, "linked-root")?
  uu.at(s, "linked-root/sys").symlink(to: uu.at(s, "sys"))?
  compare_lsmem(s, ["-s", uu.at(s, "linked-root").display(), "--summary=only"])?
}

test test_lsmem_sysroot_memory_symlink_escaping_the_root { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  uu.mkdir(s, "escaping-root/sys/devices/system")?
  uu.at(s, "escaping-root/sys/devices/system/memory").symlink(to: uu.at(s, "sys/devices/system/memory"))?
  for args in [[], ["--summary=only"]] {
    compare_lsmem(s, ["-s", uu.at(s, "escaping-root").display()].extend(args))?
  }
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

test test_lsmem_attribute_bytes_after_value { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  uu.write_bytes(s, "sys/devices/system/memory/memory0/state", b"online\x00garbage")?
  uu.write_bytes(s, "sys/devices/system/memory/memory0/removable", b"1\xff")?
  compare_lsmem_rooted(s, ["-a", "-o", "BLOCK,STATE,REMOVABLE"])?
}

test test_lsmem_wrapped_attribute_integers { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  uu.write(s, "sys/devices/system/memory/memory0/state", "online\n")?
  uu.write(s, "sys/devices/system/memory/memory0/removable", "4294967297\n")?
  compare_lsmem_rooted(s, ["-a", "-o", "BLOCK,REMOVABLE"])?
}

test test_lsmem_block_size_bare_prefix { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  for value in ["0x"] {
    uu.write(s, "sys/devices/system/memory/block_size_bytes", value)?
    compare_lsmem_rooted(s, ["--summary=only"])?
  }
}

test test_lsmem_empty_block_size_diagnostics { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  for value in ["", "\n"] {
    uu.write(s, "sys/devices/system/memory/block_size_bytes", value)?
    compare_lsmem_rooted(s, ["--summary=only"])?
  }
  uu.at(s, "sys/devices/system/memory/memory0/valid_zones").remove()?
  for value in ["", "\n"] {
    uu.write(s, "sys/devices/system/memory/block_size_bytes", value)?
    compare_lsmem_rooted(s, ["--summary=only"])?
  }
}

test test_lsmem_zones_read_limit { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let value = [" " for _ in range(1023)].join("") + "Normal\n"
  uu.write(s, "sys/devices/system/memory/memory0/valid_zones", value)?
  compare_lsmem_rooted(s, ["-a", "-o", "BLOCK,ZONES", "--summary=never"])?
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

test test_lsmem_permission_error_precedence { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let block = uu.at(s, "sys/devices/system/memory/memory1")
  block.chmod(0o000)?
  let block_result = compare_lsmem_rooted(s, ["-a"])
  block.chmod(0o755)?
  block_result?
  let size = uu.at(s, "sys/devices/system/memory/block_size_bytes")
  size.chmod(0o000)?
  let size_result = compare_lsmem_rooted(s, [])
  size.chmod(0o644)?
  size_result?
  let memory = uu.at(s, "sys/devices/system/memory")
  memory.chmod(0o311)?
  let memory_result = compare_lsmem_rooted(s, [])
  memory.chmod(0o755)?
  memory_result?
}

test test_lsmem_firmware_permission_errors { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "configured")?
  let firmware = uu.at(s, "sys/firmware/memory")
  firmware.chmod(0o311)?
  let firmware_result = compare_lsmem_rooted(s, ["-a", "--output-all"])
  firmware.chmod(0o755)?
  firmware_result?
  let block = uu.at(s, "sys/firmware/memory/memory0")
  block.chmod(0o000)?
  let block_result = compare_lsmem_rooted(s, ["-a", "--output-all"])
  block.chmod(0o755)?
  block_result?
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
  uu.write(s, "sys/devices/system/memory/memory1/state", "online\n")?
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

test test_lsmem_unsigned_block_indexes { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  for name in ["memory9223372036854775808", "memory18446744073709551615", "memory18446744073709551616"] {
    uu.mkdir(s, "sys/devices/system/memory/" + name)?
  }
  compare_lsmem_rooted(s, ["-a", "-o", "BLOCK,RANGE", "--summary=never"])?
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
  let args = ["--sysroot", "/fixture"]
  let memory_path = "/fixture/sys/devices/system/memory"
  let prefix = bytes.from_text(f"lsmem: Failed to read {memory_path}: ")
  let reference: uu.Ran = {util: "lsmem", args: args, status: 1, stdout: b"", stderr: bytes.concat([prefix, b"Not a tty\n"])}
  let candidate: uu.Ran = {util: "lsmem", args: args, status: 1, stdout: b"", stderr: bytes.concat([prefix, b"no numeric memory entries\n"])}
  assert lsmem_zero_entry_error_matches(reference, candidate, args, memory_path)
  assert ! ul.equal(reference, candidate)
  assert lsmem_zero_entry_error_matches({...reference, stderr: bytes.concat([prefix, b"reference errno changed\n"])}, candidate, args, memory_path)
  assert ! lsmem_zero_entry_error_matches(reference, {...candidate, stderr: bytes.concat([prefix, b"arbitrary candidate diagnostic\n"])}, args, memory_path)
  assert ! lsmem_zero_entry_error_matches(reference, {...candidate, stderr: bytes.concat([prefix, b"No such file or directory\n"])}, args, memory_path)
  assert ! lsmem_zero_entry_error_matches(reference, {...candidate, status: 2}, args, memory_path)
  assert ! lsmem_zero_entry_error_matches(reference, {...candidate, args: ["--sysroot", "/other"]}, args, memory_path)
  assert ! lsmem_zero_entry_error_matches(reference, {...candidate, stdout: b"unexpected\n"}, args, memory_path)
  assert ! lsmem_zero_entry_error_matches(reference, {...candidate, stderr: b"lsmem: cannot read\n"}, args, memory_path)
  assert ! lsmem_zero_entry_error_matches({...reference, stderr: b"lsmem: cannot read\n"}, candidate, args, memory_path)
  assert ! lsmem_zero_entry_error_matches(reference, {...candidate, stderr: prefix}, args, memory_path)
  assert ! lsmem_zero_entry_error_matches(reference, {...candidate, stderr: bytes.concat([prefix, b"No newline"])}, args, memory_path)
  assert ! lsmem_zero_entry_error_matches(reference, {...candidate, stderr: bytes.concat([prefix, b"No such file or directory\nextra\n"])}, args, memory_path)
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "no-numeric-blocks")?
  compare_lsmem_zero_entry_rooted(s, [])?
  compare_lsmem_zero_entry_rooted(s, ["--summary=only"])?
}

test test_lsmem_rooted_nonnumeric_memory_entry_does_not_hide_numeric_block { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "nonnumeric-entry")?
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

test test_lsmem_missing_or_nondirectory_sysroot_matches_reference { |ctx|
  let s = uu.scene(ctx)?
  let missing = uu.at(s, "missing-root")
  let regular = uu.at(s, "regular-root")
  regular.write("file")?
  for root in [missing, regular] { compare_lsmem(s, ["--sysroot", root.display()])? }
}

test test_lsmem_sysroot_error_precedes_unknown_column { |ctx|
  let s = uu.scene(ctx)?
  let missing = uu.at(s, "missing-root")
  let regular = uu.at(s, "regular-root")
  regular.write("file")?
  compare_lsmem(s, ["-o", "FOO", "-s", missing.display()])?
  compare_lsmem(s, ["-s", regular.display()], {LSMEM_COLUMNS: "FOO"})?
}

test test_lsmem_abbreviated_options_match_reference { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  for args in [["--js"], ["--summ"], ["--sum=only"], ["--summ=never"], ["--pai"], ["--ra"], ["--js", "--pai"], ["--al", "--split=STATE"]] {
    compare_lsmem_rooted(s, args)?
  }
}

test test_lsmem_option_diagnostics_and_empty_columns_match_reference { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  for args in [["-x"], ["-s"], ["-o"]] { compare_lsmem(s, args)? }
  for value in ["", ",", "RANGE,", ",RANGE", ",,", "RANGE,,SIZE", "+,STATE", "RANGE,BOGUS"] { compare_lsmem_rooted(s, ["-o", value])? }
}

test test_lsmem_long_option_diagnostics_match_reference { |ctx|
  let s = uu.scene(ctx)?
  for args in [["--s"], ["--output"], ["--json=1"], ["--bogus=x"]] { compare_lsmem(s, args)? }
}

test test_lsmem_validation_precedes_help_in_argument_order { |ctx|
  let s = uu.scene(ctx)?
  for args in [["-J", "-r", "--help"], ["--summary=bad", "--help"], ["--summary=bad", "-J", "-r"], ["-J", "-r", "--summary=bad"]] { compare_lsmem(s, args)? }
}

test test_lsmem_column_list_errors_and_limits_match_reference { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  for value in ["FOO,SIZE", "SIZE,", ",SIZE", "SIZE,,NODE", "SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE", "SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE"] {
    compare_lsmem_rooted(s, ["-S", value])?
  }
  for value in ["FOO,SIZE", "SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE", "SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE,SIZE"] {
    compare_lsmem_rooted(s, ["-o", value])?
  }
  compare_lsmem_rooted(s, [], {LSMEM_COLUMNS: "FOO,SIZE"})?
}

test test_lsmem_empty_sysroot_matches_reference { |ctx|
  let s = uu.scene(ctx)?
  let args = ["--sysroot=", "--summary=only", "--bytes"]
  let reference = ul.reference(s, "lsmem", args, timeout: 30s)?
  let candidate = uu.invoke(s, "lsmem", args, timeout: 30s)?
  ul.compare(reference, candidate)
}

test test_lsmem_non_utf8_option_values_match_reference { |ctx|
  let s = uu.scene(ctx)?
  let invalid = Path.parse_bytes(b"\xff")?
  let invalid_list = Path.parse_bytes(b"SIZE,\xff")?
  let invalid_option = Path.parse_bytes(b"--\xff")?
  let invalid_summary = Path.parse_bytes(b"--summary=\xff")?
  for args in [[p"-o", invalid], [p"-S", invalid], [p"-o", invalid_list], [invalid_option], [p"--bogus", invalid_option], [invalid_summary]] {
    let reference = ul.reference_paths(s, "lsmem", args, timeout: 5s)?
    let candidate = uu.invoke_paths(s, "lsmem", args, timeout: 5s)?
    ul.compare(reference, candidate)
  }
}

test test_lsmem_non_utf8_long_option_diagnostics { |ctx|
  let s = uu.scene(ctx)?
  for arg in [b"--out=\xff" as Path, b"--json=\xff" as Path] {
    let reference = ul.reference_paths(s, "lsmem", [arg])?
    let candidate = uu.invoke_paths(s, "lsmem", [arg])?
    ul.compare(reference, candidate)
  }
}

test test_lsmem_help_in_short_option_cluster { |ctx|
  let s = uu.scene(ctx)?
  for args in [["-hJr"], ["-Jrh"]] { compare_lsmem(s, args)? }
}

test test_lsmem_noheadings_size_width { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  compare_lsmem_rooted(s, ["-a", "-n", "--summary=never"])?
}

test test_lsmem_ascii_column_case { |ctx|
  let s = uu.scene(ctx)?
  compare_lsmem(s, ["-o", "sıze"])?
}

test test_lsmem_smartcols_json_environment { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  for mode in ["compact", "lines"] { compare_lsmem_rooted(s, ["-J"], {LIBSMARTCOLS_JSON: mode})? }
}

test test_lsmem_human_size_rounding { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  for size in ["1ee000\n", "1ee000000000\n"] {
    uu.write(s, "sys/devices/system/memory/block_size_bytes", size)?
    compare_lsmem_rooted(s, ["-a", "-o", "SIZE", "--summary=never"])?
  }
}

test test_lsmem_unreadable_optional_inventory_links { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let zones = uu.at(s, "sys/devices/system/memory/memory0/valid_zones")
  zones.remove()?
  zones.symlink(to: zones)?
  compare_lsmem_rooted(s, ["--all", "-o", "BLOCK,ZONES"])?
  uu.mkdir(s, "sys/firmware/memory")?
  let firmware = uu.at(s, "sys/firmware/memory/memory0")
  firmware.symlink(to: firmware)?
  compare_lsmem_rooted(s, ["--all", "-o", "BLOCK,ZONES"])?
}

test test_lsmem_multibyte_short_option { |ctx|
  let s = uu.scene(ctx)?
  for args in [["-é"], ["-bé"]] { compare_lsmem(s, args)? }
  compare_lsmem(s, ["-é"], {LC_ALL: "C.UTF-8"})?
  compare_lsmem(s, ["-h", "-é"])?
}

test test_lsmem_unset_locale_is_utf8 { |ctx|
  let s = uu.scene(ctx)?
  for vars in [{LC_ALL: ""}, {LC_ALL: "", LANG: "C"}, {LC_ALL: "", LC_CTYPE: "POSIX"}, {LC_ALL: "", LANG: "xx_YY"}] {
    compare_lsmem(s, ["-é"], vars)?
  }
}

test test_lsmem_option_order_and_musl_diagnostics { |ctx|
  let s = uu.scene(ctx)?
  for args in [["--=x"], ["--a=1"], ["--help=1"], ["--summary=bad", "--bogus"], ["--bogus", "--summary=bad"]] { compare_lsmem(s, args)? }
  let attached = Path.parse_bytes(b"--output=SIZE,\xff")?
  let reference = ul.reference_paths(s, "lsmem", [attached])?
  let candidate = uu.invoke_paths(s, "lsmem", [attached])?
  ul.compare(reference, candidate)
}

test test_lsmem_zero_terminal_columns { |ctx|
  let s = uu.scene(ctx)?
  install_lsmem_fixture(s, "sparse")?
  let script = fp"{ctx.core_dir}/tests/data/lsmem-zero-columns.py"
  let applet_script = fp"{ctx.core_dir}/lsmem.xsh"
  let stderr = uu.at(s, "terminal-error")
  let command = process.command_argv(p"/usr/bin/python3", [p"/usr/bin/python3", script, ctx.xsh_bin, applet_script, s.root], s.root, {LC_ALL: "C"}, b"", uu.at(s, "terminal-output"), stderr, timeout: 15s)
  let result = process.run(command)?
  assert result.exited_with(0), stderr.read_text()?
}
