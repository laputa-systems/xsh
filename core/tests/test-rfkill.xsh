use core.lib.hardware

test test_rfkill_selection_and_block_state_rendering {
  let devices: List[LinuxRfkill] = [
    {id: 0, name: "phy0", type: "wlan", soft_blocked: false, hard_blocked: true},
    {id: 2, name: "hci0", type: "bluetooth", soft_blocked: true, hard_blocked: false},
  ]
  assert hardware.rfkill_select(devices, ["wifi"])? == [devices[0]]
  assert hardware.rfkill_select(devices, ["2"])? == [devices[1]]
  let lines = hardware.rfkill_list_lines(devices)
  assert lines[0] == "0: phy0: Wireless LAN"
  assert lines[1] == "\tSoft blocked: no"
  assert lines[2] == "\tHard blocked: yes"
  assert hardware.rfkill_select(devices, ["not-a-radio"] ) is Err(_)
}

test test_rfkill_block_uses_fake_native_operation { |ctx|
  let devices: List[LinuxRfkill] = [{id: 7, name: "phy7", type: "wlan", soft_blocked: false, hard_blocked: false}]
  let log = test.temp_path(ctx, name: "rfkill.jsonl")
  test.linux_fake(ctx, {log: log})
  hardware.rfkill_set(devices, ["7"], true)?
  assert "\"op\":\"rfkill_block\"" in log.read_text()?
  assert "\"id\":\"7\"" in log.read_text()?
}

test test_rfkill_raw_escaping_and_json_numeric_ids {
  let devices: List[LinuxRfkill] = [{id: 7, name: "Radio name", type: "wlan", soft_blocked: false, hard_blocked: true}]
  let raw = hardware.rfkill_table_lines(devices, ["ID", "DEVICE", "HARD"], false, true)?
  assert raw == ["7 Radio\\x20name blocked"]
  let parsed = json.decode(hardware.rfkill_json(devices, ["ID", "DEVICE", "SOFT", "HARD"])?)?
  assert json.get(parsed, ["rfkilldevices", 0, "id"])?.require(Int)? == 7
  assert json.get(parsed, ["rfkilldevices", 0, "hard"])?.require(Str)? == "blocked"
  assert hardware.radio_columns("MISSING") is Err(_)
}

test test_rfkill_invalid_command_is_rejected_before_inventory { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/rfkill.xsh"
  let err = fp"{root}/err"
  let command = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "toggle", "all"], root, {}, b"", fp"{root}/out", err, timeout: 5s)
  assert process.run(command)?.exited_with(1)
  assert "unsupported rfkill command" in err.read_text()?
}
