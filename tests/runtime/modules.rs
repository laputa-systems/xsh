#![allow(clippy::single_call_fn)]

use super::common::*;
#[cfg(feature = "net")]
use std::sync::atomic::AtomicBool;

#[cfg(feature = "net")]
#[test]
fn dns_module_uses_explicit_server_for_a_and_aaaa_records() {
    let server = LocalDnsServer::spawn(2);
    let source = format!(
        r#"
let a = dns.lookup("fixture.test", "A", "{server}", 1s)?
let aaaa = dns.lookup("fixture.test", "AAAA", "{server}", 1s)?
print ${{a[0].name}} ${{a[0].record}} ${{a[0].value}} ${{a[0].ttl}}
print ${{aaaa[0].name}} ${{aaaa[0].record}} ${{aaaa[0].value}} ${{aaaa[0].ttl}}
"#,
        server = server.addr
    );

    let output = run_temp_script("dns-explicit-server", &source);
    let summary = server.join();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "fixture.test A 192.0.2.10 60\nfixture.test AAAA 2001:db8::42 60\n"
    );
    assert_eq!(summary.expect("DNS server").handled, 2);
}

#[cfg(feature = "net")]
#[test]
fn net_module_transfers_files_and_uses_named_pool() {
    let server = LocalHttpServer::spawn(10);
    let dest = temp_path("net-download.txt");
    let upload_source = temp_path("net-upload.txt");
    let missing_ca = temp_path("net-missing-ca.pem");
    let _ = std::fs::remove_file(&dest);
    let _ = std::fs::remove_file(&upload_source);
    let _ = std::fs::remove_file(&missing_ca);
    std::fs::write(&upload_source, "upload-body").expect("write upload source");
    let source = format!(
        r#"
let pool = net.pool("test", 4, 1s)?
let first = net.request({{method: "GET", url: "{url}/hello", pool: "test"}})?
let second = net.request({{method: "GET", url: "{url}/hello", pool: "test"}})?
let headed = net.request({{method: "HEAD", url: "{url}/hello", pool: "test"}})?
let redirected = net.request({{method: "GET", url: "{url}/redirect", redirects: 1, pool: "test"}})?
let posted = net.request({{
  method: "POST",
  url: "{url}/echo",
  headers: [{{name: "X-Test", value: "one"}}],
  body_text: "payload",
  pool: "test",
}})?
let posted_file = net.request({{
  method: "POST",
  url: "{url}/echo",
  body_file: Path({upload_source}),
  pool: "test",
}})?
let status = net.request({{method: "GET", url: "{url}/status", fail_status: false, pool: "test"}})?
let failed = net.request({{method: "GET", url: "ftp://example.invalid/file"}})
let bad_ca = net.request({{
  method: "GET",
  url: "http://127.0.0.1/",
  ca_certificate: Path({missing_ca}),
}})
let downloaded = net.download({{
  url: "{url}/file",
  dest: Path({dest}),
  overwrite: true,
  pool: "test",
}})?
let uploaded = net.upload({{
  method: "PUT",
  url: "{url}/upload",
  source: Path({upload_source}),
  headers: [{{name: "Authorization", value: "Bearer secret-token"}}],
  pool: "test",
}})?
let _closed = net.close_pool("test")?
let _closed_all = net.close_all_pools()?
match failed {{
  Err(e) => match bad_ca {{
    Err(ca) => {{
      test.error_kind(e, "net-scheme")?
      test.error_kind(ca, "net-ca-certificate")?
      print ${{pool.max_idle_per_host}} ${{pool.idle_timeout_ms}} ${{first.body.utf8()?}} ${{second.body.utf8()?}} ${{headed.status}} ${{headed.bytes}} ${{redirected.body.utf8()?}} ${{posted.body.utf8()?}} ${{posted_file.body.utf8()?}} ${{status.status}} ${{downloaded.bytes}} ${{uploaded.status}} "net-scheme" "net-ca-certificate"
    }}
  }}
}}
"#,
        url = server.url,
        dest = xsh_string_literal(dest.to_str().unwrap()),
        upload_source = xsh_string_literal(upload_source.to_str().unwrap()),
        missing_ca = xsh_string_literal(missing_ca.to_str().unwrap()),
    );

    let output = run_temp_script("net-module", &source);
    let summary = server.join();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "4 1000 hello hello 200 0 hello echo:payload echo:upload-body 404 11 201 net-scheme net-ca-certificate\n"
    );
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "downloaded\n");
    assert_eq!(summary.handled, 10);
    assert!(
        summary
            .requests
            .iter()
            .any(|request| request.method == "PUT"
                && request.path == "/upload"
                && request.header("authorization") == Some("Bearer secret-token")
                && request.body == b"upload-body"),
        "{:?}",
        summary.requests
    );

    let _ = std::fs::remove_file(dest);
    let _ = std::fs::remove_file(upload_source);
    let _ = std::fs::remove_file(missing_ca);
}

#[cfg(feature = "net")]
#[test]
fn net_module_reuses_tcp_connection_within_pool() {
    let server = LocalHttpServer::spawn(2);
    let source = format!(
        r#"
let first = net.request({{method: "GET", url: "{url}/hello", pool: "reuse"}})?
let second = net.request({{method: "GET", url: "{url}/hello", pool: "reuse"}})?
print ${{first.body.utf8()?}} ${{second.body.utf8()?}}
"#,
        url = server.url,
    );

    let output = run_temp_script("net-pool-reuse", &source);
    let summary = server.join();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello hello\n");
    assert_eq!(summary.handled, 2);
    assert_eq!(summary.connections, 1, "{:?}", summary.requests);
}

#[cfg(feature = "net")]
#[test]
fn net_module_request_many_returns_ordered_results() {
    let server = LocalHttpServer::spawn(3);
    let source = format!(
        r#"
let request_items: List[Record] = [
    {{method: "GET", url: "{url}/hello", headers: [{{name: "Connection", value: "close"}}]}},
    {{method: "GET", url: "ftp://example.test/"}},
    {{method: "GET", url: "{url}/status", headers: [{{name: "Connection", value: "close"}}], fail_status: true}},
    {{method: "GET", url: "{url}/hello", headers: [{{name: "Connection", value: "close"}}]}},
]
let responses = net.request_many({{
  requests: request_items,
  concurrency: 2,
  pool: "many",
}})?
match responses[1] {{
  Err(scheme) => match responses[2] {{
    Err(status) => {{
      test.error_kind(scheme, "net-scheme")?
      test.error_kind(status, "net-status")?
      print ${{responses[0]?.body.utf8()?}} "net-scheme" "net-status" ${{responses[3]?.body.utf8()?}}
    }}
  }}
}}
"#,
        url = server.url,
    );

    let output = run_temp_script("net-request-many", &source);
    let summary = server.join();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "hello net-scheme net-status hello\n"
    );
    assert_eq!(summary.handled, 3);
}

#[cfg(feature = "net")]
#[test]
fn net_module_request_many_refills_the_window_on_first_completion() {
    let server = BatchBarrierServer::spawn();
    // An accepted idle socket must not consume one of the three request slots.
    let _idle = std::net::TcpStream::connect(server.url.strip_prefix("http://").unwrap())
        .expect("open idle batch-barrier connection");
    let source = format!(
        r#"
let responses = net.request_many({{
  requests: [
    {{method: "GET", url: "{url}/gate-a", headers: [{{name: "Connection", value: "close"}}]}},
    {{method: "GET", url: "{url}/gate-b", headers: [{{name: "Connection", value: "close"}}]}},
    {{method: "GET", url: "{url}/gate-c", headers: [{{name: "Connection", value: "close"}}]}},
  ],
  concurrency: 2,
  pool: "completion-window",
}})?
print ${{responses[0]?.body.utf8()?}} ${{responses[1]?.body.utf8()?}} ${{responses[2]?.body.utf8()?}}
"#,
        url = server.url,
    );
    let script = std::thread::spawn(move || run_temp_script("net-completion-window", &source));

    let mut received = Vec::new();
    // The refill window starts at the first request rather than at script
    // spawn, so slow startup cannot expire it.
    let mut deadline = None;
    let third_started_before_a_completed = loop {
        if deadline.map_or(script.is_finished(), |deadline| Instant::now() >= deadline) {
            break false;
        }
        match server.events.recv_timeout(Duration::from_millis(10)) {
            Ok(path) => {
                deadline.get_or_insert_with(|| Instant::now() + Duration::from_secs(3));
                received.push(path);
                if received.last().is_some_and(|path| path == "/gate-c") {
                    break true;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break false,
        }
    };

    server.release_a();
    let output = script.join().expect("batch script thread");
    server.join();

    assert!(
        third_started_before_a_completed,
        "the third request did not start after B completed while A was gated; received {received:?}"
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "gate-a gate-b gate-c\n"
    );
}

#[cfg(feature = "net")]
#[test]
fn net_module_download_many_streams_ordered_files() {
    let server = LocalHttpServer::spawn(2);
    let first = temp_path("net-download-many-first.txt");
    let second = temp_path("net-download-many-second.txt");
    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);
    let source = format!(
        r#"
let responses = net.download_many({{
  downloads: [
    {{url: "{url}/hello", dest: Path({first}), overwrite: true}},
    {{url: "{url}/hello", dest: Path({second}), overwrite: true}},
  ],
  concurrency: 2,
  pool: "many-downloads",
}})?
print ${{responses[0]?.bytes}} ${{responses[1]?.bytes}}
"#,
        url = server.url,
        first = xsh_string_literal(first.to_str().unwrap()),
        second = xsh_string_literal(second.to_str().unwrap()),
    );

    let output = run_temp_script("net-download-many", &source);
    let summary = server.join();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "5 5\n");
    assert_eq!(std::fs::read(&first).unwrap(), b"hello");
    assert_eq!(std::fs::read(&second).unwrap(), b"hello");
    assert_eq!(summary.handled, 2);
    let _ = std::fs::remove_file(first);
    let _ = std::fs::remove_file(second);
}

#[cfg(feature = "net")]
#[test]
fn net_module_download_many_follows_redirects_and_keeps_atomic_destination_on_limit() {
    let server = LocalHttpServer::spawn(3);
    let redirected = temp_path("net-download-many-redirected.txt");
    let limited = temp_path("net-download-many-limited.txt");
    let _ = std::fs::remove_file(&redirected);
    let _ = std::fs::remove_file(&limited);
    std::fs::write(&limited, b"previous").expect("write existing download destination");
    let source = format!(
        r#"
let download_items: List[Record] = [
    {{url: "{url}/redirect", dest: Path({redirected}), atomic: true, overwrite: true, redirects: 1}},
    {{url: "{url}/hello", dest: Path({limited}), atomic: true, overwrite: true, max_body_bytes: 4}},
]
let responses = net.download_many({{
  downloads: download_items,
  concurrency: 2,
  pool: "many-download-redirects",
}})?
match responses[1] {{
  Err(limit) => {{
    test.error_kind(limit, "net-body-limit")?
    print ${{responses[0]?.bytes}} "net-body-limit"
  }}
}}
"#,
        url = server.url,
        redirected = xsh_string_literal(redirected.to_str().unwrap()),
        limited = xsh_string_literal(limited.to_str().unwrap()),
    );

    let output = run_temp_script("net-download-many-redirects", &source);
    let summary = server.join();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "5 net-body-limit\n"
    );
    assert_eq!(std::fs::read(&redirected).unwrap(), b"hello");
    assert_eq!(std::fs::read(&limited).unwrap(), b"previous");
    assert_eq!(summary.handled, 3);
    let _ = std::fs::remove_file(redirected);
    let _ = std::fs::remove_file(limited);
}

#[cfg(feature = "net")]
fn run_native_xsh_test(
    test_name: &str,
    environment: &[(&str, &str)],
    clear_ssl_cert_file: bool,
) -> std::process::Output {
    let mut command = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"));
    command
        .current_dir(cargo_env!("CARGO_MANIFEST_DIR"))
        .args(["test", "--exact", "--jobs", "1", test_name]);
    if clear_ssl_cert_file {
        command.env_remove("SSL_CERT_FILE");
    }
    for (key, value) in environment {
        command.env(key, value);
    }
    command.output().expect("run native XSH test")
}

#[cfg(feature = "net")]
fn assert_native_xsh_test(test_name: &str, output: std::process::Output) {
    assert!(
        output.status.success(),
        "native XSH test {test_name} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_dns_explicit_server_transport() {
    let server = LocalDnsServer::spawn(2);
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/dns.xsh::test_dns_explicit_server_transport",
        &[("XSH_DNS_TEST_SERVER", &server.addr)],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test("test_dns_explicit_server_transport", output);
    let summary = summary.expect("DNS server");
    assert_eq!(summary.handled, 2);
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_http_contracts() {
    let server = LocalHttpServer::spawn(11);
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_transport_http_contracts",
        &[("XSH_NET_TEST_URL", &server.url)],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test("test_net_transport_http_contracts", output);
    assert_eq!(summary.handled, 11);
    assert_eq!(summary.connections, 1, "{:?}", summary.requests);
}

#[cfg(feature = "net")]
#[test]
fn net_job_progresses_while_synchronous_request_waits() {
    let server = ConcurrentProgressServer::spawn();
    let _idle = std::net::TcpStream::connect(server.url.strip_prefix("http://").unwrap())
        .expect("open idle concurrent-progress connection");
    let source = format!(
        r#"
let job = net.start({{method: "GET", url: "{url}/job", headers: [{{name: "Connection", value: "close"}}]}})?
let foreground = net.request({{method: "GET", url: "{url}/sync", headers: [{{name: "Connection", value: "close"}}]}})?
print ${{foreground.body.utf8()?}} ${{job.wait()?.body.utf8()?}}
"#,
        url = server.url,
    );
    let output = run_temp_script("net-concurrent-progress", &source);
    let completion = server.join();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    completion.expect("concurrent-progress server");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "sync job\n");
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_job_progresses_while_synchronous_request_waits() {
    let server = ConcurrentProgressServer::spawn();
    // The fixture waits for two parsed requests, even if another socket is idle.
    let _idle = std::net::TcpStream::connect(server.url.strip_prefix("http://").unwrap())
        .expect("open idle concurrent-progress connection");
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_job_progresses_while_synchronous_request_waits",
        &[("XSH_NET_TEST_CONCURRENT_URL", &server.url)],
        false,
    );
    let completion = server.join();

    assert_native_xsh_test(
        "test_net_job_progresses_while_synchronous_request_waits",
        output,
    );
    completion.expect("concurrent-progress server");
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_job_trace_contract() {
    let server = LocalHttpServer::spawn(1);
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_job_trace_is_correlated_and_redacts_request_secrets",
        &[("XSH_NET_TEST_URL", &server.url)],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test(
        "test_net_job_trace_is_correlated_and_redacts_request_secrets",
        output,
    );
    assert_eq!(summary.handled, 1);
    assert_eq!(summary.requests[0].method, "POST");
    assert_eq!(
        summary.requests[0].header("authorization"),
        Some("Bearer trace-secret")
    );
    assert_eq!(summary.requests[0].body, b"trace-secret");
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_runtime_descriptors_do_not_survive_exec() {
    let server = LocalHttpServer::spawn(1);
    let helper = cargo_env!("CARGO_BIN_EXE_xsh-test-helper");
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_runtime_descriptors_do_not_survive_exec",
        &[
            ("XSH_NET_TEST_URL", &server.url),
            ("XSH_NET_FD_HELPER", helper),
        ],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test("test_net_runtime_descriptors_do_not_survive_exec", output);
    assert_eq!(summary.handled, 1);
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_error_contracts() {
    let server = LocalHttpServer::spawn(8);
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_transport_error_contracts",
        &[("XSH_NET_TEST_URL", &server.url)],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test("test_net_transport_error_contracts", output);
    assert_eq!(summary.handled, 8);
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_timeout_contracts() {
    let server = LocalHttpServer::spawn(5);
    let tls_stall = LocalTlsStallServer::spawn();
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_transport_timeout_contracts",
        &[
            ("XSH_NET_TEST_URL", &server.url),
            ("XSH_NET_TEST_TLS_STALL_URL", &tls_stall.url),
        ],
        false,
    );
    let summary = server.join();
    tls_stall.join();

    assert_native_xsh_test("test_net_transport_timeout_contracts", output);
    assert_eq!(summary.handled, 5);
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_batch_contracts() {
    let server = LocalHttpServer::spawn(7);
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_transport_batch_contracts",
        &[("XSH_NET_TEST_URL", &server.url)],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test("test_net_transport_batch_contracts", output);
    assert_eq!(summary.handled, 7);
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_batch_download_error_contract() {
    let server = LocalHttpServer::spawn(3);
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_transport_batch_download_error_contract",
        &[("XSH_NET_TEST_URL", &server.url)],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test("test_net_transport_batch_download_error_contract", output);
    assert_eq!(summary.handled, 3);
}

#[cfg(feature = "net")]
fn run_native_xsh_tls_contracts(versions: &[&'static rustls::SupportedProtocolVersion]) {
    static NEXT_CA_FILE: AtomicUsize = AtomicUsize::new(0);

    let server = LocalHttpsServer::spawn_with_protocol_versions(3, versions);
    let ca = temp_path(&format!(
        "native-xsh-net-ca-{}.pem",
        NEXT_CA_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&ca);
    std::fs::write(&ca, LOCAL_HTTPS_CA).expect("write local HTTPS CA");
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_transport_tls_contracts",
        &[
            ("XSH_NET_TEST_TLS_URL", &server.url),
            ("XSH_NET_TEST_CA", ca.to_str().expect("CA path is UTF-8")),
        ],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test("test_net_transport_tls_contracts", output);
    assert_eq!(summary.handled, 3);
    let _ = std::fs::remove_file(ca);
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_single_calls_force_https_http1() {
    let server = LocalHttpsServer::spawn_with_h2_offer(1, rustls::ALL_VERSIONS);
    let ca = temp_path("native-xsh-net-h1-ca.pem");
    let _ = std::fs::remove_file(&ca);
    std::fs::write(&ca, LOCAL_HTTPS_CA).expect("write local HTTPS CA");
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_transport_https_http1_contract",
        &[
            ("XSH_NET_TEST_H1_URL", &server.url),
            ("XSH_NET_TEST_CA", ca.to_str().expect("CA path is UTF-8")),
        ],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test("test_net_transport_https_http1_contract", output);
    assert_eq!(summary.handled, 1);
    assert_eq!(summary.alpn_protocols, vec![Some(b"http/1.1".to_vec())]);
    let _ = std::fs::remove_file(ca);
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_tls_contracts_over_tls12() {
    run_native_xsh_tls_contracts(&[&rustls::version::TLS12]);
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_tls_contracts_over_tls13() {
    run_native_xsh_tls_contracts(&[&rustls::version::TLS13]);
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_request_many_over_https_http2() {
    let server = LocalHttpsHttp2Server::spawn(2);
    let ca = temp_path("native-xsh-net-h2-ca.pem");
    let _ = std::fs::remove_file(&ca);
    std::fs::write(&ca, LOCAL_HTTPS_CA).expect("write local HTTPS CA");
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_transport_request_many_https_h2_contract",
        &[
            ("XSH_NET_TEST_H2_URL", &server.url),
            ("XSH_NET_TEST_CA", ca.to_str().expect("CA path is UTF-8")),
        ],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test("test_net_transport_request_many_https_h2_contract", output);
    assert_eq!(summary.handled, 2);
    let _ = std::fs::remove_file(ca);
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_job_cancel_keeps_https_http2_pool_healthy() {
    let server = LocalHttpsHttp2CancellationServer::spawn();
    let ca = temp_path("native-xsh-net-h2-cancel-ca.pem");
    let _ = std::fs::remove_file(&ca);
    std::fs::write(&ca, LOCAL_HTTPS_CA).expect("write local HTTPS CA");
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_job_cancel_keeps_h2_siblings_and_pool_healthy",
        &[
            ("XSH_NET_TEST_H2_CANCEL_URL", &server.url),
            ("XSH_NET_TEST_CA", ca.to_str().expect("CA path is UTF-8")),
        ],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test(
        "test_net_job_cancel_keeps_h2_siblings_and_pool_healthy",
        output,
    );
    assert_eq!(summary.handled, 4);
    let _ = std::fs::remove_file(ca);
}

#[cfg(feature = "net")]
#[test]
fn native_xsh_net_download_many_over_https_http2() {
    let server = LocalHttpsHttp2Server::spawn(1);
    let ca = temp_path("native-xsh-net-h2-download-ca.pem");
    let _ = std::fs::remove_file(&ca);
    std::fs::write(&ca, LOCAL_HTTPS_CA).expect("write local HTTPS CA");
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_transport_download_many_https_h2_contract",
        &[
            ("XSH_NET_TEST_H2_URL", &server.url),
            ("XSH_NET_TEST_CA", ca.to_str().expect("CA path is UTF-8")),
        ],
        false,
    );
    let summary = server.join();

    assert_native_xsh_test("test_net_transport_download_many_https_h2_contract", output);
    assert_eq!(summary.handled, 1);
    let _ = std::fs::remove_file(ca);
}

#[cfg(all(feature = "net", target_os = "linux"))]
#[test]
fn native_xsh_net_uses_ssl_cert_dir_for_linux_trust() {
    let server = LocalHttpsServer::spawn_with_protocol_versions(1, rustls::ALL_VERSIONS);
    let cert_dir = temp_path("native-xsh-net-system-ca-dir");
    let cert = cert_dir.join("local-ca.pem");
    let _ = std::fs::remove_dir_all(&cert_dir);
    std::fs::create_dir_all(&cert_dir).expect("create CA directory");
    std::fs::write(&cert, LOCAL_HTTPS_CA).expect("write local HTTPS CA");
    let output = run_native_xsh_test(
        "tests/xsh/stdlib/net.xsh::test_net_transport_linux_system_ca_dir",
        &[
            ("XSH_NET_TEST_TLS_URL", &server.url),
            (
                "SSL_CERT_DIR",
                cert_dir.to_str().expect("CA directory path is UTF-8"),
            ),
        ],
        true,
    );
    let summary = server.join();

    assert_native_xsh_test("test_net_transport_linux_system_ca_dir", output);
    assert_eq!(summary.handled, 1);
    let _ = std::fs::remove_dir_all(cert_dir);
}

#[cfg(feature = "net")]
#[test]
fn net_module_request_many_verifies_local_https_over_tls13() {
    let server = LocalHttpsServer::spawn_with_protocol_versions(2, &[&rustls::version::TLS13]);
    let ca = temp_path("net-request-many-ca.pem");
    let _ = std::fs::remove_file(&ca);
    std::fs::write(&ca, LOCAL_HTTPS_CA).expect("write local HTTPS CA");
    let source = format!(
        r#"
let responses = net.request_many({{
  requests: [
    {{method: "GET", url: "{url}/secure"}},
    {{method: "GET", url: "{url}/secure"}},
  ],
  concurrency: 2,
  ca_certificate: Path({ca}),
  pool: "many-https",
}})?
print ${{responses[0]?.body.utf8()?}} ${{responses[1]?.body.utf8()?}}
"#,
        url = server.url,
        ca = xsh_string_literal(ca.to_str().unwrap()),
    );

    let output = run_temp_script("net-request-many-https", &source);
    let summary = server.join();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "secure secure\n");
    assert_eq!(summary.handled, 2);
    let _ = std::fs::remove_file(ca);
}

#[cfg(feature = "net")]
#[test]
fn net_module_request_many_negotiates_local_https_http2() {
    let server = LocalHttpsHttp2Server::spawn(2);
    let ca = temp_path("net-request-many-h2-ca.pem");
    let _ = std::fs::remove_file(&ca);
    std::fs::write(&ca, LOCAL_HTTPS_CA).expect("write local HTTPS CA");
    let source = format!(
        r#"
let responses = net.request_many({{
  requests: [
    {{method: "GET", url: "{url}/h2"}},
    {{method: "GET", url: "{url}/h2"}},
  ],
  concurrency: 1,
  ca_certificate: Path({ca}),
  pool: "many-https-h2",
}})?
print ${{responses[0]?.body.utf8()?}} ${{responses[1]?.body.utf8()?}}
"#,
        url = server.url,
        ca = xsh_string_literal(ca.to_str().unwrap()),
    );

    let output = run_temp_script("net-request-many-https-h2", &source);
    let summary = server.join();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "h2 h2\n");
    assert_eq!(summary.handled, 2);
    let _ = std::fs::remove_file(ca);
}

#[cfg(feature = "net")]
#[test]
fn net_module_download_many_negotiates_local_https_http2() {
    let server = LocalHttpsHttp2Server::spawn(1);
    let ca = temp_path("net-download-many-h2-ca.pem");
    let dest = temp_path("net-download-many-h2.txt");
    let _ = std::fs::remove_file(&ca);
    let _ = std::fs::remove_file(&dest);
    std::fs::write(&ca, LOCAL_HTTPS_CA).expect("write local HTTPS CA");
    let source = format!(
        r#"
let responses = net.download_many({{
  downloads: [{{url: "{url}/h2", dest: Path({dest}), overwrite: true}}],
  ca_certificate: Path({ca}),
  pool: "many-download-h2",
}})?
print ${{responses[0]?.bytes}}
"#,
        url = server.url,
        ca = xsh_string_literal(ca.to_str().unwrap()),
        dest = xsh_string_literal(dest.to_str().unwrap()),
    );

    let output = run_temp_script("net-download-many-h2", &source);
    let summary = server.join();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "2\n");
    assert_eq!(std::fs::read(&dest).unwrap(), b"h2");
    assert_eq!(summary.handled, 1);
    let _ = std::fs::remove_file(ca);
    let _ = std::fs::remove_file(dest);
}

#[cfg(feature = "net")]
#[test]
fn net_module_verifies_local_https_with_custom_ca_over_tls12() {
    let server = LocalHttpsServer::spawn_with_protocol_versions(2, &[&rustls::version::TLS12]);
    let ca = temp_path("net-local-https-ca.pem");
    let dest = temp_path("net-local-https-download.txt");
    let _ = std::fs::remove_file(&ca);
    let _ = std::fs::remove_file(&dest);
    std::fs::write(&ca, LOCAL_HTTPS_CA).expect("write local HTTPS CA");
    let source = format!(
        r#"
let first = net.request({{
  method: "GET",
  url: "{url}/secure",
  ca_certificate: Path({ca}),
  pool: "https-test",
}})?
let downloaded = net.download({{
  url: "{url}/secure",
  dest: Path({dest}),
  ca_certificate: Path({ca}),
  overwrite: true,
  pool: "https-test",
}})?
print ${{first.status}} ${{first.body.utf8()?}} ${{downloaded.status}} ${{downloaded.bytes}}
"#,
        url = server.url,
        ca = xsh_string_literal(ca.to_str().unwrap()),
        dest = xsh_string_literal(dest.to_str().unwrap()),
    );

    let output = run_temp_script("net-local-https", &source);
    let summary = server.join();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "200 secure 200 6\n"
    );
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "secure");
    assert_eq!(summary.handled, 2);

    let _ = std::fs::remove_file(ca);
    let _ = std::fs::remove_file(dest);
}

#[test]
fn helper_binaries_cover_raw_argv_env_path_and_glob_boundaries() {
    let root = temp_path("raw-boundary-root");
    std::fs::create_dir_all(&root).unwrap();
    let raw_name = std::ffi::OsString::from_vec(b"raw\xfffile".to_vec());
    let raw_path = root.join(PathBuf::from(raw_name));
    let (raw_path, raw_path_expr) = match std::fs::write(&raw_path, b"ok") {
        Ok(()) => {
            let raw_path_expr = xsh_bytes_literal(raw_path.as_os_str().as_bytes());
            (raw_path, raw_path_expr)
        }
        Err(error) if cfg!(target_os = "macos") && error.raw_os_error() == Some(libc::EILSEQ) => {
            // macOS rejects this filename before XSH can observe it; the raw
            // argv and environment checks still run on that host.
            let fallback = root.join("raw-file");
            std::fs::write(&fallback, b"ok").unwrap();
            let fallback_expr = xsh_bytes_literal(fallback.as_os_str().as_bytes());
            (fallback, fallback_expr)
        }
        Err(error) => panic!("create raw filename fixture: {error}"),
    };
    let raw_arg_hex = "726177ff617267";
    let raw_path_hex = hex(raw_path.as_os_str().as_bytes());
    let glob_pattern = root.join("*");
    let source = format!(
        "\
let helper = Path({})
let root = Path({})
let raw_arg = Path.parse_bytes(b\"raw\\xffarg\")?
let argv = run.text (helper) show-argv (raw_arg) \"two words\" ?
let spliced = run.text (helper) show-argv ${{[\"one\", \"two words\"]}} ?
let env_output = run.bytes XSH_RAW=(raw_arg) printenv XSH_RAW ?
let raw_path = Path.parse_bytes({})?
let stat = run.status \"test\" -e (raw_path)
let globbed = run.text (helper) show-argv @(g{}) ?
let emitted = run.bytes printf r\"\\000\\377A\" ?
print ${{{} in argv}} ${{\"74776f20776f726473\" in argv}}
print ${{\"6f6e65\" in spliced}} ${{\"74776f20776f726473\" in spliced}}
print ${{env_output == b\"raw\\xffarg\\n\"}} ${{stat.exited_with(0)}} ${{{} in globbed}}
print ${{emitted == b\"\\0\\xffA\"}}
",
        xsh_string_literal(cargo_env!("CARGO_BIN_EXE_xsh-test-helper")),
        xsh_string_literal(root.to_str().unwrap()),
        raw_path_expr,
        xsh_string_literal(glob_pattern.to_str().unwrap()),
        xsh_string_literal(raw_arg_hex),
        xsh_string_literal(&raw_path_hex),
    );

    let output = run_temp_script("raw-boundaries", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true\ntrue true\ntrue true true\ntrue\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn command_path_shorthand_can_be_target_and_compound_interpolation_displays() {
    let source = format!(
        "\
let target = Path({})
let label = Path(\"bin/tool\")
let output = run.text $target show-argv \"${{label}} suffix\" ?
print ${{output.trim()}}
",
        xsh_string_literal(cargo_env!("CARGO_BIN_EXE_xsh-test-helper")),
    );

    let output = run_temp_script("command-path-shorthand", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "62696e2f746f6f6c20737566666978\n"
    );
}

#[cfg(feature = "net")]
struct LocalDnsServer {
    addr: String,
    handle: Option<std::thread::JoinHandle<LocalDnsSummary>>,
    stop: LocalServerStop,
}

#[derive(Debug)]
#[cfg(feature = "net")]
struct LocalDnsSummary {
    handled: usize,
}

#[cfg(feature = "net")]
impl LocalDnsServer {
    fn spawn(expected: usize) -> Self {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind DNS listener");
        socket
            .set_read_timeout(Some(Duration::from_millis(5)))
            .expect("set DNS poll timeout");
        let addr = socket.local_addr().expect("DNS listener addr").to_string();
        let stop = LocalServerStop::default();
        let server_stop = stop.clone();
        let handle = std::thread::spawn(move || {
            let mut handled = 0;
            let mut request = [0_u8; 512];
            let mut give_up_at = None;
            while handled < expected && !server_stop.expired(&mut give_up_at) {
                let (len, peer) = match socket.recv_from(&mut request) {
                    Ok(received) => received,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        continue;
                    }
                    Err(error) => panic!("read DNS request: {error}"),
                };
                let response = local_dns_response(&request[..len]);
                socket.send_to(&response, peer).expect("write DNS response");
                handled += 1;
            }
            LocalDnsSummary { handled }
        });
        Self {
            addr,
            handle: Some(handle),
            stop,
        }
    }

    fn join(mut self) -> std::thread::Result<LocalDnsSummary> {
        self.stop.signal();
        self.handle.take().expect("DNS server handle").join()
    }
}

#[cfg(feature = "net")]
impl Drop for LocalDnsServer {
    fn drop(&mut self) {
        self.stop.signal();
    }
}

#[cfg(feature = "net")]
fn local_dns_response(request: &[u8]) -> Vec<u8> {
    let mut response = Vec::new();
    response.extend_from_slice(&request[0..2]);
    response.extend_from_slice(&0x8180_u16.to_be_bytes());
    response.extend_from_slice(&1_u16.to_be_bytes());
    let mut offset = 12;
    while offset < request.len() && request[offset] != 0 {
        offset += request[offset] as usize + 1;
    }
    offset += 1;
    let qtype = u16::from_be_bytes([request[offset], request[offset + 1]]);
    let question_end = offset + 4;
    let answer = matches!(qtype, 1 | 28);
    response.extend_from_slice(&(answer as u16).to_be_bytes());
    response.extend_from_slice(&0_u16.to_be_bytes());
    response.extend_from_slice(&0_u16.to_be_bytes());
    response.extend_from_slice(&request[12..question_end]);
    if answer {
        response.extend_from_slice(&0xc00c_u16.to_be_bytes());
        response.extend_from_slice(&qtype.to_be_bytes());
        response.extend_from_slice(&1_u16.to_be_bytes());
        response.extend_from_slice(&60_u32.to_be_bytes());
        if qtype == 1 {
            response.extend_from_slice(&4_u16.to_be_bytes());
            response.extend_from_slice(&[192, 0, 2, 10]);
        } else {
            response.extend_from_slice(&16_u16.to_be_bytes());
            response.extend_from_slice(&[
                0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x42,
            ]);
        }
    }
    response
}

/// Coordinates two separate connections so the `/sync` response can only be
/// written after the independently submitted `/job` transport has progressed.
/// This tests driver progress while the evaluator checkpoint-waits in a normal
/// synchronous network call without using elapsed-time guesses.
#[cfg(feature = "net")]
struct ConcurrentProgressServer {
    url: String,
    handle: Option<std::thread::JoinHandle<()>>,
    stop: LocalServerStop,
}

#[cfg(feature = "net")]
impl ConcurrentProgressServer {
    fn spawn() -> Self {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("bind concurrent-progress listener");
        listener
            .set_nonblocking(true)
            .expect("set concurrent-progress listener nonblocking");
        let addr = listener
            .local_addr()
            .expect("concurrent-progress listener address");
        let (job_started_tx, job_started_rx) = crossbeam_channel::bounded(1);
        let (sync_started_tx, sync_started_rx) = crossbeam_channel::bounded(1);
        let (job_response_sent_tx, job_response_sent_rx) = crossbeam_channel::bounded(1);
        let stop = LocalServerStop::default();
        let server_stop = stop.clone();
        let handle = std::thread::spawn(move || {
            let completed = Arc::new(AtomicUsize::new(0));
            let done = Arc::new(AtomicBool::new(false));
            let mut give_up_at = None;
            let mut workers = Vec::new();
            while completed.load(Ordering::SeqCst) < 2 && !server_stop.expired(&mut give_up_at) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let completed = Arc::clone(&completed);
                        let done = Arc::clone(&done);
                        let job_started_tx = job_started_tx.clone();
                        let job_started_rx = job_started_rx.clone();
                        let sync_started_tx = sync_started_tx.clone();
                        let sync_started_rx = sync_started_rx.clone();
                        let job_response_sent_tx = job_response_sent_tx.clone();
                        let job_response_sent_rx = job_response_sent_rx.clone();
                        workers.push(std::thread::spawn(move || {
                            handle_concurrent_progress_connection(
                                stream,
                                job_started_tx,
                                job_started_rx,
                                sync_started_tx,
                                sync_started_rx,
                                job_response_sent_tx,
                                job_response_sent_rx,
                                completed,
                                done,
                            );
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept concurrent-progress connection: {error}"),
                }
            }
            done.store(true, Ordering::SeqCst);
            for worker in workers {
                worker.join().expect("concurrent-progress worker");
            }
            assert_eq!(
                completed.load(Ordering::SeqCst),
                2,
                "concurrent-progress fixture did not complete both requests"
            );
        });
        Self {
            url: format!("http://{addr}"),
            handle: Some(handle),
            stop,
        }
    }

    fn join(mut self) -> std::thread::Result<()> {
        self.stop.signal();
        self.handle
            .take()
            .expect("concurrent-progress server handle")
            .join()
    }
}

#[cfg(feature = "net")]
impl Drop for ConcurrentProgressServer {
    fn drop(&mut self) {
        self.stop.signal();
    }
}

#[cfg(feature = "net")]
fn handle_concurrent_progress_connection(
    mut stream: std::net::TcpStream,
    job_started_tx: crossbeam_channel::Sender<()>,
    job_started_rx: crossbeam_channel::Receiver<()>,
    sync_started_tx: crossbeam_channel::Sender<()>,
    sync_started_rx: crossbeam_channel::Receiver<()>,
    job_response_sent_tx: crossbeam_channel::Sender<()>,
    job_response_sent_rx: crossbeam_channel::Receiver<()>,
    completed: Arc<AtomicUsize>,
    done: Arc<AtomicBool>,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set concurrent-progress read timeout");
    let reader_stream = stream
        .try_clone()
        .expect("clone concurrent-progress stream");
    let mut reader = BufReader::new(reader_stream);
    let Some(request_line) = read_fixture_request_line(&mut reader, &done) else {
        return;
    };
    let path = request_line
        .split_whitespace()
        .nth(1)
        .expect("concurrent-progress request path");
    loop {
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .expect("read concurrent-progress request header");
        if header == "\r\n" || header == "\n" || header.is_empty() {
            break;
        }
    }

    match path {
        "/job" => {
            job_started_tx
                .send(())
                .expect("signal concurrent job request");
            sync_started_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("synchronous request starts before job response");
            write_concurrent_progress_response(&mut stream, "job");
            job_response_sent_tx
                .send(())
                .expect("signal concurrent job response");
        }
        "/sync" => {
            sync_started_tx
                .send(())
                .expect("signal concurrent synchronous request");
            job_started_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("background job starts while synchronous request waits");
            job_response_sent_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("background job response is released before synchronous response");
            write_concurrent_progress_response(&mut stream, "sync");
        }
        path => panic!("unexpected concurrent-progress path {path}"),
    }
    completed.fetch_add(1, Ordering::SeqCst);
}

#[cfg(feature = "net")]
fn write_concurrent_progress_response(stream: &mut std::net::TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("write concurrent-progress response");
    stream.flush().expect("flush concurrent-progress response");
}

#[cfg(feature = "net")]
struct BatchBarrierServer {
    url: String,
    events: crossbeam_channel::Receiver<String>,
    release_a_tx: crossbeam_channel::Sender<()>,
    handle: Option<std::thread::JoinHandle<()>>,
    stop: LocalServerStop,
}

#[cfg(feature = "net")]
impl BatchBarrierServer {
    fn spawn() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind batch barrier");
        listener
            .set_nonblocking(true)
            .expect("set batch barrier nonblocking");
        let addr = listener.local_addr().expect("batch barrier address");
        let (event_tx, events) = crossbeam_channel::unbounded();
        let (release_a_tx, release_a_rx) = crossbeam_channel::bounded(1);
        let stop = LocalServerStop::default();
        let server_stop = stop.clone();
        let handle = std::thread::spawn(move || {
            let completed = Arc::new(AtomicUsize::new(0));
            let done = Arc::new(AtomicBool::new(false));
            let mut give_up_at = None;
            let mut workers = Vec::new();
            while completed.load(Ordering::SeqCst) < 3 && !server_stop.expired(&mut give_up_at) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let event_tx = event_tx.clone();
                        let release_a_rx = release_a_rx.clone();
                        let completed = Arc::clone(&completed);
                        let done = Arc::clone(&done);
                        workers.push(std::thread::spawn(move || {
                            handle_batch_barrier_connection(
                                stream,
                                event_tx,
                                release_a_rx,
                                completed,
                                done,
                            );
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept batch barrier connection: {error}"),
                }
            }
            done.store(true, Ordering::SeqCst);
            for worker in workers {
                worker.join().expect("batch barrier worker");
            }
            assert_eq!(
                completed.load(Ordering::SeqCst),
                3,
                "batch barrier did not complete all requests"
            );
        });
        Self {
            url: format!("http://{addr}"),
            events,
            release_a_tx,
            handle: Some(handle),
            stop,
        }
    }

    fn release_a(&self) {
        self.release_a_tx.send(()).expect("release gated request A");
    }

    fn join(mut self) {
        self.stop.signal();
        if let Some(handle) = self.handle.take() {
            handle.join().expect("batch barrier server");
        }
    }
}

#[cfg(feature = "net")]
impl Drop for BatchBarrierServer {
    fn drop(&mut self) {
        self.stop.signal();
    }
}

#[cfg(feature = "net")]
fn handle_batch_barrier_connection(
    mut stream: std::net::TcpStream,
    event_tx: crossbeam_channel::Sender<String>,
    release_a_rx: crossbeam_channel::Receiver<()>,
    completed: Arc<AtomicUsize>,
    done: Arc<AtomicBool>,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set batch barrier read timeout");
    let reader_stream = stream.try_clone().expect("clone batch barrier stream");
    let mut reader = BufReader::new(reader_stream);
    let Some(request_line) = read_fixture_request_line(&mut reader, &done) else {
        return;
    };
    let path = request_line
        .split_whitespace()
        .nth(1)
        .expect("batch barrier request path")
        .to_string();
    loop {
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .expect("read batch barrier header");
        if header == "\r\n" || header == "\n" || header.is_empty() {
            break;
        }
    }
    event_tx
        .send(path.clone())
        .expect("send batch barrier event");
    if path == "/gate-a" {
        release_a_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("release gated request A");
    }
    let body = path.trim_start_matches('/').as_bytes();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        path.trim_start_matches('/'),
    );
    stream
        .write_all(response.as_bytes())
        .expect("write batch barrier response");
    stream.flush().expect("flush batch barrier response");
    completed.fetch_add(1, Ordering::SeqCst);
}

/// An accepted idle socket is not an HTTP request or one of the fixture's slots.
#[cfg(feature = "net")]
fn read_fixture_request_line(
    reader: &mut BufReader<std::net::TcpStream>,
    done: &AtomicBool,
) -> Option<String> {
    let mut request_line = String::new();
    loop {
        match reader.read_line(&mut request_line) {
            Ok(0) => return None,
            Ok(_) => return Some(request_line),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if done.load(Ordering::SeqCst) {
                    return None;
                }
            }
            Err(error) => panic!("read fixture HTTP request line: {error}"),
        }
    }
}

#[cfg(feature = "net")]
struct LocalHttpServer {
    url: String,
    handle: Option<std::thread::JoinHandle<LocalHttpSummary>>,
    stop: LocalServerStop,
}

#[derive(Debug)]
#[cfg(feature = "net")]
struct LocalHttpSummary {
    handled: usize,
    connections: usize,
    requests: Vec<LocalHttpRequest>,
}

#[derive(Debug)]
#[cfg(feature = "net")]
struct LocalHttpRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

#[cfg(feature = "net")]
impl LocalHttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

#[cfg(feature = "net")]
impl LocalHttpServer {
    fn spawn(expected: usize) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind HTTP listener");
        listener
            .set_nonblocking(true)
            .expect("set HTTP listener nonblocking");
        let addr = listener.local_addr().expect("HTTP listener addr");
        let url = format!("http://{addr}");
        let stop = LocalServerStop::default();
        let server_stop = stop.clone();
        let handle = std::thread::spawn(move || {
            let handled = Arc::new(AtomicUsize::new(0));
            let connections = Arc::new(AtomicUsize::new(0));
            let (request_tx, request_rx) = crossbeam_channel::unbounded();
            let mut give_up_at = None;
            let mut workers = Vec::new();
            while handled.load(Ordering::SeqCst) < expected && !server_stop.expired(&mut give_up_at)
            {
                match listener.accept() {
                    Ok((stream, _)) => {
                        connections.fetch_add(1, Ordering::SeqCst);
                        let request_tx = request_tx.clone();
                        let handled = handled.clone();
                        workers.push(std::thread::spawn(move || {
                            handle_local_http_connection(stream, handled, request_tx);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept HTTP connection: {error}"),
                }
            }
            drop(request_tx);
            for worker in workers {
                worker.join().expect("HTTP worker");
            }
            let requests = request_rx.into_iter().collect::<Vec<_>>();
            LocalHttpSummary {
                handled: handled.load(Ordering::SeqCst),
                connections: connections.load(Ordering::SeqCst),
                requests,
            }
        });
        Self {
            url,
            handle: Some(handle),
            stop,
        }
    }

    fn join(mut self) -> LocalHttpSummary {
        self.stop.signal();
        self.handle
            .take()
            .expect("HTTP server handle")
            .join()
            .expect("HTTP server")
    }
}

#[cfg(feature = "net")]
impl Drop for LocalHttpServer {
    fn drop(&mut self) {
        self.stop.signal();
    }
}

#[cfg(feature = "net")]
fn handle_local_http_connection(
    mut stream: std::net::TcpStream,
    handled: Arc<AtomicUsize>,
    request_tx: crossbeam_channel::Sender<LocalHttpRequest>,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set HTTP read timeout");
    let reader_stream = stream.try_clone().expect("clone HTTP stream");
    let mut reader = BufReader::new(reader_stream);
    loop {
        let mut request_line = String::new();
        match reader.read_line(&mut request_line) {
            Ok(0) => break,
            Ok(_) => {}
            // A live keep-alive connection can temporarily report WouldBlock
            // between client requests; only a real read timeout ends the worker.
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                break;
            }
            // Timeout tests intentionally cancel a request before the server
            // responds. Darwin reports the peer's ordinary close as reset.
            Err(error) if local_http_connection_closed(&error) => break,
            Err(error) => panic!("read HTTP request line: {error}"),
        }
        if request_line == "\r\n" {
            continue;
        }
        let parts = request_line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 2 {
            break;
        }
        let method = parts[0].to_string();
        let path = parts[1].to_string();
        let mut headers = BTreeMap::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("read HTTP header");
            if line == "\r\n" || line == "\n" || line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
            }
        }
        let content_length = headers
            .get("content-length")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = vec![0; content_length];
        if content_length > 0 {
            reader.read_exact(&mut body).expect("read HTTP body");
        }
        let request = LocalHttpRequest {
            method: method.clone(),
            path: path.clone(),
            headers,
            body,
        };
        let response = local_http_response(&request);
        request_tx.send(request).expect("send HTTP request");
        handled.fetch_add(1, Ordering::SeqCst);
        if path == "/slow-body" {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nfirst",
                )
                .expect("write first slow body frame");
            stream.flush().expect("flush first slow body frame");
            std::thread::sleep(Duration::from_millis(200));
            if let Err(error) = stream.write_all(b"second") {
                if local_http_connection_closed(&error) {
                    break;
                }
                panic!("write second slow body frame: {error}");
            }
            let _ = stream.flush();
            continue;
        }
        if path == "/slow" {
            std::thread::sleep(Duration::from_millis(200));
        }
        if let Err(error) = stream.write_all(response.as_bytes()) {
            if local_http_connection_closed(&error) {
                break;
            }
            panic!("write HTTP response: {error}");
        }
        if let Err(error) = stream.flush() {
            if local_http_connection_closed(&error) {
                break;
            }
            panic!("flush HTTP response: {error}");
        }
    }
}

#[cfg(feature = "net")]
fn local_http_connection_closed(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::NotConnected
    )
}

#[cfg(feature = "net")]
fn local_http_response(request: &LocalHttpRequest) -> String {
    let (status, reason, headers, body): (&str, &str, Vec<(&str, &str)>, Vec<u8>) =
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/hello") => ("200", "OK", Vec::new(), b"hello".to_vec()),
            ("GET", "/redirect") => ("302", "Found", vec![("Location", "/hello")], Vec::new()),
            ("GET", "/redirect-missing") => ("302", "Found", Vec::new(), Vec::new()),
            ("GET", "/redirect-loop") => (
                "302",
                "Found",
                vec![("Location", "/redirect-loop")],
                Vec::new(),
            ),
            ("GET", "/slow") => ("200", "OK", Vec::new(), b"slow".to_vec()),
            ("POST", "/echo") => {
                let mut body = b"echo:".to_vec();
                body.extend_from_slice(&request.body);
                ("200", "OK", Vec::new(), body)
            }
            ("GET", "/status") => ("404", "Not Found", Vec::new(), b"missing".to_vec()),
            ("GET", "/file") => ("200", "OK", Vec::new(), b"downloaded\n".to_vec()),
            ("GET", "/header-file") if request.header("x-download") == Some("yes") => {
                ("200", "OK", Vec::new(), b"downloaded\n".to_vec())
            }
            ("GET", "/header-file") => (
                "400",
                "Bad Request",
                Vec::new(),
                b"missing X-Download".to_vec(),
            ),
            ("PUT", "/upload")
                if request.header("authorization") == Some("Bearer secret-token") =>
            {
                let mut body = b"uploaded:".to_vec();
                body.extend_from_slice(&request.body);
                ("201", "Created", Vec::new(), body)
            }
            ("PUT", "/upload") => (
                "401",
                "Unauthorized",
                Vec::new(),
                b"missing authorization".to_vec(),
            ),
            ("HEAD", "/hello") => ("200", "OK", Vec::new(), Vec::new()),
            _ => ("404", "Not Found", Vec::new(), b"missing".to_vec()),
        };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nDate: Thu, 01 Jan 1970 00:00:00 GMT\r\nContent-Length: {}\r\nConnection: keep-alive\r\n",
        body.len()
    );
    for (name, value) in headers {
        response.push_str(name);
        response.push_str(": ");
        response.push_str(value);
        response.push_str("\r\n");
    }
    response.push_str("\r\n");
    response.push_str(&String::from_utf8_lossy(&body));
    response
}

#[cfg(feature = "net")]
struct LocalTlsStallServer {
    url: String,
    handle: Option<std::thread::JoinHandle<()>>,
    stop: LocalServerStop,
}

#[cfg(feature = "net")]
impl LocalTlsStallServer {
    fn spawn() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind TLS stall listener");
        let address = listener.local_addr().expect("TLS stall listener address");
        let stop = LocalServerStop::default();
        let server_stop = stop.clone();
        let handle = std::thread::spawn(move || {
            let Some(mut stream) = accept_local_connection_until_stopped(&listener, &server_stop)
            else {
                return;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .expect("set TLS stall read timeout");
            let mut client_hello = [0_u8; 1];
            let _ = stream.read(&mut client_hello);
            std::thread::sleep(Duration::from_millis(150));
        });
        Self {
            url: format!("https://{address}"),
            handle: Some(handle),
            stop,
        }
    }

    fn join(mut self) {
        self.stop.signal();
        if let Some(handle) = self.handle.take() {
            handle.join().expect("join TLS stall server");
        }
    }
}

#[cfg(feature = "net")]
impl Drop for LocalTlsStallServer {
    fn drop(&mut self) {
        self.stop.signal();
    }
}

#[cfg(feature = "net")]
const LOCAL_HTTPS_CA: &str = r#"-----BEGIN CERTIFICATE-----
MIIDKzCCAhOgAwIBAgIUFAXileHSG5QLTkl6QZON6CGOkQgwDQYJKoZIhvcNAQEL
BQAwHDEaMBgGA1UEAwwRWFNIIExvY2FsIFRlc3QgQ0EwIBcNMjYwNTA2MjExNjMy
WhgPMjEyNjA0MTIyMTE2MzJaMBwxGjAYBgNVBAMMEVhTSCBMb2NhbCBUZXN0IENB
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAqnJ+gPJeQwYxsSiUlp6D
73eyjYpnP2F3iFlulGcq82haQBZpx3aTFMzJKwt+GAKsSpOTZmtEMHttq/37YpAJ
tlKSnfDKawWUl3UbTUE2ssL5wlu10zSeap1U66QPmv21CdVHLQMMdqCOBQoaZSaB
4/Tg+Dd+hnRlO4M0lHVgGs5HTmsQkKHk52AccREJ0poDF8CzZWYqB8+7Ky/S6gJd
Ge+hLTAdvhcp9gDhqMyTvFTGb2B8mM7PtFp+ekiMB/SGohWltBHuiaE9F0cD/4jW
aCdgSJZnT48ludhQzCq737OkycpDp2eEhNtzSAriJicqp+geDK5xWVlRq7F5qpyS
KQIDAQABo2MwYTAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIBBjAdBgNV
HQ4EFgQUpnJnG6ya98i0TtpHGThDxGvgBVcwHwYDVR0jBBgwFoAUpnJnG6ya98i0
TtpHGThDxGvgBVcwDQYJKoZIhvcNAQELBQADggEBAB8/2NaYhnap9CYVU2sQSAdw
qjJqrjTzZazfSJKB0OHenHhhr7/SX/WnyPCGJ1zVso+TofkW/+Flyg/Rn2WrLTWu
BgunkSTXUAQqDmi+Prb7NnaploX8FqRH3kqNNDiRc2tLowXyaXp98I3smq2L+k7m
d4s/EZnmwozMwLDdheL0qUF4tEyXP1rHMSvQXiIVtWMivlxhbwaR4zlHba5ac+l1
L9GvCZj0aK44uC+peJzaUWLHOyEAE+JIyMlFls2sX4iZSzV/U1OggMmF4GXUrNV0
SFhwoEz+Yh+xAgzm9h4pWgiB/jXU6zAHhtFEUF1opmIV8Pz2v0JZE+dAXEnTSK8=
-----END CERTIFICATE-----
"#;

#[cfg(feature = "net")]
const LOCAL_HTTPS_CERT: &str = r#"-----BEGIN CERTIFICATE-----
MIIDUzCCAjugAwIBAgIUaiGXATFYeoPZWvjuGeZp4tPiy0UwDQYJKoZIhvcNAQEL
BQAwHDEaMBgGA1UEAwwRWFNIIExvY2FsIFRlc3QgQ0EwIBcNMjYwNTA2MjExNjMy
WhgPMjEyNjA0MTIyMTE2MzJaMBQxEjAQBgNVBAMMCTEyNy4wLjAuMTCCASIwDQYJ
KoZIhvcNAQEBBQADggEPADCCAQoCggEBAJuahnyFwo0uT4fI9CYZnPqK5467NQMF
UngzgP79AB01sm4XVMsmlaiEnHGITeTLR+nNoXhA0fMKQUbaoTy70qxx1alUGhcV
h8iaddjmfYcD9LVK+ec6d34BiJ+f/F0XIXHGpXozXa8LDPFrwitY2Hr+ITBe6398
Lb15VMqL8AitJBFg6CjmLdIsxNugRVQhyzFmA8V4OB4RmsQU8F/1dFp7s8HxAFi9
k277MooRWZ/CNm2mK0rpnWExi7WSnBxPbKQNiH4O+BTHywcQ9Od7tv5QVJLVIaL6
965TCzCxkG8smbUHy73lng9/J/8kh2y3352ShCiPGDdhD8nqffTH+EMCAwEAAaOB
kjCBjzAMBgNVHRMBAf8EAjAAMA4GA1UdDwEB/wQEAwIFoDATBgNVHSUEDDAKBggr
BgEFBQcDATAaBgNVHREEEzARhwR/AAABgglsb2NhbGhvc3QwHQYDVR0OBBYEFKRR
E4zT2eEiMTCBBxLZKWVylVoPMB8GA1UdIwQYMBaAFKZyZxusmvfItE7aRxk4Q8Rr
4AVXMA0GCSqGSIb3DQEBCwUAA4IBAQCeZH+AXtuGdGMKnFHWlBoTqH+imr9/g3fi
RV4+CfgZD3jRtxmo8ry1GZeADAjrgL0OiWavXbYjT8wVQYzCvF2dFsZRtuHXzFhp
7kTfOQ9S22DItqhz5mmcdn2qMh61RflbmvmojuZvaP9pfwBEH+ZXVhrNRISmvu6a
Wrl0WTjkhVEqFwczlWRSJ5bgNJMUruxAQ2PtPBDzN1xtR9Ep8EDAtzSAXQsd9FKl
SftewNs36pz1KJxTBFIuuEDRDTku7kUkpMBNMM5tzo4fk7+mxTp58DQ3SITt3KRQ
ffPVy+VvBiFrbQiNQWfZkzwMwf9v5mY0pPdmwEpTcHBeP3BWUKxP
-----END CERTIFICATE-----
"#;

#[cfg(feature = "net")]
const LOCAL_HTTPS_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQCbmoZ8hcKNLk+H
yPQmGZz6iueOuzUDBVJ4M4D+/QAdNbJuF1TLJpWohJxxiE3ky0fpzaF4QNHzCkFG
2qE8u9KscdWpVBoXFYfImnXY5n2HA/S1SvnnOnd+AYifn/xdFyFxxqV6M12vCwzx
a8IrWNh6/iEwXut/fC29eVTKi/AIrSQRYOgo5i3SLMTboEVUIcsxZgPFeDgeEZrE
FPBf9XRae7PB8QBYvZNu+zKKEVmfwjZtpitK6Z1hMYu1kpwcT2ykDYh+DvgUx8sH
EPTne7b+UFSS1SGi+veuUwswsZBvLJm1B8u95Z4Pfyf/JIdst9+dkoQojxg3YQ/J
6n30x/hDAgMBAAECggEAB5aeNWPZNxpe5mxDrg5tmlTDgHibSdXQVPONm/cmFhvr
OG4AEseDi3bVdvdGWTJUdd36SxKhSoCTeLYOdbPUvvREOXy3qFdQ2Jv+boDeoUnt
YG6oVkhJ0Wl7Zu3lCgpEWP7SMMNEoZz9LotN8W5zDDh3QxtZfDDyhItD1nw1InY4
jJYjy/FV96ywZ1lNqwhSGmpb12CZqPJCMSD/HS+9EcW2hcuCB4asB+Ms3OQel451
kgr0KWorGNxA/ObsvI5sB6mwEtcvDoiKytCmiJkRrSlguFkPOe2UthvRYtwR9ym4
IDcaLlfoCKcELKcjgJ/g25W1897+6dGP7WtA9YGRnQKBgQDL7iS810ZvCyxdiNhy
wp0y+jw96QsV4KyRbdAl/VSj+Nfy59AQM5Qjm+j3HGoYnWReDJahXMQqTQ3LzmY9
VwIs6koYvMbCvjtEBHGy19G03qvCOhzIufsqz+nqCvSN3gYg1vWPl7GgEuBRGLpq
Be2h7gqkYHPYn4T8My8L3AuV1QKBgQDDVYWbW2Tzd9ez5i+JIq8374kkGbzXpFUg
NI3muJqOAD0+TcjKRga5CfQYUCWzNiRQtaN5QgzKoPTNOhrRRfskDJcffUaB/1cW
u26ovcAeyhRoaiMRh6MI5qUnrdWB7pfIxW1TDYxFUaQpfrgsBnDWvnNJKIk9wcRB
D0eWlB3ptwKBgHstB7GskhWGeTCx9JM0q8Db1sFKXvDC+VkKLDyWDKbSKpXEoS74
CJWNmaSQ3CCsCLCqB93Fa5NlYVzl+Wk5gc3hYgoZFDESuDd4O7jblQYbrUEu2q3/
cA9G8DH2lgqOvcLeNAqchKR8YlN5jTd3BzbU0kbBH5gLmka/H76ZFcJVAoGAPQUl
ZL/rTGd+udNJvERailXI+L8VkCPk99eTEKVQmtWWTDVOaWnwxbNHTqUS8eYS+CeV
9tZcWpxnfQkOwZtj9gH837hp40hZ818AFbSZJMUqFOg7JknB85DhvQB/90QKpIyQ
N2a/EBSN/Ox6Kj6k12DNcOg531H9tflI+tAwfAcCgYARm/pm2RjJkBIZQUH9Ma8F
BahnLy2r5uAakgLMdzP8zD+jvq3gheMoAqiMy/8bjZ/Jl/lOYXbxRK2oiUGLyT7J
T3RFoA+wj/SltaKuIhGioO97KiSKKT2fz53HUaPRyjrAakl9gPD0HG5o9yX8uz0E
I8+sa866U25QjqrMXvpD7g==
-----END PRIVATE KEY-----
"#;

#[cfg(feature = "net")]
struct LocalHttpsServer {
    url: String,
    handle: Option<std::thread::JoinHandle<LocalHttpsSummary>>,
    stop: LocalServerStop,
}

#[cfg(feature = "net")]
#[derive(Debug)]
struct LocalHttpsSummary {
    handled: usize,
    alpn_protocols: Vec<Option<Vec<u8>>>,
}

#[cfg(feature = "net")]
impl LocalHttpsServer {
    fn spawn_with_protocol_versions(
        expected: usize,
        versions: &[&'static rustls::SupportedProtocolVersion],
    ) -> Self {
        Self::spawn(
            expected,
            local_https_config_with_protocol_versions(versions),
        )
    }

    fn spawn_with_h2_offer(
        expected: usize,
        versions: &[&'static rustls::SupportedProtocolVersion],
    ) -> Self {
        let mut config = local_https_config_with_protocol_versions(versions);
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Self::spawn(expected, config)
    }

    fn spawn(expected: usize, config: rustls::ServerConfig) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind HTTPS listener");
        listener
            .set_nonblocking(true)
            .expect("set HTTPS listener nonblocking");
        let addr = listener.local_addr().expect("HTTPS listener addr");
        let url = format!("https://{addr}");
        let config = Arc::new(config);
        let stop = LocalServerStop::default();
        let server_stop = stop.clone();
        let handle = std::thread::spawn(move || {
            let mut give_up_at = None;
            let mut handled = 0;
            let mut alpn_protocols = Vec::new();
            while handled < expected && !server_stop.expired(&mut give_up_at) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        alpn_protocols.push(handle_local_https_connection(stream, config.clone()));
                        handled += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept HTTPS connection: {error}"),
                }
            }
            LocalHttpsSummary {
                handled,
                alpn_protocols,
            }
        });
        Self {
            url,
            handle: Some(handle),
            stop,
        }
    }

    fn join(mut self) -> LocalHttpsSummary {
        self.stop.signal();
        self.handle
            .take()
            .expect("HTTPS server handle")
            .join()
            .expect("HTTPS server")
    }
}

#[cfg(feature = "net")]
impl Drop for LocalHttpsServer {
    fn drop(&mut self) {
        self.stop.signal();
    }
}

#[cfg(feature = "net")]
struct LocalHttpsHttp2Server {
    url: String,
    handle: Option<std::thread::JoinHandle<LocalHttpsSummary>>,
    stop: LocalServerStop,
}

#[cfg(feature = "net")]
impl LocalHttpsHttp2Server {
    fn spawn(expected: usize) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind HTTPS H2 listener");
        let addr = listener.local_addr().expect("HTTPS H2 listener addr");
        let url = format!("https://{addr}");
        let mut config = local_https_config();
        config.alpn_protocols = vec![b"h2".to_vec()];
        let config = Arc::new(config);
        let stop = LocalServerStop::default();
        let server_stop = stop.clone();
        let handle = std::thread::spawn(move || {
            let listener = async_io::Async::new(listener).expect("make HTTPS H2 listener async");
            let served = block_on_until_stopped(&server_stop, async move {
                let (stream, _) = listener.accept().await.expect("accept HTTPS H2 connection");
                let acceptor = futures_rustls::TlsAcceptor::from(config);
                let stream = acceptor.accept(stream).await.expect("accept HTTPS H2 TLS");
                let mut connection = h2::server::handshake(stream)
                    .await
                    .expect("HTTPS H2 handshake");
                for _ in 0..expected {
                    let (request, mut respond) = connection
                        .accept()
                        .await
                        .expect("HTTPS H2 request result")
                        .expect("HTTPS H2 request");
                    assert_eq!(request.version(), http::Version::HTTP_2);
                    assert_eq!(request.uri().path(), "/h2");
                    let mut body = respond
                        .send_response(
                            http::Response::builder()
                                .status(200)
                                .body(())
                                .expect("HTTPS H2 response"),
                            false,
                        )
                        .expect("send HTTPS H2 response");
                    body.send_data(bytes::Bytes::from_static(b"h2"), true)
                        .expect("send HTTPS H2 body");
                }
                connection.graceful_shutdown();
                let close_result = std::future::poll_fn(|cx| connection.poll_closed(cx)).await;
                if let Err(error) = close_result {
                    let peer_closed = error.get_io().is_some_and(|io| {
                        matches!(
                            io.kind(),
                            std::io::ErrorKind::BrokenPipe
                                | std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::UnexpectedEof
                        )
                    });
                    assert!(peer_closed, "close HTTPS H2 connection: {error:?}");
                }
            });
            LocalHttpsSummary {
                handled: if served.is_some() { expected } else { 0 },
                alpn_protocols: vec![Some(b"h2".to_vec())],
            }
        });
        Self {
            url,
            handle: Some(handle),
            stop,
        }
    }

    fn join(mut self) -> LocalHttpsSummary {
        self.stop.signal();
        self.handle
            .take()
            .expect("HTTPS H2 server handle")
            .join()
            .expect("HTTPS H2 server")
    }
}

#[cfg(feature = "net")]
impl Drop for LocalHttpsHttp2Server {
    fn drop(&mut self) {
        self.stop.signal();
    }
}

#[cfg(feature = "net")]
struct LocalHttpsHttp2CancellationServer {
    url: String,
    handle: Option<std::thread::JoinHandle<LocalHttpsSummary>>,
    stop: LocalServerStop,
}

#[cfg(feature = "net")]
impl LocalHttpsHttp2CancellationServer {
    fn spawn() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("bind HTTPS H2 cancellation listener");
        let addr = listener
            .local_addr()
            .expect("HTTPS H2 cancellation listener addr");
        let url = format!("https://{addr}");
        let mut config = local_https_config();
        config.alpn_protocols = vec![b"h2".to_vec()];
        let config = Arc::new(config);
        let stop = LocalServerStop::default();
        let server_stop = stop.clone();
        let handle = std::thread::spawn(move || {
            let listener =
                async_io::Async::new(listener).expect("make HTTPS H2 cancellation listener async");
            let served = block_on_until_stopped(&server_stop, async move {
                let (stream, _) = listener
                    .accept()
                    .await
                    .expect("accept HTTPS H2 cancellation connection");
                let acceptor = futures_rustls::TlsAcceptor::from(config);
                let stream = acceptor
                    .accept(stream)
                    .await
                    .expect("accept HTTPS H2 cancellation TLS");
                let mut connection = h2::server::handshake(stream)
                    .await
                    .expect("HTTPS H2 cancellation handshake");

                let (warm, mut respond) = connection
                    .accept()
                    .await
                    .expect("HTTPS H2 warm request result")
                    .expect("HTTPS H2 warm request");
                assert_eq!(warm.version(), http::Version::HTTP_2);
                assert_eq!(warm.uri().path(), "/warm");
                let mut warm_body = respond
                    .send_response(
                        http::Response::builder()
                            .status(200)
                            .body(())
                            .expect("HTTPS H2 warm response"),
                        false,
                    )
                    .expect("send HTTPS H2 warm response");
                warm_body
                    .send_data(bytes::Bytes::from_static(b"warm"), true)
                    .expect("send HTTPS H2 warm body");

                let mut slow_body = None;
                let mut fast_respond = None;
                for _ in 0..2 {
                    let (request, mut respond) = connection
                        .accept()
                        .await
                        .expect("HTTPS H2 sibling request result")
                        .expect("HTTPS H2 sibling request");
                    assert_eq!(request.version(), http::Version::HTTP_2);
                    match request.uri().path() {
                        "/slow" => {
                            slow_body = Some(
                                respond
                                    .send_response(
                                        http::Response::builder()
                                            .status(200)
                                            .body(())
                                            .expect("HTTPS H2 slow response"),
                                        false,
                                    )
                                    .expect("send HTTPS H2 slow response"),
                            );
                        }
                        "/fast" => fast_respond = Some(respond),
                        path => panic!("unexpected HTTPS H2 sibling path {path}"),
                    }
                }
                assert!(slow_body.is_some(), "missing stalled HTTPS H2 stream");
                let mut fast_body = fast_respond
                    .expect("missing fast HTTPS H2 stream")
                    .send_response(
                        http::Response::builder()
                            .status(200)
                            .body(())
                            .expect("HTTPS H2 fast response"),
                        false,
                    )
                    .expect("send HTTPS H2 fast response");
                fast_body
                    .send_data(bytes::Bytes::from_static(b"fast"), true)
                    .expect("send HTTPS H2 fast body");

                let (later, mut respond) = connection
                    .accept()
                    .await
                    .expect("HTTPS H2 later request result")
                    .expect("HTTPS H2 later request");
                assert_eq!(later.version(), http::Version::HTTP_2);
                assert_eq!(later.uri().path(), "/later");
                let mut later_body = respond
                    .send_response(
                        http::Response::builder()
                            .status(200)
                            .body(())
                            .expect("HTTPS H2 later response"),
                        false,
                    )
                    .expect("send HTTPS H2 later response");
                later_body
                    .send_data(bytes::Bytes::from_static(b"later"), true)
                    .expect("send HTTPS H2 later body");

                drop(slow_body);
                connection.graceful_shutdown();
                let close_result = std::future::poll_fn(|cx| connection.poll_closed(cx)).await;
                if let Err(error) = close_result {
                    let peer_closed = error.get_io().is_some_and(|io| {
                        matches!(
                            io.kind(),
                            std::io::ErrorKind::BrokenPipe
                                | std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::UnexpectedEof
                        )
                    });
                    assert!(
                        peer_closed,
                        "close HTTPS H2 cancellation connection: {error:?}"
                    );
                }
            });
            LocalHttpsSummary {
                handled: if served.is_some() { 4 } else { 0 },
                alpn_protocols: vec![Some(b"h2".to_vec())],
            }
        });
        Self {
            url,
            handle: Some(handle),
            stop,
        }
    }

    fn join(mut self) -> LocalHttpsSummary {
        self.stop.signal();
        self.handle
            .take()
            .expect("HTTPS H2 cancellation server handle")
            .join()
            .expect("HTTPS H2 cancellation server")
    }
}

#[cfg(feature = "net")]
impl Drop for LocalHttpsHttp2CancellationServer {
    fn drop(&mut self) {
        self.stop.signal();
    }
}

/// Tells a fixture server that its client has finished (`join`) or the test
/// has ended (`Drop`). A server still waiting for a connection or request then
/// gives up after `GRACE`, so a failed client cannot leave `join` blocked
/// forever. The deadline starts at the signal rather than at spawn because
/// `xsht` startup time varies widely under load.
#[cfg(feature = "net")]
#[derive(Clone, Default)]
struct LocalServerStop(Arc<AtomicBool>);

#[cfg(feature = "net")]
impl LocalServerStop {
    const GRACE: Duration = Duration::from_secs(5);

    fn signal(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    fn signaled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Whether `GRACE` has passed since the polling server first saw the signal.
    fn expired(&self, give_up_at: &mut Option<Instant>) -> bool {
        self.signaled()
            && Instant::now() >= *give_up_at.get_or_insert_with(|| Instant::now() + Self::GRACE)
    }
}

#[cfg(feature = "net")]
fn accept_local_connection_until_stopped(
    listener: &std::net::TcpListener,
    stop: &LocalServerStop,
) -> Option<std::net::TcpStream> {
    listener
        .set_nonblocking(true)
        .expect("set local listener nonblocking");
    let mut give_up_at = None;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("set local stream blocking");
                return Some(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if stop.expired(&mut give_up_at) {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("accept local connection: {error}"),
        }
    }
}

/// Runs a fixture server future, returning `None` if it is still pending
/// `LocalServerStop::GRACE` after `stop` is signaled.
#[cfg(feature = "net")]
fn block_on_until_stopped<T>(
    stop: &LocalServerStop,
    future: impl std::future::Future<Output = T>,
) -> Option<T> {
    use std::future::Future;

    async_io::block_on(async move {
        let mut future = std::pin::pin!(future);
        let mut give_up = std::pin::pin!(async {
            while !stop.signaled() {
                async_io::Timer::after(Duration::from_millis(10)).await;
            }
            async_io::Timer::after(LocalServerStop::GRACE).await;
        });
        std::future::poll_fn(|cx| {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(cx) {
                return std::task::Poll::Ready(Some(value));
            }
            if give_up.as_mut().poll(cx).is_ready() {
                return std::task::Poll::Ready(None);
            }
            std::task::Poll::Pending
        })
        .await
    })
}

#[cfg(feature = "net")]
fn local_https_config() -> rustls::ServerConfig {
    local_https_config_with_protocol_versions(rustls::ALL_VERSIONS)
}

#[cfg(feature = "net")]
fn local_https_config_with_protocol_versions(
    versions: &[&'static rustls::SupportedProtocolVersion],
) -> rustls::ServerConfig {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    let cert = CertificateDer::from_pem_slice(LOCAL_HTTPS_CERT.as_bytes())
        .expect("parse local HTTPS cert");
    let key =
        PrivateKeyDer::from_pem_slice(LOCAL_HTTPS_KEY.as_bytes()).expect("parse local HTTPS key");
    rustls::ServerConfig::builder_with_provider(Arc::new(rustls_graviola::default_provider()))
        .with_protocol_versions(versions)
        .expect("HTTPS protocol versions")
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .expect("local HTTPS certificate")
}

#[cfg(feature = "net")]
fn handle_local_https_connection(
    stream: std::net::TcpStream,
    config: Arc<rustls::ServerConfig>,
) -> Option<Vec<u8>> {
    stream
        .set_nonblocking(false)
        .expect("set HTTPS stream blocking");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set HTTPS read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .expect("set HTTPS write timeout");
    let connection = rustls::ServerConnection::new(config).expect("HTTPS connection");
    let mut stream = rustls::StreamOwned::new(connection, stream);
    let mut request = Vec::new();
    let mut byte = [0_u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        match stream.read_exact(&mut byte) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::InvalidData
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::ConnectionReset
                ) =>
            {
                return stream.conn.alpn_protocol().map(ToOwned::to_owned);
            }
            Err(error) => panic!("read HTTPS request: {error}"),
        }
        request.push(byte[0]);
        if request.len() > 8192 {
            panic!("HTTPS request header too large");
        }
    }
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecure")
        .expect("write HTTPS response");
    stream.flush().expect("flush HTTPS response");
    stream.conn.alpn_protocol().map(ToOwned::to_owned)
}

#[test]
fn path_interpolation_preserves_native_bytes_in_literals_and_compound_argv() {
    let expected = hex(b"--target=raw\xff name/'\"");
    let source = format!(
        r#"
let show = Path({show})
let raw = Path.parse_bytes(b"raw\xff name/'\"")?
let composed = fp"prefix/{{raw}}/../end"
print --flush ${{composed == Path.parse_bytes(b"prefix/raw\xff name/'\"/../end")?}}
let direct = run.text (show) show-argv "--target=$raw" ?
print --flush ${{direct == {expected}}}
let spliced = run.text (show) show-argv @([raw, p""]) ?
print --flush ${{spliced == {spliced}}}
let human = run.text (show) show-argv (f"--target={{raw}}") ?
print --flush ${{human == {human}}}
let stored = process.command {{
  run (show) show-argv "--target=${{raw}}"
}}
let _ = process.run(stored)?
"#,
        show = xsh_string_literal(cargo_env!("CARGO_BIN_EXE_xsh-test-helper")),
        expected = xsh_string_literal(&(expected.clone() + "\n")),
        spliced = xsh_string_literal(&(hex(b"raw\xff name/\'\"") + "\n\n")),
        human = xsh_string_literal(
            &(hex(String::from_utf8_lossy(b"--target=raw\xff name/\'\"").as_bytes()) + "\n")
        ),
    );
    let output = run_temp_script("native-path-interpolation", &source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("true\ntrue\ntrue\ntrue\n{expected}\n")
    );
}

#[test]
fn path_interpolation_preserves_real_native_filename_and_redirection_bytes() {
    let root = temp_path("native-path-interpolation-files");
    std::fs::create_dir_all(&root).unwrap();
    let raw_name = std::ffi::OsString::from_vec(b"source\xff name".to_vec());
    let source_path = root.join(&raw_name);
    match std::fs::write(&source_path, b"native contents") {
        Ok(()) => {}
        Err(error) if cfg!(target_os = "macos") && error.raw_os_error() == Some(libc::EILSEQ) => {
            // Some macOS filesystems reject invalid UTF-8 names at creation.
            // The separate argv fixture still exercises native bytes there.
            eprintln!("host filesystem rejected invalid UTF-8 filename: {error}");
            std::fs::remove_dir_all(root).unwrap();
            return;
        }
        Err(error) => panic!("create native filename: {error}"),
    }
    let output_path = root.join(std::ffi::OsString::from_vec(b"output\xff name".to_vec()));
    let script = format!(
        r#"
let root = Path({root})
let name = Path.parse_bytes(b"source\xff name")?
let destination = Path.parse_bytes(b"output\xff name")?
let composed = fp"{{root}}/{{name}}"
print ${{composed.read_bytes()? == b"native contents"}}
let direct = run.bytes cat < "$root/$name" ?
print ${{direct == b"native contents"}}
let command = process.command {{
  stdin = fp"{{root}}/{{name}}"
  stdout = fp"{{root}}/{{destination}}"
  run cat
}}
let _ = process.run(command)?
"#,
        root = xsh_string_literal(root.to_str().unwrap())
    );
    let output = run_temp_script("native-path-file-redirections", &script);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"true\ntrue\n");
    assert_eq!(std::fs::read(output_path).unwrap(), b"native contents");
    std::fs::remove_dir_all(root).unwrap();
}
