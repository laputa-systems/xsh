type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", vars: Record = {LC_ALL: "C", TERM: "xterm"}, cwd: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "ldd-capture")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/ldd.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), cwd ?? root, vars, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8()?, stderr: err.read_text()?})
}

test test_ldd_never_executes_non_elf { |ctx|
  let script = test.temp_file(ctx, name: "untrusted", contents: b"#!/bin/sh\necho EXECUTED\n")?
  let result = invoke(ctx, [script.display()])?
  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr.find("not a dynamic executable") != null
}

test test_ldd_reports_static_elf { |ctx|
  let object = test.temp_file(ctx, name: "static", contents: bytes.zero(64)?)?
  let _ = bytes.write_at(object, 0, b"\x7fELF\x02\x01\x01")?
  let _ = bytes.write_at(object, 16, bytes.pack_le(2, 2)?)?
  let _ = bytes.write_at(object, 18, bytes.pack_le(183, 2)?)?
  let _ = bytes.write_at(object, 52, bytes.pack_le(64, 2)?)?
  let result = invoke(ctx, [object.display()])?
  assert result.status == 0
  assert result.stdout == "\tstatically linked\n"
}

# Writes `value` little-endian in `width` bytes at `offset` of `file`.
proc put_le(file: Path, offset: Int, value: Int, width: Int) [fs, error] {
  assert bytes.write_at(file, offset, bytes.pack_le(value, width)?)? == width
}

# Writes a minimal ELF64 little-endian x86-64 shared object: three program
# headers (load, dynamic, interpreter), a dynamic section with two needed
# libraries, a soname, an rpath, and a runpath, and the string table they name.
proc write_elf64_fixture(file: Path) [fs, error] {
  file.write(bytes.zero(1536)?)
  let ident = b"\x7fELF\x02\x01\x01\x03"
  assert bytes.write_at(file, 0, ident)? == ident.len()
  # File header: type ET_DYN, machine EM_X86_64, then the header tables.
  for field in [
    {at: 16, value: 3, width: 2},
    {at: 18, value: 62, width: 2},
    {at: 20, value: 1, width: 4},
    {at: 32, value: 64, width: 8},
    {at: 40, value: 768, width: 8},
    {at: 52, value: 64, width: 2},
    {at: 54, value: 56, width: 2},
    {at: 56, value: 3, width: 2},
    {at: 58, value: 64, width: 2},
    {at: 60, value: 3, width: 2},
  ] {
    put_le(file, field.at, field.value, field.width)
  }

  for program in [
    {at: 64, kind: 1, offset: 256, vaddr: 4194304, filesz: 512},
    {at: 120, kind: 2, offset: 512, vaddr: 4194560, filesz: 128},
    {at: 176, kind: 3, offset: 640, vaddr: 4194688, filesz: 24},
  ] {
    put_le(file, program.at, program.kind, 4)
    put_le(file, program.at + 8, program.offset, 8)
    put_le(file, program.at + 16, program.vaddr, 8)
    put_le(file, program.at + 32, program.filesz, 8)
  }

  # SHT_DYNAMIC linked to section 2, and the SHT_STRTAB it reads names from.
  for section in [
    {at: 832, kind: 6, offset: 512, size: 128, link: 2, entsize: 16},
    {at: 896, kind: 3, offset: 1280, size: 128, link: 0, entsize: 0},
  ] {
    put_le(file, section.at + 4, section.kind, 4)
    put_le(file, section.at + 24, section.offset, 8)
    put_le(file, section.at + 32, section.size, 8)
    put_le(file, section.at + 40, section.link, 4)
    put_le(file, section.at + 56, section.entsize, 8)
  }

  let strings = b"\0libc.musl-x86_64.so.1\0libprivate.so\0libdemo.so\0$ORIGIN/lib\0$ORIGIN\0"
  assert bytes.write_at(file, 1280, strings)? == strings.len()
  # Each value of the first five tags is the offset of its name in `strings`.
  let dynamic = [
    {tag: 1, value: 1},
    {tag: 1, value: 23},
    {tag: 14, value: 37},
    {tag: 15, value: 48},
    {tag: 29, value: 60},
    {tag: 5, value: 4195328},
    {tag: 10, value: strings.len()},
    {tag: 0, value: 0},
  ]
  for index in range(dynamic.len()) {
    put_le(file, 512 + index * 16, dynamic[index].tag, 8)
    put_le(file, 512 + index * 16 + 8, dynamic[index].value, 8)
  }

  let interpreter = b"/lib/ld-musl-x86_64.so.1\0"
  assert bytes.write_at(file, 640, interpreter)? == interpreter.len()
}


test test_ldd_resolves_origin_dependencies_without_execution { |ctx|
  let root = test.temp_dir(ctx, name: "ldd-objects")?
  let object = fp"{root}/object"
  let library = fp"{root}/libprivate.so"
  write_elf64_fixture(object)
  write_elf64_fixture(library)
  # Keep only the private library dependency and resolve it through RUNPATH.
  for target in [object, library] {
    assert bytes.write_at(target, 520, bytes.pack_le(23, 8)?)? == 8
    assert bytes.write_at(target, 528, bytes.pack_le(14, 8)?)? == 8
  }
  let result = invoke(ctx, [object.display()])?
  assert result.status == 0, result.stderr
  assert result.stdout.find(f"libprivate.so => {library}") != null
  assert result.stdout.find("/lib/ld-musl-x86_64.so.1") != null
  library.remove()
  let missing = invoke(ctx, [object.display()])?
  assert missing.status == 1
  assert missing.stdout.find("libprivate.so => not found") != null
}
