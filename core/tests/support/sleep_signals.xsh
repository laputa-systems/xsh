##! Readiness for tests that signal a running sleep applet. A signal sent
##! before sleep has replaced the runtime's handlers is swallowed, so a test
##! waits until /proc shows the disposition it is about to exercise.

pure signal_mask_has(mask: Str, number: Int) -> Bool {
  let digits = mask.trim()
  let at = digits.count_chars() - 1 - (number - 1) / 4
  let digit = "0123456789abcdef".find(digits.split("")[at])
  digit != null and digit.bit_and([1, 2, 4, 8][(number - 1) % 4]) != 0
}

pure sleeping_image(cmdline: Str) -> Bool {
  cmdline.find("sleep.xsh") != null and cmdline.find("env.xsh") == null
}

proc proc_status_field(pid: Int, field: Str) [fs, error] -> Result[Str?, Error] {
  let status = fp"/proc/{pid}/status"
  return null when ! status.exists()

  for line in status.read_text()?.lines() {
    if line.starts_with(f"{field}:") {
      return line.split(":")[1].trim()
    }
  }
  null
}

## Waits up to 20s for `pid` to be the blocked sleep image with `signal` set or clear in `field` (SigCgt or SigIgn).
export proc await_sleeping_with(pid: Int, signal: Str, field: Str, expect_set: Bool) [fs, process, time, error] -> Result[Bool, Error] {
  let number = process.signal(signal)?.number
  let deadline = time.monotonic()? + 20000000000

  while time.monotonic()? < deadline {
    let cmdline = fp"/proc/{pid}/cmdline"
    let wchan = fp"/proc/{pid}/wchan"
    if cmdline.exists() and wchan.exists() and sleeping_image(cmdline.read_text()?) {
      if let mask = proc_status_field(pid, field)? {
        if signal_mask_has(mask, number) == expect_set and wchan.read_text()?.trim() != "0" {
          return true
        }
      }
    }
    time.sleep(10ms)
  }
  false
}
