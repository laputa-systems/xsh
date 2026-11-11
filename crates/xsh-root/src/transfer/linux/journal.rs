use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fs::{self, AtFlags, FileType, FlockOperation, Mode, OFlags};
use rustix::io::Errno;

use crate::FileKind;
use crate::transfer::{
    Ancestor, Boundary, EntryIdentity, Publication, Removal, RemovalError, Retirement,
};

use super::{MountIdentity, directory_state, observation_from_stat};

const PREFIX: &[u8] = b".fdu-quarantine-v2-";
const MAGIC: &[u8] = b"fdu quarantine\0\x02";
const INTENT_MAGIC: &[u8] = b"fdu capture\0\x02";
const MAX_RECORD: usize = 1024 * 1024;
const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::CLOEXEC);

#[derive(Clone, Debug)]
enum RecordKind {
    Retirement,
    Stage,
    Replacement(Publication),
}

struct Intent {
    root: EntryIdentity,
    parents: Vec<Ancestor>,
    name: OsString,
    expected: Option<EntryIdentity>,
    kind: RecordKind,
}

/// A locked, versioned journal directory for one root.
///
/// The session owns a private mode-0700 directory in the root, records stage,
/// retirement and replacement intents, and holds a nonblocking exclusive lock
/// on the root for its lifetime.
pub struct Session {
    owner: u32,
    root: RootLock,
    directory: OwnedFd,
    name: OsString,
    mount: MountIdentity,
    next: AtomicU64,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Session")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

struct RootLock {
    fd: OwnedFd,
    owner: u32,
}

impl AsFd for RootLock {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl RootLock {
    fn release(&self) -> io::Result<()> {
        if std::process::id() == self.owner {
            retry(|| fs::flock(self.fd.as_fd(), FlockOperation::Unlock))?;
        }
        Ok(())
    }
}

impl Drop for RootLock {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

fn euid() -> u32 {
    rustix::process::geteuid().as_raw()
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn retry<T>(mut operation: impl FnMut() -> rustix::io::Result<T>) -> io::Result<T> {
    loop {
        match operation() {
            Err(Errno::INTR) => continue,
            result => return result.map_err(Into::into),
        }
    }
}

fn sync(fd: BorrowedFd<'_>) -> io::Result<()> {
    retry(|| fs::fsync(fd))
}

pub(super) fn read_names(fd: BorrowedFd<'_>) -> io::Result<Vec<OsString>> {
    let mut names = Vec::new();
    for entry in fs::Dir::read_from(fd)? {
        let entry = entry?;
        let bytes = entry.file_name().to_bytes();
        if bytes != b"." && bytes != b".." {
            names.push(OsString::from_vec(bytes.to_vec()));
        }
    }
    Ok(names)
}

fn entry_identity(stat: &fs::Stat) -> EntryIdentity {
    observation_from_stat(stat).identity
}

fn take_kind(bytes: &mut &[u8]) -> io::Result<FileKind> {
    crate::transfer::kind_from_code(take(bytes, 1)?[0])
}

fn read(fd: BorrowedFd<'_>, name: &OsStr) -> io::Result<Vec<u8>> {
    let file = retry(|| {
        fs::openat(
            fd,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
    })?;
    let metadata = retry(|| fs::fstat(&file))?;
    if FileType::from_raw_mode(metadata.st_mode) != FileType::RegularFile
        || metadata.st_size < 0
        || metadata.st_size as u64 > MAX_RECORD as u64
        || metadata.st_uid != euid()
        || metadata.st_nlink != 1
        || metadata.st_mode & 0o077 != 0
    {
        return Err(invalid(
            "recovery record owner, mode, type, or size is invalid",
        ));
    }
    let mut bytes = Vec::new();
    let mut file = std::fs::File::from(file);
    Read::by_ref(&mut file)
        .take(MAX_RECORD as u64 + 1)
        .read_to_end(&mut bytes)?;
    let after = retry(|| fs::fstat(&file))?;
    if metadata.st_dev != after.st_dev
        || metadata.st_ino != after.st_ino
        || metadata.st_size != after.st_size
        || metadata.st_mode != after.st_mode
        || metadata.st_uid != after.st_uid
        || metadata.st_gid != after.st_gid
        || metadata.st_nlink != after.st_nlink
        || metadata.st_mtime != after.st_mtime
        || metadata.st_mtime_nsec != after.st_mtime_nsec
        || metadata.st_ctime != after.st_ctime
        || metadata.st_ctime_nsec != after.st_ctime_nsec
    {
        return Err(invalid("recovery record changed while reading"));
    }
    if bytes.len() > MAX_RECORD {
        return Err(invalid("recovery record exceeds bound"));
    }
    Ok(bytes)
}

fn write_pending(fd: BorrowedFd<'_>, temporary: &OsStr, bytes: &[u8]) -> io::Result<std::fs::File> {
    if bytes.len() > MAX_RECORD {
        return Err(invalid("recovery record exceeds bound"));
    }
    let record = retry(|| {
        fs::openat(
            fd,
            temporary,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
    })?;
    retry(|| fs::fchmod(&record, Mode::RUSR | Mode::WUSR))?;
    let mut file = std::fs::File::from(record);
    file.write_all(bytes)?;
    Ok(file)
}

fn publish(fd: BorrowedFd<'_>, temporary: &OsStr, name: &OsStr, bytes: &[u8]) -> io::Result<()> {
    let file = write_pending(fd, temporary, bytes)?;
    file.sync_all()?;
    fs::renameat_with(fd, temporary, fd, name, fs::RenameFlags::NOREPLACE)?;
    sync(fd)
}

fn put_file_identity(bytes: &mut Vec<u8>, identity: EntryIdentity) {
    bytes.extend_from_slice(&identity.device.to_le_bytes());
    bytes.extend_from_slice(&identity.inode.to_le_bytes());
}

fn take_file_identity(bytes: &mut &[u8]) -> io::Result<EntryIdentity> {
    let device = u64::from_le_bytes(take(bytes, 8)?.try_into().unwrap());
    let inode = u64::from_le_bytes(take(bytes, 8)?.try_into().unwrap());
    Ok(EntryIdentity {
        device,
        inode,
        kind: FileKind::Directory,
    })
}

fn put_identity(bytes: &mut Vec<u8>, identity: EntryIdentity) {
    put_file_identity(bytes, identity);
    bytes.push(crate::transfer::kind_code(identity.kind));
}

fn put_mount(bytes: &mut Vec<u8>, mount: MountIdentity) {
    bytes.extend_from_slice(&mount.device.to_le_bytes());
    bytes.extend_from_slice(&mount.mount_id.to_le_bytes());
}

fn put_name(bytes: &mut Vec<u8>, name: &OsStr) -> io::Result<()> {
    crate::transfer::validate_name(name.as_bytes())?;
    bytes.extend_from_slice(
        &u32::try_from(name.len())
            .map_err(|_| invalid("recovery name is too long"))?
            .to_le_bytes(),
    );
    bytes.extend_from_slice(name.as_bytes());
    Ok(())
}

fn take<'a>(bytes: &mut &'a [u8], len: usize) -> io::Result<&'a [u8]> {
    if bytes.len() < len {
        return Err(invalid("truncated recovery record"));
    }
    let (value, rest) = bytes.split_at(len);
    *bytes = rest;
    Ok(value)
}

fn take_identity(bytes: &mut &[u8]) -> io::Result<EntryIdentity> {
    let device = u64::from_le_bytes(take(bytes, 8)?.try_into().unwrap());
    let inode = u64::from_le_bytes(take(bytes, 8)?.try_into().unwrap());
    Ok(EntryIdentity {
        device,
        inode,
        kind: take_kind(bytes)?,
    })
}

fn take_mount(bytes: &mut &[u8]) -> io::Result<MountIdentity> {
    let device = u64::from_le_bytes(take(bytes, 8)?.try_into().unwrap());
    let mount_id = u64::from_le_bytes(take(bytes, 8)?.try_into().unwrap());
    Ok(MountIdentity { device, mount_id })
}

fn take_name(bytes: &mut &[u8]) -> io::Result<OsString> {
    let len = u32::from_le_bytes(take(bytes, 4)?.try_into().unwrap()) as usize;
    let name = take(bytes, len)?;
    crate::transfer::validate_name(name)?;
    Ok(OsString::from_vec(name.to_vec()))
}

impl Intent {
    fn encode(&self) -> io::Result<Vec<u8>> {
        let mut bytes = INTENT_MAGIC.to_vec();
        put_file_identity(&mut bytes, self.root);
        bytes.extend_from_slice(
            &u32::try_from(self.parents.len())
                .map_err(|_| invalid("recovery path is too deep"))?
                .to_le_bytes(),
        );
        for parent in &self.parents {
            put_name(&mut bytes, &parent.name)?;
            put_file_identity(&mut bytes, parent.identity);
        }
        put_name(&mut bytes, &self.name)?;
        bytes.push(match &self.kind {
            RecordKind::Retirement => 0,
            RecordKind::Stage => 1,
            RecordKind::Replacement(_) => 2,
        });
        match (&self.kind, self.expected) {
            (RecordKind::Retirement | RecordKind::Replacement(_), Some(expected)) => {
                put_identity(&mut bytes, expected);
            }
            (RecordKind::Stage, None) => {}
            _ => return Err(invalid("recovery record kind and expected object disagree")),
        }
        if let RecordKind::Replacement(publication) = &self.kind {
            put_identity(&mut bytes, publication.source);
            put_file_identity(&mut bytes, publication.destination_parent);
            put_name(&mut bytes, &publication.destination_name)?;
        }
        if bytes.len() > MAX_RECORD {
            return Err(invalid("recovery record exceeds bound"));
        }
        Ok(bytes)
    }

    fn decode(mut bytes: &[u8]) -> io::Result<Self> {
        if take(&mut bytes, INTENT_MAGIC.len())? != INTENT_MAGIC {
            return Err(invalid("unknown recovery intent version"));
        }
        let root = take_file_identity(&mut bytes)?;
        let count = u32::from_le_bytes(take(&mut bytes, 4)?.try_into().unwrap()) as usize;
        if count > bytes.len() / 21 {
            return Err(invalid("invalid recovery ancestor count"));
        }
        let mut parents = Vec::with_capacity(count);
        for _ in 0..count {
            parents.push(Ancestor {
                name: take_name(&mut bytes)?,
                identity: take_file_identity(&mut bytes)?,
            });
        }
        let name = take_name(&mut bytes)?;
        let tag = take(&mut bytes, 1)?[0];
        if tag > 2 {
            return Err(invalid("unknown recovery record kind"));
        }
        let expected = if tag == 1 {
            None
        } else {
            Some(take_identity(&mut bytes)?)
        };
        let kind = match tag {
            0 => RecordKind::Retirement,
            1 => RecordKind::Stage,
            2 => {
                let source = take_identity(&mut bytes)?;
                let destination_parent = take_file_identity(&mut bytes)?;
                RecordKind::Replacement(Publication::new(
                    source,
                    destination_parent,
                    take_name(&mut bytes)?,
                )?)
            }
            _ => unreachable!(),
        };
        if !bytes.is_empty() {
            return Err(invalid("trailing recovery record data"));
        }
        Ok(Self {
            root,
            parents,
            name,
            expected,
            kind,
        })
    }

    fn parent(&self, root: BorrowedFd<'_>, mount: &MountIdentity) -> io::Result<OwnedFd> {
        let (found, found_mount) = directory_state(root)?;
        if found != self.root || found_mount != *mount {
            return Err(invalid("recovery root identity changed"));
        }
        parent_of(root, &self.parents, mount)
    }
}

fn parent_of(
    root: BorrowedFd<'_>,
    parents: &[Ancestor],
    mount: &MountIdentity,
) -> io::Result<OwnedFd> {
    let mut fd = retry(|| fs::openat(root, c".", DIRECTORY_FLAGS, Mode::empty()))?;
    for parent in parents {
        let next = retry(|| fs::openat(&fd, &parent.name, DIRECTORY_FLAGS, Mode::empty()))?;
        let (found, found_mount) = directory_state(next.as_fd())?;
        if found != parent.identity || found_mount != *mount {
            return Err(invalid("recovery parent identity or mount changed"));
        }
        fd = next;
    }
    Ok(fd)
}

fn retained(workspace: &OsStr, capture: Option<&OsStr>, error: io::Error) -> io::Error {
    let mut location = std::path::PathBuf::from(workspace);
    if let Some(capture) = capture {
        location.push(capture);
    }
    io::Error::new(
        error.kind(),
        format!("{error}; retained at scan root/{location:?}; inspect it before retrying"),
    )
}

fn abandon_fresh_session(root: BorrowedFd<'_>, name: &OsStr) -> io::Result<()> {
    let observed = match retry(|| fs::statat(root, name, AtFlags::SYMLINK_NOFOLLOW)) {
        Ok(stat) => stat,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if FileType::from_raw_mode(observed.st_mode) != FileType::Directory
        || observed.st_uid != euid()
    {
        return Err(invalid(
            "fresh session cleanup found a foreign or non-directory entry",
        ));
    }
    let expected = entry_identity(&observed);
    let directory = retry(|| fs::openat(root, name, DIRECTORY_FLAGS, Mode::empty()))?;
    if entry_identity(&retry(|| fs::fstat(directory.as_fd()))?) != expected {
        return Err(invalid(
            "fresh session cleanup found a substituted directory",
        ));
    }
    for record in [OsStr::new("pending-header"), OsStr::new("header")] {
        match fs::unlinkat(&directory, record, AtFlags::empty()) {
            Ok(()) | Err(Errno::NOENT) => {}
            Err(error) => return Err(error.into()),
        }
    }
    let _ = sync(directory.as_fd());
    fs::unlinkat(root, name, AtFlags::REMOVEDIR)?;
    let _ = sync(root);
    Ok(())
}

fn start_failure(error: io::Error) -> io::Error {
    let hint = if error.kind() == io::ErrorKind::Unsupported {
        "this filesystem does not support the operations XSH staging requires"
    } else {
        "no entry was captured"
    };
    io::Error::new(
        error.kind(),
        format!("staging session initialization failed: {error}; {hint}"),
    )
}

fn lock(root: BorrowedFd<'_>) -> io::Result<RootLock> {
    let fd = retry(|| fs::openat(root, c".", DIRECTORY_FLAGS, Mode::empty()))?;
    fs::flock(&fd, FlockOperation::NonBlockingLockExclusive).map_err(|error| {
        io::Error::new(
            io::Error::from(error).kind(),
            "another filesystem operation owns this directory",
        )
    })?;
    Ok(RootLock {
        fd,
        owner: std::process::id(),
    })
}

fn validate_directory(fd: BorrowedFd<'_>) -> io::Result<()> {
    let stat = retry(|| fs::fstat(fd))?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::Directory
        || stat.st_uid != euid()
        || stat.st_mode & 0o077 != 0
    {
        return Err(invalid(
            "quarantine directory owner, mode, or type is invalid",
        ));
    }
    Ok(())
}

fn recover(root: BorrowedFd<'_>) -> io::Result<bool> {
    let (root_identity, mount) = directory_state(root)?;
    let mut recovered = false;
    for name in read_names(root)? {
        let bytes = name.as_bytes();
        if bytes.starts_with(b".ish-fs-stage-")
            || bytes.starts_with(b".ish-fs-retired-")
            || bytes.starts_with(b".ish-fs-intent-")
        {
            return Err(retained(
                &name,
                None,
                invalid("legacy ish artifact requires inspection; ownership is unknown"),
            ));
        }
        if !bytes.starts_with(b".fdu-quarantine-") {
            continue;
        }
        if !bytes.starts_with(PREFIX) {
            return Err(retained(&name, None, invalid("unknown quarantine version")));
        }
        let suffix = &bytes[PREFIX.len()..];
        if suffix.len() != 32
            || !suffix
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        {
            return Err(retained(&name, None, invalid("invalid quarantine session name")));
        }
        let directory = retry(|| fs::openat(root, &name, DIRECTORY_FLAGS, Mode::empty()))
            .map_err(|error| retained(&name, None, error))?;
        validate_directory(directory.as_fd()).map_err(|error| retained(&name, None, error))?;
        let header = read(directory.as_fd(), OsStr::new("header")).map_err(|error| {
            retained(
                &name,
                None,
                io::Error::new(
                    error.kind(),
                    format!("ambiguous quarantine directory: {error}"),
                ),
            )
        })?;
        if !header.starts_with(MAGIC) {
            return Err(retained(
                &name,
                None,
                invalid("unknown or damaged quarantine header"),
            ));
        }
        let mut data = &header[MAGIC.len()..];
        let recorded_root =
            take_file_identity(&mut data).map_err(|error| retained(&name, None, error))?;
        let uid = u32::from_le_bytes(take(&mut data, 4)?.try_into().unwrap());
        let session = take_file_identity(&mut data)?;
        let recorded_mount = take_mount(&mut data)?;
        if recorded_mount != mount {
            return Err(retained(&name, None, invalid("quarantine mount identity changed")));
        }
        if uid != euid() || session != directory_state(directory.as_fd())?.0 {
            return Err(retained(
                &name,
                None,
                invalid("quarantine header owner or session identity changed"),
            ));
        }
        if recorded_root != root_identity
            || !data.is_empty()
            || directory_state(directory.as_fd())?.1 != mount
        {
            return Err(retained(
                &name,
                None,
                invalid("quarantine root identity, format, or mount changed"),
            ));
        }
        recover_directory(root, directory.as_fd(), &name, &mount)?;
        fs::unlinkat(&directory, c"header", AtFlags::empty())?;
        sync(directory.as_fd())?;
        fs::unlinkat(root, &name, AtFlags::REMOVEDIR)?;
        sync(root)?;
        recovered = true;
    }
    Ok(recovered)
}

fn recover_directory(
    root: BorrowedFd<'_>,
    directory: BorrowedFd<'_>,
    workspace: &OsStr,
    mount: &MountIdentity,
) -> io::Result<()> {
    for name in read_names(directory)? {
        let bytes = name.as_bytes();
        if bytes == b"header" {
            continue;
        }
        if bytes.starts_with(b"pending-") {
            return Err(retained(
                workspace,
                Some(&name),
                invalid("unpublished recovery record requires inspection"),
            ));
        }
        if let Some(id) = bytes.strip_suffix(b".entry") {
            match retry(|| fs::statat(directory, &name, AtFlags::SYMLINK_NOFOLLOW)) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(retained(workspace, Some(&name), error)),
                Ok(_) => {}
            }
            let mut record = id.to_vec();
            record.extend_from_slice(b".intent");
            if retry(|| fs::statat(directory, OsStr::from_bytes(&record), AtFlags::SYMLINK_NOFOLLOW))
                .is_ok()
            {
                continue;
            }
            return Err(retained(
                workspace,
                Some(&name),
                invalid("unrecognized quarantine entry"),
            ));
        }
        let id = bytes
            .strip_suffix(b".intent")
            .ok_or_else(|| retained(workspace, Some(&name), invalid("unrecognized quarantine entry")))?;
        if id.is_empty() || !id.iter().all(u8::is_ascii_digit) {
            return Err(retained(
                workspace,
                Some(&name),
                invalid("invalid capture identifier"),
            ));
        }
        let capture = OsString::from_vec([id, b".entry"].concat());
        let intent = Intent::decode(
            &read(directory, &name).map_err(|error| retained(workspace, Some(&name), error))?,
        )
        .map_err(|error| retained(workspace, Some(&name), error))?;
        let parent =
            intent
                .parent(root, mount)
                .map_err(|error| retained(workspace, Some(&capture), error))?;
        match retry(|| fs::statat(directory, &capture, AtFlags::SYMLINK_NOFOLLOW)) {
            Ok(stat) => {
                if matches!(intent.kind, RecordKind::Stage) {
                    return Err(retained(
                        workspace,
                        Some(&capture),
                        invalid("unpublished staging object requires inspection"),
                    ));
                }
                if let Some(expected) = intent.expected
                    && entry_identity(&stat) != expected
                {
                    return Err(retained(
                        workspace,
                        Some(&capture),
                        invalid("captured object identity or kind changed"),
                    ));
                }
                fs::renameat_with(
                    directory,
                    &capture,
                    &parent,
                    &intent.name,
                    fs::RenameFlags::NOREPLACE,
                )
                .map_err(|error| retained(workspace, Some(&capture), error.into()))?;
                sync(parent.as_fd()).map_err(|error| retained(workspace, None, error))?;
                sync(directory).map_err(|error| retained(workspace, None, error))?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if let RecordKind::Replacement(publication) = &intent.kind {
                    let source = retry(|| {
                        fs::statat(parent.as_fd(), &intent.name, AtFlags::SYMLINK_NOFOLLOW)
                    })
                    .map(|stat| entry_identity(&stat));
                    if !source.is_ok_and(|source| source == publication.source) {
                        return Err(retained(
                            workspace,
                            Some(&name),
                            invalid(
                                "replacement interrupted before capture; publication is not replayed",
                            ),
                        ));
                    }
                }
                sync(parent.as_fd())?;
                sync(directory)?;
            }
            Err(error) => return Err(retained(workspace, Some(&capture), error)),
        }
        fs::unlinkat(directory, &name, AtFlags::empty())?;
        sync(directory)?;
    }
    Ok(())
}

fn random_bytes() -> io::Result<[u8; 16]> {
    let mut random = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    Ok(random)
}

fn create_session_directory(
    root: BorrowedFd<'_>,
    mut random: impl FnMut() -> io::Result<[u8; 16]>,
) -> io::Result<OsString> {
    for _ in 0..16 {
        let mut name = PREFIX.to_vec();
        for byte in random()? {
            name.extend_from_slice(format!("{byte:02x}").as_bytes());
        }
        let name = OsString::from_vec(name);
        match fs::mkdirat(root, &name, Mode::RWXU) {
            Ok(()) => return Ok(name),
            Err(Errno::EXIST) => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "quarantine random name collision limit reached",
    ))
}

fn open_created_directory(root: BorrowedFd<'_>, name: &OsStr) -> io::Result<OwnedFd> {
    let held = retry(|| {
        fs::openat(
            root,
            name,
            OFlags::PATH | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
    })?;
    validate_directory(held.as_fd())?;
    let path = format!("/proc/self/fd/{}", held.as_raw_fd());
    retry(|| fs::chmodat(root, path.as_str(), Mode::RWXU, AtFlags::empty()))?;
    let directory = retry(|| fs::openat(&held, c".", DIRECTORY_FLAGS, Mode::empty()))?;
    if directory_state(directory.as_fd())?.0 != directory_state(held.as_fd())?.0 {
        return Err(invalid("session identity changed while opening"));
    }
    Ok(directory)
}

impl Session {
    /// Locks `root`, recovers any prior journal there, and creates a new
    /// session directory.
    ///
    /// A root with recovered state fails so the caller rescans before acting.
    pub fn start(root: BorrowedFd<'_>) -> io::Result<Self> {
        let root_lock = lock(root)?;
        if recover(root_lock.as_fd())? {
            return Err(io::Error::other(
                "interrupted filesystem operation state recovered; rescan before continuing",
            ));
        }
        let mount = super::mount_identity(root_lock.as_fd())?;
        let name = create_session_directory(root_lock.as_fd(), random_bytes)?;
        let initiated = (|| -> io::Result<OwnedFd> {
            let directory = open_created_directory(root_lock.as_fd(), &name)?;
            (|| {
                fs::fchmod(&directory, Mode::RWXU)?;
                validate_directory(directory.as_fd())?;
                let (session_identity, session_mount) = directory_state(directory.as_fd())?;
                if session_mount != mount {
                    return Err(invalid("quarantine is on another mount"));
                }
                let mut header = MAGIC.to_vec();
                put_file_identity(&mut header, directory_state(root_lock.as_fd())?.0);
                header.extend_from_slice(&euid().to_le_bytes());
                put_file_identity(&mut header, session_identity);
                put_mount(&mut header, mount);
                publish(
                    directory.as_fd(),
                    OsStr::new("pending-header"),
                    OsStr::new("header"),
                    &header,
                )?;
                sync(root_lock.as_fd())
            })()?;
            Ok(directory)
        })();
        let directory = match initiated {
            Ok(directory) => directory,
            Err(error) => {
                return Err(match abandon_fresh_session(root_lock.as_fd(), &name) {
                    Ok(()) => start_failure(error),
                    Err(cleanup) => io::Error::new(
                        error.kind(),
                        format!(
                            "{error}; retained at scan root/{name:?}; empty staging directory could not be removed: {cleanup}"
                        ),
                    ),
                });
            }
        };
        Ok(Self {
            owner: std::process::id(),
            root: root_lock,
            directory,
            name,
            mount,
            next: AtomicU64::new(0),
        })
    }

    fn require_owner(&self) -> io::Result<()> {
        if std::process::id() != self.owner {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "quarantine session belongs to another process",
            ));
        }
        Ok(())
    }

    /// Locks `root` and recovers any prior journal there without creating a
    /// session.
    ///
    /// A root with recovered state fails so the caller rescans before acting.
    pub fn check_recovery(root: BorrowedFd<'_>) -> io::Result<()> {
        let root_lock = lock(root)?;
        if recover(root_lock.as_fd())? {
            return Err(io::Error::other(
                "interrupted filesystem operation state recovered; rescan before continuing",
            ));
        }
        Ok(())
    }

    /// The open private session directory.
    pub fn directory(&self) -> BorrowedFd<'_> {
        self.directory.as_fd()
    }

    /// The session directory name within the root.
    pub fn name(&self) -> &OsStr {
        &self.name
    }

    /// Reserves the next capture name for a staged object.
    pub fn stage_name(&self) -> OsString {
        OsString::from(format!("{}.entry", self.next.fetch_add(1, Ordering::Relaxed)))
    }

    /// Records an unpublished staged object before it is created.
    pub fn register_stage(&self, name: &OsStr) -> io::Result<OsString> {
        self.require_owner()?;
        let id = name
            .as_bytes()
            .strip_suffix(b".entry")
            .ok_or_else(|| invalid("stage name is not a journal entry"))?;
        if id.is_empty() || !id.iter().all(u8::is_ascii_digit) {
            return Err(invalid("invalid stage identifier"));
        }
        let record = OsString::from_vec([id, b".intent"].concat());
        let pending = OsString::from_vec([b"pending-", id].concat());
        let intent = Intent {
            root: directory_state(self.root.as_fd())?.0,
            parents: Vec::new(),
            name: name.to_os_string(),
            expected: None,
            kind: RecordKind::Stage,
        };
        publish(
            self.directory.as_fd(),
            &pending,
            &record,
            &intent.encode()?,
        )?;
        Ok(record)
    }

    /// Removes a stage record after the staged object was published or
    /// cleaned up.
    pub fn clear_stage(&self, record: &OsStr) -> io::Result<()> {
        self.require_owner()?;
        self.clear_record(record)
    }

    /// Captures and removes one entry under the journal.
    ///
    /// The callback observes each boundary. A capture is restored before an
    /// unlink on failure; a completed unlink is never replayed by recovery.
    pub fn remove(
        &self,
        parent: BorrowedFd<'_>,
        request: Retirement,
        authorize: impl FnMut(Boundary, BorrowedFd<'_>, &OsStr) -> io::Result<()>,
    ) -> Result<Removal, RemovalError> {
        self.remove_with_publication(parent, request, None, || Ok(()), authorize)
    }

    /// Captures and removes the displaced object of an atomic replacement.
    ///
    /// The replacement intent names both objects before `publish_entry`
    /// exchanges them; recovery never exchanges them again.
    pub fn remove_after_publication(
        &self,
        parent: BorrowedFd<'_>,
        request: Retirement,
        publication: Publication,
        publish_entry: impl FnOnce() -> io::Result<()>,
        authorize: impl FnMut(Boundary, BorrowedFd<'_>, &OsStr) -> io::Result<()>,
    ) -> Result<Removal, RemovalError> {
        self.remove_with_publication(
            parent,
            request,
            Some(publication),
            publish_entry,
            authorize,
        )
    }

    fn remove_with_publication(
        &self,
        parent: BorrowedFd<'_>,
        request: Retirement,
        publication: Option<Publication>,
        publish_entry: impl FnOnce() -> io::Result<()>,
        mut authorize: impl FnMut(Boundary, BorrowedFd<'_>, &OsStr) -> io::Result<()>,
    ) -> Result<Removal, RemovalError> {
        self.require_owner()
            .map_err(|error| RemovalError {
                removed: false,
                error,
            })?;
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let capture = OsString::from(format!("{id}.entry"));
        let record = OsString::from(format!("{id}.intent"));
        let pending = OsString::from(format!("pending-{id}"));
        let before = publication
            .as_ref()
            .map_or(request.expected, |publication| publication.source);
        let intent = Intent {
            root: directory_state(self.root.as_fd())
                .map_err(|error| RemovalError {
                    removed: false,
                    error,
                })?
                .0,
            parents: request.parents,
            name: request.name,
            expected: Some(request.expected),
            kind: publication.map_or(RecordKind::Retirement, RecordKind::Replacement),
        };
        let mut captured = false;
        let mut removed = false;
        let expected = request.expected;
        let result = (|| {
            let checked = intent.parent(self.root.as_fd(), &self.mount)?;
            if directory_state(checked.as_fd())? != directory_state(parent)? {
                return Err(invalid("source parent identity or mount changed"));
            }
            match retry(|| fs::statat(parent, &intent.name, AtFlags::SYMLINK_NOFOLLOW)) {
                Ok(stat) if entry_identity(&stat) != before => return Ok(Removal::Changed),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Ok(Removal::AlreadyAbsent);
                }
                Err(error) => return Err(error),
            }
            publish(
                self.directory.as_fd(),
                &pending,
                &record,
                &intent.encode()?,
            )?;
            authorize(Boundary::IntentPublished, parent, &intent.name)?;
            if matches!(&intent.kind, RecordKind::Replacement(_)) {
                if entry_identity(&retry(|| {
                    fs::statat(parent, &intent.name, AtFlags::SYMLINK_NOFOLLOW)
                })?) != before
                {
                    return Err(invalid("replacement source changed before publication"));
                }
                publish_entry()?;
                authorize(Boundary::Published, parent, &intent.name)?;
            }
            authorize(Boundary::BeforeCapture, parent, &intent.name)?;
            let checked = intent.parent(self.root.as_fd(), &self.mount)?;
            if directory_state(checked.as_fd())? != directory_state(parent)? {
                return Err(invalid("source parent identity or mount changed before capture"));
            }
            fs::renameat_with(
                parent,
                &intent.name,
                &self.directory,
                &capture,
                fs::RenameFlags::NOREPLACE,
            )?;
            captured = true;
            authorize(Boundary::Captured, self.directory.as_fd(), &capture)?;
            sync(parent)?;
            authorize(
                Boundary::CaptureParentSynced,
                self.directory.as_fd(),
                &capture,
            )?;
            sync(self.directory.as_fd())?;
            authorize(
                Boundary::CaptureDirectorySynced,
                self.directory.as_fd(),
                &capture,
            )?;
            if entry_identity(&retry(|| {
                fs::statat(self.directory.as_fd(), &capture, AtFlags::SYMLINK_NOFOLLOW)
            })?) != expected
            {
                let checked = intent.parent(self.root.as_fd(), &self.mount)?;
                self.restore(checked.as_fd(), &intent.name, &capture, &record)?;
                captured = false;
                return Ok(Removal::Changed);
            }
            authorize(Boundary::BeforeUnlink, self.directory.as_fd(), &capture)?;
            let checked = intent.parent(self.root.as_fd(), &self.mount)?;
            if directory_state(checked.as_fd())? != directory_state(parent)? {
                return Err(invalid("source parent identity or mount changed before removal"));
            }
            if entry_identity(&retry(|| {
                fs::statat(self.directory.as_fd(), &capture, AtFlags::SYMLINK_NOFOLLOW)
            })?) != expected
            {
                return Err(invalid("captured object changed before removal"));
            }
            fs::unlinkat(
                &self.directory,
                &capture,
                if expected.kind == FileKind::Directory {
                    AtFlags::REMOVEDIR
                } else {
                    AtFlags::empty()
                },
            )?;
            removed = true;
            authorize(Boundary::Unlinked, self.directory.as_fd(), &capture)?;
            sync(self.directory.as_fd())?;
            authorize(
                Boundary::RemovalDirectorySynced,
                self.directory.as_fd(),
                &capture,
            )?;
            authorize(Boundary::BeforeParentSync, parent, &intent.name)?;
            sync(parent)?;
            authorize(Boundary::ParentSynced, parent, &intent.name)?;
            fs::unlinkat(&self.directory, &record, AtFlags::empty())?;
            authorize(Boundary::IntentRemoved, parent, &intent.name)?;
            sync(self.directory.as_fd())?;
            authorize(Boundary::IntentRemovalSynced, parent, &intent.name)?;
            Ok(Removal::Removed)
        })();
        match result {
            Ok(result) => Ok(result),
            Err(error) => {
                let error = if error.raw_os_error() == Some(Errno::NOTEMPTY.raw_os_error()) {
                    io::Error::new(
                        error.kind(),
                        format!("directory gained unindexed children: {error}"),
                    )
                } else {
                    error
                };
                let error = if captured && !removed {
                    match intent
                        .parent(self.root.as_fd(), &self.mount)
                        .and_then(|checked| {
                            self.restore(checked.as_fd(), &intent.name, &capture, &record)?;
                            authorize(Boundary::Restored, checked.as_fd(), &intent.name)
                                .map_err(|verification| {
                                    io::Error::new(
                                        verification.kind(),
                                        format!(
                                            "source restored; restoration verification failed: {verification}"
                                        ),
                                    )
                                })
                        }) {
                        Ok(()) => error,
                        Err(restoration) => io::Error::new(
                            error.kind(),
                            format!("{error}; restoration failed: {restoration}"),
                        ),
                    }
                } else if !captured
                    && !removed
                    && (!matches!(&intent.kind, RecordKind::Replacement(_))
                        || retry(|| {
                            fs::statat(parent, &intent.name, AtFlags::SYMLINK_NOFOLLOW)
                        })
                        .ok()
                        .map(|stat| entry_identity(&stat))
                            == Some(before))
                    && retry(|| {
                        fs::statat(self.directory.as_fd(), &record, AtFlags::SYMLINK_NOFOLLOW)
                    })
                    .is_ok()
                {
                    match self.clear_record(&record) {
                        Ok(()) => error,
                        Err(cleanup) => io::Error::new(
                            error.kind(),
                            format!("{error}; intent cleanup failed: {cleanup}"),
                        ),
                    }
                } else {
                    error
                };
                let record_exists = retry(|| {
                    fs::statat(self.directory.as_fd(), &record, AtFlags::SYMLINK_NOFOLLOW)
                })
                .is_ok();
                let pending_exists = retry(|| {
                    fs::statat(self.directory.as_fd(), &pending, AtFlags::SYMLINK_NOFOLLOW)
                })
                .is_ok();
                let error = if record_exists || pending_exists {
                    let location = if retry(|| {
                        fs::statat(self.directory.as_fd(), &capture, AtFlags::SYMLINK_NOFOLLOW)
                    })
                    .is_ok()
                    {
                        &capture
                    } else if record_exists {
                        &record
                    } else {
                        &pending
                    };
                    retained(&self.name, Some(location), error)
                } else {
                    error
                };
                Err(RemovalError { removed, error })
            }
        }
    }

    fn restore(
        &self,
        parent: BorrowedFd<'_>,
        name: &OsStr,
        capture: &OsStr,
        record: &OsStr,
    ) -> io::Result<()> {
        fs::renameat_with(
            &self.directory,
            capture,
            parent,
            name,
            fs::RenameFlags::NOREPLACE,
        )?;
        sync(parent)?;
        sync(self.directory.as_fd())?;
        self.clear_record(record)
    }

    fn clear_record(&self, record: &OsStr) -> io::Result<()> {
        fs::unlinkat(&self.directory, record, AtFlags::empty())?;
        sync(self.directory.as_fd())
    }

    /// Removes the session directory when no unresolved record remains.
    pub fn finish(&self) -> io::Result<()> {
        self.require_owner()?;
        let occupied = read_names(self.directory.as_fd())?
            .iter()
            .any(|name| name != "header");
        if occupied {
            return Err(retained(
                &self.name,
                None,
                invalid("unresolved journal entries require recovery"),
            ));
        }
        fs::unlinkat(&self.directory, c"header", AtFlags::empty())?;
        sync(self.directory.as_fd())?;
        fs::unlinkat(&self.root, &self.name, AtFlags::REMOVEDIR)?;
        sync(self.root.as_fd())
    }
}
