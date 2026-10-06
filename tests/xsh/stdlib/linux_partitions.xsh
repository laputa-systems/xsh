test test_linux_partition_writer_rejects_invalid_records_without_mutation { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("partition tables are Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "partition-validation")?
  let image = fp"{root}/disk.img"
  let contents = bytes.zero(1048576)?
  image.write(contents)
  for part in [
    {index: 5, start: 64, end: 127, size: 64, type: "83", uuid: "", name: ""},
    {index: 1, start: 64, end: 127, size: 0, type: "83", uuid: "", name: ""},
    {index: 1, start: 64, end: 127, size: 65, type: "83", uuid: "", name: ""},
    {index: 1, start: 2048, end: 2111, size: 64, type: "83", uuid: "", name: ""},
    {index: 1, start: 64, end: 127, size: 64, type: "invalid", uuid: "", name: ""},
    {index: 1, start: 64, end: 127, size: 64, type: "05", uuid: "", name: ""},
  ] {
    test.error_kind(linux.write_partition_table(image, {label: "dos", id: "", sector_size: 512, partitions: [part]}), "linux-write-partition-table")
    assert image.read_bytes()? == contents
  }
  let part = {index: 1, start: 64, end: 127, size: 64, type: "83", uuid: "", name: ""}
  test.error_kind(linux.write_partition_table(image, {label: "dos", id: "", sector_size: 4096, partitions: [part]}), "linux-write-partition-table")
  assert image.read_bytes()? == contents
  test.error_kind(linux.write_partition_table(image, {label: "dos", id: "", sector_size: 512, partitions: [part, part]}), "linux-write-partition-table")
  assert image.read_bytes()? == contents
}

test test_linux_partition_tables_roundtrip_owned_images { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("partition tables are Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "partition-roundtrip")?
  let image = fp"{root}/disk.img"
  image.write(bytes.zero(1048576)?)
  linux.write_partition_table(image, {label: "gpt", id: "", sector_size: 512, partitions: [
    {index: 3, start: 64, end: 127, size: 64, type: "0fc63daf-8483-4772-8e79-3d69d8477de4", uuid: "", name: "root"},
  ]})
  let table = linux.partition_table(image)?
  assert table.label == "gpt"
  assert table.id != ""
  assert table.partitions[0].index == 3
  assert table.partitions[0].uuid != ""
  assert table.partitions[0].uuid != table.id
  assert table.partitions[0].name == "root"
  assert table.partitions[0].size == 64
  linux.write_partition_table(image, table)
  assert linux.partition_table(image)? == table
  linux.write_partition_table(image, {label: "dos", id: "0x12345678", sector_size: 512, partitions: [
    {index: 4, start: 64, end: 127, size: 64, type: "83", uuid: "", name: ""},
  ]})
  let dos = linux.partition_table(image)?
  assert dos.label == "dos"
  assert dos.id == "0x12345678"
  assert dos.partitions[0].index == 4
  assert dos.partitions[0].size == 64
  linux.write_partition_table(image, dos)
  assert linux.partition_table(image)? == dos
}

test test_linux_gpt_writer_rejects_bad_guids_extents_and_names { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("partition tables are Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "gpt-validation")?
  let image = fp"{root}/disk.img"
  let contents = bytes.zero(1048576)?
  image.write(contents)
  let kind = "0fc63daf-8483-4772-8e79-3d69d8477de4"
  for part in [
    {index: 129, start: 64, end: 127, size: 64, type: kind, uuid: "", name: ""},
    {index: 1, start: 1, end: 64, size: 64, type: kind, uuid: "", name: ""},
    {index: 1, start: 64, end: 127, size: 65, type: kind, uuid: "", name: ""},
    {index: 1, start: 64, end: 127, size: 64, type: "invalid", uuid: "", name: ""},
    {index: 1, start: 64, end: 127, size: 64, type: kind, uuid: "invalid", name: ""},
    {index: 1, start: 64, end: 127, size: 64, type: kind, uuid: "00000000-0000-0000-0000-000000000000", name: ""},
    {index: 1, start: 64, end: 127, size: 64, type: kind, uuid: "", name: "abcdefghijklmnopqrstuvwxyzabcdefghijk"},
  ] {
    test.error_kind(linux.write_partition_table(image, {label: "gpt", id: "", sector_size: 512, partitions: [part]}), "linux-write-partition-table")
    assert image.read_bytes()? == contents
  }
  let first = {index: 1, start: 64, end: 127, size: 64, type: kind, uuid: "", name: ""}
  let second = {index: 2, start: 127, end: 190, size: 64, type: kind, uuid: "", name: ""}
  test.error_kind(linux.write_partition_table(image, {label: "gpt", id: "", sector_size: 512, partitions: [first, second]}), "linux-write-partition-table")
  assert image.read_bytes()? == contents
  test.error_kind(linux.write_partition_table(image, {label: "gpt", id: "invalid", sector_size: 512, partitions: [first]}), "linux-write-partition-table")
  assert image.read_bytes()? == contents
}
