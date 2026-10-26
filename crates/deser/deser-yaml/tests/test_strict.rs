use std::collections::BTreeMap;
use deser_yaml::{DeserializerConfig, Version};

#[test]
fn required_yaml_12_rejects_other_declared_versions() {
    let config = DeserializerConfig::builder().version(Version::V1_2).required_version(Some(Version::V1_2)).build();
    assert!(config.from_str::<BTreeMap<String, bool>>("%YAML 1.1\n---\nflag: yes\n").is_err());
}

#[test]
fn strict_merges_reject_keys_already_present() {
    let config = DeserializerConfig::builder().reject_merge_conflicts(true).build();
    let input = "base: &base {x: one}\ntarget: {<<: *base, x: two}\n";
    assert!(config.from_str::<BTreeMap<String, BTreeMap<String, String>>>(input).is_err());
}


#[test]
fn strict_yaml_12_accepts_ordinary_and_explicit_12_documents() {
    let config = DeserializerConfig::builder().version(Version::V1_2)
        .required_version(Some(Version::V1_2)).build();
    for input in ["flag: true\n", "%YAML 1.2\n---\nflag: true\n"] {
        assert_eq!(config.from_str::<BTreeMap<String, bool>>(input).unwrap()["flag"], true);
    }
}

#[test]
fn strict_merges_accept_disjoint_sources_and_reject_duplicate_sources() {
    let config = DeserializerConfig::builder().reject_merge_conflicts(true).build();
    let disjoint = "a: &a {x: one}\nb: &b {y: two}\ntarget: {<<: [*a, *b]}\n";
    let value = config.from_str::<BTreeMap<String, BTreeMap<String, String>>>(disjoint).unwrap();
    assert_eq!(value["target"], BTreeMap::from([("x".to_string(), "one".to_string()), ("y".to_string(), "two".to_string())]));
    let duplicate = "a: &a {x: one}\nb: &b {x: two}\ntarget: {<<: [*a, *b]}\n";
    assert_eq!(config.from_str::<BTreeMap<String, BTreeMap<String, String>>>(duplicate).unwrap_err().kind(), deser::ErrorKind::DuplicateKey);
}

#[test]
fn default_yaml_retains_declared_version_and_merge_precedence() {
    let config = DeserializerConfig::new();
    assert_eq!(config.from_str::<BTreeMap<String, bool>>("%YAML 1.1\n---\nflag: yes\n").unwrap()["flag"], true);
    let input = "base: &base {x: one}\ntarget: {<<: *base, x: two}\n";
    assert_eq!(config.from_str::<BTreeMap<String, BTreeMap<String, String>>>(input).unwrap()["target"]["x"], "two");
}


#[test]
fn strict_merges_reject_repeated_merge_keys_with_disjoint_values() {
    let input = "target: {<<: {a: one}, <<: {b: two}}\n";
    let config = DeserializerConfig::builder().reject_merge_conflicts(true).build();
    assert_eq!(config.from_str::<BTreeMap<String, BTreeMap<String, String>>>(input).unwrap_err().kind(), deser::ErrorKind::DuplicateKey);
}
