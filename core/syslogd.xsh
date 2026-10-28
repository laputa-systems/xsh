#!/bin/xsh
use lib.syslog_kernel as kernel
use lib.syslog_kmsg as kmsg
use lib.syslog_limit as limit
use lib.syslog_parse as parse
use lib.syslog_render as render
use lib.syslog_socket as sock

const USAGE = """Usage: syslogd --state DIR [--socket PATH] [--kmsg PATH]
Receive local syslog datagrams on a Unix datagram socket and write one escaped
line per accepted message to standard output.

      --socket PATH  socket to publish (default /dev/log)
      --state DIR    directory that holds the exclusive syslog.lock
      --kmsg PATH    also read kernel messages from PATH (normally /dev/kmsg);
                     only messages logged after the start are read
      --help         display this help and exit

Standard output is the only sink and is meant to be captured by a supervisor.
Records carry the kernel-reported sender (pid, uid, gid) apart from the tag,
host and time the sender claims. Delivery is not lossless: the kernel drops
datagrams from a full socket queue without telling the receiver, and the
receiver drops by rate, size, descriptor passing and an unwritable sink, and
counts each. The service never writes to /dev/log itself.
"""

const DEFAULT_SOCKET = "/dev/log"
const DATAGRAM_LIMIT = 8192
const BATCH_LIMIT = 16
const REPORT_INTERVAL_US = 1000000
const SOL_SOCKET = 1
const SCM_CREDENTIALS = 2
const MSG_TRUNC = 32
const MSG_CTRUNC = 8
const EAGAIN = 11

type Options = {help: Bool, socket: Str, state: Str?, kmsg: Str?}
type Credentials = {pid: Int, uid: Int, gid: Int}

on TERM [process] {
  exit 0
}

on INT [process] {
  exit 0
}

pure credentials(control: List[LinuxControlMessage]) -> Credentials? {
  for message in control {
    if message.level == SOL_SOCKET and message.type == SCM_CREDENTIALS and message.data.len() >= 12 {
      return {
        pid: bytes.unpack_le(message.data, 4, 0) ?? 0,
        uid: bytes.unpack_le(message.data, 4, 4) ?? 0,
        gid: bytes.unpack_le(message.data, 4, 8) ?? 0,
      }
    }
  }
  null
}

proc receive(fd: Int) [process, net, error] -> Result[List[LinuxReceivedMessage], Error] {
  match linux.recvfrom(fd, DATAGRAM_LIMIT, 0, "close") {
    Ok(message) => Ok([message])
    Err(failure) => if failure.errno == EAGAIN { Ok([]) } else { Err(failure) }
  }
}

pure sender_of(found: Credentials?) -> limit.Sender? {
  if let credentials = found { return limit.Sender(uid: credentials.uid, pid: credentials.pid) }
  null
}

proc complain(text: Str) [io] {
  eprint $text
  let _ = io.flush_stderr()
}

proc announce(line: Str) [io] {
  print $line
}

proc writable(fd: Int) [process, net, error] -> Result[Bool, Error] {
  Ok("writable" in unix.poll_fd(fd, ["writable"], 0)?)
}

proc finish_line(line: Str) [process, net, io, error] {
  if writable(1)? {
    announce(line)
    if io.flush_stdout() is Err(_) { complain(line) }
  } else if writable(2)? {
    complain(line)
  }
}

type SinkCheck = {sink: kernel.Output?, removed: Bool}

proc check_sink(previous: kernel.Output?) [fs, process, net, io, error] -> Result[SinkCheck, Error] {
  if let before = previous {
    if let current = kernel.output()? {
      if ! current.linked { return Ok({sink: previous, removed: true}) }
      if current.target != before.target {
        finish_line(f"event=sink-moved from=\"{before.target}\" to=\"{current.target}\"")
        return Ok({sink: current, removed: false})
      }
    }
  }
  Ok({sink: previous, removed: false})
}

type Written = {delivered: Bool, blocked: Bool, reported_at: Int}

proc write_line(line: Str, last_report: Int) [process, net, io, time, error] -> Result[Written, Error] {
  if ! writable(1)? { return Ok({delivered: false, blocked: true, reported_at: last_report}) }
  announce(line)
  if let Err(failure) = io.flush_stdout() {
    let reported = time.boottime()? / 1000
    if reported - last_report >= REPORT_INTERVAL_US {
      complain(f"syslogd: output failed: {failure.message}")
      return Ok({delivered: false, blocked: false, reported_at: reported})
    }
    return Ok({delivered: false, blocked: false, reported_at: last_report})
  }
  Ok({delivered: true, blocked: false, reported_at: last_report})
}

pure kernel_message(record: kmsg.KernelRecord) -> parse.Parsed {
  {priority: parse.Valid, facility: record.facility, severity: record.severity, header_time: null, host: null, tag: "kernel", tag_pid: null, content: record.message}
}

proc main(...argv: List[Str]) [fs, process, net, io, time, env, error] {
  let opts: Options = cli.applet(
    argv,
    {
      gnu: {status: 1},
      help: {form: "--help", default: false, stop: true},
      socket: {form: "--socket PATH", default: DEFAULT_SOCKET},
      state: {form: "--state DIR"},
      kmsg: {form: "--kmsg PATH"},
    },
  )?
  if opts.help {
    print $USAGE
    return
  }
  let state = opts.state
  if state == null {
    complain("syslogd: --state DIR is required")
    exit 2
  }
  let socket_path = Path(opts.socket)
  process.set_signal_action("HUP", "ignore")
  let state_dir = Path(state ?? "/")
  let lock_path = Path(f"{state_dir}/syslog.lock")
  let lock = match fs.lock(lock_path, nonblocking: true) {
    Ok(held) => held
    Err(failure) => {
      complain(f"syslogd: another receiver holds {lock_path}: {failure.message}")
      exit 1
    }
  }
  defer fs.unlock(lock)

  let published = match sock.publish(socket_path) {
    Ok(socket) => socket
    Err(failure) => {
      complain(f"syslogd: cannot publish {socket_path}: {failure.message}")
      exit 1
    }
  }
  let boot_id = kernel.boot_id()?
  var limiter = limit.new_limiter(limit.DEFAULT_CONFIG)?
  var sequence = 0
  defer {
    finish_line(f"event=stop {limit.summary(limiter)}")
    match sock.unpublish(published) {
      Ok(_) => {}
      Err(failure) => complain(f"syslogd: cannot remove {socket_path}: {failure.message}")
    }
    unix.close_fd(published.fd)
  }

  let notify = e"NOTIFY_FD" ?? ""
  if ! notify.is_empty() {
    let channel = notify.parse_int()?
    assert unix.write_fd(channel, b"r")? == 1
    unix.close_fd(channel)?
  }
  let config = limit.DEFAULT_CONFIG
  finish_line(f"event=start pid={process.current_pid()?} boot=\"{boot_id}\" socket=\"{socket_path}\" state=\"{state_dir}\" receive_buffer={sock.RECEIVE_BUFFER} datagram_limit={DATAGRAM_LIMIT} batch={BATCH_LIMIT} sender_rate={config.sender_rate} sender_burst={config.sender_burst} global_rate={config.global_rate} global_burst={config.global_burst} line_limit_bytes={render.CONTENT_LIMIT} kernel_loss=unobservable")

  let kmsg_fd = match opts.kmsg {
    null => -1
    else => {
      let opened = unix.open_fd(Path(opts.kmsg ?? ""), nonblock: true)
      if opened is Err(_) {
        complain(f"syslogd: cannot open {opts.kmsg ?? ""}")
        exit 1
      }
      let descriptor = opened ?? -1
      let _ = unix.seek_fd(descriptor, 0, whence: "end")?
      descriptor
    }
  }
  var sink = kernel.output()?
  var last_report = 0
  var last_sink_check = 0
  while true {
    let watched = if kmsg_fd >= 0 { [{fd: published.fd, events: ["readable"]}, {fd: kmsg_fd, events: ["readable"]}] } else { [{fd: published.fd, events: ["readable"]}] }
    let ready = unix.poll_fds(watched)?
    if ready.is_empty() { continue }
    let checked = time.boottime()? / 1000
    if sink != null and checked - last_sink_check >= REPORT_INTERVAL_US {
      last_sink_check = checked
      let verdict = check_sink(sink)?
      sink = verdict.sink
      if verdict.removed {
        complain("syslogd: the output file was removed; stopping so the supervisor can reopen it")
        finish_line("event=sink-removed")
        exit 1
      }
    }
    if kmsg_fd >= 0 and [entry for entry in ready if entry.fd == kmsg_fd].len() > 0 {
      let batch = kmsg.drain({read: kmsg.device_read}, kmsg_fd, BATCH_LIMIT)?
      if batch.overruns > 0 { finish_line(f"event=kmsg-overrun count={batch.overruns}") }
      if batch.malformed > 0 { finish_line(f"event=kmsg-malformed count={batch.malformed}") }
      for record in batch.records {
        let now = time.boottime()?
        let admission = limit.admit(limiter, limit.Datagram(sender: null, oversize: false, control_truncated: false, rights: 0), now / 1000)
        limiter = admission.limiter
        if ! admission.accepted { continue }
        sequence += 1
        let line = render.render(render.Entry(sequence: sequence, source: "kmsg", kernel_sequence: record.sequence, kernel_microseconds: record.microseconds, boot_id: boot_id, boottime_ns: now, pid: null, uid: null, gid: null, message: kernel_message(record)))
        let written = write_line(line, last_report)?
        last_report = written.reported_at
        if written.blocked { limiter = limit.sink_dropped(limiter, .SinkBlocked, 1) } else if ! written.delivered { limiter = limit.sink_dropped(limiter, .SinkError, 1) }
      }
    }
    var drained = 0
    while drained < BATCH_LIMIT {
      let received = receive(published.fd)?
      if received.is_empty() { break }
      let datagram = received[0]
      drained += 1
      let sender = credentials(datagram.control)
      let now = time.boottime()?
      let admission = limit.admit(
        limiter,
        limit.Datagram(
          sender: sender_of(sender),
          oversize: datagram.size > datagram.data.len() or datagram.flags.bit_and(MSG_TRUNC) != 0,
          control_truncated: datagram.flags.bit_and(MSG_CTRUNC) != 0,
          rights: datagram.rights_closed,
        ),
        now / 1000,
      )
      limiter = admission.limiter
      if ! admission.accepted { continue }
      sequence += 1
      let line = render.render(render.Entry(sequence: sequence, source: "socket", kernel_sequence: null, kernel_microseconds: null, boot_id: boot_id, boottime_ns: now, pid: sender?.pid, uid: sender?.uid, gid: sender?.gid, message: parse.parse(datagram.data)))
      let written = write_line(line, last_report)?
      last_report = written.reported_at
      if written.blocked { limiter = limit.sink_dropped(limiter, .SinkBlocked, 1) } else if ! written.delivered { limiter = limit.sink_dropped(limiter, .SinkError, 1) }
    }
  }
}
