use std::ffi::OsStr;
use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rustix::fs::{self, AtFlags, Mode, OFlags, RenameFlags};

use crate::FileKind;
use crate::transfer::{
    Action, Ancestor, Boundary, EntryIdentity, MergeRequest, Observation, Publication,
    PublishRequest, Removal, Retirement, Target, TreeTransfer, decide,
};

use super::journal::{Session, read_names};
use super::{observe, observe_fd, require_unchanged, validate_entry_name};

const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

pub(crate) fn publish(
    request: PublishRequest<'_>,
    mut authorize: impl FnMut(Boundary) -> io::Result<()>,
) -> io::Result<()> {
    let PublishRequest {
        source_parent,
        source_name,
        destination_parent,
        destination_name,
        expected_source,
        expected_destination,
        action,
        durable,
        cancellation,
        progress,
    } = request;
    validate_entry_name(source_name, "source")?;
    validate_entry_name(destination_name, "destination")?;
    if !matches!(action, Action::Transfer | Action::Replace) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "staged publication accepts transfer or replacement",
        ));
    }
    if action == Action::Replace && expected_destination.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "replacement requires the observed destination",
        ));
    }
    require_unchanged(source_parent, source_name, expected_source)?;
    match (action, expected_destination) {
        (Action::Transfer, _) => match observe(destination_parent, destination_name) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "destination appeared after preflight",
                ));
            }
            Err(error) => return Err(error),
        },
        (Action::Replace, Some(expected)) => {
            require_unchanged(destination_parent, destination_name, expected)?;
            if !matches!(
                (
                    expected_source.identity.kind,
                    expected.identity.kind
                ),
                (FileKind::File, FileKind::File)
                    | (FileKind::Symlink, FileKind::Symlink)
            ) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "replacement requires same-kind leaf objects",
                ));
            }
        }
        _ => unreachable!("replacement destination was validated"),
    }

    let session = Session::start(destination_parent)?;
    let stage_name = session.stage_name();
    let mut registration = None;
    let mut staged = None;
    let mut published = false;
    let result = (|| -> io::Result<()> {
        authorize(Boundary::SessionStarted)?;
        registration = Some(session.register_stage(&stage_name)?);
        authorize(Boundary::StageRegistered)?;
        super::transfer_tree(TreeTransfer {
            source_parent,
            source_name,
            destination_parent: session.directory(),
            destination_name: &stage_name,
            cancellation,
            progress,
        })?;
        authorize(Boundary::TransferComplete)?;
        require_unchanged(source_parent, source_name, expected_source)?;
        let staged_observation = observe(session.directory(), &stage_name)?;
        staged = Some(staged_observation);
        authorize(Boundary::BeforePublish)?;

        match action {
            Action::Transfer => {
                match observe(destination_parent, destination_name) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "destination appeared before publication",
                        ));
                    }
                    Err(error) => return Err(error),
                }
                fs::renameat_with(
                    session.directory(),
                    &stage_name,
                    destination_parent,
                    destination_name,
                    RenameFlags::NOREPLACE,
                )?;
                published = true;
                let actual = observe(destination_parent, destination_name)?;
                let mut expected_published = staged_observation;
                expected_published.ctime = actual.ctime;
                if !expected_published.same_version(actual) {
                    return Err(io::Error::other(
                        "published object changed during rename",
                    ));
                }
                authorize(Boundary::Published)?;
            }
            Action::Replace => {
                let expected = expected_destination.expect("replacement destination was validated");
                require_unchanged(destination_parent, destination_name, expected)?;
                let publication = Publication::new(
                    staged_observation.identity,
                    observe_fd(destination_parent)?.identity,
                    destination_name.to_os_string(),
                )?;
                let retirement = Retirement::new(
                    vec![Ancestor {
                        name: session.name().to_os_string(),
                        identity: observe_fd(session.directory())?.identity,
                    }],
                    stage_name.clone(),
                    expected.identity,
                )?;
                let outcome = session.remove_after_publication(
                    session.directory(),
                    retirement,
                    publication,
                    || {
                        require_unchanged(source_parent, source_name, expected_source)?;
                        require_unchanged(destination_parent, destination_name, expected)?;
                        replace_by_exchange(
                            session.directory(),
                            &stage_name,
                            destination_parent,
                            destination_name,
                            expected,
                        )?;
                        published = true;
                        let actual = observe(destination_parent, destination_name)?;
                        let mut expected_published = staged_observation;
                        expected_published.ctime = actual.ctime;
                        if !expected_published.same_version(actual) {
                            return Err(io::Error::other(
                                "published object changed during rename",
                            ));
                        }
                        Ok(())
                    },
                    |boundary, _, _| authorize(boundary),
                );
                match outcome {
                    Ok(Removal::Removed) => {}
                    Ok(Removal::AlreadyAbsent | Removal::Changed) => {
                        return Err(io::Error::other(
                            "displaced destination changed before retirement",
                        ));
                    }
                    Err(error) => return Err(error.error),
                }
            }
            _ => unreachable!("publication action was validated"),
        }
        if durable {
            fs::fsync(destination_parent)?;
        }
        session.clear_stage(registration.as_deref().expect("stage registration was published"))?;
        authorize(Boundary::StageCleared)?;
        session.finish()?;
        authorize(Boundary::Finished)?;
        Ok(())
    })();

    match result {
        Ok(()) => Ok(()),
        Err(error) => Err(cleanup(
            &session,
            registration.as_deref(),
            &stage_name,
            staged,
            published,
            error,
        )),
    }
}

fn cleanup(
    session: &Session,
    registration: Option<&OsStr>,
    stage_name: &OsStr,
    staged: Option<Observation>,
    published: bool,
    primary: io::Error,
) -> io::Error {
    if published {
        if observe(session.directory(), stage_name).is_ok() {
            return io::Error::new(
                primary.kind(),
                format!(
                    "{primary}; the staged name may hold the previous destination and was retained in {:?}",
                    session.name()
                ),
            );
        }
        return match registration
            .map_or(Ok(()), |registration| session.clear_stage(registration))
            .and_then(|()| session.finish())
        {
            Ok(()) => primary,
            Err(cleanup) => io::Error::new(
                primary.kind(),
                format!("{primary}; journal cleanup failed: {cleanup}"),
            ),
        };
    }

    let removal = match staged {
        Some(staged) => remove_owned(session.directory(), stage_name, Some(staged.identity)),
        None => remove_owned(session.directory(), stage_name, None),
    };
    let removal =
        removal.and_then(|()| registration.map_or(Ok(()), |registration| session.clear_stage(registration)));
    let finish = removal.and_then(|()| session.finish());
    match finish {
        Ok(()) => primary,
        Err(cleanup) => io::Error::new(
            primary.kind(),
            format!("{primary}; staging cleanup failed: {cleanup}"),
        ),
    }
}

fn remove_owned(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    expected: Option<EntryIdentity>,
) -> io::Result<()> {
    let observed = match observe(parent, name) {
        Ok(observed) => observed,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if expected.is_some_and(|expected| expected != observed.identity) {
        return Err(io::Error::other("staged entry identity changed"));
    }
    remove_owned_observed(parent, name, observed.identity)
}

fn remove_owned_observed(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    identity: EntryIdentity,
) -> io::Result<()> {
    if identity.kind != FileKind::Directory {
        let current = observe(parent, name)?;
        if current.identity != identity {
            return Err(io::Error::other("staged entry identity changed"));
        }
        return fs::unlinkat(parent, name, AtFlags::empty()).map_err(Into::into);
    }
    let directory = fs::openat(parent, name, DIRECTORY_FLAGS, Mode::empty())?;
    if observe_fd(directory.as_fd())?.identity != identity {
        return Err(io::Error::other("staged directory identity changed"));
    }
    let _ = fs::fchmod(directory.as_fd(), Mode::RWXU);
    for child in read_names(directory.as_fd())? {
        let child_observed = observe(directory.as_fd(), &child)?;
        remove_owned_observed(directory.as_fd(), &child, child_observed.identity)?;
    }
    fs::unlinkat(parent, name, AtFlags::REMOVEDIR).map_err(Into::into)
}

fn replace_by_exchange(
    from_parent: BorrowedFd<'_>,
    from_name: &OsStr,
    to_parent: BorrowedFd<'_>,
    to_name: &OsStr,
    expected: Observation,
) -> io::Result<Observation> {
    fs::renameat_with(from_parent, from_name, to_parent, to_name, RenameFlags::EXCHANGE)?;
    let displaced = observe(from_parent, from_name);
    if let Ok(displaced) = displaced {
        let mut compared = expected;
        compared.ctime = displaced.ctime;
        if compared.same_version(displaced) {
            return Ok(displaced);
        }
    }
    fs::renameat_with(from_parent, from_name, to_parent, to_name, RenameFlags::EXCHANGE).map_err(
        |error| {
            io::Error::new(
                error.kind(),
                format!(
                    "replacement displaced an unobserved destination and could not restore it; it remains at the source name: {error}"
                ),
            )
        },
    )?;
    Err(io::Error::other(
        "destination changed during replacement; previous destination restored",
    ))
}

pub(crate) fn merge(request: MergeRequest<'_>) -> io::Result<()> {
    let MergeRequest {
        source_parent,
        source_name,
        destination_parent,
        destination_name,
        expected_source,
        expected_destination,
        options,
        durable,
        cancellation,
        progress,
    } = request;
    validate_entry_name(source_name, "source")?;
    validate_entry_name(destination_name, "destination")?;
    if expected_source.identity.kind != FileKind::Directory
        || expected_destination.identity.kind != FileKind::Directory
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "merge operands must be directories",
        ));
    }
    require_unchanged(source_parent, source_name, expected_source)?;
    require_unchanged(destination_parent, destination_name, expected_destination)?;
    let source = open_checked_directory(source_parent, source_name, expected_source)?;
    let destination =
        open_checked_directory(destination_parent, destination_name, expected_destination)?;
    let mut context = MergeContext {
        options,
        durable,
        cancellation,
        progress,
    };
    merge_directory(
        &mut context,
        source_parent,
        source_name,
        expected_source,
        source.as_fd(),
        destination.as_fd(),
    )
}

struct MergeContext<'a> {
    options: crate::transfer::PolicyOptions,
    durable: bool,
    cancellation: &'a crate::transfer::Cancellation,
    progress: &'a mut crate::transfer::Progress,
}

fn merge_directory(
    context: &mut MergeContext<'_>,
    source_entry_parent: BorrowedFd<'_>,
    source_entry_name: &OsStr,
    source_entry: Observation,
    source: BorrowedFd<'_>,
    destination: BorrowedFd<'_>,
) -> io::Result<()> {
    let before = observe_fd(source)?;
    for name in read_names(source)? {
        context.cancellation.check()?;
        let source_child = observe(source, &name)?;
        let destination_child = match observe(destination, &name) {
            Ok(observed) => Some(observed),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let target = destination_child.map_or(Target::Absent, Target::Existing);
        let action = decide(source_child, target, &context.options)?;
        context.progress.discovered_files += 1;
        match action {
            Action::Skip | Action::SameEntry => {
                context.progress.skipped_files += 1;
            }
            Action::Merge => {
                let source_directory = open_checked_directory(source, &name, source_child)?;
                let destination_observed =
                    destination_child.expect("merge decision requires a destination");
                let destination_directory =
                    open_checked_directory(destination, &name, destination_observed)?;
                merge_directory(
                    context,
                    source,
                    &name,
                    source_child,
                    source_directory.as_fd(),
                    destination_directory.as_fd(),
                )?;
            }
            Action::Transfer | Action::Replace => {
                let expected_destination = if action == Action::Replace {
                    destination_child
                } else {
                    None
                };
                publish(
                    PublishRequest {
                        source_parent: source,
                        source_name: &name,
                        destination_parent: destination,
                        destination_name: &name,
                        expected_source: source_child,
                        expected_destination,
                        action,
                        durable: context.durable,
                        cancellation: context.cancellation,
                        progress: context.progress,
                    },
                    |_| Ok(()),
                )?;
                context.progress.completed_files += 1;
            }
        }
    }
    if !before.same_version(observe_fd(source)?) {
        return Err(io::Error::other("source directory changed during merge"));
    }
    require_unchanged(source_entry_parent, source_entry_name, source_entry)
}

fn open_checked_directory(
    parent: BorrowedFd<'_>,
    name: &OsStr,
    expected: Observation,
) -> io::Result<OwnedFd> {
    let directory = fs::openat(parent, name, DIRECTORY_FLAGS, Mode::empty())?;
    if !expected.same_identity(observe_fd(directory.as_fd())?) {
        return Err(io::Error::other("merge directory identity changed"));
    }
    Ok(directory)
}
