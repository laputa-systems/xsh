type Server = {base: Str, handle: ProcessHandle}

# A loopback HTTP fixture in Perl, which the host and the test image both
# carry. It answers one request per connection and publishes its port through
# a file once it is listening. `/to?u=URL` redirects to any URL so a test can
# send a request across origins.
const SERVER = r"""
use strict;
use warnings;
use IO::Socket::INET;

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
  my $head = "HTTP/1.1 $status $reason\r\n";
  $head .= "$_\r\n" for @$headers;
  $head .= "Content-Type: text/plain\r\nContent-Length: " . length($body) . "\r\nConnection: close\r\n\r\n";
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
  my ($path, $query) = split /\?/, $target, 2;
  if ($path eq '/echo') {
    reply($sock, $method, 200, 'OK', [], "$method $target\n" . join('', map { "$_\n" } @headers) . "--\n$body");
  } elsif ($path =~ m{^/status/(\d+)$}) {
    reply($sock, $method, $1, 'Status', [], "status $1\n");
  } elsif ($path =~ m{^/redir/(\d+)$}) {
    if ($1 > 0) {
      reply($sock, $method, 302, 'Found', ['Location: /redir/' . ($1 - 1)], "moved\n");
    } else {
      reply($sock, $method, 200, 'OK', [], "arrived\n");
    }
  } elsif ($path =~ m{^/moved/(30[1-8])$}) {
    reply($sock, $method, $1, 'Redirect', ['Location: /echo'], "moved\n");
  } elsif ($path eq '/to') {
    my ($u) = ($query // '') =~ m{^u=(.*)$};
    reply($sock, $method, 302, 'Found', ["Location: $u"], "moved\n");
  } elsif ($path eq '/bare-redirect') {
    reply($sock, $method, 302, 'Found', [], "no location\n");
  } elsif ($path eq '/slow') {
    select(undef, undef, undef, 3);
    reply($sock, $method, 200, 'OK', [], "late\n");
  } elsif ($path eq '/drop') {
    return;
  } elsif ($path eq '/file') {
    reply($sock, $method, 200, 'OK', [], "file bytes\n");
  } else {
    reply($sock, $method, 404, 'Not Found', [], "no route $path\n");
  }
}
"""

# A listener that answers a connection with plain HTTP the moment it is
# accepted, whatever the client sent, as a server that does not speak TLS does
# to a TLS client.
const PLAIN_ANSWER = r"""
use strict;
use warnings;
use IO::Socket::INET;

my ($port_file) = @ARGV;
$SIG{PIPE} = 'IGNORE';
my $server = IO::Socket::INET->new(
  LocalAddr => '127.0.0.1', LocalPort => 0, Listen => 8, ReuseAddr => 1, Proto => 'tcp',
) or die "listen: $!";
open(my $out, '>', "$port_file.tmp") or die;
print $out $server->sockport, "\n";
close $out;
rename "$port_file.tmp", $port_file or die;
while (my $client = $server->accept) {
  print $client "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
  close $client;
}
"""

const SKIP = "requires perl for the loopback HTTP fixture"

# Starts the fixture and waits for its port; null means this host cannot run
# it (no perl), which the caller reports as a skip.
proc start_server(ctx: TestContext) [fs, process, time, error] -> Result[Server?] {
  start_script(ctx, SERVER)
}

proc start_script(ctx: TestContext, source: Str) [fs, process, time, error] -> Result[Server?] {
  let probe = try run.text perl -e "print 1"
  guard probe is Ok(_) else { return Ok(null) }

  let root = test.temp_dir(ctx, name: "net-server")?
  let script = fp"{root}/server.pl"
  let port_file = fp"{root}/port"
  script.write(source)?
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

# The NetError variant of a failure, or `ok-STATUS` for a response, so a test
# states which cause a request ended with.
pure outcome(result: Result[NetResponse, Error]) -> Str {
  match result {
    Ok(response) => f"ok-{response.status}"
    Err(failure) => cause(failure)
  }
}

pure cause(failure: Error) -> Str {
  match failure {
    NetError.Status {status} => f"status-{status ?? 0}"
    NetError.Dns => "dns"
    NetError.Connect => "connect"
    NetError.ConnectTimeout => "connect-timeout"
    NetError.Timeout => "timeout"
    NetError.Tls => "tls"
    NetError.Certificate => "certificate"
    NetError.TrustStore => "trust-store"
    NetError.EmptyReply => "empty-reply"
    NetError.Redirect => "redirect"
    NetError.Unsupported => "unsupported"
    NetError.Write => "write"
    NetError.Io => "io"
    NetError.Other => "other"
    else => "not-a-net-error"
  }
}

test test_net_redirect_limit_return_hands_back_the_redirect { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)
  let url = f"{fixture.base}/redir/2"

  # Default: a redirect past the limit is a failure.
  assert outcome(net.request({method: "GET", url: url, redirects: 0})) == "redirect"
  assert outcome(net.request({method: "GET", url: url, redirects: 0, redirect_limit: "error"})) == "redirect"

  let returned = net.request({method: "GET", url: url, redirects: 0, redirect_limit: "return"})?
  assert returned.status == 302
  assert returned.body as Str == "moved\n"
  assert "Location" in [header.name for header in returned.headers]
  assert returned.url == url
  assert returned.effective_url == url
  assert returned.redirect_count == 0

  # The limit applies after redirects were followed, and a redirect with no
  # Location can never be followed.
  let stopped = net.request({method: "GET", url: url, redirects: 1, redirect_limit: "return"})?
  assert stopped.status == 302
  assert stopped.effective_url == f"{fixture.base}/redir/1"
  assert stopped.redirect_count == 1
  let bare = net.request({method: "GET", url: f"{fixture.base}/bare-redirect", redirect_limit: "return"})?
  assert bare.status == 302
  assert outcome(net.request({method: "GET", url: f"{fixture.base}/bare-redirect"})) == "redirect"

  # A returned redirect is not a failed status.
  let tolerated = net.request({method: "GET", url: url, redirects: 0, redirect_limit: "return", fail_status: true})?
  assert tolerated.status == 302
  assert outcome(net.request({method: "GET", url: f"{fixture.base}/status/404", redirect_limit: "return", fail_status: true})) == "status-404"
}

test test_net_response_reports_the_effective_url_and_redirect_count { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  let direct = net.request({method: "GET", url: f"{fixture.base}/file"})?
  assert direct.effective_url == f"{fixture.base}/file"
  assert direct.redirect_count == 0

  let followed = net.request({method: "GET", url: f"{fixture.base}/redir/2"})?
  assert followed.body as Str == "arrived\n"
  assert followed.url == f"{fixture.base}/redir/2"
  assert followed.effective_url == f"{fixture.base}/redir/0"
  assert followed.redirect_count == 2

  let root = test.temp_dir(ctx, name: "net-redirect-download")?
  let dest = fp"{root}/file.txt"
  let downloaded = net.download({url: f"{fixture.base}/redir/1", dest: dest})?
  assert downloaded.effective_url == f"{fixture.base}/redir/0"
  assert downloaded.redirect_count == 1
  assert dest.read_text()? == "arrived\n"

  let page = fp"{root}/page.txt"
  let returned = net.download({url: f"{fixture.base}/redir/1", dest: page, redirects: 0, redirect_limit: "return"})?
  assert returned.status == 302
  assert page.read_text()? == "moved\n"
}

test test_net_redirect_rewrites_post_the_way_curl_does { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  for code in [301, 302, 303] {
    let response = net.request({
      method: "POST",
      url: f"{fixture.base}/moved/{code}",
      headers: [{name: "Content-Type", value: "text/plain"}],
      body_text: "payload",
    })?
    let echoed = response.body as Str
    assert echoed.starts_with("GET /echo\n"), f"{code}: {echoed}"
    assert echoed.ends_with("\n--\n"), f"{code}: {echoed}"
    assert "Content-Type" not in echoed, f"{code}: {echoed}"
    assert response.redirect_count == 1
  }
  for code in [307, 308] {
    let response = net.request({method: "POST", url: f"{fixture.base}/moved/{code}", body_text: "payload"})?
    let echoed = response.body as Str
    assert echoed.starts_with("POST /echo\n"), f"{code}: {echoed}"
    assert echoed.ends_with("\n--\npayload"), f"{code}: {echoed}"
  }

  # 303 turns every method but HEAD into GET; 301 and 302 leave PUT alone.
  let put_303 = net.request({method: "PUT", url: f"{fixture.base}/moved/303", body_text: "x"})?
  assert (put_303.body as Str).starts_with("GET /echo\n")
  let put_302 = net.request({method: "PUT", url: f"{fixture.base}/moved/302", body_text: "x"})?
  assert (put_302.body as Str).starts_with("PUT /echo\n")
}

test test_net_redirect_drops_credentials_across_origins { |ctx|
  let origin = start_server(ctx)?
  let other = start_server(ctx)?
  guard let first = origin else { test.skip(SKIP); return }
  guard let second = other else { test.skip(SKIP); return }
  defer first.handle.cancel(kill_after: 100ms)
  defer second.handle.cancel(kill_after: 100ms)
  let headers = [{name: "Authorization", value: "Bearer secret"}, {name: "X-Keep", value: "yes"}]

  let same = net.request({method: "GET", url: f"{first.base}/to?u=/echo", headers: headers})?
  assert "Authorization: Bearer secret" in same.body as Str

  let across = net.request({method: "GET", url: f"{first.base}/to?u={second.base}/echo", headers: headers})?
  assert across.effective_url == f"{second.base}/echo"
  assert "Authorization" not in across.body as Str, across.body as Str
  assert "X-Keep: yes" in across.body as Str
}

test test_net_failures_are_net_error_variants { |ctx|
  let server = start_server(ctx)?
  guard let fixture = server else { test.skip(SKIP); return }
  defer fixture.handle.cancel(kill_after: 100ms)

  assert outcome(net.request({method: "GET", url: f"{fixture.base}/status/404", fail_status: true})) == "status-404"
  assert outcome(net.request({method: "GET", url: f"{fixture.base}/status/404"})) == "ok-404"
  assert outcome(net.request({method: "GET", url: "http://127.0.0.1:1/"})) == "connect"
  assert outcome(net.request({method: "GET", url: f"{fixture.base}/slow", timeout: 300ms})) == "timeout"
  assert outcome(net.request({method: "GET", url: f"{fixture.base}/drop"})) == "empty-reply"
  assert outcome(net.request({method: "GET", url: "ftp://127.0.0.1/"})) == "unsupported"
  assert outcome(net.request({method: "GET", url: f"{fixture.base}/redir/9", redirects: 1})) == "redirect"
  assert outcome(net.request({method: "GET", url: f"{fixture.base}/file", ca_certificate: p"/nonexistent/ca.pem"})) == "trust-store"

  # A TLS handshake against a server that speaks plain HTTP fails without
  # being a certificate problem.
  let plain = start_script(ctx, PLAIN_ANSWER)?
  guard let answer = plain else { test.skip(SKIP); return }
  defer answer.handle.cancel(kill_after: 100ms)
  let https = f"https{answer.base.byte_slice(4)}/file"
  assert outcome(net.request({method: "GET", url: https, timeout: 5s})) == "tls"

  # Timeouts also match the Timeout facet, whatever phase ran out of time.
  match net.request({method: "GET", url: f"{fixture.base}/slow", timeout: 300ms}) {
    Ok(_) => assert false, "the slow response arrived"
    Err(is Timeout) => {}
    Err(failure) => assert false, failure.message
  }
}
