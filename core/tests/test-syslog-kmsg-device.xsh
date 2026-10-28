use support.syslog_service as svc

test test_the_kernel_message_source_opens_at_the_end_and_does_not_disturb_the_socket { |ctx|
  guard let opened = unix.open_fd(p"/dev/kmsg", nonblock: true) else {
    test.skip("/dev/kmsg is not readable by this user")
    return
  }
  unix.close_fd(opened)
  let lay = svc.layout(ctx, "kmsg")?
  let launch = svc.prepare(ctx, lay, extra: ["--kmsg", "/dev/kmsg"])?
  let command = launch.command
  let handle = spawn command?
  svc.attach(launch)
  svc.await_ready(launch)
  assert svc.send(lay.socket, b"<13>beside the kernel source")? > 0
  let _ = svc.await_containing(lay, "msg=\"beside the kernel source\"", 1)?
  svc.signal(lay, "TERM")?
  assert (wait handle?).exited_with(0)
  let replayed = [line for line in svc.log_text(lay)?.lines() if "source=kmsg" in line and "kseq=" in line and (rx"ktime_us=([0-9]+)".captures(line)[1].parse_int() ?? 0) > 0]
  assert replayed.len() < 50, "the ring buffer was replayed from its start"
}
