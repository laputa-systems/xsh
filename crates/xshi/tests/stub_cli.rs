#![cfg(not(feature = "laputa-asterinas"))]

use std::process::Command;

#[test]
fn disabled_frontend_explains_help_and_rejects_execution() {
    for flag in ["-h", "--help"] {
        let output = Command::new(env!("CARGO_BIN_EXE_xshi")).arg(flag).output().unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stderr).unwrap();
        assert!(help.contains("disabled"));
        assert!(help.contains("laputa-asterinas"));
        assert!(output.stdout.is_empty());
    }
    for args in [&[][..], &["-c", "--help"][..]] {
        let output = Command::new(env!("CARGO_BIN_EXE_xshi")).args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8(output.stderr).unwrap().contains("disabled"));
    }
}
