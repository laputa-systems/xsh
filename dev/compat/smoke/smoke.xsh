#!/bin/xsh --
# Clean-image smoke: real workflows through the installed applets.
#
# Runs as /bin/xsh /smoke/smoke.xsh inside an image that holds nothing but the
# static interpreter, the staged applets and a passwd/group pair, with PATH set
# to /usr/bin. Every command below is an ordinary program found on PATH, so a
# workflow that passes proves the applet starts, resolves its `use lib.*`
# imports and does its job with no help from any other userland.
#
# Each workflow reports pass, fail or skip. A skip always names its reason: an
# applet that is not installed, a capability the container does not grant, or
# state the host does not expose. The exit status is non-zero when any
# workflow failed.

error Verdict = Skip(reason: Str) | Fail(reason: Str)

type Ran = {status: Int, stdout: Str, stderr: Str}

type Applet = {name: Str, source: Str}

type Alias = {name: Str, target: Str}

type Manifest = {applets: List[Applet], aliases: List[Alias], libraries: List[Str]}

const MANIFEST = p"/smoke/applets.json"
const BIN_DIR = p"/usr/bin"

var passed = 0
var failed = 0
var skipped = 0
var scratch_counter = 0

let scratch_root = fs.tempdir()?
defer scratch_root.close()
let scratch = scratch_root.host_path()?

# Prints the verdict for one workflow as it finishes so a hang is attributable.
proc record(name: Str, outcome: Result[Unit]) {
  match outcome {
    Ok(_) => {
      passed += 1
      print f"PASS  {name}"
    }
    Err(Verdict.Skip {reason}) => {
      skipped += 1
      print f"SKIP  {name}: {reason}"
    }
    Err(Verdict.Fail {reason}) => {
      failed += 1
      print f"FAIL  {name}: {reason}"
    }
    Err(error) => {
      failed += 1
      let first = error.message.lines().get(0) ?? error.message
      print f"FAIL  {name}: {first}"
    }
  }
}

proc fail(reason: Str) -> Result[Unit] {
  Err(Verdict.Fail(reason: reason))
}

proc skip(reason: Str) -> Result[Unit] {
  Err(Verdict.Skip(reason: reason))
}

# Output comparisons do not stop a workflow: each returns the mismatch notes
# (empty when it holds) for the workflow to accumulate, so one wrong line does
# not hide the rest of what the same workflow exercises. A step that cannot
# continue (a command that must succeed) still ends the workflow at once.
pure check(cond: Bool, what: Str) -> List[Str] {
  if cond { [] } else { [what] }
}

# Compares exact text; the message shows both sides because the point of the
# smoke is that the output is right, not merely that a command exited 0.
pure eq(what: Str, got: Str, want: Str) -> List[Str] {
  if got == want {
    []
  } else {
    [f"{what}: got `{got.trim().replace("\n", with: "\\n")}` want `{want.trim().replace("\n", with: "\\n")}`"]
  }
}

pure has(what: Str, text: Str, needle: Str) -> List[Str] {
  if needle in text {
    []
  } else {
    [f"{what}: `{needle}` not in `{text.trim().replace("\n", with: "\\n")}`"]
  }
}

# Turns the notes a workflow accumulated into its verdict.
proc settle(notes: List[Str]) -> Result[Unit] {
  if notes.is_empty() {
    return Ok()
  }
  fail(notes.join("; "))
}

# Runs argv with stdin bytes and captures both streams; the exit status is
# data. A command that cannot start at all is a failure of the workflow.
proc cap(argv: List[Str], input: Bytes = b"") -> Result[Ran] {
  let out = run.capture --text --timeout=60s @argv < $input
  Ok({status: out.status.shell_code()?, stdout: out.stdout, stderr: out.stderr})
}

pure describe(argv: List[Str], r: Ran) -> Str {
  let err = r.stderr.trim().replace("\n", with: " | ")
  f"`{argv.join(" ")}` exited {r.status}: {err}"
}

# Runs argv, requires status 0 and returns stdout.
proc ok(argv: List[Str], input: Bytes = b"") -> Result[Str] {
  let r = cap(argv, input)?
  if r.status != 0 {
    return Err(Verdict.Fail(reason: describe(argv, r)))
  }
  r.stdout
}

# Notes a command whose exit status is not the expected one; the command is
# run with cap so the caller can still compare its output.
pure status_note(argv: List[Str], r: Ran, want: Int) -> List[Str] {
  if r.status == want {
    []
  } else {
    [f"{describe(argv, r)} (wanted {want})"]
  }
}

# Runs argv for its effect and requires status 0.
proc step(argv: List[Str], input: Bytes = b"") -> Result[Unit] {
  let r = cap(argv, input)?
  if r.status != 0 {
    return fail(describe(argv, r))
  }
  Ok()
}

proc workdir(name: Str) -> Result[Path] {
  scratch_counter += 1
  let dir = fp"{scratch}/{scratch_counter}-{name}"
  dir.mkdir()
  dir
}

proc installed(name: Str) -> Bool {
  fp"{BIN_DIR}/{name}".exists() ?? false
}

# A workflow whose applet has not been merged into the tree yet is a skip.
proc need(names: List[Str]) -> Result[Unit] {
  for name in names {
    if !installed(name) {
      return skip(f"{name}: not yet merged")
    }
  }
  Ok()
}

proc sorted_lines(text: Str) -> List[Str] {
  text.lines() |> sort-by { |l| l }
}

proc tree_names(dir: Str, kind: Str) -> Result[List[Str]] {
  let out = ok(["find", dir, "-type", kind])?
  Ok(sorted_lines(out.replace(dir, with: "")))
}

# Compares two trees by content with find and cmp, so the archive workflows do
# not depend on diff -r (which has its own workflow).
proc same_tree(label: Str, a: Str, b: Str) -> Result[List[Str]] {
  var notes: List[Str] = []
  let files = tree_names(a, "f")?
  notes += eq(f"{label}: file names", files.join(","), tree_names(b, "f")?.join(","))
  for name in files {
    if !name.is_empty() {
      let r = cap(["cmp", f"{a}{name}", f"{b}{name}"])?
      notes += check(r.status == 0, f"{label}: {name} differs: {r.stdout.trim()}")
    }
  }
  notes += eq(f"{label}: symlink names", tree_names(a, "l")?.join(","), tree_names(b, "l")?.join(","))
  Ok(notes)
}

# Workflow: file manipulation.
proc wf_files() -> Result[Unit] {
  var notes: List[Str] = []
  let w = workdir("files")?
  cd $w {
    step(["mkdir", "-p", "tree/sub/deep"])?
    step(["touch", "tree/a", "tree/sub/b"])?
    fp"{w}/tree/sub/deep/data".write("hello\n")
    step(["cp", "-r", "tree", "copy"])?
    let found = ok(["find", "copy", "-type", "f"])?
    notes += eq("find after cp -r", sorted_lines(found).join("\n"), "copy/a\ncopy/sub/b\ncopy/sub/deep/data")
    step(["mv", "copy", "moved"])?
    notes += eq("ls after mv", ok(["ls", "moved"])?, "a\nsub\n")
    notes += check(!fp"{w}/copy".exists()?, "mv left the source behind")
    step(["ln", "-s", "tree/sub/deep/data", "soft"])?
    notes += eq("readlink", ok(["readlink", "soft"])?, "tree/sub/deep/data\n")
    notes += eq("read through symlink", ok(["cat", "soft"])?, "hello\n")
    step(["ln", "tree/sub/deep/data", "hard"])?
    notes += eq("link count", ok(["stat", "-c", "%h", "hard"])?, "2\n")
    notes += eq("size", ok(["stat", "-c", "%s", "hard"])?, "6\n")
    step(["chmod", "600", "hard"])?
    notes += eq("mode", ok(["stat", "-c", "%a", "hard"])?, "600\n")
    let listing = ok(["find", "tree", "-name", "[ab]", "-type", "f"])?
    notes += eq("find -name", sorted_lines(listing).join(","), "tree/a,tree/sub/b")
    let counted = ok(["xargs", "-n", "1", "basename"], b"tree/a\ntree/sub/b\n")?
    notes += eq("xargs", counted, "a\nb\n")
    step(["rm", "-r", "moved"])?
    notes += check(!fp"{w}/moved".exists()?, "rm -r left the tree")
    let missing = cap(["rm", "nothing-here"])?
    notes += status_note(["rm", "nothing-here"], missing, 1)
    notes += has("rm diagnostic", missing.stderr, "nothing-here")
    step(["rm", "-f", "nothing-here"])?
  }
  settle(notes)
}

# Workflow: tar with each compressor, and extraction compared to the source.
proc wf_archives() -> Result[Unit] {
  var notes: List[Str] = []
  let w = workdir("archives")?
  cd $w {
    step(["mkdir", "-p", "src/dir"])?
    fp"{w}/src/one.txt".write("first file\n")
    fp"{w}/src/dir/two.txt".write("second file\nwith two lines\n")
    step(["ln", "-s", "one.txt", "src/link"])?

    for kind in [{flag: "-z", name: "gzip", ext: "tgz"}, {flag: "-J", name: "xz", ext: "txz"}, {flag: "-j", name: "bzip2", ext: "tbz"}] {
      let packed = f"out.{kind.ext}"
      step(["tar", "-c", kind.flag, "-f", packed, "src"])?
      let names = sorted_lines(ok(["tar", "-t", "-f", packed])?)
      notes += eq(f"tar {kind.name} listing", names.join(","), "src/,src/dir/,src/dir/two.txt,src/link,src/one.txt")
      let into = f"x-{kind.ext}"
      step(["mkdir", into])?
      step(["tar", "-x", "-f", packed, "-C", into])?
      notes += same_tree(f"tar {kind.name}", f"{w}/src", f"{w}/{into}/src")?
      notes += eq(f"tar {kind.name} symlink", ok(["readlink", f"{into}/src/link"])?, "one.txt\n")
    }

    let zst = run.capture --text tar -cf - src | run zstd -q > out.tar.zst
    notes += check(zst.status.ok, f"tar | zstd: {zst.stderr.trim()}")
    let zinto = "x-zst"
    step(["mkdir", zinto])?
    let unz = run.capture --text zstd -dc out.tar.zst | run tar -xf - -C x-zst
    notes += check(unz.status.ok, f"zstd -d | tar -x: {unz.stderr.trim()}")
    notes += same_tree("tar zstd", f"{w}/src", f"{w}/x-zst/src")?
  }
  settle(notes)
}

# Workflow: cpio in newc format, the initramfs shape.
proc wf_cpio() -> Result[Unit] {
  var notes: List[Str] = []
  let w = workdir("cpio")?
  cd $w {
    step(["mkdir", "-p", "root/etc", "root/sbin"])?
    fp"{w}/root/etc/hostname".write("smoke\n")
    fp"{w}/root/sbin/init".write("#!/bin/xsh\nprint \"init\"\n", mode: 0o755)
    cd root {
      let list = ok(["find", "."])?
      let listing = bytes.from_text(list)
      let packed = run.capture --text cpio -o -H newc < $listing > ../initrd.cpio
      notes += check(packed.status.ok, f"cpio -o failed: {packed.stderr.trim()}")
    }
    let table = sorted_lines(ok(["cpio", "-t"], fp"{w}/initrd.cpio".read_bytes()?)?)
    notes += eq("cpio -t", table.join(","), ".,etc,etc/hostname,sbin,sbin/init")
    step(["mkdir", "unpacked"])?
    cd unpacked {
      step(["cpio", "-i", "-d", "-m"], fp"{w}/initrd.cpio".read_bytes()?)?
    }
    notes += same_tree("cpio", f"{w}/root", f"{w}/unpacked")?
    notes += eq("init mode", ok(["stat", "-c", "%a", "unpacked/sbin/init"])?, "755\n")
  }
  settle(notes)
}

proc wf_checksums() -> Result[Unit] {
  var notes: List[Str] = []
  let w = workdir("checksums")?
  cd $w {
    fp"{w}/abc".write("abc")
    fp"{w}/digits".write("123456789")
    notes += eq("md5sum", ok(["md5sum", "abc"])?, "900150983cd24fb0d6963f7d28e17f72  abc\n")
    notes += eq("sha1sum", ok(["sha1sum", "abc"])?, "a9993e364706816aba3e25717850c26c9cd0d89d  abc\n")
    notes += eq(
      "sha256sum",
      ok(["sha256sum", "abc"])?,
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  abc\n",
    )
    notes += eq(
      "b2sum",
      ok(["b2sum", "abc"])?,
      "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923  abc\n",
    )
    notes += eq("cksum", ok(["cksum", "digits"])?, "930766865 9 digits\n")

    fp"{w}/sums".write(ok(["sha256sum", "abc", "digits"])?)
    notes += eq("sha256sum -c", ok(["sha256sum", "-c", "sums"])?, "abc: OK\ndigits: OK\n")
    fp"{w}/b2sums".write(ok(["b2sum", "abc", "digits"])?)
    notes += eq("b2sum -c", ok(["b2sum", "-c", "b2sums"])?, "abc: OK\ndigits: OK\n")

    fp"{w}/digits".write("123456780")
    let bad = cap(["sha256sum", "-c", "sums"])?
    notes += status_note(["sha256sum", "-c", "sums"], bad, 1)
    notes += eq("tampered -c stdout", bad.stdout, "abc: OK\ndigits: FAILED\n")
    notes += has("tampered -c stderr", bad.stderr, "1 computed checksum did NOT match")
  }
  settle(notes)
}

proc wf_text() -> Result[Unit] {
  var notes: List[Str] = []
  let w = workdir("text")?
  cd $w {
    fp"{w}/log".write("web 200 12\ndb 500 7\nweb 404 3\nweb 200 40\ndb 200 9\n")
    notes += eq("grep -c", ok(["grep", "-c", "web", "log"])?, "3\n")
    notes += eq("grep -v -n", ok(["grep", "-v", "-n", "web", "log"])?, "2:db 500 7\n5:db 200 9\n")
    notes += eq("sed s", ok(["sed", "s/200/OK/;2q", "log"])?, "web OK 12\ndb 500 7\n")
    notes += eq("sed -n p", ok(["sed", "-n", "3p", "log"])?, "web 404 3\n")
    notes += eq("awk sum", ok(["awk", "$1==\"web\"{s+=$3} END{print s}", "log"])?, "55\n")
    notes += eq("cut", ok(["cut", "-d", " ", "-f", "1,3", "log"])?, "web 12\ndb 7\nweb 3\nweb 40\ndb 9\n")
    notes += eq("tr", ok(["tr", "a-z", "A-Z"], b"web\n")?, "WEB\n")
    notes += eq("wc -l", ok(["wc", "-l", "log"])?, "5 log\n")
    notes += eq("head", ok(["head", "-n", "2", "log"])?, "web 200 12\ndb 500 7\n")
    notes += eq("tail", ok(["tail", "-n", "1", "log"])?, "db 200 9\n")
    notes += eq("sort -k3n", ok(["sort", "-k3,3n", "log"])?, "web 404 3\ndb 500 7\ndb 200 9\nweb 200 12\nweb 200 40\n")
    let hist = run.text cut -d " " -f 1 log | run sort | run uniq -c
    notes += eq("cut | sort | uniq -c", hist, "      2 db\n      3 web\n")

    fp"{w}/old".write("one\ntwo\nthree\nfour\n")
    fp"{w}/new".write("one\n2\nthree\nfour\nfive\n")
    let delta = cap(["diff", "-u", "old", "new"])?
    notes += status_note(["diff", "-u", "old", "new"], delta, 1)
    notes += has("diff -u hunk", delta.stdout, "-two\n+2\n")
    fp"{w}/change.patch".write(delta.stdout)
    step(["cp", "old", "patched"])?
    step(["patch", "patched", "change.patch"])?
    notes += eq("patched content", ok(["cat", "patched"])?, "one\n2\nthree\nfour\nfive\n")
    step(["cmp", "patched", "new"])?
  }
  settle(notes)
}

proc wf_diff_recursive() -> Result[Unit] {
  var notes: List[Str] = []
  let w = workdir("diff")?
  cd $w {
    step(["mkdir", "-p", "a/sub", "b/sub"])?
    fp"{w}/a/sub/f".write("same\n")
    fp"{w}/b/sub/f".write("same\n")
    fp"{w}/a/only".write("left\n")
    let r = cap(["diff", "-r", "a", "b"])?
    notes += status_note(["diff", "-r", "a", "b"], r, 1)
    notes += eq("diff -r", r.stdout, "Only in a: only\n")
    fp"{w}/b/sub/f".write("changed\n")
    fp"{w}/a/only".remove()
    let c = cap(["diff", "-ru", "a", "b"])?
    notes += status_note(["diff", "-ru", "a", "b"], c, 1)
    notes += has("diff -ru header", c.stdout, "--- a/sub/f")
    notes += has("diff -ru hunk", c.stdout, "-same\n+changed\n")
    step(["cp", "-r", "a", "c"])?
    let same = cap(["diff", "-r", "a", "c"])?
    notes += status_note(["diff", "-r", "a", "c"], same, 0)
  }
  settle(notes)
}

proc wf_compression() -> Result[Unit] {
  var notes: List[Str] = []
  let w = workdir("compression")?
  cd $w {
    var body = ""
    for i in range(500) {
      body += f"line {i} of the compression round trip\n"
    }
    fp"{w}/plain".write(body)
    for tool in [
      {name: "gzip", ext: "gz", back: "gunzip", cat: "zcat"},
      {name: "bzip2", ext: "bz2", back: "bunzip2", cat: "bzcat"},
      {name: "xz", ext: "xz", back: "unxz", cat: "xzcat"},
      {name: "zstd", ext: "zst", back: "unzstd", cat: "zstdcat"},
      {name: "lzma", ext: "lzma", back: "unlzma", cat: "lzcat"},
    ] {
      let packed = f"plain.{tool.ext}"
      step([tool.name, "-k", "-f", "plain"])?
      if tool.name == "zstd" {
        let named = cap(["zstd", "-q", "-f", "plain", "-o", "named.zst"])?
        notes += status_note(["zstd", "-q", "-f", "plain", "-o", "named.zst"], named, 0)
      }
      notes += check(fp"{w}/{packed}".exists()?, f"{tool.name} produced no {packed}")
      let size = fp"{w}/{packed}".metadata()?.size
      notes += check(size > 0 and size < body.byte_len(), f"{tool.name} did not shrink the input ({size} bytes)")
      notes += eq(f"{tool.cat}", ok([tool.cat, packed])?, body)
      step(["rm", "plain"])?
      step([tool.back, packed])?
      notes += eq(f"{tool.back} round trip", fp"{w}/plain".read_text()?, body)
    }
  }
  settle(notes)
}

proc wf_processes() -> Result[Unit] {
  var notes: List[Str] = []
  let w = workdir("processes")?
  let marker = "4242"
  let worker = spawn run sleep $marker ?
  defer worker.cancel()
  time.sleep(500ms)

  let ps = ok(["ps", "-p", f"{worker.pid}", "-o", "pid,args"])?
  notes += has("ps -o args", ps, f"{worker.pid}")
  notes += has("ps shows the command", ps, f"sleep {marker}")
  let found = ok(["pgrep", "-f", f"sleep {marker}"])?
  notes += check(f"{worker.pid}" in found.lines(), f"pgrep -f did not list pid {worker.pid}: {found.trim()}")

  step(["kill", "-TERM", f"{worker.pid}"])?
  let st = wait worker?
  notes += check(st.signaled() and st.signal_number()? == 15, "worker did not die from SIGTERM")

  let limited = cap(["timeout", "1", "sleep", "30"])?
  notes += status_note(["timeout", "1", "sleep", "30"], limited, 124)
  notes += eq("timeout stdout", limited.stdout, "")
  let killed = cap(["timeout", "-s", "KILL", "1", "sleep", "30"])?
  notes += status_note(["timeout", "-s", "KILL", "1", "sleep", "30"], killed, 137)
  notes += eq("timeout -s KILL stdout", killed.stdout, "")
  let passed_through = cap(["timeout", "5", "false"])?
  notes += status_note(["timeout", "5", "false"], passed_through, 1)

  notes += eq("nice -n 5", ok(["nice", "-n", "5", "nice"])?, "5\n")
  let before = ok(["nice"])?
  notes += check(before.trim().parse_int()? >= 0, "nice reported a negative niceness")

  let me = process.current_pid()?
  let allowed = ok(["taskset", "-pc", f"{me}"])?
  let first_cpu = allowed.trim().split(":")[1].trim().split(",")[0].split("-")[0].trim()
  notes += eq("taskset -c", ok(["taskset", "-c", first_cpu, "nproc"])?, "1\n")

  let sig = ok(["kill", "-l"])?
  notes += check("TERM" in sig and "KILL" in sig, "kill -l does not list TERM and KILL")
  settle(notes)
}

# Serves one canned HTTP response on a loopback port picked by the kernel.
# The listener is in this process so the workflow needs no server program; the
# client is the curl applet, which is what is under test. A plain http:// URL
# needs no TLS roots, so the first attempt omits -k; when that attempt dies
# before connecting it is recorded and retried with -k so the rest of the
# transfer is still exercised.
proc wf_curl() -> Result[Unit] {
  var notes: List[Str] = []
  need(["curl"])?
  let c = linux.net_constants()
  let listener = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(listener)
  linux.bind(listener, {family: "inet", address: "127.0.0.1", port: 0})?
  linux.listen(listener)?
  let port = linux.getsockname(listener)?.port
  let w = workdir("curl")?
  let body = "served by the smoke test\n"
  let headers = fp"{w}/headers"
  let saved = fp"{w}/body"
  let client_err = fp"{w}/curl.err"
  let url = f"http://127.0.0.1:{port}/index.txt"
  let attempts: List[List[Str]] = [[], ["-k"]]
  var served = false
  for flags in attempts {
    if served {
      continue
    }
    let client = spawn run curl -sS @flags -D $headers -o $saved $url 2> $client_err ?
    # curl runs concurrently; a wedged client must not hang the whole smoke.
    let pending = unix.poll_fd(listener, ["readable"], 5000)?
    if pending.is_empty() {
      client.cancel()
      let reason = client_err.read_text() ?? ""
      notes += [f"`curl {flags.join(" ")} {url}` never connected: {reason.trim()}"]
      continue
    }
    let conn = linux.accept(listener)?
    defer unix.close_fd(conn.fd)
    let request = linux.recvfrom(conn.fd, 4096)?.data.utf8() ?? ""
    let reply = f"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {body.byte_len()}\r\nConnection: close\r\n\r\n{body}"
    let _ = linux.sendto(conn.fd, bytes.from_text(reply))?
    linux.shutdown(conn.fd, c.SHUT_WR)?
    let st = wait client?
    notes += check(st.ok, f"curl exited {st.shell_code()?}")
    notes += has("request line", request, "GET /index.txt HTTP/1.1")
    notes += eq("downloaded body", saved.read_text()?, body)
    notes += has("response status", headers.read_text()?, "200 OK")
    served = true
  }
  if !served {
    notes += ["no curl attempt reached the loopback listener"]
  }
  settle(notes)
}

proc wf_ping() -> Result[Unit] {
  var notes: List[Str] = []
  need(["ping"])?
  let r = cap(["ping", "-c", "2", "-W", "2", "127.0.0.1"])?
  if r.status != 0 {
    if "ermission" in r.stderr or "not permitted" in r.stderr {
      return skip(f"ICMP sockets are not permitted here: {r.stderr.trim()}")
    }
    return fail(describe(["ping", "-c", "2", "127.0.0.1"], r))
  }
  notes += has("ping transmitted", r.stdout, "2 packets transmitted, 2 received, 0% packet loss")
  notes += has("ping reply", r.stdout, "64 bytes from 127.0.0.1: icmp_seq=1")
  settle(notes)
}

# Workflows that need a netcat, ss, and the legacy net-tools front ends are
# written against the OpenBSD netcat and iproute2 command lines.
proc wf_nc() -> Result[Unit] {
  var notes: List[Str] = []
  need(["nc"])?
  let w = workdir("nc")?
  # The kernel picks a free port; it is released again before nc listens.
  let c = linux.net_constants()
  let probe = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  linux.bind(probe, {family: "inet", address: "127.0.0.1", port: 0})?
  let port = f"{linux.getsockname(probe)?.port}"
  unix.close_fd(probe)

  let received = fp"{w}/received"
  let listener = spawn run nc -l 127.0.0.1 $port > $received ?
  time.sleep(500ms)
  let sent = run.capture --text --timeout=10s nc -N 127.0.0.1 $port < b"through the loopback\n"
  notes += check(sent.status.ok, f"nc client exited {sent.status.shell_code()?}: {sent.stderr.trim()}")
  let done = wait listener?
  notes += check(done.ok, f"nc listener exited {done.shell_code()?}")
  notes += eq("nc payload", received.read_text()?, "through the loopback\n")

  # A closed port is refused with the netcat status 1.
  let closed = cap(["nc", "-z", "127.0.0.1", port])?
  notes += status_note(["nc", "-z", "127.0.0.1", port], closed, 1)
  settle(notes)
}

proc wf_ss() -> Result[Unit] {
  var notes: List[Str] = []
  need(["ss"])?
  let c = linux.net_constants()
  let listener = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(listener)
  linux.bind(listener, {family: "inet", address: "127.0.0.1", port: 0})?
  linux.listen(listener)?
  let port = linux.getsockname(listener)?.port
  let out = ok(["ss", "-ltn"])?
  notes += has("ss -ltn", out, f"127.0.0.1:{port}")
  settle(notes)
}

proc wf_ip() -> Result[Unit] {
  var notes: List[Str] = []
  need(["ip"])?
  let addr = ok(["ip", "addr", "show", "dev", "lo"])?
  notes += has("ip addr lo", addr, "127.0.0.1")
  let route = ok(["ip", "route"])?
  # The first default route is cross-checked against the kernel table, which
  # lists the gateway as little-endian hex.
  let table = fp"/proc/net/route".read_text()?
  var want = ""
  for line in table.lines() {
    let f = line.fields()
    if f.len() > 3 and f[1] == "00000000" and f[2] != "00000000" {
      let hex = f[2]
      let octets = [hex.byte_slice(6, 2), hex.byte_slice(4, 2), hex.byte_slice(2, 2), hex.byte_slice(0, 2)]
      var parts: List[Str] = []
      for o in octets {
        parts += [f"{f"0x{o}".parse_int()?}"]
      }
      want = f"default via {parts.join(".")}"
      break
    }
  }
  if want == "" {
    return skip("the container has no IPv4 default route")
  }
  notes += has("ip route default gateway", route, want)
  settle(notes)
}

proc wf_ethtool() -> Result[Unit] {
  var notes: List[Str] = []
  need(["ethtool"])?
  let settings = ok(["ethtool", "lo"])?
  notes += has("ethtool lo", settings, "Settings for lo:\n")
  notes += has("ethtool lo link", settings, "\tLink detected: yes\n")
  let driver = cap(["ethtool", "-i", "lo"])?
  notes += status_note(["ethtool", "-i", "lo"], driver, 71)
  notes += has("ethtool -i lo", driver.stderr, "Cannot get driver information: Not supported")
  let missing = cap(["ethtool", "smoke-no-such-dev"])?
  notes += check(missing.status != 0, "ethtool on a missing device exited 0")
  settle(notes)
}

proc wf_iw() -> Result[Unit] {
  var notes: List[Str] = []
  need(["iw"])?
  let r = cap(["iw", "dev"])?
  notes += status_note(["iw", "dev"], r, 0)
  # A container has no wireless interfaces; the listing is empty, not an error.
  if fp"/sys/class/ieee80211".exists()? and !(fs.children(p"/sys/class/ieee80211")? |> map .name).is_empty() {
    return skip("this host has wireless interfaces; the exact listing is device specific")
  }
  notes += eq("iw dev without wireless interfaces", r.stdout, "")
  settle(notes)
}

proc wf_net_tools() -> Result[Unit] {
  var notes: List[Str] = []
  need(["ifconfig", "route", "arp"])?
  let out = ok(["ifconfig", "lo"])?
  notes += has("ifconfig lo", out, "127.0.0.1")
  settle(notes)
}

proc wf_kernel() -> Result[Unit] {
  var notes: List[Str] = []
  let uname_s = ok(["uname", "-s"])?
  notes += eq("uname -s", uname_s, "Linux\n")
  let release = fp"/proc/sys/kernel/osrelease".read_text()?
  notes += eq("uname -r", ok(["uname", "-r"])?, release)
  let machine = ok(["uname", "-m"])?
  notes += check(machine.trim() != "", "uname -m is empty")

  let cpuinfo = ok(["lscpu"])?
  notes += has("lscpu architecture", cpuinfo, "Architecture:")
  let count = ok(["nproc", "--all"])?.trim()
  var cpus = ""
  for line in cpuinfo.lines() {
    if line.starts_with("CPU(s):") {
      cpus = line.split(":")[1].trim()
    }
  }
  notes += eq("lscpu CPU(s) against nproc --all", cpus, count)

  let mem = ok(["free"])?
  let head = mem.lines()[0]
  notes += has("free header", head, "total")
  let total = mem.lines()[1].fields()
  notes += check(total[0] == "Mem:" and total[1].parse_int()? > 0, f"free Mem row: {mem.lines()[1]}")

  let root = ok(["findmnt", "-n", "-o", "TARGET", "/"])?
  notes += eq("findmnt /", root, "/\n")

  let modules = ok(["lsmod"])?
  notes += check(modules.lines()[0].starts_with("Module"), f"lsmod header: {modules.lines()[0]}")
  settle(notes)
}

proc wf_lsblk() -> Result[Unit] {
  var notes: List[Str] = []
  let r = cap(["lsblk"])?
  if r.status != 0 {
    return fail(describe(["lsblk"], r))
  }
  notes += check(r.stdout.lines()[0].starts_with("NAME"), f"lsblk header: {r.stdout.lines()[0]}")
  settle(notes)
}

proc wf_dmesg() -> Result[Unit] {
  var notes: List[Str] = []
  let r = cap(["dmesg"])?
  if r.status == 0 {
    notes += check(r.stdout.trim() != "", "dmesg printed nothing")
    return settle(notes)
  }
  if r.status == 1 and r.stderr.starts_with("dmesg:") and "not permitted" in r.stderr {
    return skip(f"kernel log is restricted: {r.stderr.trim()}")
  }
  fail(describe(["dmesg"], r))
}

proc wf_disk() -> Result[Unit] {
  need(["sfdisk", "fdisk", "mkfs.fat", "fsck.fat", "fatlabel", "blkid", "wipefs", "truncate"])?
  var notes: List[Str] = []
  let w = workdir("disk")?
  cd $w {
    # A DOS partition table written by sfdisk and read back by sfdisk, fdisk,
    # blkid and wipefs.
    step(["truncate", "-s", "64M", "disk.img"])?
    let layout = "label: dos\nlabel-id: 0x5a5a5a5a\nunit: sectors\n\ndisk.img1 : start=2048, size=65536, type=c\ndisk.img2 : start=67584, size=61440, type=83\n"
    step(["sfdisk", "disk.img"], bytes.from_text(layout))?
    let dump = ok(["sfdisk", "-d", "disk.img"])?
    notes += has("sfdisk -d label", dump, "label: dos\nlabel-id: 0x5a5a5a5a\n")
    notes += has("sfdisk -d part 1", dump, "disk.img1 : start=        2048, size=       65536, type=c\n")
    notes += has("sfdisk -d part 2", dump, "disk.img2 : start=       67584, size=       61440, type=83\n")
    let table = ok(["fdisk", "-l", "disk.img"])?
    notes += has("fdisk -l label type", table, "Disklabel type: dos\n")
    notes += has("fdisk -l identifier", table, "Disk identifier: 0x5a5a5a5a\n")
    notes += has("fdisk -l partition", table, "W95 FAT32 (LBA)")
    notes += has("blkid partition table", ok(["blkid", "disk.img"])?, "PTTYPE=\"dos\"")
    notes += has("wipefs lists the table", ok(["wipefs", "disk.img"])?, "dos")
    step(["wipefs", "-a", "disk.img"])?
    let bare = cap(["blkid", "disk.img"])?
    notes += status_note(["blkid", "disk.img"], bare, 2)
    notes += eq("blkid after wipefs", bare.stdout, "")

    # A FAT16 filesystem created, checked, identified and wiped.
    step(["truncate", "-s", "16M", "fat.img"])?
    step(["mkfs.fat", "-F", "16", "-n", "SMOKE", "-i", "1a2b3c4d", "fat.img"])?
    step(["fsck.fat", "-n", "fat.img"])?
    let id = ok(["blkid", "fat.img"])?
    notes += has("blkid type", id, "TYPE=\"vfat\"")
    notes += has("blkid label", id, "LABEL=\"SMOKE\"")
    notes += has("blkid uuid", id, "UUID=\"1A2B-3C4D\"")
    notes += eq("fatlabel", ok(["fatlabel", "fat.img"])?, "SMOKE\n")
    step(["fatlabel", "fat.img", "RENAMED"])?
    notes += eq("fatlabel after rename", ok(["fatlabel", "fat.img"])?, "RENAMED\n")
    step(["wipefs", "-a", "fat.img"])?
    let blank = cap(["blkid", "fat.img"])?
    notes += status_note(["blkid", "fat.img"], blank, 2)
    notes += eq("blkid after wipefs", blank.stdout, "")
    notes += eq("wipefs after wipefs -a", ok(["wipefs", "fat.img"])?, "")
  }
  settle(notes)
}

proc wf_smart() -> Result[Unit] {
  var notes: List[Str] = []
  need(["smartctl"])?
  let version = ok(["smartctl", "--version"])?
  notes += check(version.lines()[0].starts_with("smartctl 7."), f"smartctl --version banner: {version.lines()[0]}")
  notes += has("smartctl --version copyright", version, "Bruce Allen, Christian Franke, www.smartmontools.org")

  # Device nodes are what smartctl scans; a container has none.
  let scan = ok(["smartctl", "--scan"])?
  notes += eq("smartctl --scan without device nodes", scan, "")

  let missing = cap(["smartctl", "-i", "/dev/sdz"])?
  notes += status_note(["smartctl", "-i", "/dev/sdz"], missing, 2)
  notes += has("smartctl open failure", missing.stdout, "Smartctl open device: /dev/sdz failed:")
  settle(notes)
}

proc wf_smart_device() -> Result[Unit] {
  need(["smartctl", "nvme"])?
  skip("identify and log pages need a block or NVMe device node; the container has none and the applets' fake-device seam exists only inside the in-process test harness")
}

# The listing comes from sysfs, so a container shows the host's controllers
# (without /dev nodes, as nvme-cli does); only its shape can be asserted.
proc wf_nvme() -> Result[Unit] {
  var notes: List[Str] = []
  let listing = ok(["nvme", "list"])?
  let lines = listing.lines()
  notes += check(lines.len() >= 2, "nvme list printed no header")
  notes += eq("nvme list columns", lines[0].fields().join(","), "Node,Generic,SN,Model,Namespace,Usage,Format,FW,Rev")
  notes += check(lines[1].starts_with("-----"), f"nvme list separator: {lines[1]}")
  settle(notes)
}

proc wf_namespaces() -> Result[Unit] {
  var notes: List[Str] = []
  need(["unshare", "nsenter", "lsns", "setpriv", "prlimit"])?
  let probe = cap(["unshare", "-U", "-r", "id", "-u"])?
  if probe.status != 0 {
    return skip(f"user namespaces are not available (re-run with --userns): {probe.stderr.trim()}")
  }
  notes += eq("unshare -U -r id -u", probe.stdout, "0\n")
  let mapped = ok(["unshare", "-U", "-r", "cat", "/proc/self/uid_map"])?
  notes += check(mapped.fields().len() == 3 and mapped.fields()[0] == "0", f"uid_map {mapped.trim()}")

  # A fresh network namespace holds the loopback device and never the
  # container's own interface.
  let devices = ok(["unshare", "-U", "-r", "-n", "cat", "/proc/net/dev"])?
  let names = devices.lines() |> where { |l| ":" in l } |> map { |l| l.split(":")[0].trim() }
  notes += check("lo" in names and !("eth0" in names), f"devices in a new network namespace: {names.join(",")}")

  let dump = ok(["setpriv", "--dump"])?
  notes += has("setpriv --dump", dump, "uid: ")
  notes += has("setpriv --dump bounding", dump, "bounding set:")
  let locked = ok(["setpriv", "--no-new-privs", "setpriv", "--dump"])?
  notes += has("setpriv --no-new-privs", locked, "no_new_privs: 1")

  let limited = ok(["prlimit", "--nofile=256:512", "--", "prlimit", "--nofile", "--noheadings"])?
  notes += eq("prlimit --nofile", limited.fields().join(" "), "NOFILE max number of open files 256 512 files")

  # nsenter joins the user namespace of a process another unshare created.
  let held = spawn run unshare -U -r sleep 31337 ?
  defer held.cancel()
  time.sleep(1s)
  let mine = fp"/proc/self/ns/user".readlink()?
  var target = ""
  for pid in ok(["pgrep", "-f", "sleep 31337"])?.lines() {
    let theirs = fp"/proc/{pid}/ns/user".readlink() ?? mine
    if target == "" and theirs != mine {
      target = pid
    }
  }
  if target == "" {
    notes += ["no process of the held unshare is in a new user namespace"]
  } else {
    let entered = ok(["nsenter", "-t", target, "-U", "--preserve-credentials", "id", "-u"])?
    notes += eq("nsenter -U id -u", entered, "0\n")
    let listed = ok(["lsns", "-t", "user"])?
    notes += check(listed.lines()[0].fields()[0] == "NS", f"lsns header: {listed.lines()[0]}")
    notes += has("lsns lists the new user namespace", listed, f" {target} ")
  }
  settle(notes)
}

# efivarfs entries are a 4-byte attribute word (non-volatile, boot services,
# runtime) followed by the value; boot options are EFI_LOAD_OPTION structures.
proc efi_var(dir: Path, name: Str, value: Bytes) -> Result[Unit] {
  let attributes = bytes.pack_le(7, 4)?
  fp"{dir}/{name}-8be4df61-93ca-11d2-aa0d-00e098032b8c".write(bytes.concat([attributes, value]))
  Ok()
}

proc efi_ucs2(text: Str) -> Result[Bytes] {
  var ints: List[Int] = []
  for byte in bytes.from_text(text) {
    ints += [byte, 0]
  }
  Ok(bytes.concat([bytes.from_ints(ints)?, b"\x00\x00"]))
}

proc efi_node(kind: Int, subtype: Int, data: Bytes) -> Result[Bytes] {
  Ok(bytes.concat([bytes.from_ints([kind, subtype])?, bytes.pack_le(data.len() + 4, 2)?, data]))
}

proc efi_load_option(active: Bool, label: Str, loader: Str) -> Result[Bytes] {
  let nodes = bytes.concat([efi_node(4, 4, efi_ucs2(loader)?)?, b"\x7f\xff\x04\x00"])
  let flags = if active { 1 } else { 0 }
  Ok(bytes.concat([bytes.pack_le(flags, 4)?, bytes.pack_le(nodes.len(), 2)?, efi_ucs2(label)?, nodes]))
}

proc wf_efi() -> Result[Unit] {
  var notes: List[Str] = []
  need(["efibootmgr"])?
  let w = workdir("efi")?
  let vars = fp"{w}/efivars"
  vars.mkdir()
  efi_var(vars, "BootOrder", bytes.concat([bytes.pack_le(2, 2)?, bytes.pack_le(1, 2)?]))?
  efi_var(vars, "Timeout", bytes.pack_le(5, 2)?)?
  efi_var(vars, "Boot0001", efi_load_option(true, "Smoke Linux", "\\EFI\\smoke\\grub.efi")?)?
  efi_var(vars, "Boot0002", efi_load_option(false, "Smoke Shell", "\\shell.efi")?)?
  env EFIVARFS_PATH=$vars EFIBOOTMGR_ALLOW_ANY_DIR=1 {
    let out = ok(["efibootmgr"])?
    notes += has("efibootmgr Timeout", out, "Timeout: 5 seconds\n")
    notes += has("efibootmgr BootOrder", out, "BootOrder: 0002,0001\n")
    notes += has("efibootmgr active entry", out, "Boot0001* Smoke Linux\t\\EFI\\smoke\\grub.efi\n")
    notes += has("efibootmgr inactive entry", out, "Boot0002  Smoke Shell\t\\shell.efi\n")

    step(["efibootmgr", "-t", "9", "-o", "0001,0002"])?
    let after = ok(["efibootmgr"])?
    notes += has("efibootmgr -t", after, "Timeout: 9 seconds\n")
    notes += has("efibootmgr -o", after, "BootOrder: 0001,0002\n")
    step(["efibootmgr", "-b", "0002", "-B"])?
    let gone = ok(["efibootmgr"])?
    notes += check(!("Boot0002" in gone), f"efibootmgr -B left Boot0002: {gone.trim()}")
  }
  settle(notes)
}

# Every applet must start in the image: its `use lib.*` imports resolve and its
# --help path runs. Exit statuses other than 0 are only accepted where the
# reference tool itself answers an unknown --help that way.
const HELP_STATUS: Map[Int] = {false: 1, iw: 1, mdev: 1, nc: 1, ping: 2, ping6: 2, tracepath: 255}
const SWEEP_SKIP = ["halt", "poweroff", "reboot", "switch_root", "pivot_root", "login", "getty", "su", "passwd"]

proc wf_startup(manifest: Manifest) -> Result[Unit] {
  var names: List[Str] = [a.name for a in manifest.applets]
  for alias in manifest.aliases {
    names += [alias.name]
  }
  var broken: List[Str] = []
  var tried = 0
  for name in names |> sort-by { |n| n } {
    if name in SWEEP_SKIP {
      continue
    }
    tried += 1
    let r = cap([name, "--help"])?
    let want = HELP_STATUS.get(name) ?? 0
    let crashed = "err[" in r.stderr or "\nexecutable: " in r.stderr or r.stderr.starts_with("err:")
    if r.status != want or crashed {
      let first = r.stderr.lines().get(0) ?? ""
      broken += [f"{name} (status {r.status}): {first}"]
    }
  }
  if !broken.is_empty() {
    return fail(f"{broken.len()} of {tried} applets did not start: {broken.join("; ")}")
  }
  Ok()
}

# The image must contain nothing but XSH: the interpreter, applet scripts that
# name it, their library modules, and the account files.
proc wf_clean_image(manifest: Manifest) -> Result[Unit] {
  var notes: List[Str] = []
  var expected: List[Str] = [a.name for a in manifest.applets]
  for alias in manifest.aliases {
    expected += [alias.name]
  }
  let seen = fs.children(BIN_DIR)? |> map .name
  let extra = [n for n in seen if n != "lib" and !(n in expected)]
  let missing = [n for n in expected if !(n in seen)]
  notes += check(extra.is_empty(), f"/usr/bin holds programs that are not XSH applets: {extra.join(" ")}")
  notes += check(missing.is_empty(), f"/usr/bin lacks staged applets: {missing.join(" ")}")

  let bin = fs.children(p"/bin")? |> map .name
  notes += eq("/bin contents", bin.join(","), "xsh")

  let libs = fs.children(fp"{BIN_DIR}/lib")? |> map { |e| f"lib/{e.name}" }
  notes += eq("library modules", sorted_lines(libs.join("\n")).join(","), sorted_lines(manifest.libraries.join("\n")).join(","))

  # Every executable file anywhere in the image is the interpreter or a script
  # whose shebang names it; no file carries an ELF header or any other
  # interpreter.
  var strangers: List[Str] = []
  for top in fs.children(p"/")? {
    if top.name in ["proc", "sys", "dev", "tmp", "smoke"] or top.kind != "dir" {
      continue
    }
    for entry in fs.walk(top.path, hidden: true)? {
      if entry.kind != "file" or entry.path == p"/bin/xsh" {
        continue
      }
      let head = entry.path.read_bytes()?.slice(0, 20)
      if head.starts_with(b"\x7fELF") {
        strangers += [f"{entry.path}"]
      } else if head.starts_with(b"#!") and !head.starts_with(b"#!/bin/xsh --\n") {
        strangers += [f"{entry.path}"]
      }
    }
  }
  notes += check(strangers.is_empty(), f"files that are not XSH: {strangers.join(" ")}")

  let r = cap(["which", "sh"])?
  notes += check(r.status != 0, f"`which sh` found a shell at {r.stdout.trim()}")
  for foreign in ["sh", "bash", "busybox", "perl", "python3", "awk-gnu"] {
    notes += check(!fp"/bin/{foreign}".exists()? and !fp"/usr/bin/{foreign}".exists()?, f"{foreign} is present in the image")
  }
  settle(notes)
}

e"LC_ALL" = "C"
e"TZ" = "UTC"
let manifest: Manifest = json.read(MANIFEST)?.require()?

record("clean-image", wf_clean_image(manifest))
record("startup-sweep", wf_startup(manifest))
record("files", wf_files())
record("archives-tar", wf_archives())
record("archives-cpio", wf_cpio())
record("checksums", wf_checksums())
record("text-pipelines", wf_text())
record("text-diff-recursive", wf_diff_recursive())
record("compression", wf_compression())
record("processes-signals", wf_processes())
record("net-ping", wf_ping())
record("net-curl-loopback", wf_curl())
record("net-nc", wf_nc())
record("net-ss", wf_ss())
record("net-ip", wf_ip())
record("net-ethtool", wf_ethtool())
record("net-iw", wf_iw())
record("net-ifconfig-route-arp", wf_net_tools())
record("kernel-inspection", wf_kernel())
record("kernel-lsblk", wf_lsblk())
record("kernel-dmesg", wf_dmesg())
record("disk-partition-fat", wf_disk())
record("smart-cli", wf_smart())
record("smart-nvme-device", wf_smart_device())
record("nvme", wf_nvme())
record("namespaces", wf_namespaces())
record("efi-boot-entries", wf_efi())

print f"smoke: {passed} passed, {failed} failed, {skipped} skipped"
if failed > 0 {
  exit 1
}
