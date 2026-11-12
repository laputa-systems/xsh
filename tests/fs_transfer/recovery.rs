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

fn start_batch_crash_child(root: &Path, names: &str) -> Child {
    let mut command = Command::new(std::env::current_exe().expect("current test binary"));
    command
        .args([
            "--exact",
            "recovery::batch_crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env("XSH_JOURNAL_BATCH_ROOT", root)
        .env("XSH_JOURNAL_BATCH_NAMES", names)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command.spawn().expect("spawn batch journal child")
}

#[test]
#[ignore = "subprocess fixture for batch journal interruption"]
fn batch_crash_child() {
    let Some(root) = std::env::var_os("XSH_JOURNAL_BATCH_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let names = std::env::var("XSH_JOURNAL_BATCH_NAMES").expect("batch entry names");
    let root_file = File::open(&root).expect("open batch crash root");
    let session = Session::start(root_file.as_fd()).expect("start batch crash session");
    let entries = names
        .split(':')
        .map(|name| {
            Retirement::new(
                Vec::new(),
                name.into(),
                identity_of(&root.join(name)),
            )
            .expect("build batch retirement")
        })
        .collect();
    let group = xsh_root::transfer::RetirementGroup {
        ancestors: Vec::new(),
        entries,
    };
    let mut unlinked = 0_u32;
    session
        .retire_batch(&group, |boundary, _, _| {
            if boundary == Boundary::Unlinked {
                unlinked += 1;
                if unlinked == 2 {
                    println!("ready");
                    std::io::stdout().flush().expect("flush readiness");
                    loop {
                        std::thread::park();
                    }
                }
            }
            Ok(())
        })
        .expect("batch retirement completed unexpectedly");
    panic!("batch crash boundary was not reached");
}

#[test]
fn killed_batch_restores_every_unlinked_capture() {
    let fixture = Fixture::new();
    for name in ["first", "second", "third"] {
        fs::write(fixture.0.join(name), name.as_bytes()).expect("write batch entry");
    }
    let mut child = start_batch_crash_child(&fixture.0, "first:second:third");
    let stdout = child.stdout.take().expect("child stdout");
    let mut output = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        assert_ne!(
            output.read_line(&mut line).expect("read child stdout"),
            0,
            "batch child ended before unlinking an entry"
        );
        if line.trim() == "ready" {
            break;
        }
        line.clear();
    }
    child.kill().expect("kill batch journal child");
    assert!(!child.wait().expect("reap batch journal child").success());
    let error = recover(&fixture.0).expect_err("recovery must report the interruption");
    assert!(
        error
            .to_string()
            .contains("interrupted filesystem operation state recovered"),
        "{error}"
    );
    assert!(!fixture.0.join("first").exists());
    assert!(!fixture.0.join("second").exists());
    assert_eq!(
        fs::read(fixture.0.join("third")).expect("unreached entry keeps its name"),
        b"third"
    );
    assert_eq!(
        fs::read_dir(&fixture.0).expect("read recovery root").count(),
        1
    );
    recover(&fixture.0).expect("a second recovery is a no-op");
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
