//! Tests spawn release builds of the workspace binaries only. Cargo builds a
//! test's binary targets in the test's own profile, so a debug test build would
//! exercise debug binaries; such a run fails here with the release command
//! instead. Each test crate includes this file at its root with `#[macro_use]`.

/// Path of the binary target `$name` (Cargo's `CARGO_BIN_EXE_<name>`), checked
/// to be a release build.
macro_rules! release_bin {
    ($name:literal) => {
        crate::release_binary::checked(env!(concat!("CARGO_BIN_EXE_", $name)))
    };
}

/// Returns `path` when it lies outside Cargo's `debug` profile directory and
/// panics with the command that builds and runs the release tests otherwise.
pub(crate) fn checked(path: &'static str) -> &'static str {
    let profile = std::path::Path::new(path).parent().and_then(std::path::Path::file_name);
    assert!(
        profile.is_some_and(|profile| profile != "debug"),
        "tests run release binaries only, but {path} is a debug build; run the tests with `cargo test --release`"
    );
    path
}
