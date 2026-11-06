use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const FIXTURE: &str = "tests/fixtures/runtime/os/root-install-feed.xsh";
const FED: usize = 64 * 1024;

fn spawn_install(site: &Path, collision: &str) -> Child {
    Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .arg(FIXTURE)
        .arg("--")
        .arg(site)
        .arg(collision)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn root_install_file script")
}

fn open_feed(feed: &Path, child: &mut Child) -> File {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(feed) {
            Ok(writer) => return writer,
            Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {
                assert!(child.try_wait().unwrap().is_none(), "the script exited before reading the feed");
                assert!(Instant::now() < deadline, "the script never opened the feed");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("open feed: {error}"),
        }
    }
}

fn staged_names(site: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(site)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.file_name().unwrap().as_bytes().starts_with(b".out.install."))
        .collect()
}

fn bytes_written(site: &Path) -> u64 {
    std::fs::read_dir(site)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_name() != "feed")
        .map(|entry| entry.metadata().unwrap().len())
        .sum()
}

fn kill_install_mid_copy(site: &Path, collision: &str) {
    let feed = site.join("feed");
    let path = CString::new(feed.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let mut child = spawn_install(site, collision);
    let mut writer = open_feed(&feed, &mut child);
    writer.write_all(&vec![7_u8; FED]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while bytes_written(site) <= 1024 {
        assert!(child.try_wait().unwrap().is_none(), "the script exited before it was killed");
        assert!(Instant::now() < deadline, "the install never wrote any bytes");
        std::thread::sleep(Duration::from_millis(5));
    }
    child.kill().unwrap();
    assert!(!child.wait().unwrap().success());
}

#[test]
fn a_killed_root_install_leaves_staging_and_no_destination() {
    let site = tempfile::tempdir().unwrap();
    kill_install_mid_copy(site.path(), "refuse");

    assert!(!site.path().join("out").exists(), "a partial destination became visible");
    assert_eq!(staged_names(site.path()).len(), 1);
}

#[test]
fn a_killed_root_install_keeps_the_file_it_would_replace() {
    let site = tempfile::tempdir().unwrap();
    std::fs::write(site.path().join("out"), b"prior").unwrap();
    kill_install_mid_copy(site.path(), "replace");

    assert!(std::fs::read(site.path().join("out")).unwrap() == b"prior", "the prior file was replaced by a partial one");
    assert_eq!(staged_names(site.path()).len(), 1);
}
