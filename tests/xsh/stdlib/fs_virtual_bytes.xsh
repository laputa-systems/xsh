test test_fs_read_bytes_reads_zero_size_virtual_files { |_|
  if system.uname()?.sysname != "Linux" {
    test.skip("procfs is a Linux fixture")
  }
  let proc_root = fs.open_root(/proc)?
  let expected = proc_root.read_bytes(p"version")?
  assert ! expected.is_empty()
  assert p"/proc/version".read_bytes()? == expected
}
