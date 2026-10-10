# Storage transport primitives. Every device command here is answered from a
# recorded fixture under the linux fake, so nothing reaches a descriptor; the
# few real-ioctl checks run against a regular temp file, which the kernel
# rejects with ENOTTY.

proc is_linux() -> Bool {
  system.uname()?.sysname == "Linux"
}

# One recorded sg_io exchange: the request fields select the line and the
# response fields are what the device returns.
proc sg_line(
  cdb: Bytes,
  direction = "none",
  data_len = 0,
  data_out = b"",
  status = 0,
  host_status = 0,
  driver_status = 0,
  sense = b"",
  data = b"",
  errno: Int? = null,
) -> Record {
  {
    op: "sg_io",
    cdb: cdb.base64(),
    direction: direction,
    data_len: data_len,
    data_out: data_out.base64(),
    status: status,
    host_status: host_status,
    driver_status: driver_status,
    sense: sense.base64(),
    data: data.base64(),
    errno: errno,
  }
}

proc nvme_line(
  opcode: Int,
  nsid = 0,
  cdw10 = 0,
  cdw11 = 0,
  direction = "none",
  data_len = 0,
  data_out = b"",
  status = 0,
  result = 0,
  data = b"",
  errno: Int? = null,
) -> Record {
  {
    op: "nvme_admin",
    opcode: opcode,
    nsid: nsid,
    cdw10: cdw10,
    cdw11: cdw11,
    direction: direction,
    data_len: data_len,
    data_out: data_out.base64(),
    status: status,
    result: result,
    data: data.base64(),
    errno: errno,
  }
}

# Descriptor-format sense data with one ATA Status Return descriptor.
proc ata_sense(status: Int, error: Int, count: Int, low: Int, mid: Int, high: Int) [error] -> Result[Bytes, Error] {
  bytes.from_ints([
    114, 0, 0, 29, 0, 0, 0, 14,
    9, 12, 0, error, 0, count, 0, low, 0, mid, 0, high, 64, status,
  ])
}

proc page(first: List[Int]) [error] -> Result[Bytes, Error] {
  bytes.concat([bytes.from_ints(first)?, bytes.zero(512 - first.len())?])
}

# Installs the fake with a fixture holding `lines`, logging calls to `log`,
# and returns a descriptor number for a placeholder file. Under the fake the
# descriptor is never read.
proc install_logged(ctx: TestContext, name: Str, lines: List[Any], log: Path) [fs, process, error] -> Result[Int, Error] {
  let root = test.temp_dir(ctx, name: name)?
  let fixture = fp"{root}/fixture.jsonl"
  fixture.write(json.encode_lines(lines)?)
  test.linux_fake(ctx, {storage_fixture: fixture, log: log})?
  let device = fp"{root}/device"
  device.write(b"")
  unix.open_fd(device)?
}

proc install(ctx: TestContext, name: Str, lines: List[Any]) [fs, process, error] -> Result[Int, Error] {
  install_logged(ctx, name, lines, test.temp_file(ctx, name: f"{name}-calls", contents: b"")?)
}

proc failure_of(result: Result[Any, Error], kind: Str) [error] -> Error {
  match result {
    Ok(_) => {
      assert false, "operation unexpectedly succeeded"
      error.failure("unreachable")
    }
    Err(failure) => {
      test.error_kind(failure, kind)
      failure
    }
  }
}

test test_sg_io_returns_status_sense_and_transferred_data { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  let inquiry = b"\x12\x00\x00\x00\x24\x00"
  let ok_data = bytes.concat([b"\x00\x00\x06\x02\x1f\x00\x00\x00", bytes.from_text("ACME    "), bytes.zero(20)?])
  let sense = b"\x70\x00\x05\x00\x00\x00\x00\x0a\x00\x00\x00\x00\x20\x00"
  let test_unit_ready = b"\x00\x00\x00\x00\x00\x00"
  let fd = install(ctx, "sg-io", [
    sg_line(inquiry, direction: "from_device", data_len: 36, data: ok_data),
    sg_line(test_unit_ready, status: 2, driver_status: 8, sense: sense),
  ])?

  let reply = linux.sg_io(fd, inquiry, 36)?
  assert reply.status == 0 and reply.host_status == 0 and reply.driver_status == 0
  assert reply.data == ok_data
  assert reply.data.len() == 36 and reply.resid == 0
  assert reply.sense == b""

  # A CHECK CONDITION is data: the caller reads the sense bytes.
  let busy = linux.sg_io(fd, test_unit_ready)?
  assert busy.status == 2 and busy.driver_status == 8
  assert busy.sense == sense
  assert busy.data == b""
  unix.close_fd(fd)?
}

test test_sg_io_reports_short_transfer_as_resid { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  let cdb = b"\x12\x00\x00\x00\xff\x00"
  let fd = install(ctx, "sg-io-short", [
    sg_line(cdb, direction: "from_device", data_len: 255, data: bytes.from_text("short")),
  ])?
  let reply = linux.sg_io(fd, cdb, 255)?
  assert reply.data == bytes.from_text("short")
  assert reply.resid == 250
  unix.close_fd(fd)?
}

test test_sg_io_refuses_commands_that_can_change_the_device { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  let fd = install(ctx, "sg-io-readonly", [])?
  # WRITE(10), START STOP UNIT, ATA PASS-THROUGH(16) and FORMAT UNIT.
  for cdb in [
    b"\x2a\x00\x00\x00\x00\x00\x00\x00\x01\x00",
    b"\x1b\x00\x00\x00\x02\x00",
    b"\x85\x08\x2e\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\xec\x00",
    b"\x04\x00\x00\x00\x00\x00",
  ] {
    let failure = failure_of(linux.sg_io(fd, cdb), "linux-sg-io")
    assert "linux.sg_io_command" in failure.message
  }
  # An unmatched read-only command reaches the fixture and fails there.
  test.error_kind(linux.sg_io(fd, b"\x12\x00\x00\x00\x24\x00", 36), "linux-storage-fake")
  unix.close_fd(fd)?
}

test test_sg_io_command_sends_both_directions { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  let start_stop = b"\x1b\x00\x00\x00\x02\x00"
  let write_buffer = b"\x3b\x02\x00\x00\x00\x00\x00\x00\x04\x00"
  let payload = b"\x01\x02\x03\x04"
  let read_defects = b"\x37\x00\x08\x00\x00\x00\x00\x00\x08\x00"
  let fd = install(ctx, "sg-io-command", [
    sg_line(start_stop),
    sg_line(write_buffer, direction: "to_device", data_out: payload, status: 0),
    sg_line(read_defects, direction: "from_device", data_len: 8, data: b"\x00\x08\x00\x00\x00\x00\x00\x00"),
  ])?
  assert linux.sg_io_command(fd, start_stop, 0)?.status == 0
  assert linux.sg_io_command(fd, write_buffer, payload)?.status == 0
  assert linux.sg_io_command(fd, read_defects, 8)?.data.len() == 8
  # The payload is part of what is matched.
  test.error_kind(linux.sg_io_command(fd, write_buffer, b"\x09\x09\x09\x09"), "linux-storage-fake")
  unix.close_fd(fd)?
}

test test_fixture_errno_is_carried_and_unmatched_commands_fail_loudly { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  let cdb = b"\x12\x00\x00\x00\x24\x00"
  let fd = install(ctx, "sg-io-errno", [
    sg_line(cdb, direction: "from_device", data_len: 36, errno: 5),
  ])?
  let failure = failure_of(linux.sg_io(fd, cdb, 36), "linux-sg-io")
  assert failure.errno == 5
  let missing = failure_of(linux.sg_io(fd, b"\x25\x00\x00\x00\x00\x00\x00\x00\x00\x00", 8), "linux-storage-fake")
  assert "no recorded response" in missing.message
  unix.close_fd(fd)?
}

test test_fake_without_fixture_never_reaches_a_descriptor { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  test.linux_fake(ctx, {})?
  let device = test.temp_file(ctx, name: "no-fixture-device", contents: b"")?
  let fd = unix.open_fd(device)?
  test.error_kind(linux.sg_io(fd, b"\x00\x00\x00\x00\x00\x00"), "linux-storage-fake")
  test.error_kind(linux.ata_identify(fd), "linux-storage-fake")
  test.error_kind(linux.nvme_namespace_id(fd), "linux-storage-fake")
  test.error_kind(linux.nvme_identify_controller(fd), "linux-storage-fake")
  unix.close_fd(fd)?
}

test test_malformed_fixture_is_reported_with_its_line { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "bad-fixture")?
  let fixture = fp"{root}/fixture.jsonl"
  fixture.write("{\"op\":\"sg_io\",\"cdb\":\"AAAA\",\"cdb_typo\":1}\n")
  test.linux_fake(ctx, {storage_fixture: fixture})?
  let device = test.temp_file(ctx, name: "bad-fixture-device", contents: b"")?
  let fd = unix.open_fd(device)?
  let failure = failure_of(linux.sg_io(fd, b"\x00\x00\x00\x00\x00\x00"), "linux-storage-fake")
  assert ":1: unknown field `cdb_typo`" in failure.message
  unix.close_fd(fd)?
}

test test_ata_identify_and_smart_reads_use_sat_pass_through { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  # CDBs as smartmontools sends them, with CK_COND set so the device returns
  # its registers: IDENTIFY DEVICE, SMART READ DATA, READ THRESHOLDS and
  # READ LOG for the log directory.
  let identify_cdb = b"\x85\x08\x2e\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\xec\x00"
  let data_cdb = b"\x85\x08\x2e\x00\xd0\x00\x01\x00\x00\x00\x4f\x00\xc2\x00\xb0\x00"
  let thresholds_cdb = b"\x85\x08\x2e\x00\xd1\x00\x01\x00\x00\x00\x4f\x00\xc2\x00\xb0\x00"
  let log_cdb = b"\x85\x08\x2e\x00\xd5\x00\x02\x00\x06\x00\x4f\x00\xc2\x00\xb0\x00"
  let identify_page = page([64, 0, 255, 63])?
  let data_page = page([1, 0, 5, 33, 0, 100, 100])?
  let threshold_page = page([1, 0, 5, 10])?
  let log_pages = bytes.concat([page([7])?, page([9])?])
  let sense = ata_sense(80, 0, 1, 0, 0, 0)?
  let smart_sense = ata_sense(80, 0, 1, 0, 79, 194)?
  let fd = install(ctx, "ata-reads", [
    sg_line(identify_cdb, direction: "from_device", data_len: 512, data: identify_page, status: 2, driver_status: 8, sense: sense),
    sg_line(data_cdb, direction: "from_device", data_len: 512, data: data_page, status: 2, driver_status: 8, sense: smart_sense),
    sg_line(thresholds_cdb, direction: "from_device", data_len: 512, data: threshold_page, status: 2, driver_status: 8, sense: smart_sense),
    sg_line(log_cdb, direction: "from_device", data_len: 1024, data: log_pages, status: 2, driver_status: 8, sense: smart_sense),
  ])?

  let identify = linux.ata_identify(fd)?
  assert identify.data == identify_page and identify.data.len() == 512
  assert identify.status == 80 and identify.error == 0 and identify.sector_count == 1
  assert identify.sense == sense

  let values = linux.ata_smart_read_data(fd)?
  assert values.data == data_page
  assert values.lba_mid == 79 and values.lba_high == 194 and values.device == 64

  assert linux.ata_smart_read_thresholds(fd)?.data == threshold_page

  let log = linux.ata_smart_read_log(fd, 6, sectors: 2)?
  assert log.data == log_pages and log.data.len() == 1024
  unix.close_fd(fd)?
}

test test_ata_12_byte_pass_through_form { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  let identify_cdb = b"\xa1\x08\x2e\x00\x01\x00\x00\x00\x00\xec\x00\x00"
  let sense = ata_sense(80, 0, 1, 0, 0, 0)?
  let fd = install(ctx, "ata-12", [
    sg_line(identify_cdb, direction: "from_device", data_len: 512, data: page([1])?, status: 2, sense: sense),
  ])?
  assert linux.ata_identify(fd, cdb_size: 12)?.data.len() == 512
  test.error_kind(linux.ata_identify(fd), "linux-storage-fake")
  test.error_kind(linux.ata_identify(fd, cdb_size: 10), "invalid-argument")
  unix.close_fd(fd)?
}

test test_ata_smart_status_reads_the_verdict_from_registers { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  let cdb = b"\x85\x06\x20\x00\xda\x00\x00\x00\x00\x00\x4f\x00\xc2\x00\xb0\x00"
  let root = test.temp_dir(ctx, name: "ata-status")?
  let healthy = ata_sense(80, 0, 0, 0, 79, 194)?
  let failing = ata_sense(80, 0, 0, 0, 244, 44)?
  let odd = ata_sense(80, 0, 0, 0, 1, 2)?
  let fd = install(ctx, "ata-status", [
    sg_line(cdb, status: 2, sense: healthy),
  ])?
  let ok = linux.ata_smart_status(fd)?
  assert ok.passed and ok.lba_mid == 79 and ok.lba_high == 194 and ok.status == 80

  let fixture = fp"{root}/fixture.jsonl"
  fixture.write(json.encode_lines([sg_line(cdb, status: 2, sense: failing)])?)
  test.linux_fake(ctx, {storage_fixture: fixture})?
  assert ! linux.ata_smart_status(fd)?.passed

  fixture.write(json.encode_lines([sg_line(cdb, status: 2, sense: odd)])?)
  let failure = failure_of(linux.ata_smart_status(fd), "linux-ata")
  assert "lba_mid 0x01" in failure.message
  unix.close_fd(fd)?
}

test test_ata_mutations_are_separate_named_functions { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  let enable = b"\x85\x06\x20\x00\xd8\x00\x00\x00\x00\x00\x4f\x00\xc2\x00\xb0\x00"
  let disable = b"\x85\x06\x20\x00\xd9\x00\x00\x00\x00\x00\x4f\x00\xc2\x00\xb0\x00"
  let short_test = b"\x85\x06\x20\x00\xd4\x00\x00\x00\x01\x00\x4f\x00\xc2\x00\xb0\x00"
  let extended_test = b"\x85\x06\x20\x00\xd4\x00\x00\x00\x02\x00\x4f\x00\xc2\x00\xb0\x00"
  let abort_test = b"\x85\x06\x20\x00\xd4\x00\x00\x00\x7f\x00\x4f\x00\xc2\x00\xb0\x00"
  let sense = ata_sense(80, 0, 0, 0, 79, 194)?
  let log = test.temp_file(ctx, name: "ata-mutations-calls", contents: b"")?
  let fd = install_logged(ctx, "ata-mutations", [
    sg_line(enable, status: 2, driver_status: 8, sense: sense),
    sg_line(disable, status: 2, driver_status: 8, sense: sense),
    sg_line(short_test, status: 2, driver_status: 8, sense: sense),
    sg_line(extended_test, status: 2, driver_status: 8, sense: sense),
    sg_line(abort_test, status: 2, driver_status: 8, sense: sense),
  ], log)?
  assert linux.ata_smart_enable(fd)?.data == b""
  assert linux.ata_smart_disable(fd)?.status == 80
  let _ = linux.ata_smart_start_self_test(fd, "short")?
  let _ = linux.ata_smart_start_self_test(fd, "extended")?
  let _ = linux.ata_smart_start_self_test(fd, "abort")?
  test.error_kind(linux.ata_smart_start_self_test(fd, "selective"), "invalid-argument")
  # The fake logs each call by name, so the test can assert exactly which
  # mutations were requested; the refused kind is logged too.
  let calls = log.read_text()?.lines()
  assert calls.len() == 6
  assert "\"op\":\"ata_smart_enable\"" in calls[0]
  assert "\"op\":\"ata_smart_disable\"" in calls[1]
  for index in [2, 3, 4] {
    assert "\"op\":\"ata_smart_start_self_test\"" in calls[index]
  }
  unix.close_fd(fd)?
}

test test_ata_failures_include_scsi_and_ata_status { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  let cdb = b"\x85\x08\x2e\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\xec\x00"
  let aborted = ata_sense(81, 4, 1, 0, 0, 0)?
  let fd = install(ctx, "ata-failures", [
    sg_line(cdb, direction: "from_device", data_len: 512, data: page([1])?, status: 2, driver_status: 8, sense: aborted),
  ])?
  let failure = failure_of(linux.ata_identify(fd), "linux-ata")
  assert "status 0x51" in failure.message and "error 0x04" in failure.message
  assert "scsi status 0x02" in failure.message
  unix.close_fd(fd)?

  let root = test.temp_dir(ctx, name: "ata-failures-2")?
  let fixture = fp"{root}/fixture.jsonl"
  test.linux_fake(ctx, {storage_fixture: fixture})?
  let device = test.temp_file(ctx, name: "ata-failures-device", contents: b"")?
  let fd2 = unix.open_fd(device)?
  let sense = ata_sense(80, 0, 1, 0, 0, 0)?

  # A transport failure at the host adapter.
  fixture.write(json.encode_lines([sg_line(cdb, direction: "from_device", data_len: 512, host_status: 3)])?)
  assert "host status 0x0003" in failure_of(linux.ata_identify(fd2), "linux-ata").message

  # A good SCSI status with no ATA registers cannot be trusted.
  fixture.write(json.encode_lines([sg_line(cdb, direction: "from_device", data_len: 512, data: page([1])?)])?)
  assert "no ATA status return descriptor" in failure_of(linux.ata_identify(fd2), "linux-ata").message

  # A short data phase.
  fixture.write(json.encode_lines([sg_line(cdb, direction: "from_device", data_len: 512, data: b"\x01\x02", status: 2, sense: sense)])?)
  assert "2 of 512 data bytes" in failure_of(linux.ata_identify(fd2), "linux-ata").message

  # A BUSY SCSI status.
  fixture.write(json.encode_lines([sg_line(cdb, direction: "from_device", data_len: 512, status: 8)])?)
  assert "unexpected SCSI status" in failure_of(linux.ata_identify(fd2), "linux-ata").message
  unix.close_fd(fd2)?
}

test test_nvme_identify_log_and_feature_reads { |ctx|
  guard is_linux() else {
    test.skip("NVMe passthrough is Linux-only")
    return
  }
  let controller = bytes.concat([bytes.from_ints([134, 128])?, bytes.zero(4094)?])
  let namespace = bytes.concat([bytes.from_ints([0, 16])?, bytes.zero(4094)?])
  let smart_log = bytes.concat([b"\x00\x2a", bytes.zero(510)?])
  let fd = install(ctx, "nvme-reads", [
    {op: "nvme_namespace_id", nsid: 1},
    nvme_line(6, cdw10: 1, direction: "from_device", data_len: 4096, data: controller),
    nvme_line(6, nsid: 1, cdw10: 0, direction: "from_device", data_len: 4096, data: namespace),
    # Log page 2 (SMART / health) over the whole controller: NUMD 127 puts
    # the zero-based dword count in cdw10 bits 31:16.
    nvme_line(2, nsid: 4294967295, cdw10: 8323074, direction: "from_device", data_len: 512, data: smart_log),
    # Get Features: temperature threshold (4), current value, result 343.
    nvme_line(10, cdw10: 4, result: 343),
    nvme_line(10, cdw10: 772, nsid: 1, cdw11: 1, result: 7),
  ])?
  assert linux.nvme_namespace_id(fd)? == 1
  let id = linux.nvme_identify_controller(fd)?
  assert id.status == 0 and id.data == controller and id.data.len() == 4096
  assert linux.nvme_identify_namespace(fd, 1)?.data == namespace
  assert linux.nvme_log_page(fd, 2)?.data == smart_log
  assert linux.nvme_get_feature(fd, 4)?.result == 343
  # select 3 is the supported-capabilities selector.
  assert linux.nvme_get_feature(fd, 4, nsid: 1, select: 3, cdw11: 1)?.result == 7
  unix.close_fd(fd)?
}

test test_nvme_admin_read_form_refuses_mutating_opcodes { |ctx|
  guard is_linux() else {
    test.skip("NVMe passthrough is Linux-only")
    return
  }
  let fd = install(ctx, "nvme-guard", [
    nvme_line(6, cdw10: 1, direction: "from_device", data_len: 4096, result: 9, data: bytes.zero(4096)?),
    nvme_line(128, nsid: 1, cdw10: 0, result: 0),
    nvme_line(1, nsid: 0, cdw10: 3),
    nvme_line(17, cdw10: 1, direction: "to_device", data_out: b"\xde\xad\xbe\xef"),
  ])?
  # Format NVM (0x80), Delete I/O Submission Queue (0x00 is create-class),
  # and Firmware Download (0x11) are not reads.
  let refused = failure_of(linux.nvme_admin(fd, 128, nsid: 1), "linux-nvme")
  assert "nvme_admin_command" in refused.message
  test.error_kind(linux.nvme_admin(fd, 17, 4), "linux-nvme")

  let identify = linux.nvme_admin(fd, 6, 4096, cdw10: 1)?
  assert identify.status == 0 and identify.result == 9 and identify.data.len() == 4096

  assert linux.nvme_admin_command(fd, 128, 0, nsid: 1, cdw10: 0)?.status == 0
  assert linux.nvme_admin_command(fd, 1, 0, cdw10: 3)?.status == 0
  assert linux.nvme_admin_command(fd, 17, b"\xde\xad\xbe\xef", cdw10: 1)?.status == 0
  unix.close_fd(fd)?
}

test test_nvme_device_status_is_data_for_raw_and_error_for_typed_helpers { |ctx|
  guard is_linux() else {
    test.skip("NVMe passthrough is Linux-only")
    return
  }
  # INVALID FIELD IN COMMAND (SC 0x02) with do-not-retry set.
  let fd = install(ctx, "nvme-status", [
    nvme_line(6, cdw10: 1, direction: "from_device", data_len: 4096, status: 16386),
    nvme_line(6, nsid: 9, cdw10: 0, direction: "from_device", data_len: 4096, errno: 5),
    nvme_line(9, cdw10: 2147483652, cdw11: 5),
    nvme_line(9, cdw10: 4, cdw11: 6, status: 6),
  ])?
  let raw = linux.nvme_admin(fd, 6, 4096, cdw10: 1)?
  assert raw.status == 16386 and raw.data == b""

  let failure = failure_of(linux.nvme_identify_controller(fd), "linux-nvme")
  assert "status 0x4002" in failure.message and "do not retry" in failure.message

  let errno_failure = failure_of(linux.nvme_identify_namespace(fd, 9), "linux-nvme")
  assert errno_failure.errno == 5

  assert linux.nvme_set_feature(fd, 4, 5, save: true)?.status == 0
  test.error_kind(linux.nvme_set_feature(fd, 4, 6), "linux-nvme")
  unix.close_fd(fd)?
}

test test_nvme_arguments_are_validated_before_any_command { |ctx|
  guard is_linux() else {
    test.skip("NVMe passthrough is Linux-only")
    return
  }
  let fd = install(ctx, "nvme-args", [])?
  test.error_kind(linux.nvme_log_page(fd, 2, data_len: 6), "invalid-argument")
  test.error_kind(linux.nvme_log_page(fd, 256), "invalid-argument")
  test.error_kind(linux.nvme_identify_namespace(fd, -1), "invalid-argument")
  test.error_kind(linux.nvme_admin(fd, 6, 4096, cdw10: -1), "invalid-argument")
  test.error_kind(linux.sg_io(fd, b"", 0), "linux-sg-io")
  test.error_kind(linux.sg_io(-1, b"\x00\x00\x00\x00\x00\x00"), "invalid-argument")
  unix.close_fd(fd)?
}

test test_storage_candidates_lists_scsi_and_nvme_nodes_from_sysfs { |ctx|
  guard is_linux() else {
    test.skip("sysfs discovery is Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "candidates")?
  let sys = fp"{root}/sys"
  let dev = fp"{root}/dev"
  for name in ["sda", "sdb", "sda1", "loop0", "nvme0n1", "nvme0n2", "nvme1n1", "nvme0c0n1", "nvme10n1"] {
    fp"{sys}/block/{name}".mkdir()
  }
  fp"{sys}/block/sda/size".write("1953525168\n")
  fp"{sys}/block/sda/removable".write("0\n")
  fp"{sys}/block/sda/queue".mkdir()
  fp"{sys}/block/sda/queue/rotational".write("1\n")
  fp"{sys}/block/sda/device".mkdir()
  fp"{sys}/block/sda/device/model".write("ST1000DM010-2EP1  \n")
  fp"{sys}/block/sda/device/rev".write("CC43\n")
  fp"{sys}/block/nvme0n1/size".write("1000215216\n")
  fp"{sys}/block/nvme0n1/device".mkdir()
  fp"{sys}/block/nvme0n1/device/model".write("Samsung SSD 980 PRO 1TB\n")
  fp"{sys}/block/nvme0n1/device/serial".write("S5GXNX0T000001\n")
  fp"{sys}/block/nvme0n1/device/firmware_rev".write("5B2QGXA7\n")
  for name in ["nvme0", "nvme1", "nvme10", "nvmeX"] {
    fp"{sys}/class/nvme/{name}".mkdir()
  }
  fp"{sys}/class/nvme/nvme0/model".write("Samsung SSD 980 PRO 1TB\n")
  fp"{sys}/class/nvme/nvme0/serial".write("S5GXNX0T000001\n")
  fp"{sys}/class/nvme/nvme0/firmware_rev".write("5B2QGXA7\n")

  let found = linux.storage_candidates(sys_root: sys, dev_root: dev)?
  let names = [item.name for item in found]
  assert names == ["sda", "sdb", "nvme0", "nvme0n1", "nvme0n2", "nvme1", "nvme1n1", "nvme10", "nvme10n1"]

  let sda = found[0]
  assert sda.kind == "scsi_disk" and sda.protocol == "scsi"
  assert sda.path == fp"{dev}/sda"
  assert sda.model == "ST1000DM010-2EP1" and sda.firmware == "CC43"
  assert sda.serial == null
  assert sda.size_bytes == 1953525168 * 512
  assert sda.rotational == true and sda.removable == false
  assert sda.controller == null

  let ctrl = found[2]
  assert ctrl.kind == "nvme_controller" and ctrl.path == fp"{dev}/nvme0"
  assert ctrl.serial == "S5GXNX0T000001" and ctrl.size_bytes == null

  let ns = found[3]
  assert ns.kind == "nvme_namespace" and ns.protocol == "nvme"
  assert ns.controller == fp"{dev}/nvme0"
  assert ns.model == "Samsung SSD 980 PRO 1TB" and ns.firmware == "5B2QGXA7"
  assert ns.size_bytes == 1000215216 * 512
}

test test_real_ioctls_on_a_regular_file_fail_with_kernel_errno { |ctx|
  guard is_linux() else {
    test.skip("SG_IO is Linux-only")
    return
  }
  # A regular file is not a SCSI or NVMe node, so the kernel answers every
  # ioctl with ENOTTY and nothing is sent to a device.
  let file = test.temp_file(ctx, name: "not-a-device", contents: b"plain file")?
  let fd = unix.open_fd(file)?
  assert failure_of(linux.sg_io(fd, b"\x12\x00\x00\x00\x24\x00", 36), "linux-sg-io").errno == 25
  assert failure_of(linux.sg_io_command(fd, b"\x00\x00\x00\x00\x00\x00", 0), "linux-sg-io").errno == 25
  assert failure_of(linux.sg_io_command(fd, b"\x3b\x02\x00\x00\x00\x00\x00\x00\x04\x00", b"\x01\x02\x03\x04"), "linux-sg-io").errno == 25
  assert failure_of(linux.ata_identify(fd), "linux-sg-io").errno == 25
  assert failure_of(linux.ata_smart_read_data(fd), "linux-sg-io").errno == 25
  assert failure_of(linux.nvme_namespace_id(fd), "linux-nvme").errno == 25
  assert failure_of(linux.nvme_identify_controller(fd), "linux-nvme").errno == 25
  assert failure_of(linux.nvme_admin(fd, 6, 4096, cdw10: 1), "linux-nvme").errno == 25
  unix.close_fd(fd)?
  # A closed descriptor is EBADF.
  assert failure_of(linux.sg_io(fd, b"\x00\x00\x00\x00\x00\x00"), "linux-sg-io").errno == 9
}

test test_storage_primitives_report_unsupported_off_linux { |ctx|
  guard ! is_linux() else {
    test.skip("covers non-Linux hosts")
    return
  }
  test.error_kind(linux.sg_io(3, b"\x00\x00\x00\x00\x00\x00"), "linux-unsupported")
  test.error_kind(linux.nvme_namespace_id(3), "linux-unsupported")
  test.error_kind(linux.ata_identify(3), "linux-unsupported")
}
