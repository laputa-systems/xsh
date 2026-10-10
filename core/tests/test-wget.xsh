type Ran = {status: Int, stdout: Str, stderr: Str, dir: Path}
type Server = {base: Str, handle: ProcessHandle}

# A loopback HTTP fixture in Perl, which the host and the test image both
# carry. It answers one request per connection and publishes its port through
# a file once it is listening.
const SERVER = r"""
use strict;
use warnings;
use IO::Socket::INET;
use IO::Compress::Gzip qw(gzip $GzipError);

my ($port_file) = @ARGV;
$SIG{CHLD} = 'IGNORE';
$SIG{PIPE} = 'IGNORE';
my $server = IO::Socket::INET->new(
  LocalAddr => '127.0.0.1', LocalPort => 0, Listen => 32, ReuseAddr => 1, Proto => 'tcp',
) or die "listen: $!";
open(my $out, '>', "$port_file.tmp") or die;
print $out $server->sockport, "\n";
close $out;
rename "$port_file.tmp", $port_file or die;

sub reply {
  my ($sock, $method, $status, $reason, $headers, $body) = @_;
  $body = '' unless defined $body;
  my %seen = map { lc((split /:/, $_, 2)[0]) => 1 } @$headers;
  my $head = "HTTP/1.1 $status $reason\r\n";
  $head .= "$_\r\n" for @$headers;
  $head .= "Content-Type: text/plain\r\n" unless $seen{'content-type'};
  $head .= "Content-Length: " . length($body) . "\r\n" unless $seen{'content-length'} || $seen{'transfer-encoding'};
  $head .= "Connection: close\r\n\r\n";
  print $sock $head;
  print $sock $body unless $method eq 'HEAD';
}

while (my $client = $server->accept) {
  my $pid = fork();
  if (!defined $pid) { close $client; next; }
  if ($pid) { close $client; next; }
  close $server;
  handle($client);
  close $client;
  exit 0;
}

sub handle {
  my ($sock) = @_;
  my $line = <$sock>;
  return unless defined $line;
  $line =~ s/\r?\n$//;
  my ($method, $target) = split / /, $line;
  my (@headers, %h);
  while (defined(my $l = <$sock>)) {
    $l =~ s/\r?\n$//;
    last if $l eq '';
    push @headers, $l;
    my ($n, $v) = split /:\s*/, $l, 2;
    $h{lc $n} = $v;
  }
  my $body = '';
  read($sock, $body, $h{'content-length'}) if defined $h{'content-length'};
  my ($path) = split /\?/, $target, 2;
  if ($path eq '/echo') {
    reply($sock, $method, 200, 'OK', [], "$method $target\n" . join('', map { "$_\n" } @headers) . "--\n$body");
  } elsif ($path =~ m{^/status/(\d+)$}) {
    my %reasons = (200 => 'OK', 404 => 'Not Found', 500 => 'Internal Server Error', 503 => 'Service Unavailable');
    reply($sock, $method, $1, $reasons{$1} // 'Status', [], "status $1\n");
  } elsif ($path =~ m{^/redir/(\d+)$}) {
    if ($1 > 0) {
      reply($sock, $method, 302, 'Found', ['Location: /redir/' . ($1 - 1)], "moved\n");
    } else {
      reply($sock, $method, 200, 'OK', [], "arrived\n");
    }
  } elsif ($path =~ m{^/bytes/(\d+)$}) {
    reply($sock, $method, 200, 'OK', ['Content-Type: application/octet-stream'], 'x' x $1);
  } elsif ($path eq '/slow') {
    select(undef, undef, undef, 3);
    reply($sock, $method, 200, 'OK', [], "late\n");
  } elsif ($path eq '/auth') {
    if (($h{authorization} // '') eq 'Basic dXNlcjpwdw==') {
      reply($sock, $method, 200, 'OK', [], "authorized\n");
    } else {
      reply($sock, $method, 401, 'Unauthorized', ['WWW-Authenticate: Basic realm="fixture"'], "denied\n");
    }
  } elsif ($path eq '/gzip') {
    my $plain = "compressed body\n";
    if (($h{'accept-encoding'} // '') =~ /gzip/) {
      my $z;
      gzip(\$plain => \$z) or die $GzipError;
      reply($sock, $method, 200, 'OK', ['Content-Encoding: gzip'], $z);
    } else {
      reply($sock, $method, 200, 'OK', [], $plain);
    }
  } elsif ($path eq '/range') {
    my $all = join('', map { chr(ord('a') + $_ % 26) } 0 .. 99);
    if (($h{range} // '') =~ /^bytes=(\d+)-$/) {
      if ($1 >= 100) {
        reply($sock, $method, 416, 'Range Not Satisfiable', ['Content-Range: bytes */100'], '');
      } else {
        reply($sock, $method, 206, 'Partial Content', ["Content-Range: bytes $1-99/100"], substr($all, $1));
      }
    } else {
      reply($sock, $method, 200, 'OK', ['Accept-Ranges: bytes'], $all);
    }
  } elsif ($path eq '/cookie') {
    reply($sock, $method, 200, 'OK', ['Set-Cookie: a=b; Path=/', 'Set-Cookie: c=d; Path=/x'], "cookies\n");
  } elsif ($path eq '/chunked') {
    reply($sock, $method, 200, 'OK', ['Transfer-Encoding: chunked'], "5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n");
  } elsif ($path eq '/drop') {
    return;
  } elsif ($path =~ m{^/dir/([^/]*)$}) {
    reply($sock, $method, 200, 'OK', [], "named $1\n");
  } elsif ($path eq '/ok') {
    reply($sock, $method, 200, 'OK', ['X-Fixture: yes'], "hello\n");
  } elsif ($path eq '/') {
    reply($sock, $method, 200, 'OK', [], "root\n");
  } else {
    reply($sock, $method, 404, 'Not Found', [], "no route $path\n");
  }
}
"""

# Starts the fixture and waits for its port; null means this host cannot run
# it (no perl), which the caller reports as a skip.
proc start_server(ctx: TestContext) [fs, process, time, error] -> Result[Server?] {
  let probe = try run.text perl -e "print 1"
  guard probe is Ok(_) else { return Ok(null) }

  let root = test.temp_dir(ctx, name: "curl-server")?
  let script = fp"{root}/server.pl"
  let port_file = fp"{root}/port"
  script.write(SERVER)?
  let handle = spawn run perl $script $port_file ?
  let started = time.now()
  while !port_file.exists()? {
    if time.now() - started > 5000 {
      handle.cancel(signal: "KILL", kill_after: 0ms)?
      return Err(error.failure("the HTTP fixture did not start"))
    }
    time.sleep(20ms)?
  }
  let port = port_file.read_text()?.trim()
  Ok({base: f"http://127.0.0.1:{port}", handle: handle})
}

# Runs core/wget.xsh by its real path in a fresh working directory, feeding
# `input` on stdin and capturing both streams.
proc wget(ctx: TestContext, args: List[Str], input = b"", dir: Path? = null, timeout = 30s) [fs, process, error] -> Result[Ran] {
  let root = dir ?? test.temp_dir(ctx, name: "wget")?
  let out = fp"{test.temp_dir(ctx, name: "wget-out")?}/stdout"
  let err = fp"{test.temp_dir(ctx, name: "wget-err")?}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/wget.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, input, out, err, timeout:)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?, dir: root})
}

const SKIP = "requires perl for the loopback HTTP fixture"

# Replaces the time stamps, rates and elapsed times, which vary run to run,
# so the rest of wget's wording can be compared exactly.
pure steady(text: Str) -> Str {
  var out = rx"[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}".replace(text, with: "TS")
  out = rx"\([0-9.]+ [KMGT]?B/s\)".replace(out, with: "(RATE)")
  out = rx" +[0-9.]+[KMGT]?=[0-9.]+s".replace(out, with: " RATE=ELAPSED")
  out = rx" +[0-9.]+[KMGT]? 0s\n".replace(out, with: " RATE 0s\n")
  out = rx"clock time: [0-9.]+s".replace(out, with: "clock time: ELAPSED")
  rx"in [0-9.]+s \(".replace(out, with: "in ELAPSED (")
}

test test_wget_download_saves_under_the_remote_name { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let url = f"{fixture.base}/dir/name.txt"
  let result = wget(ctx, [url])?
  assert result.status == 0, result.stderr
  assert result.stdout == ""
  assert fp"{result.dir}/name.txt".read_text()? == "named name.txt\n"
  let host = fixture.base.byte_slice(7)
  let expected = f"--TS--  {url}\nConnecting to {host}... connected.\nHTTP request sent, awaiting response... 200 OK\nLength: 15 [text/plain]\nSaving to: 'name.txt'\n\n     0K                                                       100% RATE=ELAPSED\n\nTS (RATE) - 'name.txt' saved [15/15]\n\n"
  assert steady(result.stderr) == expected, steady(result.stderr)

  let again = wget(ctx, [url], dir: result.dir)?
  assert again.status == 0
  assert fp"{result.dir}/name.txt.1".read_text()? == "named name.txt\n"
  assert "Saving to: 'name.txt.1'\n" in again.stderr
}

test test_wget_quiet_prints_nothing_but_keeps_exit_codes { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let ok = wget(ctx, ["-q", f"{fixture.base}/dir/quiet.txt"])?
  assert ok.status == 0
  assert ok.stderr == ""
  assert fp"{ok.dir}/quiet.txt".exists()?

  let missing = wget(ctx, ["-q", f"{fixture.base}/status/404"])?
  assert missing.status == 8
  assert missing.stderr == ""
}

test test_wget_output_document_and_stdout { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let file = wget(ctx, ["-O", "chosen.txt", f"{fixture.base}/ok"])?
  assert file.status == 0
  assert fp"{file.dir}/chosen.txt".read_text()? == "hello\n"
  assert "Saving to: 'chosen.txt'\n" in file.stderr
  assert steady(file.stderr).ends_with("TS (RATE) - 'chosen.txt' saved [6/6]\n\n")

  let stdout = wget(ctx, ["-qO-", f"{fixture.base}/ok"])?
  assert stdout.status == 0
  assert stdout.stdout == "hello\n"
  assert stdout.stderr == ""

  let verbose = wget(ctx, ["-O", "-", f"{fixture.base}/ok"])?
  assert verbose.stdout == "hello\n"
  assert "Saving to: 'STDOUT'\n" in verbose.stderr
  assert steady(verbose.stderr).ends_with("TS (RATE) - written to stdout [6/6]\n\n"), verbose.stderr
}

test test_wget_server_errors_exit_8 { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = wget(ctx, [f"{fixture.base}/status/404"])?
  assert result.status == 8
  assert fp"{result.dir}/404".exists()? == false
  assert steady(result.stderr).ends_with("HTTP request sent, awaiting response... 404 Not Found\nTS ERROR 404: Not Found.\n\n"), result.stderr

  let unauthorized = wget(ctx, [f"{fixture.base}/auth"])?
  assert unauthorized.status == 6
  assert unauthorized.stderr.ends_with("401 Unauthorized\n\nUsername/Password Authentication Failed.\n"), unauthorized.stderr
}

test test_wget_network_failures_exit_4 { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let refused = wget(ctx, ["http://127.0.0.1:1/x"])?
  assert refused.status == 4
  assert steady(refused.stderr) == "--TS--  http://127.0.0.1:1/x\nConnecting to 127.0.0.1:1... failed: Connection refused.\n", refused.stderr

  let missing = wget(ctx, ["http://nonexistent.invalid/x"])?
  assert missing.status == 4
  assert steady(missing.stderr) == "--TS--  http://nonexistent.invalid/x\nResolving nonexistent.invalid (nonexistent.invalid)... failed: Name does not resolve.\nwget: unable to resolve host address 'nonexistent.invalid'\n", missing.stderr

  let slow = wget(ctx, ["-t", "1", "-T", "1", f"{fixture.base}/slow"])?
  assert slow.status == 4
  assert steady(slow.stderr).ends_with("HTTP request sent, awaiting response... Read error (Operation timed out) in headers.\nGiving up.\n\n"), slow.stderr
}

test test_wget_retries_an_empty_reply_then_gives_up { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = wget(ctx, ["-t", "2", f"{fixture.base}/drop"])?
  assert result.status == 4
  let text = steady(result.stderr)
  assert "HTTP request sent, awaiting response... No data received.\nRetrying.\n\n--TS--  (try: 2)  " in text, text
  assert text.ends_with("No data received.\nGiving up.\n\n"), text
}

test test_wget_spider_checks_without_saving { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let found = wget(ctx, ["--spider", f"{fixture.base}/ok"])?
  assert found.status == 0
  assert found.stderr.starts_with("Spider mode enabled. Check if remote file exists.\n")
  assert found.stderr.ends_with("Remote file exists and could contain further links,\nbut recursion is disabled -- not retrieving.\n\n"), found.stderr
  assert fp"{found.dir}/ok".exists()? == false

  let missing = wget(ctx, ["--spider", f"{fixture.base}/status/404"])?
  assert missing.status == 8
}

test test_wget_prefix_creates_the_directory { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = wget(ctx, ["-P", "sub/dir", f"{fixture.base}/dir/third.txt"])?
  assert result.status == 0, result.stderr
  assert fp"{result.dir}/sub/dir/third.txt".read_text()? == "named third.txt\n"
  assert "Saving to: 'sub/dir/third.txt'\n" in result.stderr
}

test test_wget_continue_resumes_a_partial_file { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let root = test.temp_dir(ctx, name: "wget-resume")?
  fp"{root}/range".write("abcdefghijklmnopqrstuvwxyzabcdefghijklmn")?
  let result = wget(ctx, ["-c", f"{fixture.base}/range"], dir: root)?
  assert result.status == 0, result.stderr
  assert "awaiting response... 206 Partial Content\nLength: 100, 60 remaining [text/plain]\n" in result.stderr, result.stderr
  assert fp"{root}/range".read_bytes()?.len() == 100
  assert steady(result.stderr).ends_with("TS (RATE) - 'range' saved [100/100]\n\n"), result.stderr

  let complete = wget(ctx, ["-c", f"{fixture.base}/range"], dir: root)?
  assert complete.status == 0
  assert complete.stderr.ends_with("416 Range Not Satisfiable\n\n    The file is already fully retrieved; nothing to do.\n\n"), complete.stderr
  assert fp"{root}/range".read_bytes()?.len() == 100
}

test test_wget_request_options_reach_the_server { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = wget(ctx, ["-qO-", "--header=X-A: 1", "--header", "X-B: 2", "-U", "agent/9", "--referer=http://ref.test/", "--user=user", "--password=pw", f"{fixture.base}/echo"])?
  assert result.status == 0, result.stderr
  assert result.stdout.starts_with("GET /echo\n"), result.stdout
  assert "\nX-A: 1\n" in result.stdout
  assert "\nX-B: 2\n" in result.stdout
  assert "\nUser-Agent: agent/9\n" in result.stdout
  assert "\nReferer: http://ref.test/\n" in result.stdout
  assert "\nAuthorization: Basic dXNlcjpwdw==\n" in result.stdout

  let plain = wget(ctx, ["-qO-", f"{fixture.base}/echo"])?
  assert "\nUser-Agent: Wget/1.25.0\n" in plain.stdout, plain.stdout
  assert "\nAccept-Encoding: identity\n" in plain.stdout

  let post = wget(ctx, ["-qO-", "--post-data=a=1&b=2", f"{fixture.base}/echo"])?
  assert post.stdout.starts_with("POST /echo\n"), post.stdout
  assert post.stdout.ends_with("\n--\na=1&b=2"), post.stdout
}

test test_wget_server_response_and_no_verbose { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let shown = wget(ctx, ["-S", "-O", "-", f"{fixture.base}/ok"])?
  assert shown.stdout == "hello\n"
  assert "awaiting response... \n  HTTP/1.1 200 OK\n  X-Fixture: yes\n  Content-Type: text/plain\n  Content-Length: 6\n  Connection: close\nLength: 6 [text/plain]\n" in shown.stderr, shown.stderr

  let terse = wget(ctx, ["-nv", f"{fixture.base}/dir/terse.txt"])?
  assert terse.status == 0
  assert steady(terse.stderr) == f"TS URL:{fixture.base}/dir/terse.txt [16/16] -> \"terse.txt\" [1]\n", terse.stderr
}

test test_wget_dot_progress_rows { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = wget(ctx, ["-O", "/dev/null", f"{fixture.base}/bytes/200000"])?
  assert result.status == 0, result.stderr
  assert "Length: 200000 (195K) [application/octet-stream]\n" in result.stderr
  let rows = [line for line in result.stderr.lines() if line.ends_with(" 0s") or "=" in line]
  assert rows.len() == 4, result.stderr
  assert rx"^     0K \.{10} \.{10} \.{10} \.{10} \.{10} 25% +[0-9.]+[KMGT] 0s$".matches(rows[0]), rows[0]
  assert rx"^    50K \.{10} \.{10} \.{10} \.{10} \.{10} 51% +[0-9.]+[KMGT] 0s$".matches(rows[1]), rows[1]
  assert rx"^   100K \.{10} \.{10} \.{10} \.{10} \.{10} 76% +[0-9.]+[KMGT] 0s$".matches(rows[2]), rows[2]
  assert rx"^   150K \.{10} \.{10} \.{10} \.{10} \.{5} {5}100% +[0-9.]+[KMGT]=[0-9.]+s$".matches(rows[3]), rows[3]

  let small = wget(ctx, ["-O", "/dev/null", f"{fixture.base}/bytes/1500"])?
  assert "Length: 1500 (1.5K) [application/octet-stream]\n" in small.stderr
  assert "     0K .                                                     100% " in small.stderr, small.stderr
}

test test_wget_several_urls_report_totals_and_first_failure { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = wget(ctx, ["-O", "all.txt", f"{fixture.base}/status/404", f"{fixture.base}/ok"])?
  assert result.status == 8
  assert fp"{result.dir}/all.txt".read_text()? == "hello\n"
  let text = steady(result.stderr)
  assert "FINISHED --TS--\nTotal wall clock time: ELAPSED\nDownloaded: 1 files, 6 in ELAPSED (RATE)\n" in text, text
}

test test_wget_input_file_lists_urls { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let root = test.temp_dir(ctx, name: "wget-input")?
  fp"{root}/urls".write(f"{fixture.base}/dir/one.txt\n{fixture.base}/dir/two.txt\n")?
  let result = wget(ctx, ["-q", "-i", "urls"], dir: root)?
  assert result.status == 0, result.stderr
  assert fp"{root}/one.txt".read_text()? == "named one.txt\n"
  assert fp"{root}/two.txt".read_text()? == "named two.txt\n"
}

test test_wget_no_clobber_and_redirect_limit { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let root = test.temp_dir(ctx, name: "wget-clobber")?
  fp"{root}/ok".write("old\n")?
  let kept = wget(ctx, ["-nc", f"{fixture.base}/ok"], dir: root)?
  assert kept.status == 0
  assert kept.stderr == "File 'ok' already there; not retrieving.\n\n", kept.stderr
  assert fp"{root}/ok".read_text()? == "old\n"

  let followed = wget(ctx, ["-qO-", f"{fixture.base}/redir/2"])?
  assert followed.stdout == "arrived\n"

  let capped = wget(ctx, ["--max-redirect=1", "-O", "capped.txt", f"{fixture.base}/redir/3"])?
  assert capped.status == 8
  assert capped.stderr.ends_with("1 redirections exceeded.\n"), capped.stderr
}

test test_wget_usage_errors { |ctx|
  let missing = wget(ctx, [])?
  assert missing.status == 1
  assert missing.stderr == "wget: missing URL\nUsage: wget [OPTION]... [URL]...\n\nTry `wget --help' for more options.\n", missing.stderr

  let unknown = wget(ctx, ["--bogus", "http://x/"])?
  assert unknown.status == 2
  assert unknown.stderr == "wget: unrecognized option '--bogus'\nUsage: wget [OPTION]... [URL]...\n\nTry `wget --help' for more options.\n", unknown.stderr

  let unsupported = wget(ctx, ["-r", "http://x/"])?
  assert unsupported.status == 2
  assert unsupported.stderr == "wget: -r: this option is not supported by this implementation\n"

  let bad_number = wget(ctx, ["--tries=many", "http://x/"])?
  assert bad_number.status == 2
  assert bad_number.stderr == "wget: --tries: Invalid number 'many'.\n"

  let ftp = wget(ctx, ["ftp://127.0.0.1/file"])?
  assert ftp.status == 1
  assert ftp.stderr == "ftp://127.0.0.1/file: Unsupported scheme 'ftp'.\n"
}

test test_wget_unwritable_destinations { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let output = wget(ctx, ["-O", "/nonexistent/dir/out", f"{fixture.base}/ok"])?
  assert output.status == 1
  assert output.stderr == "/nonexistent/dir/out: No such file or directory\n", output.stderr
}

test test_wget_version_and_help { |ctx|
  let version = wget(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("GNU Wget 1.25.0 "), version.stdout

  let help = wget(ctx, ["-h"])?
  assert help.status == 0
  assert "Usage: wget [OPTION]... [URL]...\n" in help.stdout
}

# openssl generates a throwaway CA and an `s_server` with a leaf for 127.0.0.1;
# both tools are optional, so the TLS test skips without them.
type TlsFixture = {url: Str, ca: Path, handle: ProcessHandle}

proc start_tls_server(ctx: TestContext) [fs, process, time, error] -> Result[TlsFixture?] {
  let openssl = try run.text openssl version
  let perl = try run.text perl -e "print 1"
  guard openssl is Ok(_) and perl is Ok(_) else { return Ok(null) }

  let root = test.temp_dir(ctx, name: "wget-tls")?
  let authority = run.capture --text openssl req -x509 -newkey rsa:2048 -nodes -keyout ${root}/ca.key -out ${root}/ca.pem -days 2 -subj "/CN=Fixture CA" -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign"
  guard authority.status.ok else { return Ok(null) }
  let leaf = run.capture --text openssl req -newkey rsa:2048 -nodes -keyout ${root}/leaf.key -out ${root}/leaf.csr -subj "/CN=localhost"
  guard leaf.status.ok else { return Ok(null) }
  fp"{root}/ext.cnf".write("subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n")?
  let signed = run.capture --text openssl x509 -req -in ${root}/leaf.csr -CA ${root}/ca.pem -CAkey ${root}/ca.key -CAcreateserial -out ${root}/leaf.pem -days 2 -extfile ${root}/ext.cnf
  guard signed.status.ok else { return Ok(null) }

  let number = run.text perl -MIO::Socket::INET -e r"my $s = IO::Socket::INET->new(Listen => 1, LocalAddr => '127.0.0.1', LocalPort => 0) or die; print $s->sockport"
  let accept = f"127.0.0.1:{number.trim()}"
  let handle = spawn run openssl s_server -accept $accept -cert ${root}/leaf.pem -key ${root}/leaf.key -www -quiet > /dev/null ?
  let started = time.now()
  while time.now() - started < 5000 {
    let probe = run.capture --text openssl s_client -connect $accept -brief < /dev/null
    if probe.status.ok { return Ok({url: f"https://{accept}/", ca: fp"{root}/ca.pem", handle: handle}) }
    time.sleep(100ms)?
  }
  handle.cancel(signal: "KILL", kill_after: 0ms)?
  Ok(null)
}

test test_wget_tls_verification { |ctx|
  let started = start_tls_server(ctx)?
  guard let tls = started else { test.skip("requires openssl and perl for the TLS fixture"); return }
  defer tls.handle.cancel(kill_after: 100ms)

  let untrusted = wget(ctx, ["-O", "/dev/null", tls.url])?
  assert untrusted.status == 5, untrusted.stderr
  assert "ERROR: cannot verify 127.0.0.1's certificate" in untrusted.stderr
  assert "To connect to 127.0.0.1 insecurely, use `--no-check-certificate'.\n" in untrusted.stderr

  let insecure = wget(ctx, ["--no-check-certificate", "-q", "-O", "-", tls.url])?
  assert insecure.status == 0, insecure.stderr
  assert "s_server" in insecure.stdout

  let trusted = wget(ctx, [f"--ca-certificate={tls.ca}", "-q", "-O", "-", tls.url])?
  assert trusted.status == 0, trusted.stderr
}
