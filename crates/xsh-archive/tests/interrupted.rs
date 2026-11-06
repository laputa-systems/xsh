mod support;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use support::{budget, open, tar_with};
use xsh_archive::{
    Archive, Collision, Compression, CreateOptions, Format, Policy, Source, create, extract,
    extract_beneath,
};
use xsh_root::Root;

const CHILD_ENV: &str = "XSH_ARCHIVE_INTERRUPT_CHILD";
const MEMBERS: usize = 16;
const BODY: usize = 64 * 1024;
const WAIT: Duration = Duration::from_secs(60);

fn body(member: usize) -> Vec<u8> {
    vec![member as u8 + 1; BODY]
}

fn member_name(member: usize) -> String {
    format!("pkg/f{member:02}")
}

fn archive_bytes() -> Vec<u8> {
    let names: Vec<String> = (0..MEMBERS).map(member_name).collect();
    let bodies: Vec<Vec<u8>> = (0..MEMBERS).map(body).collect();
    let entries: Vec<(&[u8], &[u8])> = names
        .iter()
        .zip(&bodies)
        .map(|(name, body)| (name.as_bytes(), body.as_slice()))
        .collect();
    tar_with(&entries)
}

struct StallsAfter {
    bytes: Vec<u8>,
    position: usize,
    stall_at: usize,
    ready: PathBuf,
}

impl Read for StallsAfter {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.stall_at {
            fs::write(&self.ready, b"stalled")?;
            loop {
                thread::sleep(Duration::from_secs(3600));
            }
        }
        let count = buffer
            .len()
            .min(self.stall_at - self.position)
            .min(self.bytes.len() - self.position);
        buffer[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

fn stalling_archive(ready: &Path) -> Archive {
    let bytes = archive_bytes();
    let reader = StallsAfter {
        stall_at: bytes.len() / 2,
        bytes,
        position: 0,
        ready: ready.to_path_buf(),
    };
    Archive::open(Source::from_reader(reader), Format::Tar, Compression::Auto, budget()).unwrap()
}

fn spawn_child(mode: &str, args: &[&Path]) -> Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "interrupted_child", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, mode)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    for (index, path) in args.iter().enumerate() {
        command.env(format!("{CHILD_ENV}_{index}"), path);
    }
    command.spawn().unwrap()
}

fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(2));
    }
}

fn kill(mut child: Child) {
    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert!(!status.success(), "the child finished before it was killed");
}

fn names(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

fn staging_in(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    names(dir)
        .into_iter()
        .filter(|name| name.starts_with(prefix))
        .map(|name| dir.join(name))
        .collect()
}

fn assert_complete_tree(dest: &Path) {
    for member in 0..MEMBERS {
        let found = fs::read(dest.join(member_name(member)))
            .unwrap_or_else(|error| panic!("{} is missing: {error}", dest.display()));
        assert_eq!(found, body(member), "{} {member}", dest.display());
    }
    assert_eq!(names(&dest.join("pkg")).len(), MEMBERS, "{}", dest.display());
}

fn staged_file_count(stage: &Path) -> usize {
    fs::read_dir(stage.join("pkg")).map_or(0, |entries| entries.count())
}

#[test]
fn interrupted_child() {
    let Ok(mode) = std::env::var(CHILD_ENV) else {
        return;
    };
    let arg = |index: usize| PathBuf::from(std::env::var_os(format!("{CHILD_ENV}_{index}")).unwrap());
    match mode.as_str() {
        "extract-path" => {
            let mut archive = stalling_archive(&arg(1));
            let _ = extract(&mut archive, &arg(0), &Policy::default(), &budget());
        }
        "extract-replace" => {
            let mut archive = stalling_archive(&arg(1));
            let policy = Policy { collision: Collision::Replace, ..Policy::default() };
            let _ = extract(&mut archive, &arg(0), &policy, &budget());
        }
        "extract-rooted" => {
            let root = Root::open(arg(0)).unwrap();
            let mut archive = stalling_archive(&arg(1));
            let _ = extract_beneath(&mut archive, &root, Path::new("made/dest"), &Policy::default(), &budget());
        }
        "extract-repeatedly" => {
            let parent = arg(0);
            let bytes = archive_bytes();
            let mut index = 0u64;
            loop {
                let mut archive = open(bytes.clone(), Format::Tar, Compression::Auto).unwrap();
                extract(&mut archive, &parent.join(format!("dest-{index:05}")), &Policy::default(), &budget())
                    .unwrap();
                index += 1;
            }
        }
        "create" => {
            let options = CreateOptions {
                format: Format::Tar,
                compression: Compression::Auto,
                level: None,
                collision: Collision::Replace,
            };
            let _ = create(&arg(0), &arg(1), &[b".".to_vec()], &options, &budget());
        }
        other => panic!("unknown child mode {other}"),
    }
}

fn wait_for_stall(ready: &Path, stage: impl Fn() -> Option<PathBuf>) -> PathBuf {
    wait_until("the extraction to stall", || ready.exists());
    let stage = stage().expect("extraction left no staging directory");
    assert!(staged_file_count(&stage) > 0, "extraction had staged nothing when it was killed");
    stage
}

#[test]
fn a_killed_extraction_leaves_staging_and_no_destination() {
    let base = tempfile::tempdir().unwrap();
    let parent = base.path().join("parent");
    fs::create_dir(&parent).unwrap();
    let dest = parent.join("dest");
    let ready = base.path().join("ready");

    let child = spawn_child("extract-path", &[&dest, &ready]);
    let stage = wait_for_stall(&ready, || staging_in(&parent, ".xsh-archive-").pop());
    kill(child);

    assert!(!dest.exists(), "a partial destination became visible");
    assert_eq!(names(&parent), [stage.file_name().unwrap().to_string_lossy().into_owned()]);
    assert!(staged_file_count(&stage) < MEMBERS);
}

#[test]
fn a_killed_extraction_into_a_retained_root_leaves_no_destination() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("root");
    fs::create_dir(&root).unwrap();
    let ready = base.path().join("ready");

    let child = spawn_child("extract-rooted", &[&root, &ready]);
    let stage = wait_for_stall(&ready, || staging_in(&root.join("made"), ".xsh-archive-").pop());
    kill(child);

    assert!(!root.join("made/dest").exists(), "a partial destination became visible");
    assert_eq!(
        names(&root.join("made")),
        [stage.file_name().unwrap().to_string_lossy().into_owned()]
    );
}

#[test]
fn a_killed_extraction_over_an_existing_destination_leaves_it_untouched() {
    let base = tempfile::tempdir().unwrap();
    let dest = base.path().join("dest");
    fs::create_dir_all(dest.join("pkg")).unwrap();
    fs::write(dest.join("pkg/f00"), b"prior").unwrap();
    fs::write(dest.join("keep"), b"keep").unwrap();
    let ready = base.path().join("ready");

    let child = spawn_child("extract-replace", &[&dest, &ready]);
    wait_for_stall(&ready, || staging_in(&dest, ".xsh-archive-").pop());
    kill(child);

    assert_eq!(fs::read(dest.join("pkg/f00")).unwrap(), b"prior");
    assert_eq!(fs::read(dest.join("keep")).unwrap(), b"keep");
    let visible: Vec<String> = names(&dest)
        .into_iter()
        .filter(|name| !name.starts_with(".xsh-archive-"))
        .collect();
    assert_eq!(visible, ["keep", "pkg"]);
    assert_eq!(names(&dest.join("pkg")), ["f00"]);
}

#[test]
fn a_kill_during_repeated_publication_never_shows_a_partial_destination() {
    let base = tempfile::tempdir().unwrap();
    let parent = base.path().join("parent");
    fs::create_dir(&parent).unwrap();

    let child = spawn_child("extract-repeatedly", &[&parent]);
    wait_until("several published destinations", || {
        names(&parent).iter().filter(|name| name.starts_with("dest-")).count() >= 8
    });
    kill(child);

    let published: Vec<String> = names(&parent)
        .into_iter()
        .filter(|name| name.starts_with("dest-"))
        .collect();
    assert!(published.len() >= 8);
    for name in published {
        assert_complete_tree(&parent.join(name));
    }
    for stage in staging_in(&parent, ".xsh-archive-") {
        assert!(stage.is_dir());
    }
}

#[test]
fn a_killed_archive_creation_never_replaces_the_prior_output_with_a_partial_one() {
    let base = tempfile::tempdir().unwrap();
    let source = base.path().join("src");
    fs::create_dir(&source).unwrap();
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for index in 0..32 {
        let data: Vec<u8> = (0..(2 << 20))
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect();
        fs::write(source.join(format!("f{index:02}")), data).unwrap();
    }
    let out = base.path().join("out");
    fs::create_dir(&out).unwrap();
    let dest = out.join("pack.tar");
    fs::write(&dest, b"prior").unwrap();

    let child = spawn_child("create", &[&dest, &source]);
    wait_until("the output to grow", || {
        fs::read_dir(&out)
            .unwrap()
            .map(|entry| entry.unwrap().metadata().unwrap().len())
            .sum::<u64>()
            > 1 << 20
    });
    kill(child);

    assert!(fs::read(&dest).unwrap() == b"prior", "the prior output was replaced by a partial one");
    let temporaries = staging_in(&out, ".xsh-archive-out-");
    assert_eq!(temporaries.len(), 1);
    assert_eq!(names(&out).len(), 2);
}
