mod support;

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use support::{budget, list, open};
use xsh_archive::{
    Collision, Compression, CreateOptions, EntryKind, EntryMeta, ErrorKind, Format, Modes, Policy,
    Special, create, extract,
};

const MTIME: i64 = 1_700_000_000;

const BUILD_TREES: &str = r#"
set -eu
umask 0
mkdir src longsrc sparsesrc
cd src
mkdir -p dir/deeper special
printf alpha > dir/a.txt
printf bravo > dir/deeper/b.txt
printf '#!/bin/sh\n' > run.sh
chmod 755 run.sh
printf secret > private.txt
chmod 600 private.txt
printf sticky > setuid.bin
chmod 4755 setuid.bin
: > empty
ln -s dir/a.txt link
ln -s missing dangling
ln dir/a.txt hard
mkfifo special/pipe
printf unicode > "$(printf 'caf\303\251')"
printf raw > "$(printf 'raw\377name')"
find . -exec touch -h -d @1700000000 {} +
cd ../longsrc
long=$(printf 'd%.0s' $(seq 1 60))
mkdir -p "$long/$long"
printf long > "$long/$long/leaf.txt"
ln -s "$long/$long/leaf.txt" longlink
find . -exec touch -h -d @1700000000 {} +
cd ../sparsesrc
truncate -s 1M sparse
printf x | dd of=sparse bs=1 seek=524288 conv=notrunc 2>/dev/null
"#;

const WRITE_WITH_REFERENCE_TOOLS: &str = r#"
set -eu
umask 0
mkdir archives
for format in ustar gnu oldgnu pax; do
  (cd src && tar --format=$format --sort=name -cf ../archives/gnu-$format.tar *)
done
(cd src && tar --format=v7 --sort=name --exclude=special -cf ../archives/gnu-v7.tar *)
for format in ustar pax gnutar; do
  (cd src && bsdtar --format $format -cf ../archives/bsd-$format.tar *)
done
(cd src && bsdtar --format v7tar --exclude special -cf ../archives/bsd-v7tar.tar *)
for format in gnu oldgnu pax; do
  (cd longsrc && tar --format=$format -cf ../archives/gnu-$format-long.tar *)
done
for format in pax gnutar; do
  (cd longsrc && bsdtar --format $format -cf ../archives/bsd-$format-long.tar *)
done
for format in gnu oldgnu pax; do
  (cd sparsesrc && tar -S --format=$format -cf ../archives/gnu-$format-sparse.tar sparse)
done
(cd sparsesrc && bsdtar --format pax --read-sparse -cf ../archives/bsd-pax-sparse.tar sparse)
"#;

const READ_WITH_REFERENCE_TOOLS: &str = r#"
set -eu
umask 0
manifest() {
  (cd "$1" && find . -mindepth 1 -printf '%P\t%y\t%#m\t%l\n' | sed 's/^\(setuid.bin\tf\t\)04755/\10755/' | LC_ALL=C sort
   find . -type f -exec sha256sum {} + | LC_ALL=C sort -k2)
}
for archive in written/*; do
  base=$(basename "$archive")
  manifest "${base%%.*}" > expected.manifest
  mkdir "out-gnu-$base" "out-bsd-$base"
  tar -xpf "$archive" -C "out-gnu-$base"
  bsdtar -xpf "$archive" -C "out-bsd-$base"
  for tool in gnu bsd; do
    manifest "out-$tool-$base" > "$tool-$base.manifest"
    if ! cmp -s expected.manifest "$tool-$base.manifest"; then
      echo "$tool tar read $base differently:"
      diff expected.manifest "$tool-$base.manifest" || true
      exit 1
    fi
  done
  gnu=$(tar --numeric-owner -tvf "$archive" | awk '{print $2, $4}' | sort -u)
  [ "$gnu" = "0/0 1970-01-01" ] || { echo "gnu tar owner or time of $base: $gnu"; exit 1; }
  bsd=$(bsdtar --numeric-owner -tvf "$archive" | awk '{print $2, $3}' | sort -u)
  [ "$bsd" = "0 0" ] || { echo "bsdtar owner of $base: $bsd"; exit 1; }
  case $base in src.*)
    for tool in tar bsdtar; do
      $tool -tvf "$archive" | grep -q '^-rwsr-xr-x .* setuid.bin$' || { echo "$tool lost the setuid bit of $base"; exit 1; }
    done ;;
  esac
  late=$(bsdtar --numeric-owner -tvf "$archive" | grep -vc ' Jan  1  1970 ' || true)
  [ "$late" = 0 ] || { echo "bsdtar time of $base"; exit 1; }
done
"#;

struct Oracle {
    script: PathBuf,
}

fn oracle() -> Option<Oracle> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/reference/oracle.sh");
    let image = std::env::var("XSH_ORACLE_IMAGE").unwrap_or_else(|_| "xsh-oracle".into());
    let present = Command::new("docker")
        .args(["image", "inspect", &image])
        .output()
        .is_ok_and(|output| output.status.success());
    if !present {
        eprintln!("skipped tar interoperability: docker image {image} is not built");
        return None;
    }
    let tools = "tar --version | head -n 1 | grep -q 'GNU tar' && bsdtar --version >/dev/null";
    let ready = Command::new(&script)
        .args(["--", "sh", "-c", tools])
        .output()
        .is_ok_and(|output| output.status.success());
    if !ready {
        eprintln!("skipped tar interoperability: image {image} lacks GNU tar or bsdtar; rebuild it with dev/reference/oracle.sh");
        return None;
    }
    Some(Oracle { script })
}

impl Oracle {
    fn run(&self, mount: &Path, script: &str) -> String {
        fs::set_permissions(mount, fs::Permissions::from_mode(0o777)).unwrap();
        let output = Command::new(&self.script)
            .arg("--mount")
            .arg(mount)
            .args(["--rw", "--", "sh", "-c", script])
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.status.success(), "{text}");
        text
    }
}

fn walk(root: &Path) -> BTreeMap<Vec<u8>, fs::Metadata> {
    fn visit(root: &Path, dir: &Path, found: &mut BTreeMap<Vec<u8>, fs::Metadata>) {
        for child in fs::read_dir(dir).unwrap() {
            let path = child.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().as_os_str().as_bytes().to_vec();
            let metadata = fs::symlink_metadata(&path).unwrap();
            let is_dir = metadata.is_dir();
            found.insert(relative, metadata);
            if is_dir {
                visit(root, &path, found);
            }
        }
    }
    let mut found = BTreeMap::new();
    visit(root, root, &mut found);
    found
}

fn expected_kind(metadata: &fs::Metadata) -> EntryKind {
    let kind = metadata.file_type();
    if kind.is_dir() {
        EntryKind::Directory
    } else if kind.is_symlink() {
        EntryKind::Symlink
    } else if kind.is_fifo() {
        EntryKind::Fifo
    } else {
        EntryKind::File
    }
}

fn describe(name: &[u8]) -> String {
    String::from_utf8_lossy(name).into_owned()
}

fn check_listing(label: &str, entries: &[EntryMeta], tree_root: &Path, tree: &BTreeMap<Vec<u8>, fs::Metadata>, hard: &[u8]) {
    let mut seen = 0;
    for entry in entries {
        let name = entry.name.strip_suffix(b"/").unwrap_or(&entry.name);
        let Some(metadata) = tree.get(name) else {
            panic!("{label}: unexpected member {}", describe(name));
        };
        seen += 1;
        let hardlinked = name == hard;
        let kind = if hardlinked { EntryKind::Hardlink } else { expected_kind(metadata) };
        assert_eq!(entry.kind, kind, "{label}: {}", describe(name));
        assert_eq!(entry.mode.map(|mode| mode & 0o7777), Some(metadata.mode() & 0o7777), "{label}: mode of {}", describe(name));
        assert_eq!(entry.modified, Some(MTIME), "{label}: mtime of {}", describe(name));
        assert_eq!(entry.uid, Some(metadata.uid()), "{label}: uid of {}", describe(name));
        assert_eq!(entry.gid, Some(metadata.gid()), "{label}: gid of {}", describe(name));
        if kind == EntryKind::File {
            assert_eq!(entry.size, Some(metadata.len()), "{label}: size of {}", describe(name));
        }
        if kind == EntryKind::Symlink {
            let target = fs::read_link(tree_root.join(std::ffi::OsStr::from_bytes(name))).unwrap();
            assert_eq!(entry.link.as_deref(), Some(target.as_os_str().as_bytes()), "{label}: link of {}", describe(name));
        }
    }
    assert_eq!(seen, tree.len(), "{label}: member count");
}

fn extract_and_compare(label: &str, bytes: Vec<u8>, tree_root: &Path, tree: &BTreeMap<Vec<u8>, fs::Metadata>, out: &Path) {
    let mut archive = open(bytes, Format::Tar, Compression::Auto).unwrap();
    let policy = Policy { special: Special::Create, modes: Modes::Exact, ..Policy::default() };
    extract(&mut archive, out, &policy, &budget()).unwrap();
    for (name, metadata) in tree {
        let relative = Path::new(std::ffi::OsStr::from_bytes(name));
        let produced = fs::symlink_metadata(out.join(relative)).unwrap_or_else(|error| panic!("{label}: {}: {error}", describe(name)));
        assert_eq!(expected_kind(&produced), expected_kind(metadata), "{label}: {}", describe(name));
        assert_eq!(produced.mode() & 0o7777, metadata.mode() & 0o7777, "{label}: mode of {}", describe(name));
        let source = tree_root.join(relative);
        if metadata.is_file() {
            assert_eq!(fs::read(out.join(relative)).unwrap(), fs::read(&source).unwrap(), "{label}: {}", describe(name));
        }
        if metadata.file_type().is_symlink() {
            assert_eq!(fs::read_link(out.join(relative)).unwrap(), fs::read_link(&source).unwrap(), "{label}: {}", describe(name));
        }
    }
    assert_eq!(walk(out).len(), tree.len(), "{label}: extracted member count");
}

#[test]
fn archives_written_by_gnu_tar_and_bsdtar_are_listed_and_extracted_exactly() {
    let Some(oracle) = oracle() else { return };
    let base = tempfile::tempdir().unwrap();
    oracle.run(base.path(), &format!("{BUILD_TREES}\ncd ..\n{WRITE_WITH_REFERENCE_TOOLS}"));
    let mut checked = 0;
    for (tree_name, long) in [("src", false), ("longsrc", true)] {
        let tree_root = base.path().join(tree_name);
        let tree = walk(&tree_root);
        for entry in fs::read_dir(base.path().join("archives")).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name.contains("sparse") || name.contains("-long") != long {
                continue;
            }
            let mut tree = tree.clone();
            if name.contains("v7") {
                tree.retain(|path, _| !path.starts_with(b"special"));
            }
            let bytes = fs::read(&path).unwrap();
            let mut archive = open(bytes.clone(), Format::Tar, Compression::Auto).unwrap();
            let listing = list(&mut archive).unwrap();
            let hard: &[u8] = if long { b"" } else { b"hard" };
            check_listing(&name, &listing.entries, &tree_root, &tree, hard);
            extract_and_compare(&name, bytes, &tree_root, &tree, &base.path().join(format!("out-{name}")));
            checked += 1;
        }
    }
    assert_eq!(checked, 14);
}

#[test]
fn archives_written_by_xsh_are_read_by_gnu_tar_and_bsdtar() {
    let Some(oracle) = oracle() else { return };
    let base = tempfile::tempdir().unwrap();
    oracle.run(base.path(), BUILD_TREES);
    let written = base.path().join("written");
    fs::create_dir(&written).unwrap();
    for tree in ["src", "longsrc"] {
        for suffix in ["tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "tar.lzma"] {
            let name = format!("{tree}.{suffix}");
            let options = CreateOptions {
                format: Format::Tar,
                compression: Compression::Auto,
                level: None,
                collision: Collision::Refuse,
            };
            create(&written.join(&name), &base.path().join(tree), &[b".".to_vec()], &options, &budget()).unwrap();
        }
    }
    oracle.run(base.path(), READ_WITH_REFERENCE_TOOLS);
}

#[test]
fn gnu_sparse_archives_are_refused_instead_of_extracted_as_compacted_files() {
    let Some(oracle) = oracle() else { return };
    let base = tempfile::tempdir().unwrap();
    oracle.run(base.path(), &format!("{BUILD_TREES}\ncd ..\n{WRITE_WITH_REFERENCE_TOOLS}"));
    let mut refused = 0;
    for entry in fs::read_dir(base.path().join("archives")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.contains("sparse") {
            continue;
        }
        let mut archive = open(fs::read(&path).unwrap(), Format::Tar, Compression::Auto).unwrap();
        let out = base.path().join(format!("out-{name}"));
        let error = extract(&mut archive, &out, &Policy::default(), &budget()).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsupported, "{name}: {error}");
        assert!(!out.exists(), "{name}");
        refused += 1;
    }
    assert_eq!(refused, 4);
}
