const X86_64: &[&str] = &["aes", "pclmulqdq", "bmi1", "bmi2", "adx", "popcnt", "avx", "avx2"];
const AARCH64: &[&str] = &["neon", "aes", "pmull", "sha2"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissingFeature(pub &'static str);

impl std::fmt::Display for MissingFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Graviola requires CPU feature {}", self.0)
    }
}

impl std::error::Error for MissingFeature {}

pub fn first_missing(required: &[&'static str], has: impl Fn(&str) -> bool) -> Option<&'static str> {
    required.iter().copied().find(|feature| !has(feature))
}

pub fn require() -> Result<(), MissingFeature> {
    let required = if cfg!(target_arch = "x86_64") { X86_64 } else if cfg!(target_arch = "aarch64") { AARCH64 } else {
        return Err(MissingFeature("a supported processor architecture"));
    };
    first_missing(required, detected).map_or(Ok(()), |feature| Err(MissingFeature(feature)))
}

#[cfg(target_arch = "x86_64")]
fn detected(feature: &str) -> bool {
    match feature {
        "aes" => std::arch::is_x86_feature_detected!("aes"),
        "pclmulqdq" => std::arch::is_x86_feature_detected!("pclmulqdq"),
        "bmi1" => std::arch::is_x86_feature_detected!("bmi1"),
        "bmi2" => std::arch::is_x86_feature_detected!("bmi2"),
        "adx" => std::arch::is_x86_feature_detected!("adx"),
        "popcnt" => std::arch::is_x86_feature_detected!("popcnt"),
        "avx" => std::arch::is_x86_feature_detected!("avx"),
        "avx2" => std::arch::is_x86_feature_detected!("avx2"),
        _ => false,
    }
}

#[cfg(target_arch = "aarch64")]
fn detected(feature: &str) -> bool {
    match feature {
        "neon" => std::arch::is_aarch64_feature_detected!("neon"),
        "aes" => std::arch::is_aarch64_feature_detected!("aes"),
        "pmull" => std::arch::is_aarch64_feature_detected!("pmull"),
        "sha2" => std::arch::is_aarch64_feature_detected!("sha2"),
        _ => false,
    }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn detected(_: &str) -> bool { false }

#[cfg(test)]
mod tests {
    use super::{AARCH64, X86_64, first_missing};

    #[test]
    fn reports_first_missing_feature() {
        assert_eq!(first_missing(X86_64, |_| true), None);
        assert_eq!(first_missing(X86_64, |_| false), Some("aes"));
        assert_eq!(first_missing(X86_64, |feature| feature != "bmi2" && feature != "avx"), Some("bmi2"));
        assert_eq!(first_missing(&[], |_| false), None);
    }

    #[test]
    fn lists_match_vendored_ssh_gate() {
        let source = include_str!("../../oxish-client/src/cpu.rs");
        for (arch, expected) in [("x86_64", X86_64), ("aarch64", AARCH64)] {
            let section = source.split(&format!("#[cfg(target_arch = \"{arch}\")]\nconst REQUIRED:")).nth(1).unwrap();
            let list = section.split_once("= &[").unwrap().1.split_once("];").unwrap().0;
            let parsed: Vec<_> = list.split(',').map(|feature| feature.trim().trim_matches('"')).filter(|feature| !feature.is_empty()).collect();
            assert_eq!(parsed, expected);
        }
    }
}
