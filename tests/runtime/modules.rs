#![allow(clippy::single_call_fn)]

use super::common::*;

#[test]
fn minimal_modules_execute_success_paths() {
    let output = xsh(["tests/fixtures/runtime/modules-basic.xsh"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true b true true true true true true 42 dcwrap:0:operand:-1\n"
    );
}

#[test]
fn loaded_modules_refine_to_typed_module_contracts() {
    let output = xsh(["tests/fixtures/runtime/module-contract.xsh"]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "demo true false 3\ndemo\n"
    );
}

#[test]
fn elf_module_inspects_dynamic_metadata_and_reports_malformed_files() {
    let elf = temp_path("elf-module-fixture.so");
    let plain = temp_path("elf-module-plain.txt");
    let bad = temp_path("elf-module-bad.bin");
    std::fs::write(&elf, elf64_fixture()).expect("write elf fixture");
    std::fs::write(&plain, "plain text").expect("write plain fixture");
    std::fs::write(&bad, b"\x7fELF\x02\x01").expect("write bad fixture");

    let source = format!(
        r#"
let info = elf.inspect(Path({elf}))?
print ${{info.type}} ${{info.class}} ${{info.endian}} ${{info.machine}} ${{info.interpreter}} ${{info.soname}} ${{info.needed[0]}} ${{info.needed[1]}} ${{info.rpath}} ${{info.runpath}}
let plain = elf.inspect(Path({plain}))?
print ${{plain.type}} ${{plain.needed.len()}}
match elf.inspect(Path({bad})) {{
  Err(error) => print ${{error.kind}}
}}
"#,
        elf = xsh_string_literal(elf.to_str().unwrap()),
        plain = xsh_string_literal(plain.to_str().unwrap()),
        bad = xsh_string_literal(bad.to_str().unwrap()),
    );

    let output = run_temp_script("elf-module", &source);
    let _ = std::fs::remove_file(elf);
    let _ = std::fs::remove_file(plain);
    let _ = std::fs::remove_file(bad);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "shared ELF64 little x86_64 /lib/ld-musl-x86_64.so.1 libdemo.so libc.musl-x86_64.so.1 libprivate.so $ORIGIN/lib $ORIGIN\nnot-elf 0\nelf-malformed\n"
    );
}

fn elf64_fixture() -> Vec<u8> {
    elf64_fixture_with_needed("libprivate.so")
}

fn elf64_fixture_with_needed(second_needed: &str) -> Vec<u8> {
    let mut data = vec![0; 0x600];
    data[..4].copy_from_slice(b"\x7fELF");
    data[4] = 2;
    data[5] = 1;
    data[6] = 1;
    data[7] = 3;
    write16(&mut data, 16, 3);
    write16(&mut data, 18, 62);
    write32(&mut data, 20, 1);
    write64(&mut data, 32, 0x40);
    write64(&mut data, 40, 0x300);
    write16(&mut data, 52, 64);
    write16(&mut data, 54, 56);
    write16(&mut data, 56, 3);
    write16(&mut data, 58, 64);
    write16(&mut data, 60, 3);

    ph64(&mut data, 0x40, 1, 0x100, 0x400000, 0x200);
    ph64(&mut data, 0x78, 2, 0x200, 0x400100, 0x80);
    ph64(&mut data, 0xb0, 3, 0x280, 0x400180, 0x18);
    sh64(&mut data, 0x340, 6, 0x200, 0x80, 2, 16);
    sh64(&mut data, 0x380, 3, 0x500, 0x80, 0, 0);

    let mut strings = Vec::new();
    strings.push(0);
    let libc_offset = strings.len() as u64;
    strings.extend_from_slice(b"libc.musl-x86_64.so.1\0");
    let second_offset = strings.len() as u64;
    strings.extend_from_slice(second_needed.as_bytes());
    strings.push(0);
    let soname_offset = strings.len() as u64;
    strings.extend_from_slice(b"libdemo.so\0");
    let rpath_offset = strings.len() as u64;
    strings.extend_from_slice(b"$ORIGIN/lib\0");
    let runpath_offset = strings.len() as u64;
    strings.extend_from_slice(b"$ORIGIN\0");
    data[0x500..0x500 + strings.len()].copy_from_slice(&strings);
    dyn64(&mut data, 0x200, 0, 1, libc_offset);
    dyn64(&mut data, 0x200, 1, 1, second_offset);
    dyn64(&mut data, 0x200, 2, 14, soname_offset);
    dyn64(&mut data, 0x200, 3, 15, rpath_offset);
    dyn64(&mut data, 0x200, 4, 29, runpath_offset);
    dyn64(&mut data, 0x200, 5, 5, 0x400400);
    dyn64(&mut data, 0x200, 6, 10, strings.len() as u64);
    dyn64(&mut data, 0x200, 7, 0, 0);
    data[0x280..0x299].copy_from_slice(b"/lib/ld-musl-x86_64.so.1\0");
    data
}

fn ph64(data: &mut [u8], base: usize, p_type: u32, offset: u64, vaddr: u64, filesz: u64) {
    write32(data, base, p_type);
    write64(data, base + 8, offset);
    write64(data, base + 16, vaddr);
    write64(data, base + 32, filesz);
}

fn sh64(
    data: &mut [u8],
    base: usize,
    sh_type: u32,
    offset: u64,
    size: u64,
    link: u32,
    entsize: u64,
) {
    write32(data, base + 4, sh_type);
    write64(data, base + 24, offset);
    write64(data, base + 32, size);
    write32(data, base + 40, link);
    write64(data, base + 56, entsize);
}

fn dyn64(data: &mut [u8], table: usize, index: usize, tag: i64, value: u64) {
    let base = table + index * 16;
    write64(data, base, tag as u64);
    write64(data, base + 8, value);
}

fn write16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn io_module_reads_stdin_and_writes_stdout() {
    let path = write_temp_script(
        "io-module",
        r#"
let data = io.stdin_bytes()?
io.write_stdout_bytes(data)?
"#,
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn xsh");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(b"hello stdin\n")
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait xsh");
    let _ = std::fs::remove_file(path);

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello stdin\n");
}

#[test]
fn io_module_reads_one_stdin_line() {
    let path = write_temp_script(
        "io-line",
        r#"
let line = io.stdin_line()?
print ${line}
"#,
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn xsh");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(b"first\r\nsecond\n")
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait xsh");
    let _ = std::fs::remove_file(path);

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "first\n");
}

#[test]
fn collection_modules_execute_success_paths() {
    let output = xsh(["tests/fixtures/runtime/collections.xsh"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "3 beta 3 delta true false alpha omega sigma\n2 true false 99 100 one two 1 2\n2 next 5 two alpha 2\nmap has no key `one`\ntrue name pkg\nmissing field `missing`\n"
    );
}

#[cfg(feature = "net")]
#[test]
fn dns_module_resolves_localhost_and_reports_unsupported_records() {
    let output = run_temp_script(
        "dns-module",
        r#"
let hosts = dns.resolve_host("localhost")?
let bad = dns.lookup("localhost", "TXT")
match bad {
  Err(e) => print ${hosts.len() > 0} ${e.kind}
}
"#,
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true dns-record\n"
    );
}

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
    assert_eq!(summary.handled, 2);
}

#[cfg(feature = "net")]
#[test]
#[ignore = "flaky on macOS: local net transfer can fail with SendRequest"]
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
    Err(ca) => print ${{pool.max_idle_per_host}} ${{pool.idle_timeout_ms}} ${{first.body.utf8()?}} ${{second.body.utf8()?}} ${{headed.status}} ${{headed.bytes}} ${{redirected.body.utf8()?}} ${{posted.body.utf8()?}} ${{posted_file.body.utf8()?}} ${{status.status}} ${{downloaded.bytes}} ${{uploaded.status}} ${{e.kind}} ${{ca.kind}}
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
#[ignore = "documents that immediate same-pool calls must not open an extra TCP connection"]
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
fn net_module_verifies_local_https_with_custom_ca() {
    let server = LocalHttpsServer::spawn(2);
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
fn dynamic_module_load_returns_exports_and_proc_call_invokes_proc_values() {
    let root = temp_path("dynamic-module-load");
    std::fs::create_dir_all(&root).expect("create dynamic module root");
    let package = root.join("package.xsh");
    let helper = root.join("helper.xsh");
    let main = root.join("main.xsh");
    std::fs::write(
        &helper,
        r#"
export let helper_name = "demo"

export pure helper_label(value: Str) -> Str {
  return f"helper:${value}"
}
"#,
    )
    .expect("write dynamic helper module");
    std::fs::write(
        &package,
        r#"
use helper

let prefix = helper_name

pure label_private(value: Str) -> Str {
  return f"${prefix}-${value}"
}

proc emit_private(value: Str) -> Result[Unit] {
  print ${label_private(value)}
}

export let name = prefix

export pure label(value: Str) -> Str {
  return label_private(value)
}

export proc build(value: Str) -> Result[Unit] {
  emit_private(value)?
}
"#,
    )
    .expect("write dynamic package module");
    std::fs::write(
        &main,
        format!(
            r#"
type DynamicPackage = module {{
  export let name: Str
  export pure label(value: Str) -> Str
  export proc build(value: Str) -> Result[Unit]
}}

let loaded = module.load(Path({}))?
let checked = loaded.require(DynamicPackage)?
let name: Str = checked.name
let rendered: Str = checked.label(name)
print ${{rendered}}
checked.build("built")?
"#,
            xsh_string_literal(package.to_str().unwrap())
        ),
    )
    .expect("write dynamic module main");

    let output = xsh([main.to_str().unwrap()]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "demo-demo\ndemo-built\n"
    );

    let private_main = root.join("private-main.xsh");
    std::fs::write(
        &private_main,
        format!(
            r#"
let loaded = module.load(Path({}))?
let value = loaded.prefix
"#,
            xsh_string_literal(package.to_str().unwrap())
        ),
    )
    .expect("write dynamic module private main");

    let private = xsh([private_main.to_str().unwrap()]);

    assert_eq!(private.status.code(), Some(3));
    let private_stderr = String::from_utf8(private.stderr).unwrap();
    assert!(private_stderr.contains("missing-field"), "{private_stderr}");
    assert!(private_stderr.contains("prefix"), "{private_stderr}");

    let contract_main = root.join("contract-main.xsh");
    std::fs::write(
        &contract_main,
        format!(
            r#"
type BadPackage = module {{
  export proc build(path: Path) -> Result[Unit]
}}

let loaded = module.load(Path({}))?
let checked = loaded.require(BadPackage)
match checked {{
  Err(e) => print ${{e.kind}}
}}
"#,
            xsh_string_literal(package.to_str().unwrap())
        ),
    )
    .expect("write dynamic module contract main");

    let contract = xsh([contract_main.to_str().unwrap()]);
    assert!(
        contract.status.success(),
        "{}",
        String::from_utf8_lossy(&contract.stderr)
    );
    assert_eq!(String::from_utf8(contract.stdout).unwrap(), "schema\n");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn package_style_module_hook_calls_run_on_compact_runtime() {
    let cwd_name = std::env::current_dir()
        .expect("read current directory")
        .file_name()
        .expect("current directory has a name")
        .to_string_lossy()
        .into_owned();
    let root = temp_path("package-style-module-hooks");
    let dynamic_src = root.join("dynamic-src");
    let dynamic_out = root.join("dynamic-out");
    let static_src = root.join("static-src");
    let static_out = root.join("static-out");
    std::fs::create_dir_all(&root).expect("create package hook root");
    std::fs::create_dir_all(&dynamic_src).expect("create dynamic source dir");
    std::fs::create_dir_all(&static_src).expect("create static source dir");
    let package = root.join("PKGBUILD.xsh");
    let dynamic_main = root.join("dynamic-main.xsh");
    let static_main = root.join("static-main.xsh");

    std::fs::write(
        &package,
        r#"
export let name = "demo"

export proc build(dest: Path) [fs, error] -> Result[Unit] {
  fs.mkdir(dest)?
  fs.write(fp"${dest}/ok", f"${name}:${fs.cwd()?.name()}\n")?
}
"#,
    )
    .expect("write package hook module");
    std::fs::write(
        &dynamic_main,
        format!(
            r#"
type Pkg = module {{
  export let name: Str
  export proc build(dest: Path) [fs, error] -> Result[Unit]
}}

let pkg = module.load(Path({package}))?.require(Pkg)?
pkg.build(Path({dest}))?
"#,
            package = xsh_string_literal(package.to_str().unwrap()),
            dest = xsh_string_literal(dynamic_out.to_str().unwrap()),
        ),
    )
    .expect("write dynamic package hook main");
    std::fs::write(
        &static_main,
        r#"
use PKGBUILD

proc main(src: Path, dest: Path) [fs, process, env, error] -> Result[Unit] {
  fs.remove(dest, missing_ok: true)?
  fs.mkdir(dest)?
  cd src {
    build(dest)?
  } ?
}

main(@args)?
"#,
    )
    .expect("write static package hook main");

    let dynamic = xsh([dynamic_main.to_str().unwrap()]);
    let static_output = xsh([
        static_main.to_str().unwrap(),
        static_src.to_str().unwrap(),
        static_out.to_str().unwrap(),
    ]);

    assert!(
        dynamic.status.success(),
        "{}",
        String::from_utf8_lossy(&dynamic.stderr)
    );
    assert!(
        static_output.status.success(),
        "{}",
        String::from_utf8_lossy(&static_output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dynamic_out.join("ok")).expect("read dynamic marker"),
        format!("demo:{cwd_name}\n")
    );
    assert_eq!(
        std::fs::read_to_string(static_out.join("ok")).expect("read static marker"),
        "demo:static-src\n"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn dynamic_module_proc_bareword_run_args_resolve_correctly() {
    let root = temp_path("bareword-run-args-root");
    std::fs::create_dir_all(&root).expect("create bareword run args root");
    let package = root.join("package.xsh");
    let main = root.join("main.xsh");

    std::fs::write(
        &package,
        r#"
export let name = "bareword-test"
export let ver = "1.0"
export let rel = "1"
export let deps: List[Str] = []
export let mkdeps: List[Str] = []
export let sources: List[Path] = []
export let checksums: List[Str] = []

export proc build(dest: Path) [fs, process, error] {
  var patched_ninja = "rule link\n command = cc\nrule compile\n command = cc\n"

  patched_ninja = patched_ninja.replace(
    """rule link
 command = cc""",
    f"""rule link
 command = /usr/bin/cc""",
  )

  patched_ninja = patched_ninja.replace(
    """rule compile
 command = cc""",
    f"""rule compile
 command = /usr/bin/cc""",
  )

  fs.write(fp"/tmp/bareword-run-test", patched_ninja)?
  run echo "-C" "build" samu ?
  run echo apples ?
}
"#,
    )
    .expect("write package module");
    std::fs::write(
        &main,
        format!(
            r#"
let loaded = module.load(Path({}))?
let build_fn: Proc = loaded.get("build")?
build_fn.call(p"/tmp/bareword-dest")?
"#,
            xsh_string_literal(package.to_str().unwrap()),
        ),
    )
    .expect("write main script");

    let output = xsh([main.to_str().unwrap()]);

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_file("/tmp/bareword-run-test");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("-C build samu"),
        "missing '-C build samu' in: {stdout}"
    );
    assert!(stdout.contains("apples"), "missing 'apples' in: {stdout}");
}

#[test]
fn dynamic_module_load_reports_module_restriction_errors() {
    let root = temp_path("dynamic-module-restriction");
    std::fs::create_dir_all(&root).expect("create dynamic module root");
    for (name, source) in [
        (
            "bad-var",
            r#"
var count = 1
export let name = "bad"
"#,
        ),
        (
            "bad-command",
            r#"
print bad
export let name = "bad"
"#,
        ),
    ] {
        let package = root.join(format!("{name}.xsh"));
        let main = root.join(format!("{name}-main.xsh"));
        std::fs::write(&package, source).expect("write bad dynamic module");
        std::fs::write(
            &main,
            format!(
                "let loaded = module.load(Path({}))?\n",
                xsh_string_literal(package.to_str().unwrap())
            ),
        )
        .expect("write dynamic module main");

        let output = xsh([main.to_str().unwrap()]);

        assert_eq!(output.status.code(), Some(3));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("module-check"), "{stderr}");
        assert!(stderr.contains("check.module-top-level"), "{stderr}");
        assert!(stderr.contains(&format!("{name}.xsh")), "{stderr}");
    }

    let hook_package = root.join("signal-hook.xsh");
    let hook_main = root.join("signal-hook-main.xsh");
    std::fs::write(&hook_package, "on SIGINT [] {\n}\n").expect("write hook dynamic module");
    std::fs::write(
        &hook_main,
        format!(
            "let loaded = module.load(Path({}))?\n",
            xsh_string_literal(hook_package.to_str().unwrap())
        ),
    )
    .expect("write dynamic module main");

    let hook_output = xsh([hook_main.to_str().unwrap()]);

    assert_eq!(hook_output.status.code(), Some(3));
    let hook_stderr = String::from_utf8(hook_output.stderr).unwrap();
    assert!(hook_stderr.contains("module-check"), "{hook_stderr}");
    assert!(
        hook_stderr.contains("check.signal-hook-module"),
        "{hook_stderr}"
    );
    assert!(hook_stderr.contains("signal-hook.xsh"), "{hook_stderr}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn module_errors_are_structured_results() {
    let output = xsh(["tests/fixtures/runtime/module-error.xsh"]);

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("fs-read"));
}

#[test]
fn env_get_rejects_invalid_utf8_values() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg("tests/fixtures/runtime/env-invalid-utf8.xsh")
        .env("XSH_BAD_UTF8", std::ffi::OsString::from_vec(vec![0xff]))
        .output()
        .expect("run xsh");

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("invalid-utf8"));
}

#[test]
fn env_function_rejects_invalid_utf8_values() {
    let path = write_temp_script(
        "env-function-invalid-utf8",
        "let _ = env(\"XSH_BAD_UTF8\") ?\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(&path)
        .env("XSH_BAD_UTF8", std::ffi::OsString::from_vec(vec![0xff]))
        .output()
        .expect("run xsh");
    let _ = std::fs::remove_file(path);

    assert_eq!(output.status.code(), Some(3));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("invalid-utf8")
    );
}

#[test]
fn source_loading_reports_invalid_utf8_as_diagnostic() {
    let path = temp_xsh_path("invalid-utf8");
    std::fs::write(&path, vec![b'l', b'e', b't', b' ', 0xff]).expect("write temp script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("run xsht");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("source.invalid-utf8"));

    std::fs::remove_file(path).expect("remove temp script");
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
        Err(_) => {
            let fallback = root.join("raw-file");
            std::fs::write(&fallback, b"ok").unwrap();
            let fallback_expr = xsh_bytes_literal(fallback.as_os_str().as_bytes());
            (fallback, fallback_expr)
        }
    };
    let raw_arg_hex = "726177ff617267";
    let raw_path_hex = hex(raw_path.as_os_str().as_bytes());
    let glob_pattern = root.join("*");
    let source = format!(
        "\
let show_argv = Path({})
let show_env = Path({})
let stat_path = Path({})
let emit_hex = Path({})
let root = Path({})
let raw_arg = Path.parse_bytes(b\"raw\\xffarg\")?
let argv = run.text (show_argv) (raw_arg) \"two words\" ?
let spliced = run.text (show_argv) ${{[\"one\", \"two words\"]}} ?
let env_output = run.text XSH_RAW=(raw_arg) (show_env) XSH_RAW ?
let raw_path = Path.parse_bytes({})?
let stat = run.text (stat_path) (raw_path) ?
let globbed = run.text (show_argv) @(g{}) ?
let emitted = run.bytes (emit_hex) 00ff41 ?
print ${{{} in argv}} ${{\"74776f20776f726473\" in argv}}
print ${{\"6f6e65\" in spliced}} ${{\"74776f20776f726473\" in spliced}}
print ${{\"XSH_RAW={}\" in env_output}} ${{{} in stat}} ${{{} in globbed}}
print ${{emitted == b\"\\0\\xffA\"}}
",
        xsh_string_literal(env!("CARGO_BIN_EXE_xsh-test-show-argv")),
        xsh_string_literal(env!("CARGO_BIN_EXE_xsh-test-show-env")),
        xsh_string_literal(env!("CARGO_BIN_EXE_xsh-test-stat-path")),
        xsh_string_literal(env!("CARGO_BIN_EXE_xsh-test-emit-hex")),
        xsh_string_literal(root.to_str().unwrap()),
        raw_path_expr,
        xsh_string_literal(glob_pattern.to_str().unwrap()),
        xsh_string_literal(raw_arg_hex),
        raw_arg_hex,
        xsh_string_literal(&raw_path_hex),
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
let output = run.text $target \"${{label}} suffix\" ?
print ${{output.trim()}}
",
        xsh_string_literal(env!("CARGO_BIN_EXE_xsh-test-show-argv")),
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

#[test]
fn user_modules_import_exports_aliases_and_cycles() {
    let root = temp_path("module-imports-root");
    std::fs::create_dir_all(&root).expect("create module root");
    let helper = root.join("helper.xsh");
    let package = root.join("package.xsh");
    let main = root.join("main.xsh");
    let cycle_main = root.join("cycle-main.xsh");
    let a = root.join("a.xsh");
    let b = root.join("b.xsh");
    std::fs::write(
        &helper,
        "\
use package as p

let greeting = \"hi\"

pure line(name: Str) -> Str {
  return f\"${greeting} ${name}\"
}

export proc greet(name: Str) -> Result[Unit] {
  print ${line(name)}
  return Ok()
}

export proc show(pkg: p.Package) -> Result[Unit] {
  print ${line(pkg.name)}
  return Ok()
}
",
    )
    .expect("write helper");
    std::fs::write(
        &package,
        "\
let secret = \"hidden\"
export type Package = {name: Str, root: Path}
export let pkg: Package = {name: \"demo\", root: Path(\"src\")}
",
    )
    .expect("write package");
    std::fs::write(
        &main,
        "\
use helper
use package as p
greet(\"world\")?
helper.greet(\"namespace\")?
show(p.pkg)?
print ${p.pkg.name}
match p.get(\"Package\") {
  Err(e) => print ${e.kind}
}
",
    )
    .expect("write main");
    std::fs::write(&cycle_main, "use a\n").expect("write cycle main");
    std::fs::write(&a, "use b\nexport let value = 1\n").expect("write a");
    std::fs::write(&b, "use a\nexport let value = 2\n").expect("write b");

    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(&main)
        .output()
        .expect("run xsh");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "hi world\nhi namespace\nhi demo\ndemo\nmissing-field\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let traced = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["trace", "--raw"])
        .arg(&main)
        .output()
        .expect("run traced xsht");
    assert!(traced.status.success());
    let trace = String::from_utf8(traced.stderr).unwrap();
    assert!(
        trace.contains("kind=pure.enter"),
        "expected pure.enter in trace: {trace}"
    );
    assert!(trace.contains("greet"));

    let cycle = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(&cycle_main)
        .output()
        .expect("run cycle xsh");
    assert_eq!(cycle.status.code(), Some(2));
    let stderr = String::from_utf8(cycle.stderr).unwrap();
    assert!(stderr.contains("parse.module-cycle"), "{stderr}");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn user_module_qualified_types_and_first_class_functions_stay_callable() {
    let root = temp_path("module-qualified-values-root");
    std::fs::create_dir_all(&root).expect("create module root");
    let package = root.join("package.xsh");
    let main = root.join("main.xsh");

    std::fs::write(
        &package,
        "\
export type Package = {name: Str}

export pure label(pkg: Package) -> Str {
  return f\"pkg:${pkg.name}\"
}

export proc show(pkg: Package) -> Result[Unit] {
  print ${label(pkg)}
}

export let pkg: Package = {name: \"demo\"}
",
    )
    .expect("write package");
    std::fs::write(
        &main,
        "\
use package as p

let labeler = p.label
let shower = p.show
let pkg: p.Package = p.pkg

print ${labeler.call(pkg)}
shower.call(pkg)?
",
    )
    .expect("write main");

    let output = xsh([main.to_str().unwrap()]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "pkg:demo\npkg:demo\n"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn proc_call_from_module_preserves_runtime_cwd() {
    let root = temp_path("module-proc-call-cwd-root");
    let src = root.join("src");
    let caller = root.join("caller.xsh");
    let callee = root.join("callee.xsh");
    let main = root.join("main.xsh");
    let out = root.join("cwd.txt");
    std::fs::create_dir_all(&src).expect("create source dir");
    std::fs::write(
        &callee,
        "\
export proc write_cwd(out: Path) [fs, error] {
  fs.write(out, fs.cwd()?.display())?
}
",
    )
    .expect("write callee");
    std::fs::write(
        &caller,
        format!(
            "\
export proc invoke(src: Path, out: Path) [fs, error] {{
  let module_exports = module.load(fp\"{}\")?
  cd src {{
    let write_cwd: Proc = module_exports.get(\"write_cwd\")?
    write_cwd.call(out)?
  }} ?
}}
",
            callee.display()
        ),
    )
    .expect("write caller");
    std::fs::write(
        &main,
        format!(
            "\
use caller as c

let src = fp\"{}\"
let out = fp\"{}\"
c.invoke(src, out)?
print ${{out.read_text()?}}
",
            src.display(),
            out.display()
        ),
    )
    .expect("write main");

    let output = xsh([main.to_str().unwrap()]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{}\n", src.display())
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn user_modules_can_resolve_from_module_path_and_default_alias() {
    let root = temp_path("module-path-root");
    let lib = root.join("lib");
    let pkg = root.join("repo/pkg");
    let module_dir = lib.join("pm");
    std::fs::create_dir_all(&module_dir).expect("create module dir");
    std::fs::create_dir_all(&pkg).expect("create pkg dir");
    std::fs::write(
        module_dir.join("configure.xsh"),
        "\
export pure label(name: Str) -> Str {
  return f\"configured ${name}\"
}
",
    )
    .expect("write configure module");
    let main = pkg.join("PKGBUILD.xsh");
    std::fs::write(
        &main,
        "\
use pm.configure
print ${configure.label(\"pkgconf\")}
",
    )
    .expect("write package script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .env("XSH_MODULE_PATH", &lib)
        .arg(&main)
        .output()
        .expect("run xsh");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "configured pkgconf\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn archive_module_roundtrips_compression_and_rejects_escape_paths() {
    let root = temp_path("archive-module");
    let _ = std::fs::remove_dir_all(&root);
    let src = root.join("src");
    let out = root.join("out");
    std::fs::create_dir_all(src.join("dir")).expect("create source");
    std::fs::create_dir_all(&out).expect("create output root");
    std::fs::write(src.join("dir").join("a.txt"), " alpha\n").expect("write source file");
    std::os::unix::fs::symlink("dir/a.txt", src.join("link")).expect("create symlink");

    let parent_escape = root.join("parent.tar");
    let absolute_escape = root.join("absolute.tar");
    let symlink_escape = root.join("symlink.tar");
    let good_zip = root.join("good.zip");
    let bad_zip = root.join("bad.zip");
    write_test_tar_file(&parent_escape, "../evil", b"bad");
    write_test_tar_file(&absolute_escape, "/evil", b"bad");
    write_test_tar_symlink(&symlink_escape, "link", "../evil");
    write_test_zip(&good_zip, &[("zip/note.txt", b"zip\n")]);
    write_test_zip(&bad_zip, &[("../escape.txt", b"bad\n")]);

    let script = format!(
        "\
let src = Path({})
let out = Path({})
let tgz = fp\"${{out}}/pkg.tar.gz\"
let tbz = fp\"${{out}}/pkg.tar.bz2\"
let txz = fp\"${{out}}/pkg.tar.xz\"
let plain = fp\"${{out}}/pkg.tar\"
let inferred = fp\"${{out}}/inferred.tgz\"
archive.tar_create(tgz, src, [Path(\".\")], compression: \"gz\")?
archive.tar_create(tbz, src, [Path(\".\")], compression: \"bz2\")?
archive.tar_create(txz, src, [Path(\".\")], compression: \"xz\")?
archive.tar_create(plain, src, [Path(\".\")])?
archive.tar_create(inferred, src, [Path(\".\")])?
let entries = archive.tar_list(tgz)?
let dest = fp\"${{out}}/dest\"
archive.tar_extract(tgz, dest)?
let data = fp\"${{dest}}/dir/a.txt\".read_text()?.trim()
let stripped = fp\"${{out}}/stripped\"
archive.tar_extract(tgz, stripped, strip_components: 1)?
let stripped_data = fp\"${{stripped}}/a.txt\".read_text()?.trim()
match archive.tar_extract(tgz, dest) {{
  Err(e) => print ${{entries.len()}} ${{data}} ${{e.kind}}
}}
print ${{stripped_data}}
print ${{archive.tar_list(tbz)?.len()}} ${{archive.tar_list(txz)?.len()}} ${{archive.tar_list(plain)?.len()}}
print ${{archive.tar_list(inferred)?.len()}}
let cpio = fp\"${{out}}/pkg.cpio\"
archive.cpio_create(cpio, src, [Path(\".\")])?
let cpio_entries = archive.cpio_list(cpio)?
let cpio_dest = fp\"${{out}}/cpio\"
archive.cpio_extract(cpio, cpio_dest)?
print ${{cpio_entries.len()}} ${{fp\"${{cpio_dest}}/dir/a.txt\".read_text()?.trim()}}
let payload = fp\"${{src}}/dir/a.txt\"
let gz = fp\"${{out}}/a.txt.gz\"
let bz2 = fp\"${{out}}/a.txt.bz2\"
let xz = fp\"${{out}}/a.txt.xz\"
let lzma = fp\"${{out}}/a.txt.lzma\"
archive.compress(payload, gz, format: \"gzip\")?
archive.compress(payload, bz2, format: \"bzip2\")?
archive.compress(payload, xz, format: \"xz\")?
archive.compress(payload, lzma, format: \"lzma\")?
let auto_gz = fp\"${{out}}/auto.gz\"
let auto_bz2 = fp\"${{out}}/auto.bz2\"
let auto_xz = fp\"${{out}}/auto.xz\"
let auto_lzma = fp\"${{out}}/auto.lzma\"
archive.compress(payload, auto_gz)?
archive.compress(payload, auto_bz2)?
archive.compress(payload, auto_xz)?
archive.compress(payload, auto_lzma)?
let gz_probe = fp\"${{out}}/gzip.probe\"
let bz2_probe = fp\"${{out}}/bzip2.probe\"
let xz_probe = fp\"${{out}}/xz.probe\"
auto_gz.copy(gz_probe)?
auto_bz2.copy(bz2_probe)?
auto_xz.copy(xz_probe)?
let gz_data = archive.decompress_bytes(gz)?.utf8()?.trim()
archive.decompress(bz2, fp\"${{out}}/a.bz2.out\")?
archive.decompress(xz, fp\"${{out}}/a.xz.out\")?
archive.decompress(lzma, fp\"${{out}}/a.lzma.out\")?
print ${{gz_data}} ${{fp\"${{out}}/a.bz2.out\".read_text()?.trim()}} ${{fp\"${{out}}/a.xz.out\".read_text()?.trim()}} ${{fp\"${{out}}/a.lzma.out\".read_text()?.trim()}}
print ${{archive.decompress_bytes(gz_probe)?.utf8()?.trim()}} ${{archive.decompress_bytes(bz2_probe)?.utf8()?.trim()}} ${{archive.decompress_bytes(xz_probe)?.utf8()?.trim()}} ${{archive.decompress_bytes(auto_lzma)?.utf8()?.trim()}}
match archive.compress(payload, fp\"${{out}}/bad.zz\", format: \"zip\") {{
  Err(e) => print ${{e.kind}}
}}
match archive.compress(payload, fp\"${{out}}/bad.gz\", format: \"auto\", level: 10) {{
  Err(e) => print ${{e.kind}}
}}
let unknown = fp\"${{out}}/unknown.bin\"
payload.copy(unknown)?
match archive.decompress(unknown, fp\"${{out}}/unknown.out\") {{
  Err(e) => print ${{e.kind}}
}}
let zip_entries = archive.zip_list(Path({}))?
archive.zip_extract(Path({}), fp\"${{out}}/zip\")?
print ${{zip_entries.len()}} ${{fp\"${{out}}/zip/zip/note.txt\".read_text()?.trim()}}
match archive.tar_extract(Path({}), fp\"${{out}}/bad-parent\") {{
  Err(e) => print ${{e.kind}}
}}
match archive.tar_extract(Path({}), fp\"${{out}}/bad-absolute\") {{
  Err(e) => print ${{e.kind}}
}}
match archive.tar_extract(Path({}), fp\"${{out}}/bad-symlink\") {{
  Err(e) => print ${{e.kind}}
}}
match archive.zip_extract(Path({}), fp\"${{out}}/bad-zip\") {{
  Err(e) => print ${{e.kind}}
}}
",
        xsh_string_literal(src.to_str().unwrap()),
        xsh_string_literal(out.to_str().unwrap()),
        xsh_string_literal(good_zip.to_str().unwrap()),
        xsh_string_literal(good_zip.to_str().unwrap()),
        xsh_string_literal(parent_escape.to_str().unwrap()),
        xsh_string_literal(absolute_escape.to_str().unwrap()),
        xsh_string_literal(symlink_escape.to_str().unwrap()),
        xsh_string_literal(bad_zip.to_str().unwrap()),
    );

    let output = run_temp_script("archive-module", &script);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "3 alpha archive-extract\nalpha\n3 3 3\n3\n3 alpha\nalpha alpha alpha alpha\nalpha alpha alpha alpha\narchive-compression\narchive-compression\narchive-compression\n1 zip\narchive-path\narchive-path\narchive-escape\narchive-path\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        std::fs::read_to_string(out.join("dest").join("dir").join("a.txt")).unwrap(),
        " alpha\n"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn archive_module_zip_extracts_many_files_and_overwrites() {
    let root = temp_path("archive-zip-many");
    let _ = std::fs::remove_dir_all(&root);
    let out = root.join("out");
    std::fs::create_dir_all(out.join("extract").join("many")).expect("create output root");
    std::fs::write(
        out.join("extract").join("many").join("file-00.txt"),
        "old\n",
    )
    .expect("write existing zip output");
    let zip = root.join("many.zip");
    let not_zip = root.join("not.zip");
    std::fs::write(&not_zip, "not a zip").expect("write not zip");

    let fixture_entries = (0..12)
        .map(|index| {
            (
                format!("many/file-{index:02}.txt"),
                format!("payload-{index:02}\n").into_bytes(),
            )
        })
        .collect::<Vec<_>>();
    let fixture_refs = fixture_entries
        .iter()
        .map(|(name, data)| (name.as_str(), data.as_slice()))
        .collect::<Vec<_>>();
    write_test_zip(&zip, &fixture_refs);

    let script = format!(
        "\
let zip = Path({})
let out = Path({})
let entries = archive.zip_list(zip)?
print ${{entries.len()}}
match archive.zip_extract(zip, fp\"${{out}}/extract\") {{
  Err(e) => print ${{e.kind}}
}}
archive.zip_extract(zip, fp\"${{out}}/extract\", overwrite: true)?
print ${{fp\"${{out}}/extract/many/file-00.txt\".read_text()?.trim()}}
let not_zip = Path({})
match archive.zip_list(not_zip) {{
  Err(e) => print ${{e.kind}}
}}
match archive.zip_extract(not_zip, fp\"${{out}}/bad-open\") {{
  Err(e) => print ${{e.kind}}
}}
",
        xsh_string_literal(zip.to_str().unwrap()),
        xsh_string_literal(out.to_str().unwrap()),
        xsh_string_literal(not_zip.to_str().unwrap()),
    );

    let output = run_temp_script("archive-zip-many", &script);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "12\narchive-zip-extract\npayload-00\narchive-zip-open\narchive-zip-open\n"
    );
    for index in 0..12 {
        assert_eq!(
            std::fs::read_to_string(
                out.join("extract")
                    .join("many")
                    .join(format!("file-{index:02}.txt"))
            )
            .unwrap(),
            format!("payload-{index:02}\n")
        );
    }

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn archive_module_preserves_tar_metadata_filters_and_overwrites() {
    let root = temp_path("archive-metadata");
    let _ = std::fs::remove_dir_all(&root);
    let src = root.join("src");
    let out = root.join("out");
    std::fs::create_dir_all(src.join("dir")).expect("create source");
    std::fs::create_dir_all(&out).expect("create output root");
    std::fs::write(src.join("dir").join("a.txt"), " alpha\n").expect("write source file");
    std::fs::write(src.join("dir").join("other.txt"), " other\n").expect("write other file");
    std::fs::set_permissions(
        src.join("dir").join("a.txt"),
        std::fs::Permissions::from_mode(0o640),
    )
    .expect("chmod source file");
    std::os::unix::fs::symlink("dir/a.txt", src.join("link")).expect("create symlink");

    let script = format!(
        "\
let src = Path({})
let out = Path({})
let tarball = fp\"${{out}}/pkg.tar\"
archive.tar_create(tarball, src, [p\".\"])?
let entries = archive.tar_list(tarball)?
var file_kind = \"missing\"
var file_mode = 0
var file_size = 0
var link_kind = \"missing\"
var link_name = \"missing\"
for entry in entries {{
  if entry.path == p\"dir/a.txt\" {{
    file_kind = entry.kind
    file_mode = entry.mode % 512
    file_size = entry.size
  }}
  if entry.path == p\"link\" {{
    link_kind = entry.kind
    link_name = entry.link_name
  }}
}}
print ${{entries.len()}} ${{file_kind}} ${{file_mode}} ${{file_size}} ${{link_kind}} ${{link_name}}
print ${{archive.tar_list(tarball, \"\", [p\"dir\"])?.len()}}
let dest = fp\"${{out}}/dest\"
archive.tar_extract(tarball, dest)?
print ${{fp\"${{dest}}/dir/a.txt\".read_text()?.trim()}} ${{fp\"${{dest}}/dir/a.txt\".metadata()?.mode % 512}} ${{fp\"${{dest}}/link\".metadata()?.kind}} ${{fp\"${{dest}}/link\".readlink()?.display()}}
fp\"${{dest}}/dir/a.txt\".write(\"stale\")?
archive.tar_extract(tarball, dest, 0, \"\", true, [p\"dir/a.txt\"])?
print ${{fp\"${{dest}}/dir/a.txt\".read_text()?.trim()}}
let selected = fp\"${{out}}/selected\"
archive.tar_extract(tarball, selected, 0, \"\", false, [p\"dir\"])?
print ${{fp\"${{selected}}/dir/a.txt\".exists()?}} ${{fp\"${{selected}}/dir/other.txt\".exists()?}} ${{fp\"${{selected}}/link\".exists()?}}
let stripped = fp\"${{out}}/stripped\"
archive.tar_extract(tarball, stripped, 2)?
print ${{fp\"${{stripped}}/a.txt\".exists()?}}
match archive.tar_extract(tarball, fp\"${{out}}/negative\", -1) {{
  Err(e) => print ${{e.kind}}
}}
",
        xsh_string_literal(src.to_str().unwrap()),
        xsh_string_literal(out.to_str().unwrap()),
    );

    let output = run_temp_script("archive-metadata", &script);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "4 file 416 7 symlink dir/a.txt\n3\nalpha 416 symlink dir/a.txt\nalpha\ntrue true false\nfalse\narchive-extract\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn archive_module_omits_pax_global_headers_from_tar_listing() {
    let root = temp_path("archive-pax-global");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create archive root");
    let tarball = root.join("global.tar");
    write_test_tar_global_header(&tarball);
    let script = format!(
        "let entries = archive.tar_list(Path({}))?\nprint entries.len() entries[0].path\n",
        xsh_string_literal(tarball.to_str().unwrap())
    );
    let output = run_temp_script("archive-pax-global", &script);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1 payload.txt\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn archive_module_streams_tar_listing_in_archive_order() {
    let root = temp_path("archive-list-stream");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create archive root");
    let tarball = root.join("stream.tar");
    write_test_tar_global_header(&tarball);
    let script = format!(
        "let entries = archive.tar_list_stream(Path({}))?\nvar paths: List[Str] = []\nfor entry in entries {{\n  paths = paths.push(entry.path.display())\n}}\nprint paths.join(\",\")\n",
        xsh_string_literal(tarball.to_str().unwrap())
    );
    let output = run_temp_script("archive-list-stream", &script);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "payload.txt\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn archive_module_extracts_tar_hardlinks_as_hardlinks() {
    let root = temp_path("archive-hardlink");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    let archive = root.join("hardlink.tar");
    let out = root.join("out");
    write_test_tar_hardlink(&archive, "dir/source.txt", "dir/copy.txt", b"shared\n");

    let script = format!(
        "\
let tar_path = Path({})
let out = Path({})
archive.tar_extract(tar_path, out)?
print ${{fp\"${{out}}/dir/source.txt\".read_text()?.trim()}} ${{fp\"${{out}}/dir/copy.txt\".read_text()?.trim()}}
",
        xsh_string_literal(archive.to_str().unwrap()),
        xsh_string_literal(out.to_str().unwrap()),
    );

    let output = run_temp_script("archive-hardlink", &script);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "shared shared\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let source = std::fs::metadata(out.join("dir").join("source.txt")).expect("source metadata");
    let copy = std::fs::metadata(out.join("dir").join("copy.txt")).expect("copy metadata");
    assert_eq!(source.ino(), copy.ino());

    let _ = std::fs::remove_dir_all(root);
}

#[cfg(feature = "net")]
struct LocalDnsServer {
    addr: String,
    handle: std::thread::JoinHandle<LocalDnsSummary>,
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
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set DNS read timeout");
        let addr = socket.local_addr().expect("DNS listener addr").to_string();
        let handle = std::thread::spawn(move || {
            let mut handled = 0;
            let mut request = [0_u8; 512];
            while handled < expected {
                let (len, peer) = socket.recv_from(&mut request).expect("read DNS request");
                let response = local_dns_response(&request[..len]);
                socket.send_to(&response, peer).expect("write DNS response");
                handled += 1;
            }
            LocalDnsSummary { handled }
        });
        Self { addr, handle }
    }

    fn join(self) -> LocalDnsSummary {
        self.handle.join().expect("DNS server")
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

#[cfg(feature = "net")]
struct LocalHttpServer {
    url: String,
    handle: std::thread::JoinHandle<LocalHttpSummary>,
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
        let handle = std::thread::spawn(move || {
            let handled = Arc::new(AtomicUsize::new(0));
            let connections = Arc::new(AtomicUsize::new(0));
            let (request_tx, request_rx) = crossbeam_channel::unbounded();
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut workers = Vec::new();
            while handled.load(Ordering::SeqCst) < expected && Instant::now() < deadline {
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
        Self { url, handle }
    }

    fn join(self) -> LocalHttpSummary {
        self.handle.join().expect("HTTP server")
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
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
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
        stream
            .write_all(response.as_bytes())
            .expect("write HTTP response");
        stream.flush().expect("flush HTTP response");
    }
}

#[cfg(feature = "net")]
fn local_http_response(request: &LocalHttpRequest) -> String {
    let (status, reason, headers, body): (&str, &str, Vec<(&str, &str)>, Vec<u8>) =
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/hello") => ("200", "OK", Vec::new(), b"hello".to_vec()),
            ("GET", "/redirect") => ("302", "Found", vec![("Location", "/hello")], Vec::new()),
            ("POST", "/echo") => {
                let mut body = b"echo:".to_vec();
                body.extend_from_slice(&request.body);
                ("200", "OK", Vec::new(), body)
            }
            ("GET", "/status") => ("404", "Not Found", Vec::new(), b"missing".to_vec()),
            ("GET", "/file") => ("200", "OK", Vec::new(), b"downloaded\n".to_vec()),
            ("PUT", "/upload") => ("201", "Created", Vec::new(), b"uploaded".to_vec()),
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
    handle: std::thread::JoinHandle<LocalHttpsSummary>,
}

#[cfg(feature = "net")]
#[derive(Debug)]
struct LocalHttpsSummary {
    handled: usize,
}

#[cfg(feature = "net")]
impl LocalHttpsServer {
    fn spawn(expected: usize) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind HTTPS listener");
        listener
            .set_nonblocking(true)
            .expect("set HTTPS listener nonblocking");
        let addr = listener.local_addr().expect("HTTPS listener addr");
        let url = format!("https://{addr}");
        let config = Arc::new(local_https_config());
        let handle = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut handled = 0;
            while handled < expected && Instant::now() < deadline {
                match listener.accept() {
                    Ok((stream, _)) => {
                        handle_local_https_connection(stream, config.clone());
                        handled += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept HTTPS connection: {error}"),
                }
            }
            LocalHttpsSummary { handled }
        });
        Self { url, handle }
    }

    fn join(self) -> LocalHttpsSummary {
        self.handle.join().expect("HTTPS server")
    }
}

#[cfg(feature = "net")]
fn local_https_config() -> rustls::ServerConfig {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    let cert = CertificateDer::from_pem_slice(LOCAL_HTTPS_CERT.as_bytes())
        .expect("parse local HTTPS cert");
    let key =
        PrivateKeyDer::from_pem_slice(LOCAL_HTTPS_KEY.as_bytes()).expect("parse local HTTPS key");
    rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("HTTPS protocol versions")
    .with_no_client_auth()
    .with_single_cert(vec![cert], key)
    .expect("local HTTPS certificate")
}

#[cfg(feature = "net")]
fn handle_local_https_connection(stream: std::net::TcpStream, config: Arc<rustls::ServerConfig>) {
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
        stream.read_exact(&mut byte).expect("read HTTPS request");
        request.push(byte[0]);
        if request.len() > 8192 {
            panic!("HTTPS request header too large");
        }
    }
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecure")
        .expect("write HTTPS response");
    stream.flush().expect("flush HTTPS response");
}
