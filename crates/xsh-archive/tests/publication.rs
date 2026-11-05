mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use support::{budget, tar_raw_header, open, tar_with};
use xsh_archive::{
    Archive, Collision, Compression, CreateOptions, Format, Policy, Source, create, extract,
};

const FILES: usize = 64;
const BODY: usize = 256 * 1024;

fn noise(seed: u64, length: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

fn observe<T>(watch: impl Fn() + Sync, act: impl FnOnce() -> T) -> T {
    let done = AtomicBool::new(false);
    thread::scope(|scope| {
        let watcher = scope.spawn(|| {
            while !done.load(Ordering::Acquire) {
                watch();
            }
        });
        let result = act();
        done.store(true, Ordering::Release);
        watcher.join().unwrap();
        result
    })
}

fn staging_names(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn a_created_archive_is_never_visible_before_it_is_complete() {
    let base = tempfile::tempdir().unwrap();
    let src = base.path().join("src");
    fs::create_dir(&src).unwrap();
    for index in 0..FILES {
        fs::write(src.join(format!("f{index}")), noise(index as u64 + 1, BODY)).unwrap();
    }
    let out = base.path().join("out");
    fs::create_dir(&out).unwrap();
    let dest = out.join("pack.tar.gz");
    let expected = FILES as u64 * (BODY as u64 + 512);

    let options = CreateOptions {
        format: Format::Tar,
        compression: Compression::Auto,
        level: None,
        collision: Collision::Refuse,
    };
    observe(
        || {
            if let Ok(metadata) = fs::metadata(&dest) {
                assert!(metadata.len() >= expected, "destination was visible with {} bytes", metadata.len());
            }
            for name in staging_names(&out) {
                assert!(name == "pack.tar.gz" || name.starts_with(".xsh-archive-out-"), "{name}");
            }
        },
        || create(&dest, &src, &[b".".to_vec()], &options, &budget()).unwrap(),
    );
    assert_eq!(staging_names(&out), ["pack.tar.gz"]);
}

#[test]
fn an_extracted_tree_is_never_visible_before_it_is_complete() {
    let base = tempfile::tempdir().unwrap();
    let bodies: Vec<Vec<u8>> = (0..FILES).map(|index| noise(index as u64 + 1, BODY)).collect();
    let names: Vec<String> = (0..FILES).map(|index| format!("pkg/f{index:03}")).collect();
    let entries: Vec<(&[u8], &[u8])> = names
        .iter()
        .zip(&bodies)
        .map(|(name, body)| (name.as_bytes(), body.as_slice()))
        .collect();
    let bytes = tar_with(&entries);
    let parent = base.path().join("parent");
    fs::create_dir(&parent).unwrap();
    let dest: PathBuf = parent.join("dest");

    observe(
        || {
            if dest.exists() {
                for (name, body) in names.iter().zip(&bodies) {
                    let metadata = fs::metadata(dest.join(name)).unwrap_or_else(|error| panic!("{name}: {error}"));
                    assert_eq!(metadata.len(), body.len() as u64, "{name}");
                }
            }
            for name in staging_names(&parent) {
                assert!(name == "dest" || name.starts_with(".xsh-archive-"), "{name}");
            }
        },
        || {
            let mut archive = open(bytes.clone(), Format::Tar, Compression::Auto).unwrap();
            extract(&mut archive, &dest, &Policy::default(), &budget()).unwrap();
        },
    );
    assert_eq!(staging_names(&parent), ["dest"]);
}

struct SwapsAncestorWhenExtractionStarts {
    bytes: Vec<u8>,
    position: usize,
    created: PathBuf,
    ancestor: PathBuf,
    moved: PathBuf,
    outside: PathBuf,
}

impl std::io::Read for SwapsAncestorWhenExtractionStarts {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.created.exists() {
            fs::rename(&self.ancestor, &self.moved)?;
            std::os::unix::fs::symlink(&self.outside, &self.ancestor)?;
            return Err(std::io::Error::other("source failed after the ancestor was swapped"));
        }
        let count = buffer.len().min(self.bytes.len() - self.position);
        buffer[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

#[test]
fn rolling_back_created_ancestors_never_follows_a_swapped_ancestor() {
    let base = tempfile::tempdir().unwrap();
    let outside = base.path().join("outside");
    fs::create_dir_all(outside.join("inner")).unwrap();
    let mut bytes = tar_raw_header(b"member", b'0', 1 << 20, b"").to_vec();
    bytes.extend_from_slice(&[7; 16]);
    let ancestor = base.path().join("first");
    let reader = SwapsAncestorWhenExtractionStarts {
        bytes,
        position: 0,
        created: ancestor.join("inner"),
        ancestor: ancestor.clone(),
        moved: base.path().join("moved"),
        outside: outside.clone(),
    };
    let mut archive = Archive::open(
        Source::from_reader(reader),
        Format::Tar,
        Compression::Auto,
        budget(),
    )
    .unwrap();

    let dest = ancestor.join("inner").join("dest");
    extract(&mut archive, &dest, &Policy::default(), &budget()).unwrap_err();

    assert!(outside.join("inner").is_dir(), "rollback removed a directory outside the destination");
}
