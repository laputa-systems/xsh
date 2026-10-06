use super::Evaluator;
use crate::runtime::value::RuntimeError;
use crate::source::Span;
#[cfg(feature = "native-tests")]
use std::collections::BTreeMap;

/// The native-test double for the `linux` module. While installed, every
/// `linux.*` entry returns fixed values instead of touching the host and
/// appends one JSON line per call to `log`. Only the test harness installs it
/// (`test.linux_fake` and the scripts that test runs); no environment variable
/// or production flag reaches it.
#[cfg(feature = "native-tests")]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LinuxFake {
    values: BTreeMap<&'static str, String>,
}

#[cfg(feature = "native-tests")]
impl LinuxFake {
    /// Settings a fake accepts: the call log path and the fixed values some
    /// queries report.
    pub const KEYS: [&'static str; 7] = [
        "log",
        "root_device",
        "sysctl_value",
        "file_attrs_flags",
        "file_version",
        "file_project",
        "hwclock_epoch_ms",
    ];

    pub fn set(&mut self, key: &str, value: impl Into<String>) -> Result<(), String> {
        let Some(key) = Self::KEYS.into_iter().find(|known| *known == key) else {
            return Err(format!(
                "unknown linux fake setting `{key}`; expected one of {}",
                Self::KEYS.join(", ")
            ));
        };
        let value = value.into();
        if key == "file_project" {
            let project = value.parse::<u32>().map_err(|_| "linux fake file_project must fit in u32".to_string())?;
            self.values.insert(key, project.to_string());
        } else {
            self.values.insert(key, value);
        }
        Ok(())
    }

    pub fn settings(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.values
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
    }
}

impl Evaluator {
    #[cfg(feature = "native-tests")]
    pub fn with_linux_fake(mut self, fake: LinuxFake) -> Self {
        self.linux_fake = Some(std::sync::Arc::new(fake));
        self
    }

    /// One fake setting, or `None` when no fake is installed. Builds without
    /// `native-tests` have no fake at all.
    fn linux_fake_setting(&self, key: &str) -> Option<&str> {
        #[cfg(feature = "native-tests")]
        return self
            .linux_fake
            .as_deref()?
            .values
            .get(key)
            .map(String::as_str);
        #[cfg(not(feature = "native-tests"))]
        {
            let _ = key;
            None
        }
    }

    pub(in crate::runtime::eval) fn linux_fake_active(&self) -> bool {
        #[cfg(feature = "native-tests")]
        return self.linux_fake.is_some();
        #[cfg(not(feature = "native-tests"))]
        false
    }

    pub(in crate::runtime::eval) fn linux_fake_value(&self, key: &str, default: &str) -> String {
        self.linux_fake_setting(key).unwrap_or(default).to_string()
    }

    pub(in crate::runtime::eval) fn linux_fake_log(
        &self,
        op: &str,
        fields: &[(&str, String)],
        span: Span,
    ) -> Result<(), RuntimeError> {
        let Some(path) = self.linux_fake_setting("log") else {
            return Ok(());
        };
        append_fake_log(path, "linux-fake-log", op, fields, span)
    }
}

/// Append one JSON line naming `op` and its `fields` to a test fake's call log,
/// creating missing parent directories. Failures raise `kind`.
pub(super) fn append_fake_log(
    path: &str,
    kind: &'static str,
    op: &str,
    fields: &[(&str, String)],
    span: Span,
) -> Result<(), RuntimeError> {
    let path = std::path::PathBuf::from(path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| RuntimeError::host(kind, &error).with_span(span))?;
    }
    let mut json_fields = Vec::with_capacity(fields.len() + 1);
    json_fields.push(("op".to_string(), crate::modules::json::raw_json_string(op)));
    for (name, value) in fields {
        json_fields.push((
            (*name).to_string(),
            crate::modules::json::raw_json_string(value.clone()),
        ));
    }
    let line =
        crate::modules::json::compact_raw_json(&crate::modules::json::raw_json_object(json_fields));
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| RuntimeError::host(kind, &error).with_span(span))?;
    writeln!(file, "{line}").map_err(|error| RuntimeError::host(kind, &error).with_span(span))
}
