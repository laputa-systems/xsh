use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
const CPU_PERIOD_US: i64 = 100_000;
#[cfg(target_os = "linux")]
static NEXT_SCOPE_ID: AtomicU64 = AtomicU64::new(1);
#[cfg(target_os = "linux")]
use std::sync::atomic::AtomicU64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CgroupError {
    pub(crate) kind: String,
    pub(crate) message: String,
}

impl CgroupError {
    fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CgroupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for CgroupError {}

#[derive(Debug)]
pub(crate) struct CgroupScope {
    path: Option<PathBuf>,
    fake_root: bool,
}

impl CgroupScope {
    pub(crate) fn none() -> Self {
        Self {
            path: None,
            fake_root: false,
        }
    }

    pub(crate) fn cpu_max(cpu_max: Option<i64>, prefix: &str) -> Result<Self, CgroupError> {
        let Some(cpu_max) = cpu_max else {
            return Ok(Self::none());
        };
        if cpu_max <= 0 {
            return Err(CgroupError::new("cgroup", "cpu_max must be positive"));
        }
        create_cpu_scope(cpu_max, prefix)
    }

    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub(crate) fn assign_pid(&self, pid: i64) -> Result<(), CgroupError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        write_control_file(
            &path.join("cgroup.procs"),
            &format!("{pid}\n"),
            self.fake_root,
            false,
        )
        .map_err(|error| {
            CgroupError::new(
                "cgroup",
                format!(
                    "failed to move pid {pid} into '{}': {error}",
                    path.display()
                ),
            )
        })
    }
}

impl Drop for CgroupScope {
    fn drop(&mut self) {
        let Some(path) = &self.path else {
            return;
        };
        if self.fake_root {
            let _ = std::fs::remove_file(path.join("cgroup.procs"));
            let _ = std::fs::remove_file(path.join("cpu.max"));
        }
        let _ = std::fs::remove_dir(path);
    }
}

#[cfg(target_os = "linux")]
fn create_cpu_scope(cpu_max: i64, prefix: &str) -> Result<CgroupScope, CgroupError> {
    let root_from_env = std::env::var_os("XSH_CGROUP_ROOT").is_some();
    let root = cgroup_root()?;
    create_cpu_scope_in_root(cpu_max, prefix, root, root_from_env)
}

#[cfg(target_os = "linux")]
fn create_cpu_scope_in_root(
    cpu_max: i64,
    prefix: &str,
    root: PathBuf,
    root_from_env: bool,
) -> Result<CgroupScope, CgroupError> {
    use std::fs;

    let fake_root = root_from_env && !root.join("cgroup.controllers").exists();
    if !fake_root && !root.join("cgroup.controllers").exists() {
        return Err(CgroupError::new(
            "cgroup",
            format!("cgroups v2 is not available at '{}'", root.display()),
        ));
    }
    fs::create_dir_all(&root).map_err(|error| {
        CgroupError::new(
            "cgroup",
            format!("failed to access cgroup root '{}': {error}", root.display()),
        )
    })?;
    let id = NEXT_SCOPE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = root.join(format!("{prefix}-{}-{id}", std::process::id()));
    fs::create_dir(&path).map_err(|error| {
        CgroupError::new(
            "cgroup",
            format!("failed to create cgroup '{}': {error}", path.display()),
        )
    })?;
    let quota = cpu_max
        .checked_mul(1000)
        .ok_or_else(|| CgroupError::new("cgroup", "cpu_max is too large for cgroup cpu.max"))?;
    if let Err(error) = write_control_file(
        &path.join("cpu.max"),
        &format!("{quota} {CPU_PERIOD_US}\n"),
        fake_root,
        true,
    ) {
        let scope = CgroupScope {
            path: Some(path),
            fake_root,
        };
        drop(scope);
        return Err(CgroupError::new(
            "cgroup",
            format!("failed to write cpu.max: {error}"),
        ));
    }
    Ok(CgroupScope {
        path: Some(path),
        fake_root,
    })
}

#[cfg(target_os = "linux")]
fn cgroup_root() -> Result<PathBuf, CgroupError> {
    if let Some(root) = std::env::var_os("XSH_CGROUP_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let text = std::fs::read_to_string("/proc/self/cgroup").map_err(|error| {
        CgroupError::new(
            "cgroup",
            format!("failed to read /proc/self/cgroup: {error}"),
        )
    })?;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("0::") {
            let relative = path.trim_start_matches('/');
            return Ok(Path::new("/sys/fs/cgroup").join(relative));
        }
    }
    Err(CgroupError::new(
        "cgroup",
        "current process is not in a cgroups v2 hierarchy",
    ))
}

#[cfg(target_os = "macos")]
fn create_cpu_scope(_cpu_max: i64, _prefix: &str) -> Result<CgroupScope, CgroupError> {
    Ok(CgroupScope::none())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn create_cpu_scope(_cpu_max: i64, _prefix: &str) -> Result<CgroupScope, CgroupError> {
    Err(CgroupError::new(
        "unsupported-platform",
        "--cpumax requires Linux cgroups v2",
    ))
}

fn write_control_file(
    path: &Path,
    content: &str,
    fake_root: bool,
    replace: bool,
) -> std::io::Result<()> {
    use std::io::Write as _;

    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if fake_root {
        options.create(true);
        if replace {
            options.truncate(true);
        } else {
            options.append(true);
        }
    }
    let mut file = options.open(path)?;
    file.write_all(content.as_bytes())
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    fn fake_root_writes_cpu_max_and_pid_then_cleans_up() {
        use super::*;

        let root = tempfile::tempdir().expect("tempdir");
        let scope =
            create_cpu_scope_in_root(80, "test", root.path().to_path_buf(), true).expect("scope");
        let path = scope.path().expect("path").to_path_buf();
        assert_eq!(
            std::fs::read_to_string(path.join("cpu.max")).expect("cpu.max"),
            "80000 100000\n"
        );

        scope.assign_pid(1234).expect("assign pid");
        assert_eq!(
            std::fs::read_to_string(path.join("cgroup.procs")).expect("cgroup.procs"),
            "1234\n"
        );

        drop(scope);
        assert!(!path.exists());
    }
}
