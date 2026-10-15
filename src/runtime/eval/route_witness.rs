use super::indexed::full::FullTag;
use super::indexed::full::schema::RouteWitnessCapability;
use std::cell::RefCell;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("route-witness descriptor transport supports Linux and macOS only");

pub(crate) const INVOCATION_TEST_ID_OVERLAY: &[u8] =
    b"XSH_INTERNAL_TEST_FORCE_FRAMES_ROUTE_WITNESS_INVOCATION_TEST_ID";
pub(crate) const INVOCATION_OUTPUT_DIR_OVERLAY: &[u8] =
    b"XSH_INTERNAL_TEST_FORCE_FRAMES_ROUTE_WITNESS_INVOCATION_OUTPUT_DIR";

const ARTIFACT_FILE: &str = "route-witness.json";
const ARTIFACT_VERSION: u64 = 1;
pub(crate) const ENV_PREFIX: &str = "XSH_INTERNAL_TEST_FORCE_FRAMES_ROUTE_WITNESS";
pub(crate) const HARNESS_ENV_PREFIX: &str = "XSHT_INTERNAL_ROUTE_WITNESS";
const TRANSPORT_MARKER_ENV: &str = "XSH_INTERNAL_ROUTE_WITNESS_FD_MARKER";
const TRANSPORT_MARKER_ENV_BYTES: &[u8] = TRANSPORT_MARKER_ENV.as_bytes();
const TRANSPORT_MAGIC: &[u8; 8] = b"XSHRWT01";
const TRANSPORT_NONCE_BYTES: usize = 16;
const TRANSPORT_SOURCE_HASH_BYTES: usize = 64;
const MAX_TRANSPORT_BYTES: usize = 64 * 1024;
const FIRST_INHERITABLE_FD: RawFd = 3;

thread_local! {
    static ACTIVE_TRANSPORTS: RefCell<Vec<Option<NativeTestRouteWitnessTransport>>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InvocationContext {
    pub(crate) test_id: String,
    pub(crate) output_dir: PathBuf,
}

pub(crate) struct ChildCapture {
    artifact: PathBuf,
    test_id: String,
    source_hash: String,
    sink: RouteWitnessSink,
}

pub(crate) struct ChildCaptureContext {
    test_id: String,
    output_dir: PathBuf,
    source_hash: String,
}

impl ChildCaptureContext {
    pub(crate) fn from_inherited_descriptor() -> Result<Option<Self>, String> {
        let marker = std::env::var_os(TRANSPORT_MARKER_ENV);
        let Some((fd, nonce)) = inherited_transport_descriptor(marker.as_deref())? else {
            return Ok(None);
        };
        Self::from_owned_descriptor(fd, &nonce).map(Some)
    }

    fn from_owned_descriptor(
        fd: OwnedFd,
        expected_nonce: &[u8; TRANSPORT_NONCE_BYTES],
    ) -> Result<Self, String> {
        let file = std::fs::File::from(fd);
        let metadata = file
            .metadata()
            .map_err(|error| format!("failed to inspect route-witness descriptor: {error}"))?;
        if !metadata.file_type().is_file() {
            return Err("route-witness descriptor is not a regular file".to_string());
        }
        if metadata.len() > MAX_TRANSPORT_BYTES as u64 {
            return Err("route-witness descriptor exceeds the transport limit".to_string());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file
            .take((MAX_TRANSPORT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("failed to read route-witness descriptor: {error}"))?;
        if bytes.len() > MAX_TRANSPORT_BYTES {
            return Err("route-witness descriptor exceeds the transport limit".to_string());
        }
        let (test_id, output_dir, source_hash) = decode_transport(&bytes, expected_nonce)?;
        Ok(Self {
            test_id,
            output_dir,
            source_hash,
        })
    }

    pub(crate) fn validate_source(self, source: &[u8]) -> Result<ChildCapture, String> {
        if self.source_hash != source_sha256(source) {
            return Err("route-witness source hash does not match the loaded script".to_string());
        }
        if !self.output_dir.is_dir() {
            return Err("route-witness output directory is unavailable".to_string());
        }
        let sink = RouteWitnessSink::for_tag(route_oracle_tag(&self.test_id)?);
        Ok(ChildCapture {
            artifact: self.output_dir.join(ARTIFACT_FILE),
            test_id: self.test_id,
            source_hash: self.source_hash,
            sink,
        })
    }
}

#[derive(Debug)]
pub(crate) struct NativeTestRouteWitnessTransport {
    fd: OwnedFd,
    nonce: [u8; TRANSPORT_NONCE_BYTES],
}

impl NativeTestRouteWitnessTransport {
    pub(crate) fn new(
        test_id: &str,
        output_dir: &Path,
        source: &[u8],
    ) -> std::io::Result<Self> {
        let test_id = test_id.as_bytes();
        let output_dir_bytes = output_dir.as_os_str().as_bytes();
        if test_id.is_empty()
            || test_id.contains(&0)
            || output_dir_bytes.is_empty()
            || output_dir_bytes.contains(&0)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "route-witness context has an empty or invalid field",
            ));
        }
        let test_id_len = u32::try_from(test_id.len()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "test identity is too long")
        })?;
        let output_dir_len = u32::try_from(output_dir_bytes.len()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "output path is too long")
        })?;
        let nonce = random_nonce()?;
        let payload_len = TRANSPORT_MAGIC
            .len()
            .checked_add(TRANSPORT_NONCE_BYTES + 8 + TRANSPORT_SOURCE_HASH_BYTES)
            .and_then(|length| length.checked_add(test_id.len()))
            .and_then(|length| length.checked_add(output_dir_bytes.len()))
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "route-witness context exceeds the transport limit",
                )
            })?;
        if payload_len > MAX_TRANSPORT_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "route-witness context exceeds the transport limit",
            ));
        }
        let source_hash = source_sha256(source);
        let mut payload = Vec::with_capacity(payload_len);
        payload.extend_from_slice(TRANSPORT_MAGIC);
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&test_id_len.to_be_bytes());
        payload.extend_from_slice(&output_dir_len.to_be_bytes());
        payload.extend_from_slice(source_hash.as_bytes());
        payload.extend_from_slice(test_id);
        payload.extend_from_slice(output_dir_bytes);
        let mut file = tempfile::tempfile_in(output_dir)?;
        file.write_all(&payload)?;
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        Ok(Self {
            fd: duplicate_cloexec_fd(file.as_fd())?,
            nonce,
        })
    }

    fn attach_to(&self, command: &mut Command) -> std::io::Result<()> {
        let child_fd = duplicate_cloexec_fd(self.fd.as_fd())?;
        let child_fd_number = child_fd.as_raw_fd();
        command.env(
            TRANSPORT_MARKER_ENV,
            format!("{child_fd_number}:{}", encode_nonce(&self.nonce)),
        );
        // SAFETY: both rustix fcntl operations are async-signal-safe, and
        // child_fd remains owned until spawn completes.
        unsafe {
            command.pre_exec(move || {
                let flags = rustix::io::fcntl_getfd(child_fd.as_fd())?;
                rustix::io::fcntl_setfd(
                    child_fd.as_fd(),
                    flags & !rustix::io::FdFlags::CLOEXEC,
                )?;
                Ok(())
            });
        }
        Ok(())
    }
}

struct ActiveTransportGuard;

impl Drop for ActiveTransportGuard {
    fn drop(&mut self) {
        ACTIVE_TRANSPORTS.with(|transports| {
            transports.borrow_mut().pop();
        });
    }
}

pub(crate) fn with_transport<T>(
    transport: Option<NativeTestRouteWitnessTransport>,
    callback: impl FnOnce() -> T,
) -> T {
    ACTIVE_TRANSPORTS.with(|transports| transports.borrow_mut().push(transport));
    let _guard = ActiveTransportGuard;
    callback()
}

pub(crate) fn attach_active_transport(command: &mut Command) -> std::io::Result<bool> {
    command.env_remove(TRANSPORT_MARKER_ENV);
    ACTIVE_TRANSPORTS.with(|transports| {
        let transports = transports.borrow();
        let Some(Some(transport)) = transports.last() else {
            return Ok(false);
        };
        transport.attach_to(command)?;
        Ok(true)
    })
}

pub(crate) fn is_harness_control_env_name_bytes(name: &[u8]) -> bool {
    let Some(suffix) = name.strip_prefix(HARNESS_ENV_PREFIX.as_bytes()) else {
        return name == TRANSPORT_MARKER_ENV_BYTES;
    };
    suffix == b"_TEST_ID" || suffix == b"_OUTPUT_DIR"
}

fn inherited_transport_descriptor(
    marker: Option<&std::ffi::OsStr>,
) -> Result<Option<(OwnedFd, [u8; TRANSPORT_NONCE_BYTES])>, String> {
    let Some(marker) = marker else {
        return Ok(None);
    };
    let marker = marker
        .to_str()
        .ok_or_else(|| "route-witness descriptor marker is not UTF-8".to_string())?;
    let (fd, nonce) = marker
        .split_once(':')
        .ok_or_else(|| "route-witness descriptor marker is malformed".to_string())?;
    let fd: RawFd = fd
        .parse()
        .map_err(|_| "route-witness descriptor marker has an invalid fd".to_string())?;
    if fd < FIRST_INHERITABLE_FD {
        return Err("route-witness descriptor marker has a reserved fd".to_string());
    }
    let nonce = decode_nonce(nonce)
        .ok_or_else(|| "route-witness descriptor marker has an invalid nonce".to_string())?;

    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd, metadata.as_mut_ptr()) } == -1 {
        return Err(format!(
            "failed to inspect route-witness descriptor: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: successful fstat initialized the stat structure.
    let metadata = unsafe { metadata.assume_init() };
    if rustix::fs::FileType::from_raw_mode(metadata.st_mode)
        != rustix::fs::FileType::RegularFile
        || metadata.st_size
            < (TRANSPORT_MAGIC.len() + TRANSPORT_NONCE_BYTES) as libc::off_t
    {
        return Ok(None);
    }
    let mut header = [0u8; TRANSPORT_MAGIC.len() + TRANSPORT_NONCE_BYTES];
    let read = unsafe {
        libc::pread(
            fd,
            header.as_mut_ptr().cast(),
            header.len(),
            0,
        )
    };
    if read != header.len() as libc::ssize_t
        || &header[..TRANSPORT_MAGIC.len()] != TRANSPORT_MAGIC
        || header[TRANSPORT_MAGIC.len()..] != nonce
    {
        return Ok(None);
    }

    // SAFETY: the marker nonce and descriptor header authenticate this fd as
    // the one installed by the harness. The caller now owns it and consumes it.
    Ok(Some((unsafe { OwnedFd::from_raw_fd(fd) }, nonce)))
}

fn random_nonce() -> std::io::Result<[u8; TRANSPORT_NONCE_BYTES]> {
    let mut nonce = [0; TRANSPORT_NONCE_BYTES];
    fs::File::open("/dev/urandom")?.read_exact(&mut nonce)?;
    Ok(nonce)
}

fn encode_nonce(nonce: &[u8; TRANSPORT_NONCE_BYTES]) -> String {
    nonce.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_nonce(encoded: &str) -> Option<[u8; TRANSPORT_NONCE_BYTES]> {
    if encoded.len() != TRANSPORT_NONCE_BYTES * 2 {
        return None;
    }
    let mut nonce = [0; TRANSPORT_NONCE_BYTES];
    for (index, pair) in encoded.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        nonce[index] = high << 4 | low;
    }
    Some(nonce)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

impl ChildCapture {
    pub(crate) fn sink(&self) -> RouteWitnessSink {
        self.sink.clone()
    }

    pub(crate) fn write_records(&self, records: &[RouteWitnessRecord]) -> std::io::Result<()> {
        let mut payload = format!(
            "{{\"version\":{ARTIFACT_VERSION},\"test_id\":{},\"source_hash\":{},\"records\":[",
            miniserde::json::to_string(&self.test_id),
            miniserde::json::to_string(&self.source_hash),
        );
        for (index, record) in records.iter().enumerate() {
            if index > 0 {
                payload.push(',');
            }
            payload.push_str(&format!(
                "{{\"tag_name\":{},\"tag_ordinal\":{},\"route\":{},\"mode\":{}}}",
                miniserde::json::to_string(record.tag.name),
                record.tag.ordinal,
                miniserde::json::to_string(record.route.name()),
                miniserde::json::to_string(record.mode.name()),
            ));
        }
        payload.push_str("]}\n");

        let parent = self.artifact.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "artifact path has no parent")
        })?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(payload.as_bytes())?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        temporary
            .persist_noclobber(&self.artifact)
            .map(|_| ())
            .map_err(|error| error.error)
    }

}

pub(crate) fn take_invocation_context(
    overlay: Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<(Option<InvocationContext>, Vec<(Vec<u8>, Vec<u8>)>), String> {
    let mut test_id = None;
    let mut output_dir = None;
    let mut script_overlay = Vec::with_capacity(overlay.len());
    for (name, value) in overlay {
        match name.as_slice() {
            INVOCATION_TEST_ID_OVERLAY => {
                if test_id.replace(value).is_some() {
                    return Err(
                        "route-witness invocation has duplicate test identities".to_string(),
                    );
                }
            }
            INVOCATION_OUTPUT_DIR_OVERLAY => {
                if output_dir.replace(value).is_some() {
                    return Err(
                        "route-witness invocation has duplicate output directories".to_string(),
                    );
                }
            }
            _ => script_overlay.push((name, value)),
        }
    }
    let context = match (test_id, output_dir) {
        (None, None) => None,
        (Some(_), None) | (None, Some(_)) => {
            return Err("route-witness invocation context is incomplete".to_string());
        }
        (Some(test_id), Some(output_dir)) => {
            let test_id = String::from_utf8(test_id)
                .map_err(|_| "route-witness invocation test identity is not UTF-8".to_string())?;
            if test_id.is_empty() {
                return Err("route-witness invocation test identity is empty".to_string());
            }
            let output_dir = PathBuf::from(OsString::from_vec(output_dir));
            if output_dir.as_os_str().as_bytes().is_empty()
                || output_dir.as_os_str().as_bytes().contains(&0)
            {
                return Err("route-witness invocation output directory is invalid".to_string());
            }
            Some(InvocationContext { test_id, output_dir })
        }
    };
    Ok((context, script_overlay))
}

fn duplicate_cloexec_fd(source_fd: impl AsFd) -> std::io::Result<OwnedFd> {
    rustix::io::fcntl_dupfd_cloexec(source_fd, FIRST_INHERITABLE_FD)
        .map_err(std::io::Error::from)
}

fn decode_transport(
    bytes: &[u8],
    expected_nonce: &[u8; TRANSPORT_NONCE_BYTES],
) -> Result<(String, PathBuf, String), String> {
    let header_len = TRANSPORT_MAGIC.len() + TRANSPORT_NONCE_BYTES + 8 + TRANSPORT_SOURCE_HASH_BYTES;
    if bytes.len() > MAX_TRANSPORT_BYTES {
        return Err("route-witness descriptor exceeds the transport limit".to_string());
    }
    if bytes.len() < header_len {
        return Err("route-witness descriptor is short".to_string());
    }
    if &bytes[..TRANSPORT_MAGIC.len()] != TRANSPORT_MAGIC {
        return Err("route-witness descriptor has an invalid header".to_string());
    }
    let nonce_start = TRANSPORT_MAGIC.len();
    let nonce_end = nonce_start + TRANSPORT_NONCE_BYTES;
    if &bytes[nonce_start..nonce_end] != expected_nonce {
        return Err("route-witness descriptor marker does not match its envelope".to_string());
    }
    let test_id_len_at = nonce_end;
    let output_dir_len_at = test_id_len_at + 4;
    let test_id_len = u32::from_be_bytes(
        bytes[test_id_len_at..test_id_len_at + 4]
            .try_into()
            .expect("fixed-width test ID length"),
    ) as usize;
    let output_dir_len = u32::from_be_bytes(
        bytes[output_dir_len_at..output_dir_len_at + 4]
            .try_into()
            .expect("fixed-width output path length"),
    ) as usize;
    let hash_start = output_dir_len_at + 4;
    let test_id_start = hash_start + TRANSPORT_SOURCE_HASH_BYTES;
    let expected_len = test_id_start
        .checked_add(test_id_len)
        .and_then(|length| length.checked_add(output_dir_len))
        .ok_or_else(|| "route-witness descriptor has invalid lengths".to_string())?;
    if expected_len > MAX_TRANSPORT_BYTES {
        return Err("route-witness descriptor exceeds the transport limit".to_string());
    }
    if bytes.len() < expected_len {
        return Err("route-witness descriptor is short".to_string());
    }
    if bytes.len() != expected_len {
        return Err("route-witness descriptor has trailing data".to_string());
    }

    let source_hash = std::str::from_utf8(&bytes[hash_start..test_id_start])
        .map_err(|_| "route-witness source hash is not UTF-8".to_string())?;
    if !is_lowercase_sha256(source_hash) {
        return Err("route-witness source hash is not lowercase SHA-256".to_string());
    }
    let test_id_end = test_id_start + test_id_len;
    let test_id = std::str::from_utf8(&bytes[test_id_start..test_id_end])
        .map_err(|_| "route-witness test identity is not UTF-8".to_string())?;
    if test_id.is_empty() || test_id.as_bytes().contains(&0) {
        return Err("route-witness test identity is invalid".to_string());
    }
    let output_dir_bytes = &bytes[test_id_end..];
    if output_dir_bytes.is_empty() || output_dir_bytes.contains(&0) {
        return Err("route-witness output directory is invalid".to_string());
    }
    Ok((
        test_id.to_string(),
        PathBuf::from(OsString::from_vec(output_dir_bytes.to_vec())),
        source_hash.to_string(),
    ))
}

pub(crate) fn validate_artifact(
    output_dir: &Path,
    test_id: &str,
    source: &[u8],
    forced: bool,
) -> Result<(), String> {
    #[derive(miniserde::Deserialize)]
    struct Artifact {
        version: u64,
        test_id: String,
        source_hash: String,
        records: Vec<Record>,
    }

    #[derive(miniserde::Deserialize)]
    struct Record {
        tag_name: String,
        tag_ordinal: u32,
        route: String,
        mode: String,
    }

    let path = output_dir.join(ARTIFACT_FILE);
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read route-witness artifact: {error}"))?;
    let raw: miniserde::json::Value = miniserde::json::from_str(&text)
        .map_err(|_| "route-witness artifact has invalid JSON schema".to_string())?;
    let has_fields = |value: &miniserde::json::Value, fields: &[&str]| match value {
        miniserde::json::Value::Object(object) => {
            object.len() == fields.len() && fields.iter().all(|field| object.contains_key(*field))
        }
        _ => false,
    };
    let records_have_fields = match &raw {
        miniserde::json::Value::Object(object) => match object.get("records") {
            Some(miniserde::json::Value::Array(records)) => records.iter().all(|record| {
                has_fields(record, &["tag_name", "tag_ordinal", "route", "mode"])
            }),
            _ => false,
        },
        _ => false,
    };
    if !has_fields(&raw, &["version", "test_id", "source_hash", "records"])
        || !records_have_fields
    {
        return Err("route-witness artifact has an unexpected object shape".to_string());
    }
    let artifact: Artifact = miniserde::json::from_str(&text)
        .map_err(|_| "route-witness artifact has invalid JSON schema".to_string())?;
    if artifact.version != ARTIFACT_VERSION {
        return Err("route-witness artifact version is unsupported".to_string());
    }
    if artifact.test_id != test_id {
        return Err("route-witness artifact has a stale test identity".to_string());
    }
    if artifact.source_hash != source_sha256(source) {
        return Err("route-witness artifact has a stale source hash".to_string());
    }
    if artifact.records.len() != 1 {
        return Err("route-witness artifact must contain one expression oracle record".to_string());
    }
    let expected_mode = if forced {
        RouteWitnessMode::Forced
    } else {
        RouteWitnessMode::Normal
    };
    let target_tag = route_oracle_tag(test_id)?;
    for record in &artifact.records {
        let tag = FullTag::from_identity(&record.tag_name, record.tag_ordinal).ok_or_else(|| {
            "route-witness artifact has an unknown or inconsistent FullTag identity".to_string()
        })?;
        let capability = tag.route_witness_capability().ok_or_else(|| {
            "route-witness artifact has a tag without expression route capability".to_string()
        })?;
        let route = RouteWitnessRoute::from_name(&record.route).ok_or_else(|| {
            "route-witness artifact has an unknown route".to_string()
        })?;
        let mode = RouteWitnessMode::from_name(&record.mode).ok_or_else(|| {
            "route-witness artifact has an unknown scheduler mode".to_string()
        })?;
        if mode != expected_mode {
            return Err("route-witness artifact has the wrong scheduler mode".to_string());
        }
        if !capability_accepts_route(capability, route, forced) {
            return Err(
                "route-witness artifact has an inconsistent route and scheduler mode".to_string(),
            );
        }
        if target_tag != tag {
            return Err("route-witness artifact does not match its expression oracle".to_string());
        }
    }
    Ok(())
}

pub(crate) fn artifact_path(output_dir: &Path) -> PathBuf {
    output_dir.join(ARTIFACT_FILE)
}

pub(crate) fn artifact_exists(output_dir: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(artifact_path(output_dir)) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("failed to inspect route-witness artifact: {error}")),
    }
}

#[cfg(test)]
pub(crate) fn write_active_artifact_for_test(source: &[u8], forced: bool) -> Result<(), String> {
    let capture = ACTIVE_TRANSPORTS.with(|transports| {
        let transports = transports.borrow();
        let Some(Some(transport)) = transports.last() else {
            return Err("route-witness transport is not active".to_string());
        };
        ChildCaptureContext::from_owned_descriptor(
            duplicate_cloexec_fd(transport.fd.as_fd())
                .map_err(|error| format!("failed to duplicate route-witness descriptor: {error}"))?,
            &transport.nonce,
        )?
        .validate_source(source)
    })?;
    capture
        .write_records(&[RouteWitnessRecord {
            tag: FullTag::ExprEmptyMap.into(),
            route: if forced {
                RouteWitnessRoute::GenericOperandApply
            } else {
                RouteWitnessRoute::ExplicitFrameApply
            },
            mode: if forced {
                RouteWitnessMode::Forced
            } else {
                RouteWitnessMode::Normal
            },
        }])
        .map_err(|error| format!("failed to write route-witness artifact: {error}"))
}

fn route_oracle_tag(test_id: &str) -> Result<FullTag, String> {
    let (_, test_name) = test_id
        .rsplit_once("::")
        .ok_or_else(|| "route-witness test identity has no test name".to_string())?;
    let suffix = test_name
        .strip_prefix("test_expr_")
        .or_else(|| (test_name == "test_frame_apply_empty_map").then_some("empty_map"))
        .ok_or_else(|| "route-witness test identity is not an expression oracle".to_string())?;
    FullTag::expression_test_tag(suffix)
        .ok_or_else(|| "route-witness test identity does not map to a FullTag".to_string())
}

pub(in crate::runtime::eval) fn recursive_route(
    tag: FullTag,
    in_place: bool,
) -> Option<RouteWitnessRoute> {
    match tag.route_witness_capability()? {
        RouteWitnessCapability::RecursiveApply | RouteWitnessCapability::RecursiveControl => {
            Some(if in_place {
                RouteWitnessRoute::InPlaceEvaluator
            } else {
                RouteWitnessRoute::RecursiveEvaluator
            })
        }
        RouteWitnessCapability::ScheduledApply | RouteWitnessCapability::ScheduledControl => None,
    }
}

pub(in crate::runtime::eval) fn frame_route(
    tag: FullTag,
    forced: bool,
) -> Option<RouteWitnessRoute> {
    match tag.route_witness_capability()? {
        RouteWitnessCapability::RecursiveApply | RouteWitnessCapability::ScheduledApply => {
            Some(if forced && tag.is_eager_atom() {
                RouteWitnessRoute::EagerFrameApply
            } else if forced {
                RouteWitnessRoute::GenericOperandApply
            } else {
                RouteWitnessRoute::ExplicitFrameApply
            })
        }
        RouteWitnessCapability::RecursiveControl | RouteWitnessCapability::ScheduledControl => {
            Some(RouteWitnessRoute::ExplicitFrameControl)
        }
    }
}

fn capability_accepts_route(
    capability: RouteWitnessCapability,
    route: RouteWitnessRoute,
    forced: bool,
) -> bool {
    match (capability, forced) {
        (
            RouteWitnessCapability::RecursiveApply | RouteWitnessCapability::ScheduledApply,
            true,
        ) => matches!(
            route,
            RouteWitnessRoute::GenericOperandApply | RouteWitnessRoute::EagerFrameApply
        ),
        (
            RouteWitnessCapability::RecursiveControl | RouteWitnessCapability::ScheduledControl,
            true,
        ) => {
            route == RouteWitnessRoute::ExplicitFrameControl
        }
        (RouteWitnessCapability::RecursiveApply, false) => matches!(
            route,
            RouteWitnessRoute::InPlaceEvaluator
                | RouteWitnessRoute::RecursiveEvaluator
                | RouteWitnessRoute::ExplicitFrameApply
        ),
        (RouteWitnessCapability::RecursiveControl, false) => matches!(
            route,
            RouteWitnessRoute::InPlaceEvaluator
                | RouteWitnessRoute::RecursiveEvaluator
                | RouteWitnessRoute::ExplicitFrameControl
        ),
        (RouteWitnessCapability::ScheduledApply, false) => {
            route == RouteWitnessRoute::ExplicitFrameApply
        }
        (RouteWitnessCapability::ScheduledControl, false) => {
            route == RouteWitnessRoute::ExplicitFrameControl
        }
    }
}

pub(crate) fn source_sha256(source: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    lowercase_hex(Sha256::digest(source).as_slice())
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX_DIGITS[(byte >> 4) as usize]));
        encoded.push(char::from(HEX_DIGITS[(byte & 0x0f) as usize]));
    }
    encoded
}

pub(crate) fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RouteWitnessMode {
    Normal,
    Forced,
}

impl RouteWitnessMode {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Forced => "forced",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "normal" => Some(Self::Normal),
            "forced" => Some(Self::Forced),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RouteWitnessRoute {
    InPlaceEvaluator,
    RecursiveEvaluator,
    ExplicitFrameApply,
    ExplicitFrameControl,
    GenericOperandApply,
    EagerFrameApply,
}

impl RouteWitnessRoute {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::InPlaceEvaluator => "in_place_evaluator_path",
            Self::RecursiveEvaluator => "recursive_evaluator_path",
            Self::ExplicitFrameApply => "explicit_frame_apply_path",
            Self::ExplicitFrameControl => "explicit_frame_control_path",
            Self::GenericOperandApply => "generic_operand_apply_path",
            Self::EagerFrameApply => "eager_frame_apply_path",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "in_place_evaluator_path" => Some(Self::InPlaceEvaluator),
            "recursive_evaluator_path" => Some(Self::RecursiveEvaluator),
            "explicit_frame_apply_path" => Some(Self::ExplicitFrameApply),
            "explicit_frame_control_path" => Some(Self::ExplicitFrameControl),
            "generic_operand_apply_path" => Some(Self::GenericOperandApply),
            "eager_frame_apply_path" => Some(Self::EagerFrameApply),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FullTagIdentity {
    pub(crate) name: &'static str,
    pub(crate) ordinal: u32,
}

impl From<FullTag> for FullTagIdentity {
    fn from(tag: FullTag) -> Self {
        Self {
            name: tag.name(),
            ordinal: tag as u32,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RouteWitnessRecord {
    pub(crate) tag: FullTagIdentity,
    pub(crate) route: RouteWitnessRoute,
    pub(crate) mode: RouteWitnessMode,
}

#[derive(Clone, Default)]
pub(crate) struct RouteWitnessSink {
    records: Arc<Mutex<Vec<RouteWitnessRecord>>>,
    tag_filter: Option<FullTag>,
}

impl RouteWitnessSink {
    fn for_tag(tag: FullTag) -> Self {
        Self {
            records: Arc::default(),
            tag_filter: Some(tag),
        }
    }

    pub(in crate::runtime::eval) fn record(
        &self,
        tag: FullTag,
        route: RouteWitnessRoute,
        mode: RouteWitnessMode,
    ) {
        if self.tag_filter.is_some_and(|tag_filter| tag_filter != tag) {
            return;
        }
        self.records
            .lock()
            .expect("route witness sink mutex poisoned")
            .push(RouteWitnessRecord {
                tag: tag.into(),
                route,
                mode,
            });
    }

    pub(crate) fn take_records(&self) -> Vec<RouteWitnessRecord> {
        std::mem::take(
            &mut *self
                .records
                .lock()
                .expect("route witness sink mutex poisoned"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::IntoRawFd;
    use std::os::unix::fs::MetadataExt;

    fn route_witness_tempdir() -> tempfile::TempDir {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        fs::create_dir_all(&scratch).expect("create repository scratch directory");
        tempfile::tempdir_in(scratch).expect("create route-witness fixture")
    }

    fn write_artifact(
        output_dir: &Path,
        test_id: &str,
        source: &[u8],
        records: &str,
    ) -> PathBuf {
        let path = artifact_path(output_dir);
        fs::write(
            &path,
            format!(
                "{{\"version\":1,\"test_id\":{},\"source_hash\":{},\"records\":[{}]}}\n",
                miniserde::json::to_string(test_id),
                miniserde::json::to_string(&source_sha256(source)),
                records,
            ),
        )
        .expect("write route-witness artifact");
        path
    }

    #[test]
    fn separate_evaluation_sinks_do_not_share_records() {
        let first = RouteWitnessSink::default();
        let second = RouteWitnessSink::default();

        first.record(
            FullTag::ExprEmptyMap,
            RouteWitnessRoute::ExplicitFrameApply,
            RouteWitnessMode::Normal,
        );

        assert_eq!(first.take_records().len(), 1);
        assert!(second.take_records().is_empty());
    }

    #[test]
    fn route_oracle_identity_resolves_schema_tags_without_guessing_frame_tests() {
        assert_eq!(
            route_oracle_tag(
                "tests/xsh/expression-route-oracles.xsh::test_expr_if_non_in_place_selection",
            )
            .expect("expression oracle maps to its schema tag"),
            FullTag::ExprIf,
        );
        assert_eq!(
            route_oracle_tag("tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map")
                .expect("empty-map frame oracle maps to its schema tag"),
            FullTag::ExprEmptyMap,
        );
        assert!(route_oracle_tag(
            "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_bytes_concat_keeps_element_order"
        )
        .is_err());
    }

    #[test]
    fn invocation_overlay_rejects_incomplete_duplicate_and_invalid_pairs() {
        let test_key = INVOCATION_TEST_ID_OVERLAY.to_vec();
        let output_key = INVOCATION_OUTPUT_DIR_OVERLAY.to_vec();
        assert_eq!(
            take_invocation_context(Vec::new()).expect("absent invocation is valid"),
            (None, Vec::new()),
        );
        assert!(take_invocation_context(vec![(test_key.clone(), b"tests/a.xsh::case".to_vec())])
            .is_err());
        assert!(take_invocation_context(vec![(output_key.clone(), b"/tmp/out".to_vec())])
            .is_err());

        let invalid_pairs = [
            (Vec::new(), b"/tmp/out".to_vec()),
            (vec![0xff], b"/tmp/out".to_vec()),
            (b"tests/a.xsh::case".to_vec(), Vec::new()),
            (b"tests/a.xsh::case".to_vec(), vec![b'/', 0, b't', b'm', b'p']),
        ];
        for (test_id, output_dir) in invalid_pairs {
            assert!(take_invocation_context(vec![
                (test_key.clone(), test_id),
                (output_key.clone(), output_dir),
            ])
            .is_err());
        }

        assert!(take_invocation_context(vec![
            (test_key.clone(), b"tests/a.xsh::case".to_vec()),
            (test_key, b"tests/b.xsh::case".to_vec()),
            (output_key, b"/tmp/out".to_vec()),
        ])
        .is_err());

        assert!(take_invocation_context(vec![
            (
                INVOCATION_TEST_ID_OVERLAY.to_vec(),
                b"tests/a.xsh::case".to_vec(),
            ),
            (INVOCATION_OUTPUT_DIR_OVERLAY.to_vec(), b"/tmp/one".to_vec()),
            (INVOCATION_OUTPUT_DIR_OVERLAY.to_vec(), b"/tmp/two".to_vec()),
        ])
        .is_err());
    }

    fn owned_descriptor(bytes: &[u8]) -> OwnedFd {
        let directory = route_witness_tempdir();
        let mut file = tempfile::tempfile_in(directory.path()).expect("create transport file");
        file.write_all(bytes).expect("write transport file");
        file.seek(SeekFrom::Start(0)).expect("rewind transport file");
        file.into()
    }

    fn descriptor_marker(fd: RawFd, nonce: &[u8; TRANSPORT_NONCE_BYTES]) -> String {
        format!("{fd}:{}", encode_nonce(nonce))
    }

    fn descriptor_is_same_file(fd: RawFd, expected: &std::fs::Metadata) -> bool {
        let path = Path::new("/dev/fd").join(fd.to_string());
        let Ok(actual) = std::fs::metadata(path) else {
            return false;
        };
        actual.dev() == expected.dev() && actual.ino() == expected.ino()
    }

    #[test]
    fn descriptor_transport_round_trips_and_checks_the_loaded_source() {
        let directory = route_witness_tempdir();
        let source = b"let empty: Map[Str, Int] = {}\n";
        let transport = NativeTestRouteWitnessTransport::new(
            "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map",
            directory.path(),
            source,
        )
        .expect("prepare descriptor");
        let fd = duplicate_cloexec_fd(transport.fd.as_fd())
            .expect("duplicate test transport");
        let raw_fd = fd.into_raw_fd();
        let marker = descriptor_marker(raw_fd, &transport.nonce);
        let (fd, nonce) = inherited_transport_descriptor(Some(std::ffi::OsStr::new(&marker)))
            .expect("authenticate descriptor")
            .expect("descriptor marker matches");
        let context = ChildCaptureContext::from_owned_descriptor(fd, &nonce)
            .expect("decode descriptor");
        assert!(context.validate_source(b"different source").is_err());

        let transport = NativeTestRouteWitnessTransport::new(
            "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map",
            directory.path(),
            source,
        )
        .expect("prepare second descriptor");
        let fd = duplicate_cloexec_fd(transport.fd.as_fd())
            .expect("duplicate second test transport");
        let raw_fd = fd.into_raw_fd();
        let marker = descriptor_marker(raw_fd, &transport.nonce);
        let (fd, nonce) = inherited_transport_descriptor(Some(std::ffi::OsStr::new(&marker)))
            .expect("authenticate second descriptor")
            .expect("descriptor marker matches");
        let capture = ChildCaptureContext::from_owned_descriptor(fd, &nonce)
            .expect("decode descriptor")
            .validate_source(source)
            .expect("validate source");
        assert_eq!(
            capture.test_id,
            "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map"
        );
        assert_eq!(capture.source_hash, source_sha256(source));
    }

    #[test]
    fn invalid_short_and_oversized_descriptors_fail_closed_and_close_the_fd() {
        let mut oversized = vec![b'x'; MAX_TRANSPORT_BYTES + 1];
        let header_len = TRANSPORT_MAGIC.len()
            + TRANSPORT_NONCE_BYTES
            + 8
            + TRANSPORT_SOURCE_HASH_BYTES;
        let mut short = vec![0; header_len - 1];
        let mut invalid_header = vec![0; header_len];
        invalid_header[..TRANSPORT_MAGIC.len()].copy_from_slice(b"BADMAGIC");
        let mut invalid_hash = Vec::new();
        invalid_hash.extend_from_slice(TRANSPORT_MAGIC);
        invalid_hash.extend_from_slice(&[0; TRANSPORT_NONCE_BYTES]);
        invalid_hash.extend_from_slice(&0u32.to_be_bytes());
        invalid_hash.extend_from_slice(&0u32.to_be_bytes());
        invalid_hash.extend_from_slice(&[b'A'; TRANSPORT_SOURCE_HASH_BYTES]);
        let mut trailing = Vec::new();
        trailing.extend_from_slice(TRANSPORT_MAGIC);
        trailing.extend_from_slice(&[0; TRANSPORT_NONCE_BYTES]);
        trailing.extend_from_slice(&0u32.to_be_bytes());
        trailing.extend_from_slice(&0u32.to_be_bytes());
        trailing.extend_from_slice(&[b'a'; TRANSPORT_SOURCE_HASH_BYTES]);
        trailing.push(0);
        oversized[0] = b'X';
        short[0] = b'X';

        for bytes in [oversized, short, invalid_header, invalid_hash, trailing] {
            let fd = owned_descriptor(&bytes);
            let raw = fd.as_raw_fd();
            let identity = std::fs::File::from(
                duplicate_cloexec_fd(fd.as_fd()).expect("keep transport identity available"),
            );
            let identity = identity.metadata().expect("inspect transport identity");
            assert!(ChildCaptureContext::from_owned_descriptor(fd, &[0; TRANSPORT_NONCE_BYTES]).is_err());
            assert!(
                !descriptor_is_same_file(raw, &identity),
                "malformed transport kept the original descriptor open",
            );
        }
    }

    #[test]
    fn invalid_unopened_descriptor_marker_returns_inspection_error() {
        let marker = descriptor_marker(RawFd::MAX, &[0; TRANSPORT_NONCE_BYTES]);
        let result = inherited_transport_descriptor(Some(std::ffi::OsStr::new(&marker)));
        assert!(matches!(
            result,
            Err(message) if message.starts_with("failed to inspect route-witness descriptor: ")
        ));
    }

    #[test]
    fn unmarked_inherited_fd_at_or_above_198_is_not_inspected_or_consumed() {
        let unrelated = owned_descriptor(b"ordinary inherited descriptor");
        let inherited = rustix::io::fcntl_dupfd_cloexec(unrelated.as_fd(), 198)
            .expect("duplicate an unrelated descriptor above the reserved range");
        let before_flags = rustix::io::fcntl_getfd(inherited.as_fd())
            .expect("inspect inherited descriptor flags");
        let before_offset = rustix::fs::tell(inherited.as_fd()).expect("inspect file offset");
        assert!(inherited_transport_descriptor(None)
            .expect("an absent marker is normal")
            .is_none());
        assert_eq!(
            rustix::io::fcntl_getfd(inherited.as_fd()).expect("inspect inherited descriptor flags"),
            before_flags,
        );
        assert_eq!(
            rustix::fs::tell(inherited.as_fd()).expect("inspect file offset"),
            before_offset,
        );
    }

    #[test]
    fn mismatched_descriptor_marker_leaves_an_unrelated_fd_untouched() {
        let fd = owned_descriptor(b"ordinary inherited descriptor");
        let raw = fd.as_raw_fd();
        let before = rustix::fs::tell(fd.as_fd()).expect("inspect file offset");
        let marker = descriptor_marker(raw, &[0xff; TRANSPORT_NONCE_BYTES]);
        assert!(inherited_transport_descriptor(Some(std::ffi::OsStr::new(&marker)))
            .expect("a marker mismatch is unrelated")
            .is_none());
        rustix::io::fcntl_getfd(fd.as_fd()).expect("unrelated descriptor remains open");
        assert_eq!(rustix::fs::tell(fd.as_fd()).expect("inspect file offset"), before);
    }

    #[test]
    fn artifact_round_trip_preserves_identity_tag_route_and_mode() {
        let test_id = "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map";
        let source = b"let empty: Map[Str, Int] = {}\nassert empty.len() == 0\n";
        for (forced, route, mode) in [
            (
                false,
                RouteWitnessRoute::ExplicitFrameApply,
                RouteWitnessMode::Normal,
            ),
            (
                true,
                RouteWitnessRoute::GenericOperandApply,
                RouteWitnessMode::Forced,
            ),
        ] {
            let directory = route_witness_tempdir();
            let capture = ChildCapture {
                artifact: directory.path().join(ARTIFACT_FILE),
                test_id: test_id.to_string(),
                source_hash: source_sha256(source),
                sink: RouteWitnessSink::default(),
            };
            capture
                .write_records(&[RouteWitnessRecord {
                    tag: FullTag::ExprEmptyMap.into(),
                    route,
                    mode,
                }])
                .expect("write witness artifact");

            validate_artifact(directory.path(), test_id, source, forced)
                .expect("validate witness artifact");
            assert!(validate_artifact(directory.path(), "tests/a.xsh::case", source, forced)
                .is_err());
            assert!(validate_artifact(directory.path(), test_id, b"stale", forced).is_err());
        }
    }

    #[test]
    fn artifact_validation_rejects_malformed_and_wrong_mode_records() {
        let directory = route_witness_tempdir();
        let artifact = directory.path().join(ARTIFACT_FILE);
        let test_id = "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map";
        let source = b"assert true\n";
        fs::write(&artifact, b"not JSON").expect("write malformed artifact");
        assert!(validate_artifact(directory.path(), test_id, source, false).is_err());
        let extra_field = format!(
            "{{\"version\":1,\"test_id\":\"{test_id}\",\"source_hash\":\"{}\",\"records\":[],\"extra\":true}}",
            source_sha256(source)
        );
        fs::write(&artifact, extra_field).expect("write artifact with extra field");
        assert!(validate_artifact(directory.path(), test_id, source, false).is_err());

        let wrong_mode_dir = route_witness_tempdir();
        let capture = ChildCapture {
            artifact: wrong_mode_dir.path().join(ARTIFACT_FILE),
            test_id: test_id.to_string(),
            source_hash: source_sha256(source),
            sink: RouteWitnessSink::default(),
        };
        capture
            .write_records(&[RouteWitnessRecord {
                tag: FullTag::ExprEmptyMap.into(),
                route: RouteWitnessRoute::ExplicitFrameApply,
                mode: RouteWitnessMode::Forced,
            }])
            .expect("write valid artifact");
        assert!(validate_artifact(wrong_mode_dir.path(), test_id, source, false).is_err());
    }

    #[test]
    fn target_artifact_rejects_mixed_inconsistent_and_extra_records() {
        let directory = route_witness_tempdir();
        let test_id = "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map";
        let source = b"let empty: Map[Str, Int] = {}\nassert empty.len() == 0\n";
        let normal = r#"{
            "tag_name":"ExprEmptyMap",
            "tag_ordinal":49,
            "route":"explicit_frame_apply_path",
            "mode":"normal"
        }"#;
        let forced = r#"{
            "tag_name":"ExprEmptyMap",
            "tag_ordinal":49,
            "route":"generic_operand_apply_path",
            "mode":"forced"
        }"#;
        let inconsistent = r#"{
            "tag_name":"ExprEmptyMap",
            "tag_ordinal":49,
            "route":"generic_operand_apply_path",
            "mode":"normal"
        }"#;

        for records in [
            format!("{normal},{forced}"),
            format!("{normal},{inconsistent}"),
            format!("{normal},{normal}"),
        ] {
            write_artifact(directory.path(), test_id, source, &records);
            assert!(validate_artifact(directory.path(), test_id, source, false).is_err());
        }
    }

    #[test]
    fn artifact_validation_rejects_unknown_and_inconsistent_tag_identities() {
        let directory = route_witness_tempdir();
        let test_id = "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map";
        let source = b"assert true\n";
        let normal = r#"{
            "tag_name":"ExprEmptyMap",
            "tag_ordinal":49,
            "route":"explicit_frame_apply_path",
            "mode":"normal"
        }"#;
        let unknown = r#"{
            "tag_name":"UnknownTag",
            "tag_ordinal":49,
            "route":"explicit_frame_apply_path",
            "mode":"normal"
        }"#;
        let wrong_ordinal = r#"{
            "tag_name":"ExprEmptyMap",
            "tag_ordinal":50,
            "route":"explicit_frame_apply_path",
            "mode":"normal"
        }"#;
        let known_but_unobserved = r#"{
            "tag_name":"StmtReturn",
            "tag_ordinal":150,
            "route":"explicit_frame_apply_path",
            "mode":"normal"
        }"#;

        for records in [
            format!("{normal},{unknown}"),
            format!("{normal},{wrong_ordinal}"),
            format!("{normal},{known_but_unobserved}"),
        ] {
            write_artifact(directory.path(), test_id, source, &records);
            assert!(validate_artifact(directory.path(), test_id, source, false).is_err());
        }
    }
}
