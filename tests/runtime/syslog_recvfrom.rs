use super::common::*;

const RECEIVER: &str = r#"proc open_fds() [fs, error] -> Result[Int, Error] {
  let names = [entry.name for entry in fs.children(Path("/proc/self/fd"))?]
  Ok(names.len())
}

proc summarize(message: LinuxReceivedMessage) -> Str {
  let kinds = [f"{m.level}:{m.type}" for m in message.control].join(",")
  f"size={message.size} data={message.data.len()} flags={message.flags} closed={message.rights_closed} control={kinds}"
}

proc main(...argv: List[Str]) [process, net, fs, io, error] {
  let socket = Path(argv[0])
  let ready = Path(argv[1])
  let count = argv[2].parse_int()?
  let rights = argv[3]
  let limit = argv[4].parse_int()?
  let c = linux.net_constants()
  if limit > 0 { process.set_rlimit("nofile", soft: limit)? }
  let server = linux.socket(c.AF_UNIX, c.SOCK_DGRAM.bit_or(c.SOCK_NONBLOCK))?
  linux.bind(server, {family: "unix", address: socket.display()})
  linux.setsockopt_int(server, c.SOL_SOCKET, c.SO_PASSCRED, 1)
  let before = open_fds()?
  ready.write("ready")
  var received = 0
  while received < count {
    let events = unix.poll_fd(server, ["readable"], 20000)?
    if events.is_empty() { print "timeout after ${received}"; break }
    let message = linux.recvfrom(server, 8192, 0, rights)?
    print summarize(message)
    received += 1
  }
  print "fds_before=${before} fds_after=${open_fds()?}"
}
"#;

const SENDER: &str = r#"import array, errno, os, socket, sys

target, scenario = sys.argv[1], sys.argv[2]
sender = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
sender.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 1 << 20)
sender.connect(target)

def send(data, count=0):
    descriptors = [os.open("/dev/null", os.O_RDONLY) for _ in range(count)]
    ancillary = []
    if descriptors:
        ancillary = [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", descriptors))]
    try:
        sender.sendmsg([data], ancillary)
    except OSError as error:
        print("send-error", count, errno.errorcode[error.errno], flush=True)
    finally:
        for descriptor in descriptors:
            os.close(descriptor)

if scenario == "mixed":
    send(b"plain")
    send(b"one", 1)
    send(b"many", 253)
    send(b"toomany", 300)
    send(b"")
    send(b"a" * 8192)
    send(b"b" * 8193)
    send(b"c" * 65536)
elif scenario == "flood":
    for index in range(int(sys.argv[3])):
        send(b"m%d" % index, 1 + index % 3)
elif scenario == "wide":
    send(b"wide", 253)
"#;

struct Run {
    lines: Vec<String>,
    sender: String,
}

fn run(scenario: &[&str], count: usize, rights: &str, limit: i64) -> Run {
    let root = temp_path(&format!("syslog-recvfrom-{}-{rights}-{limit}", scenario[0]));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create run directory");
    let socket = root.join("log");
    let ready = root.join("ready");
    let script = root.join("receiver.xsh");
    let sender = root.join("sender.py");
    std::fs::write(&script, RECEIVER).expect("write receiver");
    std::fs::write(&sender, SENDER).expect("write sender");
    let mut receiver = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(&script)
        .arg(&socket)
        .arg(&ready)
        .arg(count.to_string())
        .arg(rights)
        .arg(limit.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn receiver");
    wait_for_path(&ready, Duration::from_secs(10), &mut receiver);
    let sent = Command::new("python3")
        .arg("-I")
        .arg(&sender)
        .arg(&socket)
        .args(scenario)
        .output()
        .expect("run python3 sender");
    assert!(sent.status.success(), "{}", String::from_utf8_lossy(&sent.stderr));
    let output = receiver.wait_with_output().expect("wait receiver");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let _ = std::fs::remove_dir_all(&root);
    Run {
        lines: String::from_utf8(output.stdout).expect("utf-8").lines().map(str::to_owned).collect(),
        sender: String::from_utf8(sent.stdout).expect("utf-8"),
    }
}

fn fields(line: &str) -> std::collections::BTreeMap<&str, &str> {
    line.split_whitespace().filter_map(|field| field.split_once('=')).collect()
}

fn number(line: &str, name: &str) -> i64 {
    fields(line)[name].parse().expect("numeric field")
}

#[test]
fn recvfrom_reports_real_sizes_and_closes_every_received_descriptor() {
    let run = run(&["mixed"], 7, "close", 0);
    assert!(run.sender.contains("send-error 300"), "300 descriptors cannot be sent: {}", run.sender);
    let (messages, tail) = run.lines.split_at(7);
    let expected = [(5, 5, 0), (3, 3, 1), (4, 4, 253), (0, 0, 0), (8192, 8192, 0), (8193, 8192, 0), (65536, 8192, 0)];
    for (line, (size, length, closed)) in messages.iter().zip(expected) {
        assert_eq!(number(line, "size"), size, "{line}");
        assert_eq!(number(line, "data"), length, "{line}");
        assert_eq!(number(line, "closed"), closed, "{line}");
        let truncated = number(line, "flags") & 32 != 0;
        assert_eq!(truncated, size > length, "{line}");
        assert_eq!(number(line, "flags") & 8, 0, "{line}");
        assert_eq!(fields(line)["control"], "1:2", "credentials only: {line}");
    }
    let counts = fields(&tail[0]);
    assert_eq!(counts["fds_before"], counts["fds_after"], "{tail:?}");
}

#[test]
fn recvfrom_keep_leaves_descriptors_in_control_messages() {
    let run = run(&["wide"], 1, "keep", 0);
    let line = &run.lines[0];
    assert_eq!(number(line, "closed"), 0, "{line}");
    assert!(fields(line)["control"].contains("1:1"), "{line}");
    let counts = fields(&run.lines[1]);
    assert_eq!(
        number(&run.lines[1], "fds_after") - number(&run.lines[1], "fds_before"),
        253,
        "{counts:?}"
    );
}

#[test]
fn recvfrom_leaks_no_descriptor_across_ten_thousand_messages_with_descriptors() {
    let count = 10_000;
    let run = run(&["flood", &count.to_string()], count, "close", 0);
    assert_eq!(run.lines.len(), count + 1);
    let closed: i64 = run.lines[..count].iter().map(|line| number(line, "closed")).sum();
    let expected: usize = (0..count).map(|index| 1 + index % 3).sum();
    assert_eq!(closed, expected as i64);
    let counts = fields(&run.lines[count]);
    assert_eq!(counts["fds_before"], counts["fds_after"]);
}

#[test]
fn recvfrom_under_descriptor_exhaustion_reports_truncation_and_leaks_nothing() {
    let run = run(&["wide"], 1, "close", 24);
    let line = &run.lines[0];
    assert_ne!(number(line, "flags") & 8, 0, "MSG_CTRUNC must be reported: {line}");
    assert!(number(line, "closed") < 253, "{line}");
    let counts = fields(&run.lines[1]);
    assert_eq!(counts["fds_before"], counts["fds_after"], "{line}");
}
