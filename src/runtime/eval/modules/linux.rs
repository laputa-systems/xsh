use super::Evaluator;
use crate::runtime::value::RuntimeError;
use crate::source::Span;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

impl Evaluator {
    pub(in crate::runtime::eval) fn linux_dry_run(&self) -> bool {
        self.env
            .get_owned(b"XSH_LINUX_DRY_RUN".as_slice())
            .and_then(|value| String::from_utf8(value).ok())
            .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
    }

    pub(in crate::runtime::eval) fn linux_real(&self) -> bool {
        self.env
            .get_owned(b"XSH_LINUX_REAL".as_slice())
            .and_then(|value| String::from_utf8(value).ok())
            .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
    }
    pub(in crate::runtime::eval) fn linux_dry_run_log(
        &self,
        op: &str,
        fields: &[(&str, String)],
        span: Span,
    ) -> Result<(), RuntimeError> {
        let Some(path) = self.env.get_owned(b"XSH_LINUX_DRY_RUN_LOG".as_slice()) else {
            return Ok(());
        };
        let path = std::path::PathBuf::from(OsString::from_vec(path));
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                RuntimeError::new("linux-dry-run-log", error.to_string()).with_span(span)
            })?;
        }
        let mut json_fields = Vec::with_capacity(fields.len() + 1);
        json_fields.push(("op".to_string(), crate::modules::json::raw_json_string(op)));
        for (name, value) in fields {
            json_fields.push((
                (*name).to_string(),
                crate::modules::json::raw_json_string(value.clone()),
            ));
        }
        let line = crate::modules::json::compact_raw_json(&crate::modules::json::raw_json_object(
            json_fields,
        ));
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| {
                RuntimeError::new("linux-dry-run-log", error.to_string()).with_span(span)
            })?;
        writeln!(file, "{line}").map_err(|error| {
            RuntimeError::new("linux-dry-run-log", error.to_string()).with_span(span)
        })
    }
}
