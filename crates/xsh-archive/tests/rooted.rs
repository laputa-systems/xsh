mod support;

use std::fs;
use std::io::Read;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use support::specs::{dir, file, symlink as link_member, tar_of};
use support::{budget, kind_of, open, tar_raw_header};
use xsh_archive::{
    Archive, Collision, Compression, ErrorKind, Format, Policy, Source, extract_beneath,
};
use xsh_root::Root;

fn tar(entries: &[(&str, &str)]) -> Archive {
    let specs: Vec<_> = entries
        .iter()
        .map(|(name, body)| file(name.as_bytes(), body.as_bytes()))
        .collect();
    open(tar_of(&specs), Format::Tar, Compression::Auto).unwrap()
}

struct Fixture {
    _base: tempfile::TempDir,
    base: PathBuf,
    root: PathBuf,
    outside: PathBuf,
}

fn fixture() -> Fixture {
    let base = tempfile::tempdir().unwrap();
    let path = base.path().canonicalize().unwrap();
    let root = path.join("root");
    let outside = path.join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir_all(outside.join("sentinel-dir")).unwrap();
    fs::write(outside.join("sentinel"), "sentinel").unwrap();
    Fixture { _base: base, base: path, root, outside }
}

fn names(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

fn assert_outside_untouched(fixture: &Fixture) {
    assert_eq!(names(&fixture.outside), ["sentinel", "sentinel-dir"]);
    assert_eq!(fs::read_to_string(fixture.outside.join("sentinel")).unwrap(), "sentinel");
    assert!(names(&fixture.outside.join("sentinel-dir")).is_empty());
}

fn extract_into(root: &Root, dest: &str, archive: &mut Archive, policy: &Policy) -> xsh_archive::Result<()> {
    extract_beneath(archive, root, Path::new(dest), policy, &budget()).map(drop)
}

#[test]
fn a_new_nested_destination_is_created_beneath_the_root_with_its_ancestors() {
    let fixture = fixture();
    let root = Root::open(&fixture.root).unwrap();
    let mut archive = tar(&[("pkg/a", "alpha"), ("b", "beta")]);

    extract_into(&root, "gen/1/tree", &mut archive, &Policy::default()).unwrap();

    let tree = fixture.root.join("gen/1/tree");
    assert_eq!(fs::read_to_string(tree.join("pkg/a")).unwrap(), "alpha");
    assert_eq!(fs::read_to_string(tree.join("b")).unwrap(), "beta");
    assert_eq!(names(&fixture.root.join("gen/1")), ["tree"]);
}

#[test]
fn the_root_itself_is_an_existing_destination_and_a_dot_prefix_is_accepted() {
    let fixture = fixture();
    fs::write(fixture.root.join("kept"), "kept").unwrap();
    let root = Root::open(&fixture.root).unwrap();

    extract_into(&root, ".", &mut tar(&[("new", "n")]), &Policy::default()).unwrap();
    extract_into(&root, "./sub", &mut tar(&[("deep", "d")]), &Policy::default()).unwrap();

    assert_eq!(names(&fixture.root), ["kept", "new", "sub"]);
    assert_eq!(fs::read_to_string(fixture.root.join("sub/deep")).unwrap(), "d");
}

#[test]
fn replacing_the_root_path_with_a_symlink_after_opening_keeps_extraction_inside_the_original() {
    let fixture = fixture();
    let root = Root::open(&fixture.root).unwrap();
    let moved = fixture.base.join("moved");
    fs::rename(&fixture.root, &moved).unwrap();
    symlink(&fixture.outside, &fixture.root).unwrap();

    extract_into(&root, "dest", &mut tar(&[("f", "inside")]), &Policy::default()).unwrap();
    extract_into(&root, "made/dest", &mut tar(&[("g", "inside")]), &Policy::default()).unwrap();

    assert_eq!(fs::read_to_string(moved.join("dest/f")).unwrap(), "inside");
    assert_eq!(fs::read_to_string(moved.join("made/dest/g")).unwrap(), "inside");
    assert_outside_untouched(&fixture);
}

struct SwapsAncestorOnce {
    bytes: Vec<u8>,
    position: usize,
    created: PathBuf,
    ancestor: PathBuf,
    moved: PathBuf,
    outside: PathBuf,
    swapped: bool,
    fail: bool,
}

impl Read for SwapsAncestorOnce {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if !self.swapped && self.created.exists() {
            self.swapped = true;
            fs::rename(&self.ancestor, &self.moved)?;
            symlink(&self.outside, &self.ancestor)?;
            if self.fail {
                return Err(std::io::Error::other("source failed after the ancestor was swapped"));
            }
        }
        let count = buffer.len().min(self.bytes.len() - self.position);
        buffer[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

fn swapping_archive(fixture: &Fixture, fail: bool, bytes: Vec<u8>) -> Archive {
    let reader = SwapsAncestorOnce {
        bytes,
        position: 0,
        created: fixture.root.join("first/inner"),
        ancestor: fixture.root.join("first"),
        moved: fixture.base.join("moved"),
        outside: fixture.outside.clone(),
        swapped: false,
        fail,
    };
    Archive::open(Source::from_reader(reader), Format::Tar, Compression::Auto, budget()).unwrap()
}

#[test]
fn an_ancestor_swapped_for_a_symlink_mid_extraction_does_not_redirect_publication() {
    let fixture = fixture();
    let root = Root::open(&fixture.root).unwrap();
    let body = vec![7_u8; 1 << 16];
    let mut archive = swapping_archive(
        &fixture,
        false,
        tar_of(&[file(b"f", &body)]),
    );

    extract_into(&root, "first/inner/dest", &mut archive, &Policy::default()).unwrap();

    assert!(fixture.base.join("moved").exists(), "the swap did not happen");
    assert_outside_untouched(&fixture);
    assert_eq!(fs::read(fixture.base.join("moved/inner/dest/f")).unwrap(), body);
}

#[test]
fn rolling_back_after_an_ancestor_swap_never_touches_the_swapped_target() {
    let fixture = fixture();
    fs::create_dir(fixture.outside.join("inner")).unwrap();
    let root = Root::open(&fixture.root).unwrap();
    let mut bytes = tar_raw_header(b"member", b'0', 1 << 20, b"").to_vec();
    bytes.extend_from_slice(&[7; 16]);
    let mut archive = swapping_archive(&fixture, true, bytes);

    extract_into(&root, "first/inner/dest", &mut archive, &Policy::default()).unwrap_err();

    assert!(fixture.outside.join("inner").is_dir());
    assert_eq!(names(&fixture.outside), ["inner", "sentinel", "sentinel-dir"]);
    assert_eq!(fs::read_to_string(fixture.outside.join("sentinel")).unwrap(), "sentinel");
}

#[test]
fn destinations_that_leave_the_root_are_refused_before_anything_is_created() {
    let fixture = fixture();
    let root = Root::open(&fixture.root).unwrap();
    let outside = fixture.outside.join("abs");

    for dest in ["../outside/x", "a/../../outside/x", outside.to_str().unwrap()] {
        let error = extract_into(&root, dest, &mut tar(&[("f", "x")]), &Policy::default()).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Options, "{dest}");
    }
    assert!(names(&fixture.root).is_empty());
    assert_outside_untouched(&fixture);
}

#[test]
fn a_destination_reached_through_or_naming_a_symlink_is_refused() {
    let fixture = fixture();
    symlink(&fixture.outside, fixture.root.join("via")).unwrap();
    fs::create_dir(fixture.root.join("real")).unwrap();
    symlink("real", fixture.root.join("alias")).unwrap();
    let root = Root::open(&fixture.root).unwrap();

    for dest in ["via", "via/sub", "alias", "alias/sub"] {
        extract_into(&root, dest, &mut tar(&[("f", "x")]), &Policy::default()).unwrap_err();
    }
    assert_outside_untouched(&fixture);
    assert!(names(&fixture.root.join("real")).is_empty());
}

#[test]
fn traversal_absolute_and_escaping_link_members_fail_and_leave_no_destination() {
    let fixture = fixture();
    let root = Root::open(&fixture.root).unwrap();
    let cases: Vec<Vec<u8>> = vec![
        tar_of(&[file(b"ok", b"ok"), file(b"../escape", b"bad")]),
        tar_of(&[file(b"ok", b"ok"), file(b"/abs", b"bad")]),
        tar_of(&[file(b"ok", b"ok"), link_member(b"l", b"../../outside")]),
        tar_of(&[file(b"ok", b"ok"), link_member(b"l", b"/etc")]),
    ];
    for bytes in cases {
        let mut archive = open(bytes, Format::Tar, Compression::Auto).unwrap();
        extract_into(&root, "gen/dest", &mut archive, &Policy::default()).unwrap_err();
        assert!(names(&fixture.root).is_empty(), "{:?}", names(&fixture.root));
    }
    assert_outside_untouched(&fixture);
}

#[test]
fn a_member_cannot_be_written_through_a_link_extracted_earlier_in_the_archive() {
    let fixture = fixture();
    let root = Root::open(&fixture.root).unwrap();
    let bytes = tar_of(&[dir(b"d"), link_member(b"d/up", b".."), file(b"d/up/x", b"x")]);
    let mut archive = open(bytes, Format::Tar, Compression::Auto).unwrap();

    let error = extract_into(&root, "dest", &mut archive, &Policy::default()).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::Unsafe);
    assert!(names(&fixture.root).is_empty());
    assert_outside_untouched(&fixture);
}

#[test]
fn an_existing_destination_is_never_clobbered_unless_replace_is_asked() {
    let fixture = fixture();
    fs::create_dir(fixture.root.join("dest")).unwrap();
    fs::write(fixture.root.join("dest/a"), "old").unwrap();
    let root = Root::open(&fixture.root).unwrap();
    let mut archive = tar(&[("fresh", "new"), ("a", "new")]);

    let refused = extract_into(&root, "dest", &mut archive, &Policy::default());

    assert_eq!(kind_of(refused.map(|()| 0)), ErrorKind::Collision);
    assert_eq!(fs::read_to_string(fixture.root.join("dest/a")).unwrap(), "old");
    assert_eq!(names(&fixture.root.join("dest")), ["a"]);

    let replace = Policy { collision: Collision::Replace, ..Policy::default() };
    extract_into(&root, "dest", &mut tar(&[("fresh", "new"), ("a", "new")]), &replace).unwrap();
    assert_eq!(fs::read_to_string(fixture.root.join("dest/a")).unwrap(), "new");
    assert_eq!(names(&fixture.root.join("dest")), ["a", "fresh"]);
}

#[test]
fn a_new_destination_never_replaces_an_existing_file_or_link() {
    let fixture = fixture();
    fs::write(fixture.root.join("file"), "keep").unwrap();
    let root = Root::open(&fixture.root).unwrap();

    let error = extract_into(&root, "file", &mut tar(&[("f", "x")]), &Policy::default()).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::Collision);
    assert_eq!(fs::read_to_string(fixture.root.join("file")).unwrap(), "keep");
}

#[test]
fn a_failure_mid_archive_leaves_prior_outputs_and_no_staging() {
    let fixture = fixture();
    fs::create_dir(fixture.root.join("prior")).unwrap();
    fs::write(fixture.root.join("prior/out"), "prior").unwrap();
    let root = Root::open(&fixture.root).unwrap();
    let mut truncated = tar_of(&[file(b"a", b"alpha"), file(b"b", &[1; 4096])]);
    truncated.truncate(512 + 512 + 512 + 100);
    let mut archive = open(truncated, Format::Tar, Compression::Auto).unwrap();

    extract_into(&root, "prior", &mut archive, &Policy::default()).unwrap_err();
    let mut archive = open(
        tar_of(&[file(b"a", b"alpha"), file(b"b", &[1; 4096])])[..1100].to_vec(),
        Format::Tar,
        Compression::Auto,
    )
    .unwrap();
    extract_into(&root, "gen/new", &mut archive, &Policy::default()).unwrap_err();

    assert_eq!(names(&fixture.root), ["prior"]);
    assert_eq!(names(&fixture.root.join("prior")), ["out"]);
    assert_eq!(fs::read_to_string(fixture.root.join("prior/out")).unwrap(), "prior");
    assert_outside_untouched(&fixture);
}
