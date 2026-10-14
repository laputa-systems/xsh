//! Private, opt-in observations of the runtime route selected for an instruction.
//!
//! A capture belongs to one evaluation and may be cloned only into evaluators
//! created for that evaluation. The collector's transport envelope must keep
//! `test_id` and `source_hash` as separate optional context fields, and all
//! records in one envelope must come from the single invocation that installed
//! the capture. Dispatch records carry neither value: the empty-map
//! instruction has no source span, and the dispatch boundary has no stable
//! test identity. `test_id` is the harness identity of that exact invocation.
//! `source_hash` is lowercase SHA-256 over its exact UTF-8 source input bytes,
//! without normalization. A collector may add either value only from the
//! exact enclosing invocation. If it cannot prove that association, it must
//! serialize the field as absent; tag names, instruction ordinals, source
//! paths, and cancellation handles are not substitutes.

use super::indexed::full::FullTag;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(crate) const ENV_PREFIX: &str = "XSH_INTERNAL_TEST_FORCE_FRAMES_ROUTE_WITNESS";
pub(crate) const HARNESS_ENV_PREFIX: &str = "XSHT_INTERNAL_ROUTE_WITNESS";

pub(crate) const INVOCATION_TEST_ID_OVERLAY: &[u8] =
    b"XSH_INTERNAL_TEST_FORCE_FRAMES_ROUTE_WITNESS_INVOCATION_TEST_ID";
pub(crate) const INVOCATION_OUTPUT_DIR_OVERLAY: &[u8] =
    b"XSH_INTERNAL_TEST_FORCE_FRAMES_ROUTE_WITNESS_INVOCATION_OUTPUT_DIR";

const TRANSPORT_TEST_ID: &str = "XSH_INTERNAL_TEST_FORCE_FRAMES_ROUTE_WITNESS_TRANSPORT_TEST_ID";
const TRANSPORT_OUTPUT_DIR: &str =
    "XSH_INTERNAL_TEST_FORCE_FRAMES_ROUTE_WITNESS_TRANSPORT_OUTPUT_DIR";
const TRANSPORT_SOURCE_HASH: &str =
    "XSH_INTERNAL_TEST_FORCE_FRAMES_ROUTE_WITNESS_TRANSPORT_SOURCE_HASH";

const ARTIFACT_FILE: &str = "route-witness.json";
const ARTIFACT_VERSION: u64 = 1;

/// The exact outer native-test invocation and its private output directory.
/// This value travels only through harness-owned evaluator state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InvocationContext {
    pub(crate) test_id: String,
    pub(crate) output_dir: PathBuf,
}

/// A child-side capture is constructed from one complete host-provided
/// envelope and verified against the exact source bytes loaded by the runner.
pub(crate) struct ChildCapture {
    artifact: PathBuf,
    test_id: String,
    source_hash: String,
    sink: RouteWitnessSink,
}

impl ChildCapture {
    pub(crate) fn from_environment(source: &[u8]) -> Result<Option<Self>, String> {
        Self::from_values(
            source,
            std::env::var_os(TRANSPORT_TEST_ID),
            std::env::var_os(TRANSPORT_OUTPUT_DIR),
            std::env::var_os(TRANSPORT_SOURCE_HASH),
        )
    }

    fn from_values(
        source: &[u8],
        test_id: Option<OsString>,
        output_dir: Option<OsString>,
        source_hash: Option<OsString>,
    ) -> Result<Option<Self>, String> {
        let (test_id, output_dir, source_hash) = match (test_id, output_dir, source_hash) {
            (None, None, None) => return Ok(None),
            (Some(test_id), Some(output_dir), Some(source_hash)) => {
                (test_id, output_dir, source_hash)
            }
            _ => return Err("route-witness child context is incomplete".to_string()),
        };
        let test_id = test_id
            .into_string()
            .map_err(|_| "route-witness test identity is not UTF-8".to_string())?;
        let source_hash = source_hash
            .into_string()
            .map_err(|_| "route-witness source hash is not UTF-8".to_string())?;
        if test_id.is_empty() {
            return Err("route-witness test identity is empty".to_string());
        }
        if !is_lowercase_sha256(&source_hash) {
            return Err("route-witness source hash is not lowercase SHA-256".to_string());
        }
        let actual_hash = source_sha256(source);
        if source_hash != actual_hash {
            return Err("route-witness source hash does not match the loaded script".to_string());
        }
        let output_dir = PathBuf::from(output_dir);
        if !output_dir.is_dir() {
            return Err("route-witness output directory is unavailable".to_string());
        }
        Ok(Some(Self {
            artifact: output_dir.join(ARTIFACT_FILE),
            test_id,
            source_hash,
            sink: RouteWitnessSink::default(),
        }))
    }

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

/// Removes harness-only invocation keys from the script overlay and rejects
/// any partial or malformed context instead of silently disabling capture.
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

/// Adds transport-only values after rejecting any script-supplied attempt to
/// use one of the private route-witness names.
pub(crate) fn transport_env(
    test_id: &str,
    output_dir: &Path,
    source: &[u8],
    env: &mut std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<(), &'static str> {
    reject_reserved_env(env)?;
    env.insert(TRANSPORT_TEST_ID.to_string(), test_id.as_bytes().to_vec());
    env.insert(
        TRANSPORT_OUTPUT_DIR.to_string(),
        output_dir.as_os_str().as_bytes().to_vec(),
    );
    env.insert(
        TRANSPORT_SOURCE_HASH.to_string(),
        source_sha256(source).into_bytes(),
    );
    Ok(())
}

pub(crate) fn reject_reserved_env(
    env: &std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<(), &'static str> {
    if env.keys().any(is_private_env_name) {
        Err("native-test env uses a reserved route-witness name")
    } else {
        Ok(())
    }
}

/// Validates the artifact after the child exits successfully, while the
/// caller still owns the per-child temporary directory.
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
    if artifact.records.is_empty() {
        return Err("route-witness artifact has no route records".to_string());
    }
    let expected_mode = if forced { "forced" } else { "normal" };
    for record in &artifact.records {
        if !FullTag::matches_identity(&record.tag_name, record.tag_ordinal) {
            return Err(
                "route-witness artifact has an unknown or inconsistent FullTag identity"
                    .to_string(),
            );
        }
        if record.tag_name != "ExprEmptyMap" || record.tag_ordinal != 49 {
            return Err("route-witness artifact has an unsupported FullTag record".to_string());
        }
        if !matches!(
            (record.route.as_str(), record.mode.as_str()),
            ("recursive_bridge_to_explicit_frames", "normal")
                | ("generic_operand_apply_path", "forced")
        ) {
            return Err(
                "route-witness artifact has an inconsistent route and scheduler mode".to_string(),
            );
        }
        if record.mode != expected_mode {
            return Err("route-witness artifact has the wrong scheduler mode".to_string());
        }
    }
    if test_id == "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map" {
        let expected_route = if forced {
            "generic_operand_apply_path"
        } else {
            "recursive_bridge_to_explicit_frames"
        };
        let exact_evidence = artifact.records.len() == 1
            && artifact.records.first().is_some_and(|record| {
                record.tag_name == "ExprEmptyMap"
                    && record.tag_ordinal == 49
                    && record.route == expected_route
                    && record.mode == expected_mode
            });
        if !exact_evidence {
            return Err(
                "route-witness artifact does not contain exactly the expected empty-map route"
                    .to_string(),
            );
        }
    }
    Ok(())
}

/// Hashes the exact UTF-8 source bytes transported to the child.
pub(crate) fn source_sha256(source: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(source))
}

pub(crate) fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn is_private_env_name(name: &str) -> bool {
    is_private_env_name_bytes(name.as_bytes())
}

pub(crate) fn is_private_env_name_bytes(name: &[u8]) -> bool {
    name.starts_with(ENV_PREFIX.as_bytes()) || name.starts_with(HARNESS_ENV_PREFIX.as_bytes())
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RouteWitnessRoute {
    RecursiveBridgeToExplicitFrames,
    GenericOperandApplyPath,
}

impl RouteWitnessRoute {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::RecursiveBridgeToExplicitFrames => "recursive_bridge_to_explicit_frames",
            Self::GenericOperandApplyPath => "generic_operand_apply_path",
        }
    }
}

/// The schema identity of a dispatched FullTag.
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

/// One observation at the boundary where the evaluator chooses its route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RouteWitnessRecord {
    pub(crate) tag: FullTagIdentity,
    pub(crate) route: RouteWitnessRoute,
    pub(crate) mode: RouteWitnessMode,
}

/// A per-evaluation sink. It has no ambient state and is cloned only when the
/// evaluation creates a worker evaluator. A collector creates one per test
/// invocation, drains it after workers finish, and then drops it.
#[derive(Clone, Default)]
pub(crate) struct RouteWitnessSink {
    records: Arc<Mutex<Vec<RouteWitnessRecord>>>,
}

impl RouteWitnessSink {
    pub(crate) fn record(
        &self,
        tag: FullTag,
        route: RouteWitnessRoute,
        mode: RouteWitnessMode,
    ) {
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
        let path = output_dir.join(ARTIFACT_FILE);
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
            RouteWitnessRoute::RecursiveBridgeToExplicitFrames,
            RouteWitnessMode::Normal,
        );

        assert_eq!(first.take_records().len(), 1);
        assert!(second.take_records().is_empty());
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

    #[test]
    fn missing_or_incomplete_transport_never_forges_child_context() {
        assert!(ChildCapture::from_values(b"source", None, None, None)
            .expect("absent transport is valid")
            .is_none());

        let directory = route_witness_tempdir();
        let incomplete = ChildCapture::from_values(
            b"source",
            Some("tests/a.xsh::case".into()),
            Some(directory.path().as_os_str().to_os_string()),
            None,
        );
        assert!(incomplete.is_err());
    }

    #[test]
    fn child_transport_rejects_stale_source_and_malformed_hashes() {
        let directory = route_witness_tempdir();
        let valid_id = Some(OsString::from("tests/a.xsh::case"));
        let valid_directory = Some(directory.path().as_os_str().to_os_string());
        let stale = ChildCapture::from_values(
            b"new source",
            valid_id.clone(),
            valid_directory.clone(),
            Some(source_sha256(b"old source").into()),
        );
        assert!(stale.is_err());

        let malformed = ChildCapture::from_values(
            b"source",
            valid_id,
            valid_directory,
            Some("AABB".into()),
        );
        assert!(malformed.is_err());
    }

    #[test]
    fn artifact_round_trip_preserves_identity_tag_route_and_mode() {
        let test_id = "tests/xsh/frame-apply-oracles.xsh::test_frame_apply_empty_map";
        let source = b"let empty: Map[Str, Int] = {}\nassert empty.len() == 0\n";
        for (forced, route, mode) in [
            (
                false,
                RouteWitnessRoute::RecursiveBridgeToExplicitFrames,
                RouteWitnessMode::Normal,
            ),
            (
                true,
                RouteWitnessRoute::GenericOperandApplyPath,
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
        let test_id = "tests/a.xsh::case";
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
                route: RouteWitnessRoute::RecursiveBridgeToExplicitFrames,
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
            "route":"recursive_bridge_to_explicit_frames",
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
        let test_id = "tests/a.xsh::case";
        let source = b"assert true\n";
        let normal = r#"{
            "tag_name":"ExprEmptyMap",
            "tag_ordinal":49,
            "route":"recursive_bridge_to_explicit_frames",
            "mode":"normal"
        }"#;
        let unknown = r#"{
            "tag_name":"UnknownTag",
            "tag_ordinal":49,
            "route":"recursive_bridge_to_explicit_frames",
            "mode":"normal"
        }"#;
        let wrong_ordinal = r#"{
            "tag_name":"ExprEmptyMap",
            "tag_ordinal":50,
            "route":"recursive_bridge_to_explicit_frames",
            "mode":"normal"
        }"#;
        let known_but_unobserved = r#"{
            "tag_name":"StmtReturn",
            "tag_ordinal":150,
            "route":"recursive_bridge_to_explicit_frames",
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
