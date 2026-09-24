# Full consumption of two fixed rfkill records after each directory read.
# The pinned container stages the committed fixture at `/sys/class/rfkill`.
proc main() [io, env, error] {
  var checksum = 0
  var round = 0
  while round < 200 {
    let devices = linux.rfkill_list()?.collect()
    if devices.len() != 2 {
      return error.fail("rfkill fixture must have two devices")
    }
    if devices[0].id != 2 or devices[0].name != "Wi-Fi" or devices[0].type != "wlan" or !devices[0].soft_blocked or devices[0].hard_blocked {
      return error.fail("rfkill2 fixture mismatch")
    }
    if devices[1].id != 10 or devices[1].name != "Bluetooth" or devices[1].type != "bluetooth" or devices[1].soft_blocked or !devices[1].hard_blocked {
      return error.fail("rfkill10 fixture mismatch")
    }
    checksum = checksum + devices[0].id + devices[1].id
    round = round + 1
  }
  print $checksum
}
