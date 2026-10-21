use super::*;
use std::sync::OnceLock;

const POLICY_FILE: &str = "/etc/laputa/trust-policy";
const POLICY_LIMIT: u64 = 1024 * 1024;
#[cfg(not(target_os = "macos"))]
const SYSTEM_FILES: &[&str] = &[
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/ssl/ca-bundle.pem",
    "/etc/pki/tls/cacert.pem",
    "/etc/ssl/cert.pem",
    "/opt/etc/ssl/certs/ca-certificates.crt",
    "/etc/ssl/certs/cacert.pem",
];

type RootFiles = Vec<(PathBuf, Vec<u8>)>;

#[derive(Debug)]
pub(super) struct TrustGuard {
    policy_file: PathBuf,
    roots: RootSource,
    snapshot: OnceLock<NetResult<Snapshot>>,
    failure: OnceLock<NetError>,
}

#[derive(Debug)]
enum RootSource {
    Disabled,
    Custom(PathBuf, Vec<u8>),
    System { file: Option<PathBuf>, directories: Vec<PathBuf> },
}

#[derive(Debug, Eq, PartialEq)]
struct Snapshot {
    policy: Option<Vec<u8>>,
    blocked: Vec<[u8; 32]>,
    roots: RootFiles,
}

fn policy_error(message: impl Into<String>) -> NetError {
    NetError::new("net-ca-certificate", message)
}

fn read_file(path: &Path) -> NetResult<Vec<u8>> {
    fs::read(path).map_err(|error| policy_error(format!("{}: {error}", path.display())))
}

impl TrustGuard {
    pub(super) fn new(key: &NetAgentKey) -> NetResult<Self> {
        let roots = if !key.tls_verify {
            RootSource::Disabled
        } else if let Some(path) = &key.ca_certificate {
            RootSource::Custom(path.clone(), read_file(path)?)
        } else {
            RootSource::System {
                file: key.system_ca_file.clone().or_else(|| std::env::var_os("SSL_CERT_FILE").map(PathBuf::from)),
                directories: std::env::var_os("SSL_CERT_DIR")
                    .map(|value| std::env::split_paths(&value).collect()).unwrap_or_default(),
            }
        };
        Ok(Self { policy_file: POLICY_FILE.into(), roots, snapshot: OnceLock::new(), failure: OnceLock::new() })
    }

    fn capture(&self) -> NetResult<Snapshot> {
        let policy = match File::open(&self.policy_file) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(POLICY_LIMIT + 1).read_to_end(&mut bytes)
                    .map_err(|error| policy_error(format!("{}: {error}", self.policy_file.display())))?;
                if bytes.len() as u64 > POLICY_LIMIT {
                    return Err(policy_error("trust policy exceeds 1 MiB"));
                }
                Some(bytes)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match fs::symlink_metadata(&self.policy_file) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                    Ok(_) => return Err(policy_error(format!("{}: policy target does not exist", self.policy_file.display()))),
                    Err(error) => return Err(policy_error(format!("{}: {error}", self.policy_file.display()))),
                }
            },
            Err(error) => return Err(policy_error(format!("{}: {error}", self.policy_file.display()))),
        };
        let blocked = policy.as_deref().map(parse_policy).transpose()?.unwrap_or_default();
        let mut roots = Vec::new();
        match &self.roots {
            RootSource::Disabled => {}
            RootSource::Custom(path, expected) => {
                let bytes = read_file(path)?;
                if &bytes != expected {
                    return Err(policy_error("custom CA bundle changed; recreate the network client"));
                }
                roots.push((path.clone(), bytes));
            }
            RootSource::System { file, directories } => {
                #[cfg(not(target_os = "macos"))]
                let file = file.clone().or_else(|| SYSTEM_FILES.iter().map(PathBuf::from).find(|path| {
                    !matches!(fs::symlink_metadata(path), Err(error) if error.kind() == io::ErrorKind::NotFound)
                }));
                #[cfg(target_os = "macos")]
                let file = file.clone();
                if let Some(path) = file {
                    roots.push((path.clone(), read_file(&path)?));
                }
                for directory in directories {
                    let entries = fs::read_dir(directory).map_err(|error| policy_error(format!("{}: {error}", directory.display())))?;
                    let mut paths = Vec::new();
                    for entry in entries {
                        let path = entry.map_err(|error| policy_error(error.to_string()))?.path();
                        let metadata = fs::metadata(&path).map_err(|error| policy_error(format!("{}: {error}", path.display())))?;
                        if metadata.is_file() { paths.push(path); }
                    }
                    paths.sort();
                    for path in paths { roots.push((path.clone(), read_file(&path)?)); }
                }
            }
        }
        Ok(Snapshot { policy, blocked, roots })
    }

    fn current(&self) -> NetResult<&Snapshot> {
        if let Some(error) = self.failure.get() { return Err(error.clone()); }
        let result = self.current_snapshot();
        if let Err(error) = &result { let _ = self.failure.set(error.clone()); }
        result
    }

    fn current_snapshot(&self) -> NetResult<&Snapshot> {
        let snapshot = self.snapshot.get_or_init(|| {
            let snapshot = self.capture()?;
            for (path, bytes) in &snapshot.roots {
                parse_root_bundle(bytes).map_err(|error| policy_error(format!("{}: {}", path.display(), error.message)))?;
            }
            Ok(snapshot)
        }).as_ref().map_err(Clone::clone)?;
        let current = self.capture()?;
        if &current != snapshot {
            return Err(policy_error("TLS trust policy or roots changed; recreate the network client"));
        }
        Ok(snapshot)
    }

    pub(super) fn check(&self) -> NetResult<()> { self.current().map(|_| ()) }

    pub(super) fn custom_roots(&self) -> NetResult<RootCertStore> {
        match &self.roots {
            RootSource::Custom(path, bytes) => roots_from_bytes(&[(path.clone(), bytes.clone())], &[]),
            _ => Err(policy_error("custom CA bundle is not configured")),
        }
    }

    pub(super) fn roots(&self) -> NetResult<RootCertStore> {
        let result = self.current().and_then(|snapshot| roots_from_bytes(&snapshot.roots, &snapshot.blocked));
        if let Err(error) = &result { let _ = self.failure.set(error.clone()); }
        result
    }

    fn check_chain(&self, leaf: &CertificateDer<'_>, intermediates: &[CertificateDer<'_>]) -> Result<(), rustls::Error> {
        let snapshot = self.current().map_err(|error| rustls::Error::General(error.message))?;
        if snapshot.blocked.is_empty() { return Ok(()); }
        for cert in std::iter::once(leaf).chain(intermediates) {
            let digest = spki_digest(cert)?;
            if snapshot.blocked.contains(&digest) {
                return Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure));
            }
        }
        Ok(())
    }
}

fn spki_sequence(content: &[u8]) -> Vec<u8> {
    let mut encoded = vec![0x30];
    if content.len() < 128 {
        encoded.push(content.len() as u8);
    } else {
        let length = content.len().to_be_bytes();
        let start = length.iter().position(|byte| *byte != 0).unwrap_or(length.len() - 1);
        encoded.push(0x80 | (length.len() - start) as u8);
        encoded.extend_from_slice(&length[start..]);
    }
    encoded.extend_from_slice(content);
    encoded
}

fn spki_digest(cert: &CertificateDer<'_>) -> Result<[u8; 32], rustls::Error> {
    let anchor = webpki::anchor_from_trusted_cert(cert)
        .map_err(|_| rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))?;
    let spki = spki_sequence(anchor.subject_public_key_info.as_ref());
    let mut digest = graviola::hashing::sha2::Sha256Context::new();
    digest.update(&spki);
    Ok(digest.finish())
}

pub(super) fn parse_root_bundle(mut bytes: &[u8]) -> NetResult<Vec<CertificateDer<'static>>> {
    let mut certificates = Vec::new();
    loop {
        bytes = bytes.trim_ascii_start();
        if bytes.is_empty() { break; }
        if bytes.starts_with(b"#") {
            bytes = bytes.iter().position(|byte| *byte == b'\n').map_or(&[], |index| &bytes[index + 1..]);
            continue;
        }
        let mut lines = bytes.split_inclusive(|byte| *byte == b'\n');
        let first = lines.next().ok_or_else(|| policy_error("missing certificate PEM section"))?;
        if first.trim_ascii() != b"-----BEGIN CERTIFICATE-----" {
            return Err(policy_error("root bundle must contain only PEM certificates, whitespace, or # comments"));
        }
        let mut section_len = first.len();
        let mut ended = false;
        for line in lines {
            section_len += line.len();
            if line.trim_ascii() == b"-----END CERTIFICATE-----" { ended = true; break; }
            if line.trim_ascii().starts_with(b"-----") {
                return Err(policy_error("unexpected PEM marker inside certificate section"));
            }
        }
        if !ended { return Err(policy_error("missing certificate PEM end marker")); }
        let (item, remainder) = rustls_pemfile::read_one_from_slice(&bytes[..section_len])
            .map_err(|error| policy_error(format!("invalid certificate PEM: {error:?}")))?
            .ok_or_else(|| policy_error("missing certificate PEM section"))?;
        if !remainder.trim_ascii().is_empty() { return Err(policy_error("unexpected data after certificate PEM section")); }
        let rest = &bytes[section_len..];
        let rustls_pemfile::Item::X509Certificate(certificate) = item else {
            return Err(policy_error("root bundle contains a non-certificate PEM section"));
        };
        RootCertStore::empty().add(certificate.clone())
            .map_err(|error| policy_error(format!("invalid certificate DER: {error}")))?;
        certificates.push(certificate);
        bytes = rest;
    }
    if certificates.is_empty() { return Err(policy_error("root bundle contains no certificates")); }
    Ok(certificates)
}

fn roots_from_bytes(files: &[(PathBuf, Vec<u8>)], blocked: &[[u8; 32]]) -> NetResult<RootCertStore> {
    let mut roots = RootCertStore::empty();
    for (path, bytes) in files {
        let certificates = parse_root_bundle(bytes).map_err(|error| policy_error(format!("{}: {}", path.display(), error.message)))?;
        for certificate in certificates {
            let digest = spki_digest(&certificate).map_err(|error| policy_error(format!("{}: {error}", path.display())))?;
            if blocked.contains(&digest) { continue; }
            roots.add(certificate).map_err(|error| policy_error(format!("{}: {error}", path.display())))?;
        }
    }
    if roots.is_empty() { return Err(policy_error("no CA certificates were loaded from the system")); }
    Ok(roots)
}

fn parse_policy(bytes: &[u8]) -> NetResult<Vec<[u8; 32]>> {
    let invalid = || policy_error("malformed Laputa trust policy");
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    if !text.ends_with('\n') || text.contains('\r') { return Err(invalid()); }
    let mut lines = text.lines();
    if lines.next() != Some("laputa-trust-policy-v1") { return Err(invalid()); }
    let generation = lines.next().and_then(|line| line.strip_prefix("generation ")).ok_or_else(invalid)?;
    let number = generation.parse::<u64>().map_err(|_| invalid())?;
    if number == 0 || number.to_string() != generation { return Err(invalid()); }
    let mut blocked = Vec::new();
    for line in lines {
        let hex = line.strip_prefix("blocked-spki-sha256 ").ok_or_else(invalid)?;
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) { return Err(invalid()); }
        let mut digest = [0_u8; 32];
        for (index, byte) in digest.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).map_err(|_| invalid())?;
        }
        if blocked.contains(&digest) { return Err(invalid()); }
        blocked.push(digest);
    }
    Ok(blocked)
}

#[derive(Debug)]
pub(super) struct PolicyVerifier {
    pub(super) inner: Arc<dyn ServerCertVerifier>,
    pub(super) trust: Arc<TrustGuard>,
}

impl ServerCertVerifier for PolicyVerifier {
    fn verify_server_cert(&self, leaf: &CertificateDer<'_>, intermediates: &[CertificateDer<'_>], name: &ServerName<'_>, ocsp: &[u8], now: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        self.trust.check_chain(leaf, intermediates)?;
        let result = self.inner.verify_server_cert(leaf, intermediates, name, ocsp, now)?;
        self.trust.check().map_err(|error| rustls::Error::General(error.message))?;
        Ok(result)
    }
    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }
    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> { self.inner.supported_verify_schemes() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    const ROOT_A: &[u8] = include_bytes!("../tests/fixtures/root-a.der");
    const ROOT_B: &[u8] = include_bytes!("../tests/fixtures/root-b.der");
    const ISSUER_A: &[u8] = include_bytes!("../tests/fixtures/issuer-a.der");
    const ISSUER_B: &[u8] = include_bytes!("../tests/fixtures/issuer-b.der");
    const LEAF: &[u8] = include_bytes!("../tests/fixtures/leaf.der");
    const ROOT_PEM: &[u8] = include_bytes!("../tests/fixtures/root-a.pem");

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.work/tmp");
            fs::create_dir_all(&base).unwrap();
            let path = base.join(format!("trust-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) { fs::remove_dir_all(&self.0).unwrap(); }
    }

    fn policy(generation: u64, blocked: &[&[u8]]) -> Vec<u8> {
        let mut text = format!("laputa-trust-policy-v1\ngeneration {generation}\n");
        for cert in blocked {
            let digest = spki_digest(&CertificateDer::from(*cert)).unwrap();
            let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
            text.push_str(&format!("blocked-spki-sha256 {hex}\n"));
        }
        text.into_bytes()
    }

    fn guard(dir: &Path, bytes: Option<&[u8]>) -> Arc<TrustGuard> {
        let path = dir.join("policy");
        if let Some(bytes) = bytes { fs::write(&path, bytes).unwrap(); }
        Arc::new(TrustGuard { policy_file: path, roots: RootSource::Disabled, snapshot: OnceLock::new(), failure: OnceLock::new() })
    }

    fn verifier(trust: Arc<TrustGuard>, roots: &[&[u8]], verify: bool) -> PolicyVerifier {
        let inner: Arc<dyn ServerCertVerifier> = if verify {
            let mut store = RootCertStore::empty();
            for root in roots { store.add(CertificateDer::from(*root)).unwrap(); }
            rustls::client::WebPkiServerVerifier::builder_with_provider(Arc::new(store), Arc::new(default_provider())).build().unwrap()
        } else { Arc::new(DisabledVerifier) };
        PolicyVerifier { inner, trust }
    }

    fn verify(verifier: &PolicyVerifier, issuer: &[u8], name: &str) -> Result<ServerCertVerified, rustls::Error> {
        verifier.verify_server_cert(&CertificateDer::from(LEAF), &[CertificateDer::from(issuer)], &ServerName::try_from(name).unwrap(), &[], UnixTime::now())
    }

    #[test]
    fn policy_rejects_malformed_unknown_duplicate_and_noncanonical_data() {
        let valid = policy(1, &[ISSUER_A]);
        assert_eq!(parse_policy(&valid).unwrap().len(), 1);
        for invalid in [
            b"".as_slice(), b"laputa-trust-policy-v2\ngeneration 1\n",
            b"laputa-trust-policy-v1\ngeneration 0\n",
            b"laputa-trust-policy-v1\ngeneration 01\n",
            b"laputa-trust-policy-v1\ngeneration 1",
            b"laputa-trust-policy-v1\ngeneration 1\nunknown field\n",
            b"laputa-trust-policy-v1\ngeneration 1\nblocked-spki-sha256 aa\n",
            b"laputa-trust-policy-v1\ngeneration 1\n\n",
        ] { assert!(parse_policy(invalid).is_err(), "{invalid:?}"); }
        assert!(parse_policy(&policy(1, &[ISSUER_A, ISSUER_B])).is_err());
        let mut non_utf8 = valid.clone(); non_utf8.push(0xff);
        assert!(parse_policy(&non_utf8).is_err());
    }

    #[test]
    fn cross_signed_issuer_blocks_both_anchors_and_custom_or_disabled_verification() {
        let scratch = Scratch::new();
        let trust = guard(&scratch.0, Some(&policy(1, &[ISSUER_A])));
        assert_eq!(spki_digest(&CertificateDer::from(ISSUER_A)).unwrap(), spki_digest(&CertificateDer::from(ISSUER_B)).unwrap());
        for (root, issuer) in [(ROOT_A, ISSUER_A), (ROOT_B, ISSUER_B)] {
            for enabled in [true, false] {
                assert!(matches!(verify(&verifier(Arc::clone(&trust), &[root], enabled), issuer, "localhost"), Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure))));
            }
        }
    }

    #[test]
    fn unblocked_valid_chains_preserve_hostname_validation() {
        let scratch = Scratch::new();
        let trust = guard(&scratch.0, Some(&policy(1, &[])));
        for (root, issuer) in [(ROOT_A, ISSUER_A), (ROOT_B, ISSUER_B)] {
            let verifier = verifier(Arc::clone(&trust), &[root], true);
            assert!(verify(&verifier, issuer, "localhost").is_ok());
            assert!(verify(&verifier, issuer, "wrong.example").is_err());
            let mut tampered = LEAF.to_vec();
            *tampered.last_mut().unwrap() ^= 1;
            assert!(verifier.verify_server_cert(&CertificateDer::from(tampered), &[CertificateDer::from(issuer)], &ServerName::try_from("localhost").unwrap(), &[], UnixTime::now()).is_err());
        }
    }

    #[test]
    fn supplied_leaf_and_unused_peer_certificate_cannot_escape_policy() {
        for blocked in [LEAF, ISSUER_B] {
            let scratch = Scratch::new();
            let trust = guard(&scratch.0, Some(&policy(1, &[blocked])));
            let verifier = verifier(trust, &[ROOT_A], true);
            let result = verifier.verify_server_cert(&CertificateDer::from(LEAF), &[CertificateDer::from(ISSUER_A), CertificateDer::from(ISSUER_B)], &ServerName::try_from("localhost").unwrap(), &[], UnixTime::now());
            assert!(matches!(result, Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure))));
        }
    }

    #[test]
    fn policy_updates_removal_and_malformed_replacements_permanently_invalidate_client() {
        for replacement in [Some(policy(2, &[])), None, Some(b"malformed".to_vec())] {
            let scratch = Scratch::new();
            let initial = policy(1, &[]);
            let trust = guard(&scratch.0, Some(&initial));
            trust.check().unwrap();
            match replacement { Some(bytes) => fs::write(&trust.policy_file, bytes).unwrap(), None => fs::remove_file(&trust.policy_file).unwrap() }
            assert_eq!(trust.check().unwrap_err().kind, "net-ca-certificate");
            fs::write(&trust.policy_file, initial).unwrap();
            assert!(trust.check().is_err());
            assert!(verify(&verifier(trust, &[ROOT_A], true), ISSUER_A, "localhost").is_err());
        }
        let scratch = Scratch::new();
        let trust = guard(&scratch.0, None);
        trust.check().unwrap();
        fs::write(&trust.policy_file, policy(1, &[])).unwrap();
        assert!(trust.check().is_err());
    }

    #[test]
    fn root_bundle_and_explicit_directory_updates_invalidate_client() {
        for directory in [false, true] {
            let scratch = Scratch::new();
            let ca = scratch.0.join("ca.pem"); fs::write(&ca, ROOT_PEM).unwrap();
            let extra = scratch.0.join("extra"); fs::create_dir(&extra).unwrap();
            let trust = TrustGuard { policy_file: scratch.0.join("policy"), roots: RootSource::System { file: Some(ca.clone()), directories: vec![extra.clone()] }, snapshot: OnceLock::new(), failure: OnceLock::new() };
            trust.roots().unwrap();
            if directory { fs::write(extra.join("new.pem"), ROOT_PEM).unwrap(); }
            else { fs::write(ca, [ROOT_PEM, b"\n"].concat()).unwrap(); }
            assert!(trust.check().is_err());
        }
    }

    #[test]
    fn blocked_unsent_trust_anchor_is_removed_from_custom_roots() {
        let scratch = Scratch::new();
        let ca = scratch.0.join("ca.pem"); fs::write(&ca, ROOT_PEM).unwrap();
        let mut trust = guard(&scratch.0, Some(&policy(1, &[ROOT_A])));
        Arc::get_mut(&mut trust).unwrap().roots = RootSource::Custom(ca, ROOT_PEM.to_vec());
        assert!(trust.roots().is_err());
    }

    fn test_agent(trust: Arc<TrustGuard>, ca: PathBuf, executor: &Arc<Executor<'static>>) -> NetAgent {
        let key = NetAgentKey { pool: "test".into(), tls_verify: true, ca_certificate: Some(ca), system_ca_file: None, max_idle_per_host: 1, idle_timeout: Duration::from_secs(60) };
        let tls = tls_config_for_key(&key, Arc::clone(&trust)).unwrap();
        NetAgent {
            h1_client: h12_client(Arc::clone(executor), tls.clone(), Arc::clone(&trust), 1, Duration::from_secs(60), true),
            auto_client: h12_client(Arc::clone(executor), tls, trust, 1, Duration::from_secs(60), false),
        }
    }

    fn fixture_request(port: u16) -> NetRequest {
        NetRequest {
            method: "GET".into(), url: format!("https://localhost:{port}/"), headers: Vec::new(), body: NetBody::Empty,
            timeout: Some(Duration::from_secs(4)), dns_timeout: None, connect_timeout: None, tls_timeout: None, headers_timeout: None, body_idle_timeout: None,
            redirects: 0, redirect_limit: RedirectLimit::Error, fail_status: true, max_body_bytes: 100,
        }
    }

    fn fixture_accept(listener: &Async<std::net::TcpListener>) -> std::net::TcpStream {
        async_io::block_on(futures_lite::future::race(
            listener.read_with(|listener| listener.accept()),
            async {
                async_io::Timer::after(Duration::from_secs(5)).await;
                Err(io::Error::new(io::ErrorKind::TimedOut, "fixture accept timed out"))
            },
        )).unwrap().0
    }

    fn fixture_stream(socket: std::net::TcpStream) -> rustls::StreamOwned<rustls::ServerConnection, std::net::TcpStream> {
        socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(default_provider()))
            .with_safe_default_protocol_versions().unwrap().with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(LEAF), CertificateDer::from(ISSUER_A)], rustls::pki_types::PrivatePkcs8KeyDer::from(include_bytes!("../tests/fixtures/leaf-key.der").to_vec()).into()).unwrap();
        rustls::StreamOwned::new(rustls::ServerConnection::new(Arc::new(config)).unwrap(), socket)
    }

    fn fixture_read_request(stream: &mut impl Read) {
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0]; stream.read_exact(&mut byte).unwrap(); request.push(byte[0]);
            assert!(request.len() < 8192);
        }
    }

    fn fixture_respond(stream: &mut impl Write) {
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").unwrap();
        stream.flush().unwrap();
    }

    #[test]
    fn pooled_https_admission_rejects_waiting_operations_and_recreated_agent_loads_update() {
        let scratch = Scratch::new();
        let ca = scratch.0.join("ca.pem"); fs::write(&ca, ROOT_PEM).unwrap();
        let mut trust = guard(&scratch.0, Some(&policy(1, &[])));
        Arc::get_mut(&mut trust).unwrap().roots = RootSource::Custom(ca.clone(), ROOT_PEM.to_vec());
        let executor = Arc::new(Executor::new());
        let agent = test_agent(Arc::clone(&trust), ca.clone(), &executor);
        let listener = Async::new(std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap()).unwrap();
        let port = listener.get_ref().local_addr().unwrap().port();
        let (control_client, mut control_server) = std::os::unix::net::UnixStream::pair().unwrap();
        control_server.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        control_server.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        let control = Async::new(control_client).unwrap();
        thread::scope(|scope| {
            let server = scope.spawn(move || {
                let mut stream = fixture_stream(fixture_accept(&listener));
                for _ in 0..2 { fixture_read_request(&mut stream); fixture_respond(&mut stream); }
                fixture_read_request(&mut stream);
                control_server.write_all(&[1]).unwrap();
                control_server.read_exact(&mut [0]).unwrap();
                fixture_respond(&mut stream);
                let mut recreated = fixture_stream(fixture_accept(&listener));
                fixture_read_request(&mut recreated);
                fixture_respond(&mut recreated);
            });
            async_io::block_on(executor.run(async {
                for _ in 0..2 {
                    let response = run_request(&agent, fixture_request(port), runtime::NetProtocol::Http1, Instant::now()).await.unwrap();
                    assert_eq!(response.body.as_deref(), Some(b"ok".as_slice()));
                }
                let waiting = run_request(&agent, fixture_request(port), runtime::NetProtocol::Http1, Instant::now());
                let admitted_agent = agent.clone();
                let admitted = executor.spawn(async move {
                    run_request(&admitted_agent, fixture_request(port), runtime::NetProtocol::Http1, Instant::now()).await
                });
                control.read_with(|mut socket| socket.read_exact(&mut [0])).await.unwrap();
                fs::write(&trust.policy_file, policy(2, &[])).unwrap();
                control.write_with(|mut socket| socket.write_all(&[1])).await.unwrap();
                assert_eq!(admitted.await.unwrap().body.as_deref(), Some(b"ok".as_slice()));
                assert_eq!(waiting.await.unwrap_err().kind, "net-ca-certificate");
                assert_eq!(run_request(&agent, fixture_request(port), runtime::NetProtocol::Http1, Instant::now()).await.unwrap_err().kind, "net-ca-certificate");
                let mut refreshed = guard(&scratch.0, None);
                Arc::get_mut(&mut refreshed).unwrap().roots = RootSource::Custom(ca.clone(), ROOT_PEM.to_vec());
                let refreshed_agent = test_agent(refreshed, ca, &executor);
                assert_eq!(run_request(&refreshed_agent, fixture_request(port), runtime::NetProtocol::Http1, Instant::now()).await.unwrap().body.as_deref(), Some(b"ok".as_slice()));
            }));
            server.join().unwrap();
        });
    }

    #[test]
    fn handshake_observed_policy_change_retains_trust_store_error_category() {
        let scratch = Scratch::new();
        let ca = scratch.0.join("ca.pem"); fs::write(&ca, ROOT_PEM).unwrap();
        let mut trust = guard(&scratch.0, Some(&policy(1, &[])));
        Arc::get_mut(&mut trust).unwrap().roots = RootSource::Custom(ca.clone(), ROOT_PEM.to_vec());
        let executor = Arc::new(Executor::new());
        let agent = test_agent(Arc::clone(&trust), ca, &executor);
        let listener = Async::new(std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap()).unwrap();
        let port = listener.get_ref().local_addr().unwrap().port();
        let policy_file = trust.policy_file.clone();
        thread::scope(|scope| {
            let server = scope.spawn(move || {
                let socket = fixture_accept(&listener);
                fs::write(policy_file, policy(2, &[])).unwrap();
                let mut stream = fixture_stream(socket);
                assert!(stream.read_exact(&mut [0]).is_err());
            });
            async_io::block_on(executor.run(async {
                let error = run_request(&agent, fixture_request(port), runtime::NetProtocol::Http1, Instant::now()).await.unwrap_err();
                assert_eq!(error.kind, "net-ca-certificate");
            }));
            server.join().unwrap();
        });
    }

    #[test]
    fn root_bundle_validation_rejects_empty_der_mixed_and_noncertificate_material() {
        validate_root_bundle(ROOT_PEM).unwrap();
        validate_root_bundle(&[ROOT_PEM, b"\n# administrator note\n", ROOT_PEM].concat()).unwrap();
        for bytes in [
            b"".as_slice(), b" \n# no certificates\n", ROOT_A,
            b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n",
            b"-----BEGIN CERTIFICATE-----\ninvalid base64!\n-----END CERTIFICATE-----\n",
            b"-----BEGIN CERTIFICATE-----\nAAAA\n",
            b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n",
        ] {
            assert_eq!(validate_root_bundle(bytes).unwrap_err().kind, "net-ca-certificate");
            if !bytes.trim_ascii().is_empty() && bytes != b" \n# no certificates\n" {
                assert!(validate_root_bundle(&[ROOT_PEM, bytes].concat()).is_err(), "invalid mixed bundle: {bytes:?}");
            }
        }
        assert!(validate_root_bundle(&[ROOT_PEM, b"garbage"].concat()).is_err());
    }


    #[test]
    fn root_bundle_validation_rejects_nested_or_unknown_pem_sections() {
        for prefix in [
            b"-----BEGIN CERTIFICATE-----\n".as_slice(),
            b"-----BEGIN CERTIFICATE-----\nAAAA\n",
            b"-----BEGIN CERTIFICATE-----garbage\n",
            b"-----BEGIN UNKNOWN-----\nAAAA\n-----END UNKNOWN-----\n",
        ] {
            assert!(validate_root_bundle(&[prefix, ROOT_PEM].concat()).is_err());
        }
    }


    #[test]
    fn v1_anchors_and_spki_sequence_framing_match_independent_openssl_oracles() {
        let cases: &[(&[u8], &[u8], &str)] = &[
            (ROOT_A, include_bytes!("../tests/fixtures/root-a-spki.der"), "5d48100876f8b3a852d84dfc058145240fc85e2493b964a9baa51f2e9df3c50f"),
            (include_bytes!("../tests/fixtures/root-v1.der"), include_bytes!("../tests/fixtures/root-v1-spki.der"), "5d48100876f8b3a852d84dfc058145240fc85e2493b964a9baa51f2e9df3c50f"),
            (include_bytes!("../tests/fixtures/root-rsa.der"), include_bytes!("../tests/fixtures/root-rsa-spki.der"), "3ecabacaf5f69547b60f3375b80b06086a077a789b1ce23c44dba27223c333b7"),
            (LEAF, include_bytes!("../tests/fixtures/leaf-spki.der"), "3a1c22d6d095b9ed7f1aa3c86c55fb69be3c47803179c339a2aa6e88580fa7de"),
        ];
        for (cert, spki, expected_hash) in cases {
            let certificate = CertificateDer::from(*cert);
            let anchor = webpki::anchor_from_trusted_cert(&certificate).unwrap();
            assert_eq!(spki_sequence(anchor.subject_public_key_info.as_ref()), *spki);
            let digest: String = spki_digest(&certificate).unwrap().iter().map(|byte| format!("{byte:02x}")).collect();
            assert_eq!(&digest, expected_hash);
            if let Ok(parsed) = webpki::EndEntityCert::try_from(&certificate) {
                assert_eq!(parsed.subject_public_key_info().as_ref(), *spki);
            }
        }
        for (length, prefix) in [(0, vec![0x30, 0]), (1, vec![0x30, 1]), (127, vec![0x30, 127]), (128, vec![0x30, 0x81, 128]), (255, vec![0x30, 0x81, 255]), (256, vec![0x30, 0x82, 1, 0])] {
            let content = vec![0x55; length];
            let encoded = spki_sequence(&content);
            assert_eq!(&encoded[..prefix.len()], &prefix);
            assert_eq!(&encoded[prefix.len()..], &content);
        }
        let pem = include_bytes!("../tests/fixtures/root-v1.pem");
        validate_root_bundle(pem).unwrap();
        assert_eq!(roots_from_bytes(&[(PathBuf::from("v1.pem"), pem.to_vec())], &[]).unwrap().len(), 1);
        assert!(roots_from_bytes(&[(PathBuf::from("v1.pem"), pem.to_vec())], &[spki_digest(&CertificateDer::from(ROOT_A)).unwrap()]).is_err());
    }


    #[test]
    fn initial_configuration_rejects_dangling_policy_symlink() {
        let scratch = Scratch::new();
        let trust = guard(&scratch.0, None);
        std::os::unix::fs::symlink(scratch.0.join("missing-policy"), &trust.policy_file).unwrap();
        assert_eq!(trust.check().unwrap_err().kind, "net-ca-certificate");
    }

    #[test]
    fn initial_configuration_rejects_malformed_system_bundle_before_admission() {
        for bytes in [b"".as_slice(), b"invalid PEM"] {
            let scratch = Scratch::new();
            let ca = scratch.0.join("ca.pem"); fs::write(&ca, bytes).unwrap();
            let trust = TrustGuard { policy_file: scratch.0.join("policy"), roots: RootSource::System { file: Some(ca), directories: Vec::new() }, snapshot: OnceLock::new(), failure: OnceLock::new() };
            assert_eq!(trust.check().unwrap_err().kind, "net-ca-certificate");
        }
    }

}
