test test_elf_inspect { |ctx|
  let plain = test.temp_file(ctx, name: "plain.txt", contents: b"plain")?
  assert elf.inspect(plain)?.type == "not-elf"
  let bad = test.temp_file(ctx, name: "bad-elf.bin", contents: b"\x7fELF\x02\x01")?
  test.error_kind(elf.inspect(bad), "elf-malformed")
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

test test_elf_inspect_reads_dynamic_metadata { |ctx|
  let object = test.temp_path(ctx, name: "fixture.so")
  write_elf64_fixture(object)

  let info = elf.inspect(object)?
  assert info.type == "shared"
  assert info.class == "ELF64"
  assert info.endian == "little"
  assert info.machine == "x86_64"
  assert info.interpreter == "/lib/ld-musl-x86_64.so.1"
  assert info.soname == "libdemo.so"
  assert info.needed == ["libc.musl-x86_64.so.1", "libprivate.so"]
  assert info.rpath == "$ORIGIN/lib"
  assert info.runpath == "$ORIGIN"

  let plain = elf.inspect(test.temp_file(ctx, name: "plain.txt", contents: b"plain text")?)?
  assert plain.type == "not-elf"
  assert plain.needed.is_empty()
  let bad = test.temp_file(ctx, name: "bad.bin", contents: b"\x7fELF\x02\x01")?
  test.error_kind(elf.inspect(bad), "elf-malformed")
}
