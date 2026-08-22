proc test_net_module_with_mocks(ctx: TestContext) [fs, net, error] {
  let response = {
    status: 200,
    reason: "OK",
    bytes: 2,
    headers: [
      {
        name: "content-type",
        value: "text/plain",
      },
    ],
    url: "https://example.test/",
    body: b"ok",
  }

  test.mock(ctx, "net.request", {url: "https://example.test/"}, Ok(response))?

  test.mock(
    ctx,
    "net.request_many",
    {pool: "stdlib-test"},
    Ok([Ok(response)]),
  )?

  test.mock(
    ctx,
    "net.download",
    {url: "https://example.test/file"},
    Ok({
      status: 200,
      reason: "OK",
      bytes: 2,
      headers: [{name: "content-type", value: "text/plain"}],
      url: "https://example.test/file",
    }),
  )?

  test.mock(
    ctx,
    "net.download_many",
    {pool: "stdlib-test"},
    Ok(
      [
        Ok({
          status: 200,
          reason: "OK",
          bytes: 2,
          headers: [{name: "content-type", value: "text/plain"}],
          url: "https://example.test/file",
        }),
      ],
    ),
  )?

  test.mock(
    ctx,
    "net.upload",
    {url: "https://example.test/upload"},
    Ok({
      status: 200,
      reason: "OK",
      bytes: 2,
      headers: [{name: "content-type", value: "text/plain"}],
      url: "https://example.test/upload",
    }),
  )?

  test.eq(net.request({method: "GET", url: "https://example.test/"})?.body, b"ok")?
  let many = net.request_many({
    requests: [{method: "GET", url: "https://example.test/"}],
    pool: "stdlib-test",
  })?
  test.eq(many[0]?.body, b"ok")?
  test.eq(net.download({method: "GET", url: "https://example.test/file", dest: p"out"})?.status, 200)?
  let downloaded = net.download_many({
    downloads: [{url: "https://example.test/file", dest: p"out"}],
    pool: "stdlib-test",
  })?
  test.eq(downloaded[0]?.bytes, 2)?
  test.eq(net.upload({method: "PUT", url: "https://example.test/upload", source: p"in"})?.bytes, 2)?
  let pool = net.pool(name: "stdlib-test", max_idle_per_host: 1, idle_timeout: 1s)?
  test.eq(pool.name, "stdlib-test")?
  test.eq(pool.max_idle_per_host, 1)?
  net.close_pool("stdlib-test")?
  net.close_all_pools()?
  test.eq(test.calls(ctx, "net.request")[0].args.method, "GET")?
  test.eq(test.calls(ctx, "net.request_many")[0].args.requests[0].method, "GET")?
  test.eq(test.calls(ctx, "net.download")[0].args.dest.display(), "out")?
  test.eq(test.calls(ctx, "net.download_many")[0].args.downloads[0].url, "https://example.test/file")?
  test.eq(test.calls(ctx, "net.upload")[0].args.source.display(), "in")?
}

proc net_start_scoped_helper() [net] -> Result[NetJob] {
  return net.start({
    method: "GET",
    url: "https://example.test/returned-job",
    max_body_bytes: 1,
  })
}

proc test_net_start_mock_job_is_single_consumption(ctx: TestContext) [net, error] {
  let response = {
    status: 200,
    reason: "OK",
    bytes: 2,
    headers: [],
    url: "https://example.test/",
    body: b"ok",
  }
  test.mock(ctx, "net.start", {url: "https://example.test/"}, Ok(response))?

  let job = net.start({method: "GET", url: "https://example.test/"})?
  test.eq(job.wait()?.body, b"ok")?
  test.error_kind(job.cancel(), "net-job-not-live")?
}

proc test_net_job_progresses_while_synchronous_request_waits() [net, env, error] {
  let url = env.get_or("XSH_NET_TEST_CONCURRENT_URL", "")?
  if url == "" {
    test.skip("requires concurrent NetJob transport fixture")
    return
  }

  # The fixture holds the synchronous response until the driver's job response
  # has been released. `net.start` therefore must keep progressing while this
  # evaluator is checkpoint-waiting in `net.request`.
  let job = net.start({
    method: "GET",
    url: f"${url}/job",
    headers: [{name: "Connection", value: "close"}],
  })?
  let foreground = net.request({
    method: "GET",
    url: f"${url}/sync",
    headers: [{name: "Connection", value: "close"}],
  })?

  test.eq(foreground.body.utf8()?, "sync")?
  test.eq(job.wait()?.body.utf8()?, "job")?
}

proc test_net_start_transfers_returned_job_ownership(ctx: TestContext) [net, error] {
  let response = {
    status: 200,
    reason: "OK",
    bytes: 2,
    headers: [],
    url: "https://example.test/returned-job",
    body: b"ok",
  }
  test.mock(ctx, "net.start", {url: "https://example.test/returned-job"}, Ok(response))?

  let job = net_start_scoped_helper()?
  test.eq(job.wait()?.body, b"ok")?
}

proc test_net_start_aliases_share_one_consumption(ctx: TestContext) [net, error] {
  let response = {
    status: 204,
    reason: "No Content",
    bytes: 0,
    headers: [],
    url: "https://example.test/alias",
    body: b"",
  }
  test.mock(ctx, "net.start", {url: "https://example.test/alias"}, Ok(response))?

  let job = net.start({method: "GET", url: "https://example.test/alias"})?
  let alias = job
  alias.cancel()?
  test.error_kind(job.wait(), "net-job-not-live")?
  test.eq(test.calls(ctx, "net.start")[0].args.method, "GET")?
}

proc test_net_start_enforces_live_job_capacity(ctx: TestContext) [net, error] {
  let response = {
    status: 204,
    reason: "No Content",
    bytes: 0,
    headers: [],
    url: "https://example.test/capacity",
    body: b"",
  }
  test.mock(ctx, "net.start", {url: "https://example.test/capacity"}, Ok(response), 66)?

  test.error_kind(
    net.start({
      method: "GET",
      url: "https://example.test/capacity",
      max_body_bytes: 67108865,
    }),
    "net-overload",
  )?

  var jobs = [
    net.start({
      method: "GET",
      url: "https://example.test/capacity",
      max_body_bytes: 1,
    })?
    for _ in range(64)
  ]
  test.error_kind(
    net.start({method: "GET", url: "https://example.test/capacity", max_body_bytes: 1}),
    "net-overload",
  )?
  for job in jobs {
    job.cancel()?
  }
}

proc test_net_start_scope_cleanup_releases_admission(ctx: TestContext) [net, error] {
  let response = {
    status: 204,
    reason: "No Content",
    bytes: 0,
    headers: [],
    url: "https://example.test/cleanup",
    body: b"",
  }
  test.mock(ctx, "net.start", {url: "https://example.test/cleanup"}, Ok(response), 65)?

  let should_cleanup = ctx.keys().len() > 0
  if should_cleanup {
    let jobs = [
      net.start({
        method: "GET",
        url: "https://example.test/cleanup",
        max_body_bytes: 1,
      })?
      for _ in range(64)
    ]
    test.eq(jobs.len(), 64)?
  }

  let released = net.start({
    method: "GET",
    url: "https://example.test/cleanup",
    max_body_bytes: 1,
  })?
  released.cancel()?
}

proc test_net_start_loop_control_cleans_lexical_job_scopes(ctx: TestContext) [net, error] {
  let response = {
    status: 204,
    reason: "No Content",
    bytes: 0,
    headers: [],
    url: "https://example.test/loop-cleanup",
    body: b"",
  }
  test.mock(
    ctx,
    "net.start",
    {url: "https://example.test/loop-cleanup"},
    Ok(response),
    66,
  )?

  # Both control paths discard a statement block. Its unconsumed job must be
  # canceled before the next iteration, rather than staying live until this
  # function returns and exhausting the evaluator's job capacity.
  for _ in [0] {
    let _ = net.start({
      method: "GET",
      url: "https://example.test/loop-cleanup",
      max_body_bytes: 1,
    })?
    break
  }

  for _ in range(64) {
    let _ = net.start({
      method: "GET",
      url: "https://example.test/loop-cleanup",
      max_body_bytes: 1,
    })?
    continue
  }

  let final_job = net.start({
    method: "GET",
    url: "https://example.test/loop-cleanup",
    max_body_bytes: 1,
  })?
  test.eq(final_job.wait()?.status, 204)?
}

proc test_net_job_trace_is_correlated_and_redacts_request_secrets(ctx: TestContext) [net, env, error] {
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
  } else {
    let source = """
let url = env.get_or("XSH_NET_TEST_URL", "")?
let job = net.start({
  method: "POST",
  url: url + "/echo",
  headers: [{name: "authorization", value: "Bearer trace-secret"}],
  body: b"trace-secret",
})?
let response = job.wait()?
print \${response.status}
"""
    let trace = test.run_xsht_trace(ctx, source, ["--trace", "--raw", "--trace-format", "jsonl"])?
    test.ok(trace.success, trace.stderr)?
    test.eq(
      trace.stdout,
      """200
""",
    )?
    test.contains(trace.stderr, "\"kind\":\"net.job.accepted\"")?
    test.contains(trace.stderr, "\"kind\":\"net.job.scheduled\"")?
    test.contains(trace.stderr, "\"kind\":\"net.transport.started\"")?
    test.contains(trace.stderr, "\"kind\":\"net.transport.completed\"")?
    test.contains(trace.stderr, "\"kind\":\"net.job.wait\"")?
    test.contains(trace.stderr, "\"api_id\":\"module.net.start\"")?
    test.contains(trace.stderr, "\"api_id\":\"method.NetJob.wait\"")?
    test.contains(trace.stderr, "\"job_id\":1")?
    test.contains(trace.stderr, "\"queue_duration_us\":")?
    test.contains(trace.stderr, "\"transport_duration_us\":")?
    test.ok("trace-secret" not in trace.stderr, trace.stderr)?
  }
}

proc test_net_runtime_descriptors_do_not_survive_exec(ctx: TestContext) [fs, net, process, env, error] {
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  let helper = env.get_or("XSH_NET_FD_HELPER", "")?
  if url == "" or helper == "" {
    test.skip("requires local network and descriptor fixtures")
    return
  }

  let root = test.temp_dir(ctx, name: "net-runtime-fds")?
  let output = fp"${root}/fds.txt"
  let job = net.start({method: "GET", url: url + "/hello"})?
  run ${helper} > output ?
  job.cancel()?
  test.eq(output.read_text()?, "")?
}

proc test_net_transport_http_contracts(ctx: TestContext) [fs, net, env, error] {
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
    return
  }

  let root = test.temp_dir(ctx, name: "net-http")?
  let upload_source = fp"${root}/upload.txt"
  let download_dest = fp"${root}/download.txt"
  let missing_ca = fp"${root}/missing-ca.pem"
  fs.write(upload_source, "upload-body")?

  let pool = net.pool("fixture", 4, 1s)?
  let first = net.request({method: "GET", url: f"${url}/hello", pool: "fixture"})?
  let second = net.request({method: "GET", url: f"${url}/hello", pool: "fixture"})?
  let headed = net.request({method: "HEAD", url: f"${url}/hello", pool: "fixture"})?
  let redirected = net.request({method: "GET", url: f"${url}/redirect", redirects: 1, pool: "fixture"})?
  let posted = net.request({
    method: "POST",
    url: f"${url}/echo",
    headers: [{name: "X-Test", value: "one"}],
    body_text: "payload",
    pool: "fixture",
  })?
  let posted_file = net.request({
    method: "POST",
    url: f"${url}/echo",
    body_file: upload_source,
    pool: "fixture",
  })?
  let posted_bytes = net.request({
    method: "POST",
    url: f"${url}/echo",
    body: b"bytes",
    pool: "fixture",
  })?
  let status = net.request({method: "GET", url: f"${url}/status", pool: "fixture"})?
  let downloaded = net.download({
    url: f"${url}/header-file",
    dest: download_dest,
    headers: [{name: "X-Download", value: "yes"}],
    overwrite: true,
    pool: "fixture",
  })?
  let uploaded = net.upload({
    url: f"${url}/upload",
    source: upload_source,
    headers: [{name: "Authorization", value: "Bearer secret-token"}],
    pool: "fixture",
  })?

  test.eq(pool.max_idle_per_host, 4)?
  test.eq(pool.idle_timeout_ms, 1000)?
  test.eq(first.reason, "OK")?
  test.eq(first.url, f"${url}/hello")?
  test.eq(first.body.utf8()?, "hello")?
  test.eq(second.body.utf8()?, "hello")?
  test.eq(headed.status, 200)?
  test.eq(headed.bytes, 0)?
  test.eq(redirected.body.utf8()?, "hello")?
  test.eq(posted.body.utf8()?, "echo:payload")?
  test.eq(posted_file.body.utf8()?, "echo:upload-body")?
  test.eq(posted_bytes.body.utf8()?, "echo:bytes")?
  test.eq(status.status, 404)?
  test.eq(downloaded.status, 200)?
  test.eq(downloaded.bytes, 11)?
  test.eq(
    download_dest.read_text()?,
    """downloaded
""",
  )?
  test.eq(uploaded.status, 201)?
  test.eq(uploaded.reason, "Created")?
  test.eq(uploaded.bytes, 20)?
  test.eq(uploaded.url, f"${url}/upload")?
  test.eq(first.headers[0].name, "Date")?
  test.eq(first.headers[1].value, "5")?
  test.error_kind(net.request({method: "GET", url: "ftp://example.invalid/file"}), "net-scheme")?
  test.error_kind(
    net.request({method: "GET", url: f"${url}/hello", ca_certificate: missing_ca}),
    "net-ca-certificate",
  )?
  net.close_pool("fixture")?
  net.close_all_pools()?
}

proc test_net_transport_error_contracts(ctx: TestContext) [fs, net, env, error] {
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
    return
  }

  let root = test.temp_dir(ctx, name: "net-errors")?
  let existing = fp"${root}/existing.txt"
  let limited = fp"${root}/limited.txt"
  let in_place = fp"${root}/in-place.txt"
  fs.write(existing, "previous")?
  fs.write(limited, "limited before")?
  fs.write(in_place, "old")?

  let redirect_limit = net.request({method: "GET", url: f"${url}/redirect", redirects: 0})
  let missing_location = net.request({method: "GET", url: f"${url}/redirect-missing", redirects: 1})
  let redirect_loop = net.request({method: "GET", url: f"${url}/redirect-loop", redirects: 1})
  let body_limit = net.request({method: "GET", url: f"${url}/hello", max_body_bytes: 4})
  let status = net.request({method: "GET", url: f"${url}/status", fail_status: true})
  let existing_result = net.download({url: f"${url}/file", dest: existing})
  let limited_result = net.download({
    url: f"${url}/hello",
    dest: limited,
    atomic: true,
    overwrite: true,
    max_body_bytes: 4,
  })
  let in_place_result = net.download({
    url: f"${url}/file",
    dest: in_place,
    atomic: false,
    overwrite: true,
  })?

  test.error_kind(redirect_limit, "net-redirect")?
  test.error_kind(missing_location, "net-redirect")?
  test.error_kind(redirect_loop, "net-redirect")?
  test.error_kind(body_limit, "net-body-limit")?
  test.error_kind(status, "net-status")?
  test.error_kind(existing_result, "net-dest")?
  test.error_kind(limited_result, "net-body-limit")?
  test.eq(existing.read_text()?, "previous")?
  test.eq(limited.read_text()?, "limited before")?
  test.eq(in_place_result.bytes, 11)?
  test.eq(
    in_place.read_text()?,
    """downloaded
""",
  )?
}

proc assert_invalid_net_input(ctx: TestContext, source: Str, kind: Str) [error] {
  let output = test.run_script(ctx, source)?
  test.eq(output.status, 3)?
  test.contains(output.stderr, kind)?
}

proc test_net_transport_rejects_invalid_shapes(ctx: TestContext) [fs, net, error] {
  let root = test.temp_dir(ctx, name: "net-invalid")?
  let missing_source = fp"${root}/missing.txt"

  test.error_kind(net.request({method: "TRACE", url: "http://127.0.0.1:9/"}), "net-method")?
  test.error_kind(net.request({method: "GET", url: "ftp://example.test/"}), "net-scheme")?
  test.error_kind(
    net.request({
      method: "GET",
      url: "http://127.0.0.1:9/",
      headers: [{name: "", value: "bad"}],
    }),
    "net-header",
  )?
  test.error_kind(
    net.request({method: "POST", url: "http://127.0.0.1:9/", body_file: missing_source}),
    "net-body-file",
  )?
  test.error_kind(
    net.upload({url: "http://127.0.0.1:9/", source: missing_source}),
    "net-source",
  )?
  test.error_kind(net.request_many({requests: [], concurrency: 0}), "net-concurrency")?
  test.error_kind(net.download_many({downloads: [], concurrency: -1}), "net-concurrency")?
  test.error_kind(net.pool("invalid", -1), "net-pool")?
  assert_invalid_net_input(
    ctx,
    """let _ = net.request({
  method: "POST",
  url: "http://127.0.0.1:9/",
  body: b"one",
  body_text: "two",
})
""",
    "net-body",
  )?
  assert_invalid_net_input(
    ctx,
    """let _ = net.request({method: "GET", url: "http://127.0.0.1:9/", redirects: -1})
""",
    "range-error",
  )?
  assert_invalid_net_input(
    ctx,
    """let _ = net.download({url: "http://127.0.0.1:9/", dest: p"missing", max_body_bytes: -1})
""",
    "range-error",
  )?
  test.error_kind(net.request({method: "GET", url: "http://"}), "net-url")?
  test.error_kind(
    net.upload({method: "TRACE", url: "http://127.0.0.1:9/", source: missing_source}),
    "net-method",
  )?
}

proc test_net_transport_timeout_contracts(ctx: TestContext) [fs, net, env, error] {
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
    return
  }

  test.error_kind(net.request({method: "GET", url: f"${url}/slow", timeout: 50ms}), "net-timeout")?
  let response = net.request({method: "GET", url: f"${url}/slow", connect_timeout: 50ms})?
  test.eq(response.body.utf8()?, "slow")?
  test.error_kind(
    net.request({method: "GET", url: f"${url}/slow", headers_timeout: 50ms}),
    "net-headers-timeout",
  )?
  test.error_kind(
    net.request({method: "GET", url: f"${url}/slow-body", body_idle_timeout: 50ms}),
    "net-body-idle-timeout",
  )?
  let root = test.temp_dir(ctx, name: "net-total-timeout")?
  let destination = fp"${root}/download.txt"
  test.error_kind(
    net.download({url: f"${url}/slow-body", dest: destination, timeout: 50ms}),
    "net-timeout",
  )?
  test.ok(! destination.exists()?)?

  let tls_stall_url = env.get_or("XSH_NET_TEST_TLS_STALL_URL", "")?
  if tls_stall_url != "" {
    test.error_kind(
      net.request({method: "GET", url: tls_stall_url, tls_timeout: 50ms}),
      "net-tls-timeout",
    )?
  }
}

proc test_net_transport_batch_contracts(ctx: TestContext) [fs, net, env, error] {
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
    return
  }

  let root = test.temp_dir(ctx, name: "net-batch")?
  let first = fp"${root}/first.txt"
  let second = fp"${root}/second.txt"
  let request_items = [
    {
      method: "GET",
      url: f"${url}/hello",
      headers: [
        {
          name: "Connection",
          value: "close",
        },
      ],
    },
    {
      method: "GET",
      url: "ftp://example.test/",
    },
    {
      method: "GET",
      url: f"${url}/status",
      headers: [
        {
          name: "Connection",
          value: "close",
        },
      ],
      fail_status: true,
    },
    {
      method: "GET",
      url: f"${url}/hello",
      headers: [
        {
          name: "Connection",
          value: "close",
        },
      ],
    },
  ]
  let download_items = [
    {
      url: f"${url}/hello",
      dest: first,
      overwrite: true,
    },
    {
      url: f"${url}/hello",
      dest: second,
      overwrite: true,
    },
  ]
  let requests = net.request_many({
    requests: request_items,
    concurrency: 2,
    pool: "batch",
  })?
  let downloads = net.download_many({
    downloads: download_items,
    concurrency: 2,
    pool: "batch-downloads",
  })?

  # A batch item is admitted only when its sliding-window slot opens. Its
  # total deadline must therefore not elapse while the preceding item runs.
  let queued_timeout_requests = net.request_many({
    requests: [{method: "GET", url: f"${url}/slow"}, {method: "GET", url: f"${url}/hello", timeout: 50ms}],
    concurrency: 1,
    pool: "batch-queued-timeout",
  })?

  test.eq(requests[0]?.body.utf8()?, "hello")?
  test.error_kind(requests[1], "net-scheme")?
  test.error_kind(requests[2], "net-status")?
  test.eq(requests[3]?.body.utf8()?, "hello")?
  test.eq(downloads[0]?.bytes, 5)?
  test.eq(downloads[1]?.bytes, 5)?
  test.eq(first.read_text()?, "hello")?
  test.eq(second.read_text()?, "hello")?
  test.eq(queued_timeout_requests[0]?.body.utf8()?, "slow")?
  test.eq(queued_timeout_requests[1]?.body.utf8()?, "hello")?
}

proc test_net_transport_batch_download_error_contract(ctx: TestContext) [fs, net, env, error] {
  let url = env.get_or("XSH_NET_TEST_URL", "")?
  if url == "" {
    test.skip("requires XSH_NET_TEST_URL fixture")
    return
  }

  let root = test.temp_dir(ctx, name: "net-batch-errors")?
  let redirected = fp"${root}/redirected.txt"
  let limited = fp"${root}/limited.txt"
  fs.write(limited, "previous")?
  let download_items = [
    {
      url: f"${url}/redirect",
      dest: redirected,
      atomic: true,
      overwrite: true,
      redirects: 1,
      max_body_bytes: 1024,
    },
    {
      url: f"${url}/hello",
      dest: limited,
      atomic: true,
      overwrite: true,
      redirects: 1,
      max_body_bytes: 4,
    },
  ]
  let responses = net.download_many({
    downloads: download_items,
    concurrency: 2,
    pool: "batch-errors",
  })?

  test.eq(responses[0]?.bytes, 5)?
  test.error_kind(responses[1], "net-body-limit")?
  test.eq(redirected.read_text()?, "hello")?
  test.eq(limited.read_text()?, "previous")?
}

proc test_net_transport_tls_contracts() [net, env, error] {
  let url = env.get_or("XSH_NET_TEST_TLS_URL", "")?
  let ca = env.get_or("XSH_NET_TEST_CA", "")?
  if url == "" or ca == "" {
    test.skip("requires TLS fixture")
    return
  }

  let rejected = net.request({method: "GET", url: f"${url}/secure"})
  let unverified = net.request({
    method: "GET",
    url: f"${url}/secure",
    tls_verify: false,
  })?
  let verified = net.request({
    method: "GET",
    url: f"${url}/secure",
    ca_certificate: fp"${ca}",
  })?

  test.error_kind(rejected, "net-tls")?
  test.eq(unverified.body.utf8()?, "secure")?
  test.eq(verified.body.utf8()?, "secure")?
}

proc test_net_transport_https_http1_contract() [net, env, error] {
  let url = env.get_or("XSH_NET_TEST_H1_URL", "")?
  let ca = env.get_or("XSH_NET_TEST_CA", "")?
  if url == "" or ca == "" {
    test.skip("requires HTTPS H1 fixture")
    return
  }

  let response = net.request({
    method: "GET",
    url: f"${url}/secure",
    ca_certificate: fp"${ca}",
    pool: "h1-alpn",
  })?

  test.eq(response.body.utf8()?, "secure")?
}

proc test_net_transport_request_many_https_h2_contract() [net, env, error] {
  let url = env.get_or("XSH_NET_TEST_H2_URL", "")?
  let ca = env.get_or("XSH_NET_TEST_CA", "")?
  if url == "" or ca == "" {
    test.skip("requires HTTPS H2 fixture")
    return
  }

  let request_items = [
    {
      method: "GET",
      url: f"${url}/h2",
    },
    {
      method: "GET",
      url: f"${url}/h2",
    },
  ]
  let batch = {
    requests: request_items,
    concurrency: 1,
    ca_certificate: fp"${ca}",
    pool: "h2",
  }
  let requests = net.request_many(batch)?

  test.eq(requests[0]?.body.utf8()?, "h2")?
  test.eq(requests[1]?.body.utf8()?, "h2")?
}

proc test_net_job_cancel_keeps_h2_siblings_and_pool_healthy() [net, env, error] {
  let url = env.get_or("XSH_NET_TEST_H2_CANCEL_URL", "")?
  let ca = env.get_or("XSH_NET_TEST_CA", "")?
  if url == "" or ca == "" {
    test.skip("requires HTTPS H2 cancellation fixture")
    return
  }

  let pool = "h2-cancel"
  let warmed = net.request_many({
    requests: [{method: "GET", url: f"${url}/warm"}],
    concurrency: 1,
    ca_certificate: fp"${ca}",
    pool: pool,
  })?
  test.eq(warmed[0]?.body.utf8()?, "warm")?

  let stalled = net.start({
    method: "GET",
    url: f"${url}/slow",
    ca_certificate: fp"${ca}",
    pool: pool,
  })?
  let sibling = net.start({
    method: "GET",
    url: f"${url}/fast",
    ca_certificate: fp"${ca}",
    pool: pool,
  })?
  let fast = sibling.wait()?
  test.eq(fast.body.utf8()?, "fast")?
  stalled.cancel()?

  let later = net.request_many({
    requests: [{method: "GET", url: f"${url}/later"}],
    concurrency: 1,
    ca_certificate: fp"${ca}",
    pool: pool,
  })?
  test.eq(later[0]?.body.utf8()?, "later")?
}

proc test_net_transport_download_many_https_h2_contract(ctx: TestContext) [fs, net, env, error] {
  let url = env.get_or("XSH_NET_TEST_H2_URL", "")?
  let ca = env.get_or("XSH_NET_TEST_CA", "")?
  if url == "" or ca == "" {
    test.skip("requires HTTPS H2 fixture")
    return
  }

  let root = test.temp_dir(ctx, name: "net-h2")?
  let dest = fp"${root}/h2.txt"
  let download_items = [{url: f"${url}/h2", dest: dest, overwrite: true}]
  let batch = {
    downloads: download_items,
    ca_certificate: fp"${ca}",
    pool: "h2-download",
  }
  let downloads = net.download_many(batch)?

  test.eq(downloads[0]?.bytes, 2)?
  test.eq(dest.read_text()?, "h2")?
}

proc test_net_transport_linux_system_ca_dir() [net, env, error] {
  let url = env.get_or("XSH_NET_TEST_TLS_URL", "")?
  if url == "" {
    test.skip("requires Linux TLS fixture")
    return
  }

  let response = net.request({method: "GET", url: f"${url}/secure"})?
  test.eq(response.status, 200)?
  test.eq(response.body.utf8()?, "secure")?
}
