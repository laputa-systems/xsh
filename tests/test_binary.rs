//! Process tests require optimized products. Cargo builds binary targets in
//! the test's own profile, so reject every profile except verification and
//! release. Each test crate includes this file at its root with `#[macro_use]`.

/// Cargo's binary target path, checked to use an accepted test profile.
macro_rules! test_bin {
    ($name:literal) => {
        crate::test_binary::checked(env!(concat!("CARGO_BIN_EXE_", $name)))
    };
}

/// Rejects profiles that cannot provide the process-test execution contract.
pub(crate) fn checked(path: &'static str) -> &'static str {
    let profile = std::path::Path::new(path)
        .parent()
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str);
    assert!(
        matches!(profile, Some("verification" | "release")),
        "process tests require verification or release binaries, but {path} has an unsupported profile; run `cargo test --profile verification`"
    );
    path
}

#[cfg(test)]
mod tests {
    #[test]
    fn optimized_product_paths_are_accepted() {
        for path in ["/target/verification/xsh", "/target/aarch64-unknown-linux-musl/release/xsht"] {
            assert_eq!(super::checked(path), path);
        }
    }

    #[test]
    fn other_profiles_are_rejected() {
        for path in ["/target/debug/xsh", "/target/dist/xsh", "/target/custom/xsh", "xsh"] {
            assert!(std::panic::catch_unwind(|| super::checked(path)).is_err(), "accepted {path}");
        }
    }
}
