use core.lib.http_transfer

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

# Runs core/curl.xsh by its real path in a fresh working directory, feeding
# `input` on stdin and capturing both streams.
proc curl(ctx: TestContext, args: List[Str], input = b"", dir: Path? = null, timeout = 20s) [fs, process, error] -> Result[Ran] {
  let root = dir ?? test.temp_dir(ctx, name: "curl")?
  let out = fp"{test.temp_dir(ctx, name: "curl-out")?}/stdout"
  let err = fp"{test.temp_dir(ctx, name: "curl-err")?}/stderr"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/curl.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, input, out, err, timeout:)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?, dir: root})
}

# The progress meter's second-to-last and last lines are timing dependent, so
# compare them by shape.
const METER_HEAD = "  % Total    % Received % Xferd  Average Speed  Time    Time    Time   Current\n                                 Dload  Upload  Total   Spent   Left   Speed\n"

const SKIP = "requires perl for the loopback HTTP fixture"

# The failure cause as a name, so tests can state the classification.
pure cause(failure: http_transfer.TransferFailure) -> Str {
  match failure {
    http_transfer.TransferFailure.Dns => "dns"
    http_transfer.TransferFailure.Connect => "connect"
    http_transfer.TransferFailure.ConnectTimeout => "connect-timeout"
    http_transfer.TransferFailure.Timeout => "timeout"
    http_transfer.TransferFailure.Certificate => "certificate"
    http_transfer.TransferFailure.Handshake => "handshake"
    http_transfer.TransferFailure.EmptyReply => "empty-reply"
    http_transfer.TransferFailure.Redirects => "redirects"
    http_transfer.TransferFailure.Status {code} => f"status-{code}"
    http_transfer.TransferFailure.Scheme => "scheme"
    http_transfer.TransferFailure.Write => "write"
    http_transfer.TransferFailure.TrustStore => "trust-store"
    http_transfer.TransferFailure.Other => "other"
  }
}

test test_curl_classifies_net_failure_messages {
  assert cause(http_transfer.classify("Connection refused (os error 111)", true)) == "connect"
  assert cause(http_transfer.classify("failed to lookup address information: Name does not resolve", true)) == "dns"
  assert cause(http_transfer.classify("request timed out", true)) == "timeout"
  assert cause(http_transfer.classify("TCP connection establishment timed out", true)) == "connect-timeout"
  assert cause(http_transfer.classify("request dispatch failed", true)) == "empty-reply"
  assert cause(http_transfer.classify("too many redirects", true)) == "redirects"
  assert cause(http_transfer.classify("HTTP status 404", true)) == "status-404"
  assert cause(http_transfer.classify("TLS negotiation or certificate validation failed", true)) == "certificate"
  assert cause(http_transfer.classify("TLS negotiation or certificate validation failed", false)) == "handshake"
  assert cause(http_transfer.classify("no certificates found", true)) == "trust-store"
  assert cause(http_transfer.classify("URL scheme must be http or https", true)) == "scheme"
  assert cause(http_transfer.classify("No such file or directory (os error 2)", true)) == "write"
}

test test_curl_url_helpers {
  assert http_transfer.url_scheme("HTTPS://h/p") == "https"
  assert http_transfer.url_scheme("h/p") == null
  assert http_transfer.url_host("http://user:pw@example.test:8080/a/b?q=1") == "example.test"
  assert http_transfer.url_port("http://example.test:8080/") == 8080
  assert http_transfer.url_port("https://example.test/") == 443
  assert http_transfer.url_authority("http://user:pw@example.test:8080/a") == "example.test:8080"
  assert http_transfer.url_path("http://example.test") == "/"
  assert http_transfer.url_file_name("http://example.test/a/b.txt?x=1#f") == "b.txt"
  assert http_transfer.url_file_name("http://example.test/a/") == ""
  assert http_transfer.percent_encode(b"a b&c", true) == "a+b%26c"
  assert http_transfer.percent_encode(b"a b", false) == "a%20b"
}

test test_curl_get_writes_the_body_to_stdout { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-s", f"{fixture.base}/ok"])?
  assert result.status == 0, result.stderr
  assert result.stdout == "hello\n"
  assert result.stderr == ""
}

test test_curl_progress_meter_for_a_file_download { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-o", "out.txt", f"{fixture.base}/ok"])?
  assert result.status == 0, result.stderr
  assert result.stdout == ""
  assert fp"{result.dir}/out.txt".read_text()? == "hello\n"
  assert result.stderr.starts_with(METER_HEAD), result.stderr
  let rows = result.stderr.byte_slice(METER_HEAD.byte_len()).split("\r")
  assert rows.len() == 3, result.stderr
  assert rows[0] == ""
  assert rows[1] == "  0      0   0      0   0      0      0      0                              0", rows[1]
  assert rx"^100      6 100      6   0      0 [ 0-9.kMG]{6}      0 {30}0\n$".matches(rows[2]), rows[2]
}

test test_curl_silent_hides_meter_and_show_error_restores_errors { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let quiet = curl(ctx, ["-s", "-f", f"{fixture.base}/status/404"])?
  assert quiet.status == 22
  assert quiet.stdout == ""
  assert quiet.stderr == ""

  let loud = curl(ctx, ["-sS", "-f", f"{fixture.base}/status/404"])?
  assert loud.status == 22
  assert loud.stderr == "curl: (22) The requested URL returned error: 404\n", loud.stderr

  let meter = curl(ctx, ["-f", f"{fixture.base}/status/404"])?
  assert meter.status == 22
  assert meter.stderr.starts_with(METER_HEAD)
  assert meter.stderr.ends_with("curl: (22) The requested URL returned error: 404\n"), meter.stderr
}

test test_curl_http_errors_are_data_without_fail { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let plain = curl(ctx, ["-s", f"{fixture.base}/status/404"])?
  assert plain.status == 0
  assert plain.stdout == "status 404\n"

  let body = curl(ctx, ["-sS", "--fail-with-body", f"{fixture.base}/status/404"])?
  assert body.status == 22
  assert body.stdout == "status 404\n"
  assert body.stderr == "curl: (22) The requested URL returned error: 404\n"
}

test test_curl_fail_writes_no_output_file { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-sf", "-o", "never.txt", f"{fixture.base}/status/500"])?
  assert result.status == 22
  assert !fp"{result.dir}/never.txt".exists()?

  let kept = curl(ctx, ["-s", "-o", "error.txt", f"{fixture.base}/status/500"])?
  assert kept.status == 0
  assert fp"{kept.dir}/error.txt".read_text()? == "status 500\n"
}

test test_curl_include_and_head_show_the_response_head { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let included = curl(ctx, ["-s", "-i", f"{fixture.base}/ok"])?
  assert included.status == 0
  assert included.stdout.starts_with("HTTP/1.1 200 OK\r\n"), included.stdout
  assert "X-Fixture: yes\r\n" in included.stdout
  assert included.stdout.ends_with("\r\n\r\nhello\n"), included.stdout

  let head = curl(ctx, ["-s", "-I", f"{fixture.base}/ok"])?
  assert head.status == 0
  assert head.stdout.starts_with("HTTP/1.1 200 OK\r\n")
  assert head.stdout.ends_with("\r\n\r\n")
  assert !("hello" in head.stdout)

  let failing = curl(ctx, ["-s", "-I", "-f", f"{fixture.base}/status/404"])?
  assert failing.status == 22
  assert failing.stdout.starts_with("HTTP/1.1 404 Not Found\r\n")
}

test test_curl_dump_header_writes_the_head_to_a_file { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-s", "-D", "head.txt", "-o", "body.txt", f"{fixture.base}/ok"])?
  assert result.status == 0
  assert result.stdout == ""
  let head = fp"{result.dir}/head.txt".read_text()?
  assert head.starts_with("HTTP/1.1 200 OK\r\n"), head
  assert head.ends_with("\r\n\r\n")
  assert fp"{result.dir}/body.txt".read_text()? == "hello\n"
}

test test_curl_request_options_reach_the_server { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-s", "-X", "PATCH", "-H", "X-A: 1", "-H", "X-B:2", "-A", "agent/1", "-e", "http://ref.test/", "-u", "user:pw", f"{fixture.base}/echo"])?
  assert result.status == 0, result.stderr
  assert result.stdout.starts_with("PATCH /echo\n"), result.stdout
  assert "\nX-A: 1\n" in result.stdout
  assert "\nX-B: 2\n" in result.stdout
  assert "\nUser-Agent: agent/1\n" in result.stdout
  assert "\nReferer: http://ref.test/\n" in result.stdout
  assert "\nAuthorization: Basic dXNlcjpwdw==\n" in result.stdout
}

test test_curl_default_headers_and_removal { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let plain = curl(ctx, ["-s", f"{fixture.base}/echo"])?
  assert "\nUser-Agent: curl/8.22.0\n" in plain.stdout, plain.stdout
  assert "\nAccept: */*\n" in plain.stdout

  let removed = curl(ctx, ["-s", "-H", "Accept:", "-H", "User-Agent: custom", f"{fixture.base}/echo"])?
  assert !("\nAccept:" in removed.stdout), removed.stdout
  assert "\nUser-Agent: custom\n" in removed.stdout
}

test test_curl_data_options_build_the_body { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let joined = curl(ctx, ["-s", "-d", "a=1", "-d", "b=2", f"{fixture.base}/echo"])?
  assert joined.stdout.starts_with("POST /echo\n"), joined.stdout
  assert joined.stdout.ends_with("\n--\na=1&b=2"), joined.stdout
  assert "\nContent-Type: application/x-www-form-urlencoded\n" in joined.stdout
  assert "\nContent-Length: 7\n" in joined.stdout

  let root = test.temp_dir(ctx, name: "curl-data")?
  fp"{root}/data.txt".write("line one\nline two\n")?
  let stripped = curl(ctx, ["-s", "-d", "@data.txt", f"{fixture.base}/echo"], dir: root)?
  assert stripped.stdout.ends_with("\n--\nline oneline two"), stripped.stdout
  let binary = curl(ctx, ["-s", "--data-binary", "@data.txt", f"{fixture.base}/echo"], dir: root)?
  assert binary.stdout.ends_with("\n--\nline one\nline two\n"), binary.stdout

  let stdin = curl(ctx, ["-s", "-d", "@-", f"{fixture.base}/echo"], input: b"from stdin")?
  assert stdin.stdout.ends_with("\n--\nfrom stdin"), stdin.stdout

  let encoded = curl(ctx, ["-s", "--data-urlencode", "q=a b&c", f"{fixture.base}/echo"])?
  assert encoded.stdout.ends_with("\n--\nq=a+b%26c"), encoded.stdout

  let query = curl(ctx, ["-s", "-G", "-d", "a=1", "-d", "b=2", f"{fixture.base}/echo"])?
  assert query.stdout.starts_with("GET /echo?a=1&b=2\n"), query.stdout

  let explicit = curl(ctx, ["-s", "-H", "Content-Type: text/x-test", "--data-raw", "@literal", f"{fixture.base}/echo"])?
  assert "\nContent-Type: text/x-test\n" in explicit.stdout
  assert explicit.stdout.ends_with("\n--\n@literal"), explicit.stdout
}

test test_curl_upload_file_uses_put { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let root = test.temp_dir(ctx, name: "curl-upload")?
  fp"{root}/payload.txt".write("uploaded bytes")?
  let result = curl(ctx, ["-s", "-T", "payload.txt", f"{fixture.base}/echo"], dir: root)?
  assert result.status == 0, result.stderr
  assert result.stdout.starts_with("PUT /echo\n"), result.stdout
  assert result.stdout.ends_with("\n--\nuploaded bytes"), result.stdout
}

test test_curl_basic_credentials_prompt_for_a_missing_password { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-s", "-u", "user", f"{fixture.base}/auth"], input: b"pw\n")?
  assert result.stdout == "authorized\n", result.stdout
  assert result.stderr == "Enter host password for user 'user':"
}

test test_curl_redirects_need_location { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let followed = curl(ctx, ["-sSL", f"{fixture.base}/redir/3"])?
  assert followed.status == 0, followed.stderr
  assert followed.stdout == "arrived\n"

  let capped = curl(ctx, ["-sSL", "--max-redirs", "1", f"{fixture.base}/redir/3"])?
  assert capped.status == 47
  assert capped.stderr == "curl: (47) Maximum (1) redirects followed\n", capped.stderr

  let refused = curl(ctx, ["-sS", f"{fixture.base}/redir/1"])?
  assert refused.status == 47
  assert refused.stdout == ""
}

test test_curl_output_names { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let named = curl(ctx, ["-s", "-O", f"{fixture.base}/dir/name.txt"])?
  assert named.status == 0
  assert fp"{named.dir}/name.txt".read_text()? == "named name.txt\n"

  let nameless = curl(ctx, ["-O", f"{fixture.base}/"])?
  assert nameless.status == 0
  assert fp"{nameless.dir}/curl_response".read_text()? == "root\n"
  assert nameless.stderr.starts_with("Warning: No remote filename, uses \"curl_response\"\n"), nameless.stderr

  let nested = curl(ctx, ["-s", "--create-dirs", "-o", "sub/dir/out.txt", f"{fixture.base}/ok"])?
  assert nested.status == 0
  assert fp"{nested.dir}/sub/dir/out.txt".read_text()? == "hello\n"

  let dash = curl(ctx, ["-s", "-o", "-", f"{fixture.base}/ok"])?
  assert dash.stdout == "hello\n"
}

test test_curl_several_urls_pair_with_outputs_and_exit_with_the_last { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-sf", "-o", "first.txt", f"{fixture.base}/ok", f"{fixture.base}/status/404", f"{fixture.base}/ok"])?
  assert result.status == 0
  assert fp"{result.dir}/first.txt".read_text()? == "hello\n"
  assert result.stdout == "hello\n"

  let last = curl(ctx, ["-sf", f"{fixture.base}/ok", f"{fixture.base}/status/404"])?
  assert last.status == 22
  assert last.stdout == "hello\n"
}

test test_curl_write_out_variables { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let url = f"{fixture.base}/ok"
  let result = curl(ctx, ["-s", "-o", "/dev/null", "-w", "%{http_code} %{size_download} %{url_effective} %{content_type} %{exitcode} %{method} %{scheme}\\n", url])?
  assert result.stdout == f"200 6 {url} text/plain 0 GET http\n", result.stdout

  let escapes = curl(ctx, ["-s", "-o", "/dev/null", "-w", "[%header{x-fixture}]%%\\t|%{http_version}|%{size_header}\\n", url])?
  assert escapes.stdout == "[yes]%\t|1.1|99\n", escapes.stdout

  let streams = curl(ctx, ["-s", "-o", "/dev/null", "-w", "out%{stderr}err%{stdout}again", url])?
  assert streams.stdout == "outagain", streams.stdout
  assert streams.stderr == "err", streams.stderr

  let unknown = curl(ctx, ["-s", "-o", "/dev/null", "-w", "%{no_such_variable}", url])?
  assert unknown.stdout == ""
  assert unknown.stderr == "curl: unknown --write-out variable: 'no_such_variable'\n"
}

test test_curl_connection_failures_have_curl_exit_codes { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let refused = curl(ctx, ["-sS", "http://127.0.0.1:1/"])?
  assert refused.status == 7
  assert rx"^curl: \(7\) Failed to connect to 127\.0\.0\.1:1 after [0-9]+ ms: Could not connect to server\n$".matches(refused.stderr), refused.stderr

  let timed = curl(ctx, ["-sS", "-m", "1", f"{fixture.base}/slow"])?
  assert timed.status == 28
  assert rx"^curl: \(28\) Operation timed out after [0-9]+ milliseconds with 0 bytes received\n$".matches(timed.stderr), timed.stderr

  let empty = curl(ctx, ["-sS", f"{fixture.base}/drop"])?
  assert empty.status == 52
  assert empty.stderr == "curl: (52) Empty reply from server\n"

  let missing = curl(ctx, ["-sS", "http://nonexistent.invalid/"])?
  assert missing.status == 6
  assert missing.stderr == "curl: (6) Could not resolve host: nonexistent.invalid (Domain name not found)\n", missing.stderr
}

test test_curl_usage_and_unsupported_options { |ctx|
  let none = curl(ctx, [])?
  assert none.status == 2
  assert none.stderr == "curl: try 'curl --help' or 'curl --manual' for more information\n"

  let no_url = curl(ctx, ["-s"])?
  assert no_url.status == 2
  assert no_url.stderr == "curl: (2) no URL specified\ncurl: try 'curl --help' or 'curl --manual' for more information\n", no_url.stderr

  let unknown = curl(ctx, ["--bogus", "http://x/"])?
  assert unknown.status == 2
  assert unknown.stderr == "curl: option --bogus: is unknown\ncurl: try 'curl --help' or 'curl --manual' for more information\n"

  let number = curl(ctx, ["--max-time", "abc", "http://x/"])?
  assert number.status == 2
  assert number.stderr == "curl: option --max-time: expected a proper numerical parameter\ncurl: try 'curl --help' or 'curl --manual' for more information\n"

  let missing_value = curl(ctx, ["-o"])?
  assert missing_value.status == 2
  assert missing_value.stderr.starts_with("curl: option -o: requires parameter\n")

  let proxy = curl(ctx, ["-x", "http://proxy.test:3128", "http://x/"])?
  assert proxy.status == 4
  assert proxy.stderr.starts_with("curl: option -x: the installed libcurl version doesn't support this\n"), proxy.stderr

  let scheme = curl(ctx, ["-sS", "ftp://127.0.0.1/file"])?
  assert scheme.status == 1
  assert scheme.stderr == "curl: (1) Protocol \"ftp\" not supported\n"

  let method = curl(ctx, ["-sS", "-X", "OPTIONS", "http://127.0.0.1:1/"])?
  assert method.status == 4
  assert method.stderr == "curl: (4) HTTP method OPTIONS is not supported by this implementation\n"

  let glob = curl(ctx, ["-sS", "http://127.0.0.1:1/[1-3]"])?
  assert glob.status == 3

  let data = curl(ctx, ["-s", "-d", "@/nonexistent/file", "http://x/"])?
  assert data.status == 26
  assert data.stderr == "curl: Failed to open /nonexistent/file\ncurl: option -d: error encountered when reading a file\ncurl: try 'curl --help' or 'curl --manual' for more information\n"
}

test test_curl_version_and_help { |ctx|
  let version = curl(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("curl 8.22.0 "), version.stdout
  assert "Protocols: http https\n" in version.stdout

  let help = curl(ctx, ["-h"])?
  assert help.status == 0
  assert help.stdout.starts_with("Usage: curl [options...] <url>\n")
}

test test_curl_write_failure_exits_23 { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-sS", "-o", "/nonexistent/dir/out", f"{fixture.base}/ok"])?
  assert result.status == 23
  assert result.stderr.starts_with("curl: (23) client returned ERROR on write"), result.stderr
}

test test_curl_retry_repeats_transient_failures { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-o", "/dev/null", "--retry", "2", "--retry-delay", "1", f"{fixture.base}/status/503"])?
  assert result.status == 0
  assert "Warning: Problem : HTTP error. Retrying in 1 second. 2 retries left.\n" in result.stderr, result.stderr
  assert "Warning: Problem : HTTP error. Retrying in 1 second. 1 retry left.\n" in result.stderr, result.stderr

  let failing = curl(ctx, ["-sS", "-f", "--retry", "1", "--retry-delay", "1", f"{fixture.base}/status/503"])?
  assert failing.status == 22
  assert failing.stderr == "curl: (22) The requested URL returned error: 503\ncurl: (22) The requested URL returned error: 503\n", failing.stderr

  let permanent = curl(ctx, ["-sS", "-f", "--retry", "2", "--retry-delay", "1", f"{fixture.base}/status/404"])?
  assert permanent.status == 22
  assert permanent.stderr == "curl: (22) The requested URL returned error: 404\n"

  let refused = curl(ctx, ["--retry", "1", "--retry-delay", "1", "--retry-connrefused", "http://127.0.0.1:1/"])?
  assert refused.status == 7
  assert "Warning: Problem : connection refused. Retrying in 1 second. 1 retry left.\n" in refused.stderr, refused.stderr
}

test test_curl_cookie_jar_round_trips { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let root = test.temp_dir(ctx, name: "curl-cookies")?
  let stored = curl(ctx, ["-s", "-c", "jar.txt", f"{fixture.base}/cookie"], dir: root)?
  assert stored.status == 0
  let jar = fp"{root}/jar.txt".read_text()?
  assert jar.starts_with("# Netscape HTTP Cookie File\n# https://curl.se/docs/http-cookies.html\n"), jar
  assert "127.0.0.1\tFALSE\t/\tFALSE\t0\ta\tb\n" in jar
  assert "127.0.0.1\tFALSE\t/x\tFALSE\t0\tc\td\n" in jar

  let sent = curl(ctx, ["-s", "-b", "jar.txt", f"{fixture.base}/echo"], dir: root)?
  assert "\nCookie: a=b\n" in sent.stdout, sent.stdout

  let literal = curl(ctx, ["-s", "-b", "k=v; z=y", f"{fixture.base}/echo"])?
  assert "\nCookie: k=v; z=y\n" in literal.stdout, literal.stdout
}

test test_curl_compressed_requests_and_decodes_gzip { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-s", "--compressed", f"{fixture.base}/gzip"])?
  assert result.status == 0, result.stderr
  assert result.stdout == "compressed body\n"

  let raw = curl(ctx, ["-s", f"{fixture.base}/gzip"])?
  assert raw.stdout == "compressed body\n"
}

test test_curl_ranges_and_resume { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let part = curl(ctx, ["-s", "-r", "0-4", f"{fixture.base}/echo"])?
  assert part.status == 0
  assert "\nRange: bytes=0-4\n" in part.stdout, part.stdout

  let root = test.temp_dir(ctx, name: "curl-resume")?
  fp"{root}/file.bin".write("abcdefghijklmnopqrst")?
  let resumed = curl(ctx, ["-s", "-C", "-", "-o", "file.bin", f"{fixture.base}/range"], dir: root)?
  assert resumed.status == 0, resumed.stderr
  assert fp"{root}/file.bin".read_bytes()?.len() == 100

  let complete = curl(ctx, ["-s", "-C", "-", "-o", "file.bin", f"{fixture.base}/range"], dir: root)?
  assert complete.status == 0
  assert fp"{root}/file.bin".read_bytes()?.len() == 100
}

test test_curl_max_filesize { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-sS", "--max-filesize", "3", f"{fixture.base}/ok"])?
  assert result.status == 63
  assert result.stdout == ""
  assert result.stderr == "curl: (63) Maximum file size exceeded\n"
}

test test_curl_verbose_describes_the_exchange { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let result = curl(ctx, ["-s", "-v", f"{fixture.base}/ok"])?
  assert result.stdout == "hello\n"
  assert "> GET /ok HTTP/1.1\n" in result.stderr, result.stderr
  assert "\n> User-Agent: curl/8.22.0\n" in result.stderr
  assert "\n< HTTP/1.1 200 OK\n" in result.stderr
  assert "\n< X-Fixture: yes\n" in result.stderr
  assert "\n{ [6 bytes data]\n" in result.stderr
}

test test_curl_progress_bar_and_stderr_redirection { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let bar = curl(ctx, ["-#", "-o", "/dev/null", f"{fixture.base}/ok"])?
  assert bar.stderr == "\r########################################################################" + " 100.0%\n", bar.stderr

  let quiet = curl(ctx, ["--no-progress-meter", "-o", "/dev/null", f"{fixture.base}/ok"])?
  assert quiet.stderr == ""

  let redirected = curl(ctx, ["-sS", "--stderr", "-", "http://127.0.0.1:1/"])?
  assert redirected.stderr == ""
  assert redirected.stdout.starts_with("curl: (7) Failed to connect to 127.0.0.1:1")
}

test test_curl_unreadable_trust_file_is_a_usage_error { |ctx|
  let result = curl(ctx, ["-sS", "--cacert", "/nonexistent/ca.pem", "https://127.0.0.1:1/"])?
  assert result.status == 2
  assert result.stderr == "curl: The file '/nonexistent/ca.pem' provided to --cacert does not exist\ncurl: option --cacert: is badly used here\ncurl: try 'curl --help' or 'curl --manual' for more information\n", result.stderr
}

# openssl generates a throwaway CA and an `s_server` with a leaf for 127.0.0.1;
# both tools are optional, so each TLS test skips without them.
type TlsFixture = {url: Str, ca: Path, handle: ProcessHandle}

proc start_tls_server(ctx: TestContext) [fs, process, time, error] -> Result[TlsFixture?] {
  let openssl = try run.text openssl version
  let perl = try run.text perl -e "print 1"
  guard openssl is Ok(_) and perl is Ok(_) else { return Ok(null) }

  let root = test.temp_dir(ctx, name: "curl-tls")?
  let quiet = run.capture --text openssl req -x509 -newkey rsa:2048 -nodes -keyout ${root}/ca.key -out ${root}/ca.pem -days 2 -subj "/CN=Fixture CA" -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign"
  guard quiet.status.ok else { return Ok(null) }
  let leaf = run.capture --text openssl req -newkey rsa:2048 -nodes -keyout ${root}/leaf.key -out ${root}/leaf.csr -subj "/CN=localhost"
  guard leaf.status.ok else { return Ok(null) }
  fp"{root}/ext.cnf".write("subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n")?
  let signed = run.capture --text openssl x509 -req -in ${root}/leaf.csr -CA ${root}/ca.pem -CAkey ${root}/ca.key -CAcreateserial -out ${root}/leaf.pem -days 2 -extfile ${root}/ext.cnf
  guard signed.status.ok else { return Ok(null) }

  let number = run.text perl -MIO::Socket::INET -e r"my $s = IO::Socket::INET->new(Listen => 1, LocalAddr => '127.0.0.1', LocalPort => 0) or die; print $s->sockport"
  let accept = f"127.0.0.1:{number.trim()}"
  let handle = spawn run openssl s_server -accept $accept -cert ${root}/leaf.pem -key ${root}/leaf.key -www -quiet > /dev/null ?
  # s_server prints nothing when ready, so poll until a TLS connection works.
  let started = time.now()
  while time.now() - started < 5000 {
    let probe = run.capture --text openssl s_client -connect $accept -brief < /dev/null
    if probe.status.ok { return Ok({url: f"https://{accept}/", ca: fp"{root}/ca.pem", handle: handle}) }
    time.sleep(100ms)?
  }
  handle.cancel(signal: "KILL", kill_after: 0ms)?
  Ok(null)
}

test test_curl_tls_verification { |ctx|
  let started = start_tls_server(ctx)?
  guard let tls = started else { test.skip("requires openssl and perl for the TLS fixture"); return }
  defer tls.handle.cancel(kill_after: 100ms)

  let untrusted = curl(ctx, ["-sS", tls.url])?
  assert untrusted.status == 60, untrusted.stderr
  assert untrusted.stderr.starts_with("curl: (60) SSL certificate problem: "), untrusted.stderr
  assert "More details here: https://curl.se/docs/sslcerts.html\n" in untrusted.stderr

  let insecure = curl(ctx, ["-sSk", "-o", "/dev/null", "-w", "%{http_code}", tls.url])?
  assert insecure.status == 0, insecure.stderr
  assert insecure.stdout == "200"

  let trusted = curl(ctx, ["-sS", "--cacert", f"{tls.ca}", "-o", "/dev/null", "-w", "%{http_code}", tls.url])?
  assert trusted.status == 0, trusted.stderr
  assert trusted.stdout == "200"

  let bad = curl(ctx, ["-sS", "--cacert", f"{fp"{tls.ca.parent()}/leaf.key"}", tls.url])?
  assert bad.status == 77, bad.stderr
}
