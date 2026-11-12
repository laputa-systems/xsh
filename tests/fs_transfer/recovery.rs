use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fs::Mode;

use xsh_root::transfer::{
    Ancestor, Boundary, EntryIdentity, Removal, Retirement, Session, observe_entry, observe_fd,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        fs::create_dir_all(&base).expect("create shared test scratch");
        let path = base.join(format!(
            "journal-recovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create recovery fixture");
        Self(path)
    }

    fn root(&self) -> File {
        File::open(&self.0).expect("open recovery root")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn identity_of(path: &Path) -> EntryIdentity {
    let parent = path.parent().expect("fixture path has a parent");
    let name = path.file_name().expect("fixture path has a final name");
    let directory = File::open(parent).expect("open fixture parent");
    observe_entry(directory.as_fd(), name)
        .expect("observe fixture entry")
        .identity
}

fn start_crash_child(root: &Path, boundary: Boundary, restrictive_umask: bool) -> Child {
    let mut command = Command::new(std::env::current_exe().expect("current test binary"));
    command
        .args([
            "--exact",
            "recovery::crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env("XSH_JOURNAL_CRASH_ROOT", root)
        .env("XSH_JOURNAL_CRASH_BOUNDARY", format!("{boundary:?}"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if restrictive_umask {
        command.env("XSH_JOURNAL_RESTRICTIVE_UMASK", "1");
    }
    command.spawn().expect("spawn journal child")
}

fn wait_ready(child: &mut Child, boundary: Boundary) {
    let stdout = child.stdout.take().expect("child stdout");
    let mut output = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        assert_ne!(
            output.read_line(&mut line).expect("read child stdout"),
            0,
            "child ended before {boundary:?}"
        );
        if line.trim() == "ready" {
            return;
        }
        line.clear();
    }
}

fn recover(root: &Path) -> std::io::Result<()> {
    let root = File::open(root).expect("open recovery root");
    Session::check_recovery(root.as_fd())
}

#[test]
#[ignore = "subprocess fixture for journal interruption"]
fn crash_child() {
    let Some(root) = std::env::var_os("XSH_JOURNAL_CRASH_ROOT") else {
        return;
    };
    if std::env::var_os("XSH_JOURNAL_RESTRICTIVE_UMASK").is_some() {
        rustix::process::umask(Mode::RWXU | Mode::RWXG | Mode::RWXO);
    }
    let boundary = std::env::var("XSH_JOURNAL_CRASH_BOUNDARY").expect("crash boundary");
    let root = PathBuf::from(root);
    let root_file = File::open(&root).expect("open crash root");
    let session = Session::start(root_file.as_fd()).expect("start crash session");
    let expected = identity_of(&root.join("source"));
    let request = Retirement::new(Vec::new(), "source".into(), expected).expect("build retirement");
    session
        .remove(root_file.as_fd(), request, |point, _, _| {
            if format!("{point:?}") == boundary {
                println!("ready");
                std::io::stdout().flush().expect("flush readiness");
                loop {
                    std::thread::park();
                }
            }
            Ok(())
        })
        .expect("retirement completed unexpectedly");
    panic!("crash boundary was not reached");
}

#[test]
#[ignore = "subprocess fixture for batch journal interruption"]
fn crash_batch_child() {
    let Some(root) = std::env::var_os("XSH_JOURNAL_CRASH_ROOT") else {
        return;
    };
    let boundary = std::env::var("XSH_JOURNAL_CRASH_BOUNDARY").expect("crash boundary");
    let root = PathBuf::from(root);
    let root_file = File::open(&root).expect("open crash root");
    let session = Session::start(root_file.as_fd()).expect("start crash session");
    let requests = ["source-a", "source-b", "source-c"]
        .into_iter()
        .map(|name| {
            Retirement::new(Vec::new(), name.into(), identity_of(&root.join(name)))
                .expect("build batch retirement")
        })
        .collect();
    let results = session.remove_batch(root_file.as_fd(), requests, |point| {
        if format!("{point:?}") == boundary {
            println!("ready");
            std::io::stdout().flush().expect("flush readiness");
            loop {
                std::thread::park();
            }
        }
        Ok(())
    });
    drop(results);
    panic!("crash boundary was not reached");
}

#[test]
fn killed_batch_restores_every_unlinked_capture() {
    for boundary in [
        Boundary::IntentPublished,
        Boundary::BeforeCapture,
        Boundary::BeforeUnlink,
    ] {
        let fixture = Fixture::new();
        for name in ["source-a", "source-b", "source-c"] {
            fs::write(fixture.0.join(name), name.as_bytes()).expect("write batch source");
        }
        fs::write(fixture.0.join("untouched"), b"kept").expect("write untouched entry");
        let mut child = Command::new(std::env::current_exe().expect("current test binary"))
            .args([
                "--exact",
                "recovery::crash_batch_child",
                "--ignored",
                "--nocapture",
            ])
            .env("XSH_JOURNAL_CRASH_ROOT", &fixture.0)
            .env("XSH_JOURNAL_CRASH_BOUNDARY", format!("{boundary:?}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn batch journal child");
        wait_ready(&mut child, boundary);
        child.kill().expect("kill batch journal child");
        assert!(!child.wait().expect("reap batch journal child").success());
        let error = recover(&fixture.0).expect_err("recovery must report the interruption");
        assert!(
            error
                .to_string()
                .contains("interrupted filesystem operation state recovered"),
            "{boundary:?}: {error}"
        );
        for name in ["source-a", "source-b", "source-c"] {
            assert_eq!(
                fs::read(fixture.0.join(name)).expect("read restored batch source"),
                name.as_bytes(),
                "{boundary:?}"
            );
        }
        assert_eq!(
            fs::read(fixture.0.join("untouched")).expect("read untouched entry"),
            b"kept"
        );
        assert_eq!(
            fs::read_dir(&fixture.0).expect("read recovery root").count(),
            4,
            "{boundary:?}"
        );
        recover(&fixture.0).expect("a second recovery is a no-op");
    }
}

#[test]
fn batch_retirement_removes_entries_and_reports_absent_and_changed_names() {
    let fixture = Fixture::new();
    for name in ["first", "second", "changed"] {
        fs::write(fixture.0.join(name), name.as_bytes()).expect("write batch entry");
    }
    let root = fixture.root();
    let session = Session::start(root.as_fd()).expect("start session");
    let requests = vec![
        Retirement::new(Vec::new(), "first".into(), identity_of(&fixture.0.join("first")))
            .expect("build first retirement"),
        Retirement::new(Vec::new(), "changed".into(), identity_of(&fixture.0.join("changed")))
            .expect("build changed retirement"),
        Retirement::new(Vec::new(), "second".into(), identity_of(&fixture.0.join("second")))
            .expect("build second retirement"),
        Retirement::new(Vec::new(), "missing".into(), identity_of(&fixture.0.join("first")))
            .expect("build missing retirement"),
    ];
    fs::write(fixture.0.join("replacement"), b"replacement").expect("write replacement entry");
    fs::rename(fixture.0.join("replacement"), fixture.0.join("changed"))
        .expect("replace changed entry");
    let results = session.remove_batch(root.as_fd(), requests, |_| Ok(()));
    let outcomes: Vec<Removal> = results
        .into_iter()
        .map(|result| result.expect("batch entry outcome"))
        .collect();
    assert_eq!(
        outcomes,
        [
            Removal::Removed,
            Removal::Changed,
            Removal::Removed,
            Removal::AlreadyAbsent
        ]
    );
    assert!(!session.recovery_required());
    session.finish().expect("finish session");
    assert!(!fixture.0.join("first").exists());
    assert!(!fixture.0.join("second").exists());
    assert_eq!(
        fs::read(fixture.0.join("changed")).expect("read live replacement"),
        b"replacement"
    );
    assert_eq!(fs::read_dir(&fixture.0).expect("read root").count(), 1);
}

#[test]
fn refused_batch_boundary_restores_every_capture_at_its_name() {
    for refused in [Boundary::BeforeCapture, Boundary::BeforeUnlink] {
        let fixture = Fixture::new();
        for name in ["first", "second"] {
            fs::write(fixture.0.join(name), name.as_bytes()).expect("write batch entry");
        }
        let root = fixture.root();
        let session = Session::start(root.as_fd()).expect("start session");
        let requests = ["first", "second"]
            .into_iter()
            .map(|name| {
                Retirement::new(Vec::new(), name.into(), identity_of(&fixture.0.join(name)))
                    .expect("build batch retirement")
            })
            .collect();
        let results = session.remove_batch(root.as_fd(), requests, |boundary| {
            if boundary == refused {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "deletion cancelled",
                ));
            }
            Ok(())
        });
        for result in results {
            let error = result.expect_err("a refused boundary fails every entry");
            assert!(!error.removed, "{refused:?}");
            assert_eq!(error.error.kind(), std::io::ErrorKind::Interrupted);
        }
        assert!(!session.recovery_required());
        session.finish().expect("a refused batch leaves no journal records");
        for name in ["first", "second"] {
            assert_eq!(
                fs::read(fixture.0.join(name)).expect("read restored entry"),
                name.as_bytes(),
                "{refused:?}"
            );
        }
        assert_eq!(fs::read_dir(&fixture.0).expect("read root").count(), 2);
    }
}

#[test]
fn killed_retirement_under_restrictive_umask_recovers_records() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("source"), b"data").expect("write source");
    let mut child = start_crash_child(&fixture.0, Boundary::BeforeCapture, true);
    wait_ready(&mut child, Boundary::BeforeCapture);
    let directory = fs::read_dir(&fixture.0)
        .expect("read recovery root")
        .map(|entry| entry.expect("recovery entry").path())
        .find(|path| {
            path.file_name()
                .expect("session name")
                .to_string_lossy()
                .starts_with(".fdu-quarantine-v2-")
        })
        .expect("session directory");
    for record in [directory.join("header"), directory.join("0.intent")] {
        assert_eq!(
            fs::metadata(record)
                .expect("record metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    child.kill().expect("kill journal child");
    assert!(!child.wait().expect("reap journal child").success());
    let error = recover(&fixture.0).expect_err("recovery must report the interruption");
    assert!(
        error
            .to_string()
            .contains("interrupted filesystem operation state recovered"),
        "{error}"
    );
    assert_eq!(fs::read(fixture.0.join("source")).expect("read source"), b"data");
    assert_eq!(
        fs::read_dir(&fixture.0).expect("read recovery root").count(),
        1
    );
    recover(&fixture.0).expect("a second recovery is a no-op");
}

#[test]
fn killed_retirement_restores_only_before_unlink_and_never_replays_deletion() {
    for boundary in [
        Boundary::IntentPublished,
        Boundary::BeforeCapture,
        Boundary::Captured,
        Boundary::CaptureParentSynced,
        Boundary::CaptureDirectorySynced,
        Boundary::BeforeUnlink,
        Boundary::Unlinked,
        Boundary::RemovalDirectorySynced,
        Boundary::BeforeParentSync,
        Boundary::ParentSynced,
        Boundary::IntentRemoved,
        Boundary::IntentRemovalSynced,
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), b"data").expect("write source");
        fs::hard_link(fixture.0.join("source"), fixture.0.join("alias")).expect("create alias");
        fs::write(fixture.0.join("destination"), b"published data").expect("write destination");
        let mut child = start_crash_child(&fixture.0, boundary, false);
        wait_ready(&mut child, boundary);
        child.kill().expect("kill journal child");
        assert!(!child.wait().expect("reap journal child").success());
        let result = recover(&fixture.0);
        assert!(
            result
                .expect_err("recovery must report the interruption")
                .to_string()
                .contains("interrupted filesystem operation state recovered"),
            "{boundary:?}"
        );
        let removed = matches!(
            boundary,
            Boundary::Unlinked
                | Boundary::RemovalDirectorySynced
                | Boundary::BeforeParentSync
                | Boundary::ParentSynced
                | Boundary::IntentRemoved
                | Boundary::IntentRemovalSynced
        );
        assert_eq!(fixture.0.join("source").exists(), !removed, "{boundary:?}");
        assert_eq!(
            fs::read(fixture.0.join("alias")).expect("read alias"),
            b"data"
        );
        assert_eq!(
            fs::read(fixture.0.join("destination")).expect("read destination"),
            b"published data"
        );
        if !removed {
            assert_eq!(
                fs::metadata(fixture.0.join("source"))
                    .expect("source metadata")
                    .ino(),
                fs::metadata(fixture.0.join("alias"))
                    .expect("alias metadata")
                    .ino()
            );
        }
        recover(&fixture.0).expect("a second recovery is a no-op");
    }
}

#[test]
fn current_records_validate_version_owner_mode_mount_and_captured_identity() {
    for mutation in [
        "version",
        "owner",
        "mode",
        "mount",
        "identity",
        "record-kind",
        "extra",
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("source"), b"captured").expect("write source");
        let root = fixture.root();
        let session = Session::start(root.as_fd()).expect("start session");
        let directory = fixture.0.join(session.name());
        let request =
            Retirement::new(Vec::new(), "source".into(), identity_of(&fixture.0.join("source")))
                .expect("build retirement");
        let error = session
            .remove(root.as_fd(), request, |point, _, _| {
                if point == Boundary::BeforeUnlink {
                    fs::write(fixture.0.join("source"), b"live").expect("replace source");
                    return Err(std::io::Error::other("interrupted"));
                }
                Ok(())
            })
            .expect_err("interrupted retirement must fail");
        assert!(!error.removed);
        drop(session);
        let header = directory.join("header");
        let record = directory.join("0.intent");
        match mutation {
            "mode" => fs::set_permissions(&record, fs::Permissions::from_mode(0o644))
                .expect("change record mode"),
            "identity" => {
                fs::rename(directory.join("0.entry"), directory.join("held"))
                    .expect("move capture");
                fs::write(directory.join("0.entry"), b"substitute").expect("write substitute");
            }
            "record-kind" => {
                let mut bytes = fs::read(&record).expect("read record");
                let index = bytes.len() - 18;
                bytes[index] = 99;
                fs::write(&record, bytes).expect("write mutated record");
            }
            "extra" => {
                fs::write(directory.join("unrelated"), b"retain").expect("write unrelated entry");
            }
            _ => {
                let mut bytes = fs::read(&header).expect("read header");
                let index = match mutation {
                    "version" => 15,
                    "owner" => 32,
                    "mount" => bytes.len() - 1,
                    _ => unreachable!(),
                };
                bytes[index] ^= 0xff;
                fs::write(&header, bytes).expect("write mutated header");
            }
        }
        let error = recover(&fixture.0).expect_err("mutated journal state must be retained");
        assert!(error.to_string().contains("retained"), "{mutation}: {error}");
        assert_eq!(
            fs::read(fixture.0.join("source")).expect("read live source"),
            b"live"
        );
        assert!(directory.join("0.entry").exists());
    }
}

#[test]
fn lock_conflict_is_nonblocking_and_symlink_hardlinks_are_never_followed() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("target"), b"target bytes").expect("write target");
    std::os::unix::fs::symlink("target", fixture.0.join("source")).expect("create symlink source");
    fs::hard_link(fixture.0.join("source"), fixture.0.join("alias")).expect("hardlink the symlink");
    let root = fixture.root();
    let session = Session::start(root.as_fd()).expect("start session");
    let conflict = Session::start(root.as_fd()).expect_err("the root lock must conflict");
    assert!(
        conflict
            .to_string()
            .contains("another filesystem operation owns this directory"),
        "{conflict}"
    );
    let request =
        Retirement::new(Vec::new(), "source".into(), identity_of(&fixture.0.join("source")))
            .expect("build retirement");
    assert_eq!(
        session
            .remove(root.as_fd(), request, |_, _, _| Ok(()))
            .expect("retire symlink"),
        Removal::Removed
    );
    session.finish().expect("finish session");
    assert_eq!(
        fs::read_link(fixture.0.join("alias")).expect("read alias"),
        Path::new("target")
    );
    assert_eq!(
        fs::read(fixture.0.join("target")).expect("read target"),
        b"target bytes"
    );
    let alias = File::open(fixture.0.as_path()).expect("open fixture root");
    assert_eq!(
        observe_entry(alias.as_fd(), OsStr::new("alias"))
            .expect("observe alias")
            .identity
            .kind,
        xsh_root::FileKind::Symlink
    );
}

#[test]
fn ancestor_substitution_retains_the_capture_and_live_replacement() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.0.join("parent")).expect("create parent");
    fs::write(fixture.0.join("parent/source"), b"captured").expect("write captured source");
    let parent = File::open(fixture.0.join("parent")).expect("open parent");
    let parent_identity = observe_fd(parent.as_fd()).expect("observe parent").identity;
    let root = fixture.root();
    let session = Session::start(root.as_fd()).expect("start session");
    let directory = fixture.0.join(session.name());
    let request = Retirement::new(
        vec![Ancestor {
            name: "parent".into(),
            identity: parent_identity,
        }],
        "source".into(),
        identity_of(&fixture.0.join("parent/source")),
    )
    .expect("build retirement");
    session
        .remove(parent.as_fd(), request, |point, _, _| {
            if point == Boundary::BeforeUnlink {
                fs::rename(fixture.0.join("parent"), fixture.0.join("original-parent"))
                    .expect("move original parent");
                fs::create_dir(fixture.0.join("parent")).expect("create replacement parent");
                fs::write(fixture.0.join("parent/source"), b"replacement")
                    .expect("write replacement source");
                return Err(std::io::Error::other("parent substituted"));
            }
            Ok(())
        })
        .expect_err("substituted ancestry must fail");
    drop(session);
    let result = recover(&fixture.0).expect_err("substituted ancestry must be retained");
    assert!(
        result
            .to_string()
            .contains("parent identity or mount changed"),
        "{result}"
    );
    assert_eq!(
        fs::read(fixture.0.join("parent/source")).expect("read replacement source"),
        b"replacement"
    );
    assert_eq!(
        fs::read(directory.join("0.entry")).expect("read retained capture"),
        b"captured"
    );
}

#[test]
fn restored_pre_unlink_error_does_not_claim_a_retained_capture() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("source"), b"retained at original name").expect("write source");
    let root = fixture.root();
    let session = Session::start(root.as_fd()).expect("start session");
    let request =
        Retirement::new(Vec::new(), "source".into(), identity_of(&fixture.0.join("source")))
            .expect("build retirement");
    let error = session
        .remove(root.as_fd(), request, |boundary, _, _| {
            if boundary == Boundary::BeforeUnlink {
                return Err(std::io::Error::other("authorization failed"));
            }
            Ok(())
        })
        .expect_err("authorization failure must fail the removal");
    assert!(!error.removed);
    assert_eq!(error.to_string(), "authorization failed");
    assert_eq!(
        fs::read(fixture.0.join("source")).expect("read source"),
        b"retained at original name"
    );
    assert_eq!(
        fs::read_dir(fixture.0.join(session.name()))
            .expect("read session directory")
            .count(),
        1
    );
    session.finish().expect("finish restored session");
}

#[test]
fn inherited_pre_exec_descriptor_does_not_extend_a_finished_session_lock() {
    let fixture = Fixture::new();
    let root = fixture.root();
    let session = Session::start(root.as_fd()).expect("start session");
    session.finish().expect("finish session");
    let (mut parent, child) = UnixStream::pair().expect("create socket pair");
    parent
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("set read timeout");
    let process = std::thread::spawn(move || {
        let mut command = Command::new(std::env::current_exe().expect("current test binary"));
        command
            .args([
                "--exact",
                "recovery::crash_child",
                "--ignored",
                "--nocapture",
            ])
            .stdout(Stdio::null());
        // SAFETY: the child hook performs only descriptor reads and writes before exec.
        unsafe {
            command.pre_exec(move || {
                rustix::io::write(child.as_fd(), b"ready")?;
                let mut release = [0u8; 1];
                rustix::io::read(child.as_fd(), &mut release)?;
                Ok(())
            });
        }
        let mut process = command.spawn().expect("spawn descendant");
        process.wait().expect("wait for descendant")
    });
    let mut ready = [0u8; 5];
    parent.read_exact(&mut ready).expect("read readiness");
    assert_eq!(&ready, b"ready");
    drop(session);
    let replacement = Session::start(root.as_fd()).expect("replacement session");
    parent.write_all(b"x").expect("release descendant");
    assert!(process.join().expect("join descendant thread").success());
    assert!(Session::start(root.as_fd()).is_err());
    replacement.finish().expect("finish replacement session");
}
