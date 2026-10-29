use crate::archive::Archive;
use crate::budget::Budget;
use crate::cursor::Completion;
use crate::entry::{EntryKind, EntryMeta};
use crate::error::{Error, ErrorKind, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use xsh_root::{FileKind, NodeKind, OpenOptions, RenameMode, Root};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Collision {
    Refuse,
    Replace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Links {
    Refuse,
    Contained,
    Verbatim,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Special {
    Refuse,
    Create,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Modes {
    Plain,
    Exact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Ownership {
    Ignore,
    Apply,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    pub collision: Collision,
    pub links: Links,
    pub special: Special,
    pub modes: Modes,
    pub ownership: Ownership,
    pub strip_components: usize,
    pub members: Vec<Vec<u8>>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            collision: Collision::Refuse,
            links: Links::Contained,
            special: Special::Refuse,
            modes: Modes::Plain,
            ownership: Ownership::Ignore,
            strip_components: 0,
            members: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Report {
    pub entries: u64,
    pub files: u64,
    pub bytes: u64,
    pub completion: Completion,
}

type Components = Vec<Vec<u8>>;

pub fn normalize(name: &[u8]) -> Result<Components> {
    if name.first() == Some(&b'/') {
        return Err(Error::unsafe_entry(name, "member name is absolute"));
    }
    let mut parts = Vec::new();
    for part in name.split(|byte| *byte == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                return Err(Error::unsafe_entry(name, "member name contains `..`"));
            }
            part if part.contains(&0) => {
                return Err(Error::unsafe_entry(name, "member name contains a NUL byte"));
            }
            part => parts.push(part.to_vec()),
        }
    }
    Ok(parts)
}

fn to_path(components: &[Vec<u8>]) -> PathBuf {
    let mut path = PathBuf::new();
    for part in components {
        path.push(OsStr::from_bytes(part));
    }
    path
}

fn io_error(error: &io::Error, entry: &[u8]) -> Error {
    Error::from_host(error).with_entry(entry)
}

pub fn extract(
    archive: &mut Archive,
    dest: &Path,
    policy: &Policy,
    budget: &Arc<Budget>,
) -> Result<Report> {
    let filters = policy
        .members
        .iter()
        .map(|member| normalize(member))
        .collect::<Result<Vec<_>>>()?;
    let mut placement = Placement::prepare(dest)?;
    let outcome = placement
        .open_stage()
        .and_then(|stage| run(archive, &stage, &placement, policy, &filters, budget));
    match outcome {
        Ok(report) => Ok(report),
        Err(error) => match placement.abort() {
            Ok(()) => Err(error),
            Err(cleanup) => Err(error.with_cleanup(&cleanup)),
        },
    }
}

struct Stage {
    root: Root,
}

enum Target {
    Fresh {
        parent: Root,
        leaf: OsString,
        directory_mode: u32,
    },
    Existing {
        destination: Root,
    },
}

struct Placement {
    target: Target,
    created_ancestors: Vec<PathBuf>,
    stage_name: Option<OsString>,
}

static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Placement {
    fn prepare(dest: &Path) -> Result<Self> {
        let open = |error: io::Error| Error::from_open(&error);
        match fs::symlink_metadata(dest) {
            Ok(metadata) if metadata.is_dir() => {
                let destination = Root::open(dest).map_err(open)?;
                return Ok(Self {
                    target: Target::Existing { destination },
                    created_ancestors: Vec::new(),
                    stage_name: None,
                });
            }
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let resolved = fs::metadata(dest).map_err(|error| Error::from_open(&error))?;
                if resolved.is_dir() {
                    let destination = Root::open(dest).map_err(open)?;
                    return Ok(Self {
                        target: Target::Existing { destination },
                        created_ancestors: Vec::new(),
                        stage_name: None,
                    });
                }
                return Err(Error::new(
                    ErrorKind::Collision,
                    "destination exists and is not a directory",
                ));
            }
            Ok(_) => {
                return Err(Error::new(
                    ErrorKind::Collision,
                    "destination exists and is not a directory",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::from_open(&error)),
        }
        let leaf = dest
            .file_name()
            .ok_or_else(|| Error::options("destination has no final name"))?
            .to_os_string();
        let parent_path = match dest.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let created_ancestors = missing_ancestors(&parent_path);
        fs::create_dir_all(&parent_path).map_err(|error| Error::from_open(&error))?;
        let parent = Root::open(&parent_path).map_err(open)?;
        let directory_mode = probe_directory_mode(&parent)?;
        Ok(Self {
            target: Target::Fresh {
                parent,
                leaf,
                directory_mode,
            },
            created_ancestors,
            stage_name: None,
        })
    }

    fn holder(&self) -> &Root {
        match &self.target {
            Target::Fresh { parent, .. } => parent,
            Target::Existing { destination } => destination,
        }
    }

    fn open_stage(&mut self) -> Result<Stage> {
        for _ in 0..16 {
            let name = OsString::from(format!(
                ".xsh-archive-{}-{}",
                std::process::id(),
                STAGE_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            match self.holder().mkdir(&name, 0o700) {
                Ok(()) => {
                    self.stage_name = Some(name.clone());
                    let root = self
                        .holder()
                        .open_dir_strict(&name)
                        .map_err(|error| Error::from_host(&error))?;
                    return Ok(Stage { root });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(Error::from_host(&error).with_context("creating the staging directory")),
            }
        }
        Err(Error::new(
            ErrorKind::Io,
            "could not create a unique staging directory",
        ))
    }

    fn abort(&mut self) -> Result<()> {
        let mut failures = Vec::new();
        if let Some(name) = self.stage_name.take()
            && let Err(error) = remove_tree(self.holder(), Path::new(&name))
        {
            failures.push(format!("removing the staging directory: {error}"));
        }
        for ancestor in self.created_ancestors.iter().rev() {
            if let Err(error) = fs::remove_dir(ancestor) {
                failures.push(format!("removing {}: {error}", ancestor.display()));
            }
        }
        self.created_ancestors.clear();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::Io, failures.join("; ")))
        }
    }
}

fn missing_ancestors(path: &Path) -> Vec<PathBuf> {
    let mut missing = Vec::new();
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate.as_os_str().is_empty() || fs::symlink_metadata(candidate).is_ok() {
            break;
        }
        missing.push(candidate.to_path_buf());
        current = candidate.parent();
    }
    missing.reverse();
    missing
}

fn probe_directory_mode(parent: &Root) -> Result<u32> {
    let name = format!(
        ".xsh-archive-mode-{}-{}",
        std::process::id(),
        STAGE_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    parent
        .mkdir(&name, 0o777)
        .map_err(|error| Error::from_host(&error))?;
    let mode = parent.metadata(&name).map(|metadata| metadata.mode);
    let removed = parent.remove_dir(&name);
    let mode = mode.map_err(|error| Error::from_host(&error))?;
    removed.map_err(|error| Error::from_host(&error))?;
    Ok(mode & 0o777)
}

pub fn remove_tree(root: &Root, path: &Path) -> io::Result<()> {
    match root.metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
        Ok(metadata) if metadata.kind != FileKind::Directory => return root.remove_file(path),
        Ok(_) => {}
    }
    let _ = root.set_mode(path, 0o700);
    for entry in root.read_dir(path)? {
        remove_tree(root, &path.join(&entry.name))?;
    }
    root.remove_dir(path)
}

struct State<'a> {
    stage: &'a Stage,
    policy: &'a Policy,
    filters: &'a [Components],
    known_dirs: BTreeSet<Components>,
    explicit_dirs: BTreeMap<Components, u32>,
    implicit_dirs: BTreeSet<Components>,
    seen: BTreeSet<Components>,
    files: BTreeSet<Components>,
    entries: u64,
    file_count: u64,
    bytes: u64,
}

fn run(
    archive: &mut Archive,
    stage: &Stage,
    placement: &Placement,
    policy: &Policy,
    filters: &[Components],
    budget: &Arc<Budget>,
) -> Result<Report> {
    let mut state = State {
        stage,
        policy,
        filters,
        known_dirs: BTreeSet::new(),
        explicit_dirs: BTreeMap::new(),
        implicit_dirs: BTreeSet::new(),
        seen: BTreeSet::new(),
        files: BTreeSet::new(),
        entries: 0,
        file_count: 0,
        bytes: 0,
    };
    while let Some(mut entry) = archive.next_entry()? {
        budget.checkpoint()?;
        let meta = entry.meta().clone();
        state.apply(&meta, &mut entry, budget)?;
    }
    let completion = archive.finish(true)?;
    if !completion.contains(Completion::STRUCTURE_COMPLETE)
        || !completion.contains(Completion::OUTER_COMPLETE)
    {
        return Err(Error::truncated("archive did not reach its end"));
    }
    budget.checkpoint()?;
    let report = Report {
        entries: state.entries,
        files: state.file_count,
        bytes: state.bytes,
        completion,
    };
    state.finish_directories(placement)?;
    publish(&state, placement, budget)?;
    Ok(report)
}

impl State<'_> {
    fn selected(&self, components: &Components) -> bool {
        self.filters.is_empty()
            || self
                .filters
                .iter()
                .any(|filter| components.starts_with(filter))
    }

    fn apply(&mut self, meta: &EntryMeta, body: &mut impl Read, budget: &Arc<Budget>) -> Result<()> {
        let full = normalize(&meta.name)?;
        if !self.selected(&full) {
            return Ok(());
        }
        if full.len() <= self.policy.strip_components {
            return Ok(());
        }
        let components: Components = full[self.policy.strip_components..].to_vec();
        self.entries += 1;
        match meta.kind {
            EntryKind::Directory => self.directory(meta, &components),
            EntryKind::File => self.file(meta, &components, body, budget),
            EntryKind::Symlink => self.symlink(meta, &components),
            EntryKind::Hardlink => self.hardlink(meta, &components),
            EntryKind::Fifo | EntryKind::CharDevice | EntryKind::BlockDevice => {
                self.special(meta, &components)
            }
            EntryKind::Other => Err(Error::unsupported("member has an unsupported entry type")
                .with_entry(&meta.name)),
        }
    }

    fn fail<T>(&self, meta: &EntryMeta, error: &io::Error) -> Result<T> {
        Err(io_error(error, &meta.name))
    }

    fn ensure_parents(&mut self, meta: &EntryMeta, components: &Components) -> Result<()> {
        let mut prefix: Components = Vec::new();
        for part in &components[..components.len().saturating_sub(1)] {
            prefix.push(part.clone());
            if self.known_dirs.contains(&prefix) {
                continue;
            }
            let path = to_path(&prefix);
            match self.stage.root.mkdir(&path, 0o700) {
                Ok(()) => {
                    self.implicit_dirs.insert(prefix.clone());
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    let existing = self
                        .stage
                        .root
                        .metadata(&path)
                        .map_err(|error| io_error(&error, &meta.name))?;
                    match existing.kind {
                        FileKind::Directory => {}
                        FileKind::Symlink => {
                            return Err(Error::unsafe_entry(
                                &meta.name,
                                "a parent of the member is a symbolic link",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorKind::Collision,
                                "a parent of the member is not a directory",
                            )
                            .with_entry(&meta.name));
                        }
                    }
                }
                Err(error) => return self.fail(meta, &error),
            }
            self.known_dirs.insert(prefix.clone());
        }
        Ok(())
    }

    fn claim(&mut self, meta: &EntryMeta, components: &Components) -> Result<bool> {
        let replacing = !self.seen.insert(components.clone());
        if replacing && self.policy.collision == Collision::Refuse {
            return Err(Error::new(ErrorKind::Collision, "member name appears twice")
                .with_entry(&meta.name));
        }
        Ok(replacing)
    }

    fn directory(&mut self, meta: &EntryMeta, components: &Components) -> Result<()> {
        if components.is_empty() {
            return Ok(());
        }
        self.ensure_parents(meta, components)?;
        let path = to_path(components);
        if !self.known_dirs.contains(components) {
            match self.stage.root.mkdir(&path, 0o700) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    let existing = self
                        .stage
                        .root
                        .metadata(&path)
                        .map_err(|error| io_error(&error, &meta.name))?;
                    if existing.kind != FileKind::Directory {
                        if self.policy.collision == Collision::Replace {
                            self.stage
                                .root
                                .remove_file(&path)
                                .map_err(|error| io_error(&error, &meta.name))?;
                            self.files.remove(components);
                            self.stage
                                .root
                                .mkdir(&path, 0o700)
                                .map_err(|error| io_error(&error, &meta.name))?;
                        } else {
                            return Err(Error::new(
                                ErrorKind::Collision,
                                "a directory member collides with a non-directory",
                            )
                            .with_entry(&meta.name));
                        }
                    }
                }
                Err(error) => return self.fail(meta, &error),
            }
            self.known_dirs.insert(components.clone());
        }
        self.implicit_dirs.remove(components);
        self.explicit_dirs
            .insert(components.clone(), self.directory_mode(meta, 0o755));
        self.apply_owner(meta, &path)?;
        Ok(())
    }

    fn directory_mode(&self, meta: &EntryMeta, default: u32) -> u32 {
        self.permission_bits(meta.mode.unwrap_or(default))
    }

    fn permission_bits(&self, mode: u32) -> u32 {
        match self.policy.modes {
            Modes::Plain => mode & 0o777,
            Modes::Exact => mode & 0o7777,
        }
    }

    fn apply_owner(&self, meta: &EntryMeta, path: &Path) -> Result<()> {
        if self.policy.ownership == Ownership::Apply
            && let (Some(uid), Some(gid)) = (meta.uid, meta.gid)
        {
            self.stage
                .root
                .set_owner(path, uid, gid)
                .map_err(|error| io_error(&error, &meta.name))?;
        }
        Ok(())
    }

    fn prepare_leaf(&mut self, meta: &EntryMeta, components: &Components) -> Result<bool> {
        if components.is_empty() {
            return Err(Error::unsafe_entry(&meta.name, "member names the extraction root"));
        }
        self.ensure_parents(meta, components)?;
        let replacing = self.claim(meta, components)?;
        if self.known_dirs.contains(components) {
            return Err(Error::new(
                ErrorKind::Collision,
                "a member collides with a directory of the same name",
            )
            .with_entry(&meta.name));
        }
        Ok(replacing)
    }

    fn clear(&mut self, meta: &EntryMeta, components: &Components) -> Result<()> {
        let path = to_path(components);
        match self.stage.root.remove_file(&path) {
            Ok(()) => {
                self.files.remove(components);
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => self.fail(meta, &error),
        }
    }

    fn file(
        &mut self,
        meta: &EntryMeta,
        components: &Components,
        body: &mut impl Read,
        budget: &Arc<Budget>,
    ) -> Result<()> {
        if meta.link_count.is_some_and(|count| count > 1) && meta.size == Some(0) {
            return Err(Error::unsupported(
                "cpio hard-link sets store their data with the last link and are not supported",
            )
            .with_entry(&meta.name));
        }
        let replacing = self.prepare_leaf(meta, components)?;
        let path = to_path(components);
        if replacing {
            self.clear(meta, components)?;
        }
        let mut file = self
            .stage
            .root
            .open_with(
                &path,
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .no_symlinks(true)
                    .mode(0o600),
            )
            .map_err(|error| io_error(&error, &meta.name))?;
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            budget.checkpoint()?;
            let count = match body.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(Error::from_read(&error).with_entry(&meta.name)),
            };
            file.write_all(&buffer[..count])
                .map_err(|error| io_error(&error, &meta.name))?;
            self.bytes += count as u64;
        }
        if self.policy.ownership == Ownership::Apply
            && let (Some(uid), Some(gid)) = (meta.uid, meta.gid)
        {
            std::os::unix::fs::fchown(&file, Some(uid), Some(gid))
                .map_err(|error| io_error(&error, &meta.name))?;
        }
        let mode = self.permission_bits(meta.mode.unwrap_or(0o644));
        file.set_permissions(fs::Permissions::from_mode(mode))
            .map_err(|error| io_error(&error, &meta.name))?;
        self.files.insert(components.clone());
        self.file_count += 1;
        Ok(())
    }

    fn symlink(&mut self, meta: &EntryMeta, components: &Components) -> Result<()> {
        let Some(target) = meta.link.as_deref().filter(|target| !target.is_empty()) else {
            return Err(Error::corrupt("symbolic link has no target").with_entry(&meta.name));
        };
        match self.policy.links {
            Links::Refuse => {
                return Err(Error::unsafe_entry(
                    &meta.name,
                    "symbolic links are refused by the extraction policy",
                ));
            }
            Links::Contained => contained(&meta.name, components, target)?,
            Links::Verbatim => {}
        }
        if target.contains(&0) {
            return Err(Error::unsafe_entry(&meta.name, "link target contains a NUL byte"));
        }
        let replacing = self.prepare_leaf(meta, components)?;
        let path = to_path(components);
        if replacing {
            self.clear(meta, components)?;
        }
        self.stage
            .root
            .symlink(OsStr::from_bytes(target), &path)
            .map_err(|error| io_error(&error, &meta.name))?;
        self.apply_owner(meta, &path)?;
        self.files.insert(components.clone());
        Ok(())
    }

    fn hardlink(&mut self, meta: &EntryMeta, components: &Components) -> Result<()> {
        if self.policy.links == Links::Refuse {
            return Err(Error::unsafe_entry(
                &meta.name,
                "hard links are refused by the extraction policy",
            ));
        }
        let Some(target_name) = meta.link.as_deref() else {
            return Err(Error::corrupt("hard link has no target").with_entry(&meta.name));
        };
        let target_full = normalize(target_name)?;
        if target_full.len() <= self.policy.strip_components {
            return Err(Error::unsafe_entry(
                &meta.name,
                "hard link target is stripped out of the extracted tree",
            ));
        }
        let target: Components = target_full[self.policy.strip_components..].to_vec();
        if !self.files.contains(&target) {
            return Err(Error::unsafe_entry(
                &meta.name,
                "hard link target is not an earlier regular file of this extraction",
            ));
        }
        let target_path = to_path(&target);
        let info = self
            .stage
            .root
            .metadata(&target_path)
            .map_err(|error| io_error(&error, &meta.name))?;
        if info.kind != FileKind::File {
            return Err(Error::unsafe_entry(
                &meta.name,
                "hard link target is not a regular file",
            ));
        }
        let replacing = self.prepare_leaf(meta, components)?;
        let path = to_path(components);
        if replacing {
            self.clear(meta, components)?;
        }
        self.stage
            .root
            .hard_link(&target_path, &path)
            .map_err(|error| io_error(&error, &meta.name))?;
        self.files.insert(components.clone());
        Ok(())
    }

    fn special(&mut self, meta: &EntryMeta, components: &Components) -> Result<()> {
        if self.policy.special == Special::Refuse {
            return Err(Error::unsafe_entry(
                &meta.name,
                "special files are refused by the extraction policy",
            ));
        }
        let kind = match meta.kind {
            EntryKind::Fifo => NodeKind::Fifo,
            EntryKind::CharDevice => NodeKind::CharDevice,
            _ => NodeKind::BlockDevice,
        };
        let (major, minor) = meta.device.unwrap_or((0, 0));
        let replacing = self.prepare_leaf(meta, components)?;
        let path = to_path(components);
        if replacing {
            self.clear(meta, components)?;
        }
        let mode = self.permission_bits(meta.mode.unwrap_or(0o600));
        self.stage
            .root
            .make_node(&path, kind, mode, major, minor)
            .map_err(|error| io_error(&error, &meta.name))?;
        self.apply_owner(meta, &path)?;
        self.files.insert(components.clone());
        Ok(())
    }

    fn finish_directories(&mut self, placement: &Placement) -> Result<()> {
        let mut modes: Vec<(Components, u32)> = self
            .implicit_dirs
            .iter()
            .map(|path| (path.clone(), 0o755))
            .chain(self.explicit_dirs.iter().map(|(path, mode)| (path.clone(), *mode)))
            .collect();
        let merged = match &placement.target {
            Target::Existing { destination } => existing_directories(destination, &modes),
            Target::Fresh { .. } => BTreeSet::new(),
        };
        modes.sort_by(|left, right| right.0.len().cmp(&left.0.len()).then_with(|| right.0.cmp(&left.0)));
        for (path, mode) in modes {
            if merged.contains(&path) {
                continue;
            }
            self.stage
                .root
                .set_mode(to_path(&path), mode)
                .map_err(|error| Error::from_host(&error).with_context("setting a directory mode"))?;
        }
        if let Target::Fresh { directory_mode, .. } = &placement.target {
            let name = placement
                .stage_name
                .as_ref()
                .expect("a stage exists while extracting");
            placement
                .holder()
                .set_mode(Path::new(name), *directory_mode)
                .map_err(|error| Error::from_host(&error).with_context("setting the destination mode"))?;
        }
        Ok(())
    }
}

fn existing_directories(destination: &Root, directories: &[(Components, u32)]) -> BTreeSet<Components> {
    directories
        .iter()
        .filter(|(path, _)| {
            destination
                .metadata(to_path(path))
                .is_ok_and(|metadata| metadata.kind == FileKind::Directory)
        })
        .map(|(path, _)| path.clone())
        .collect()
}

fn contained(entry: &[u8], link: &Components, target: &[u8]) -> Result<()> {
    if target.first() == Some(&b'/') {
        return Err(Error::unsafe_entry(entry, "symbolic link target is absolute"));
    }
    let mut depth = link.len().saturating_sub(1);
    for part in target.split(|byte| *byte == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                if depth == 0 {
                    return Err(Error::unsafe_entry(
                        entry,
                        "symbolic link target escapes the extraction root",
                    ));
                }
                depth -= 1;
            }
            _ => depth += 1,
        }
    }
    Ok(())
}

fn publish(state: &State<'_>, placement: &Placement, budget: &Arc<Budget>) -> Result<()> {
    let name = placement
        .stage_name
        .as_ref()
        .expect("a stage exists while extracting");
    match &placement.target {
        Target::Fresh { parent, leaf, .. } => parent
            .rename(Path::new(name), Path::new(leaf), RenameMode::NoReplace)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    Error::new(ErrorKind::Collision, "destination appeared during extraction")
                } else {
                    Error::from_host(&error).with_context("publishing the extracted tree")
                }
            }),
        Target::Existing { destination } => {
            let stage = Path::new(name);
            if state.policy.collision == Collision::Refuse {
                check_conflicts(destination, stage, Path::new(""), budget)?;
            }
            let mut published = 0_u64;
            merge(
                destination,
                stage,
                Path::new(""),
                state.policy.collision,
                &mut published,
                budget,
            )
            .map_err(|error| {
                if published > 0 {
                    error.with_context(&format!("{published} entries were already published"))
                } else {
                    error
                }
            })?;
            remove_tree(destination, stage).map_err(|error| Error::from_host(&error))
        }
    }
}

fn join(base: &Path, name: &OsStr) -> PathBuf {
    if base.as_os_str().is_empty() {
        PathBuf::from(name)
    } else {
        base.join(name)
    }
}

fn check_conflicts(destination: &Root, stage: &Path, relative: &Path, budget: &Arc<Budget>) -> Result<()> {
    let staged = join(stage, relative.as_os_str());
    let entries = destination_entries(destination, &staged)?;
    for entry in entries {
        budget.checkpoint()?;
        let relative_path = join(relative, &entry.0);
        let target = relative_path.clone();
        match destination.metadata(&target) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::from_host(&error)),
            Ok(existing) => {
                if entry.1 == FileKind::Directory && existing.kind == FileKind::Directory {
                    check_conflicts(destination, stage, &relative_path, budget)?;
                } else {
                    return Err(Error::new(
                        ErrorKind::Collision,
                        format!("destination already has {}", relative_path.display()),
                    )
                    .with_entry(relative_path.as_os_str().as_bytes()));
                }
            }
        }
    }
    Ok(())
}

fn destination_entries(destination: &Root, path: &Path) -> Result<Vec<(OsString, FileKind)>> {
    destination
        .read_dir(path)
        .map(|entries| entries.into_iter().map(|entry| (entry.name, entry.kind)).collect())
        .map_err(|error| Error::from_host(&error))
}

fn merge(
    destination: &Root,
    stage: &Path,
    relative: &Path,
    collision: Collision,
    published: &mut u64,
    budget: &Arc<Budget>,
) -> Result<()> {
    let staged_dir = join(stage, relative.as_os_str());
    for (name, kind) in destination_entries(destination, &staged_dir)? {
        budget.checkpoint()?;
        let relative_path = join(relative, &name);
        let from = stage.join(&relative_path);
        match destination.metadata(&relative_path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                destination
                    .rename(&from, &relative_path, RenameMode::NoReplace)
                    .map_err(|error| rename_error(&error, &relative_path))?;
                *published += 1;
            }
            Err(error) => return Err(Error::from_host(&error)),
            Ok(existing) if kind == FileKind::Directory && existing.kind == FileKind::Directory => {
                merge(destination, stage, &relative_path, collision, published, budget)?;
            }
            Ok(existing) if kind == FileKind::Directory && collision == Collision::Replace => {
                destination
                    .remove_file(&relative_path)
                    .map_err(|error| rename_error(&error, &relative_path))?;
                destination
                    .rename(&from, &relative_path, RenameMode::NoReplace)
                    .map_err(|error| rename_error(&error, &relative_path))?;
                let _ = existing;
                *published += 1;
            }
            Ok(existing) => {
                if collision == Collision::Refuse || existing.kind == FileKind::Directory {
                    return Err(Error::new(
                        ErrorKind::Collision,
                        format!("destination already has {}", relative_path.display()),
                    )
                    .with_entry(relative_path.as_os_str().as_bytes()));
                }
                destination
                    .rename(&from, &relative_path, RenameMode::Replace)
                    .map_err(|error| rename_error(&error, &relative_path))?;
                *published += 1;
            }
        }
    }
    Ok(())
}

fn rename_error(error: &io::Error, path: &Path) -> Error {
    if error.kind() == io::ErrorKind::AlreadyExists {
        Error::new(
            ErrorKind::Collision,
            format!("{} appeared in the destination during publication", path.display()),
        )
        .with_entry(path.as_os_str().as_bytes())
    } else {
        Error::from_host(error).with_entry(path.as_os_str().as_bytes())
    }
}
