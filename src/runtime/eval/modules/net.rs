use super::{
    Evaluator, net_body_from_record, record_bool, record_duration, record_headers,
    record_nonnegative_usize, record_optional_positive_u64, record_path, record_positive_u64,
    record_str, value_to_path,
};
use crate::modules::net::{self, NetAgentKey, NetCallOptions, NetDownload, NetRequest, NetUpload};
use crate::runtime::value::{RecordMap, RuntimeError};
use crate::source::Span;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

impl Evaluator {
    pub(in crate::runtime::eval) fn net_agent(
        &mut self,
        options: &NetCallOptions,
        span: Span,
    ) -> Result<net::NetAgent, RuntimeError> {
        let pool = self
            .net_pool_options
            .get(&options.pool)
            .cloned()
            .unwrap_or_default();
        let key = NetAgentKey {
            pool: options.pool.clone(),
            tls_verify: options.tls_verify,
            ca_certificate: options.ca_certificate.clone(),
            max_idle_per_host: pool.max_idle_per_host,
            idle_timeout: pool.idle_timeout,
        };
        if let Some(agent) = self.net_agents.get(&key) {
            return Ok(agent.clone());
        }
        let agent = net::make_agent(&key, span)?;
        self.net_agents.insert(key, agent.clone());
        Ok(agent)
    }

    pub(in crate::runtime::eval) fn net_call_options(
        &mut self,
        record: &RecordMap,
        span: Span,
    ) -> Result<NetCallOptions, RuntimeError> {
        let pool = record_str(record, "pool", Some("default"), span)?;
        let tls_verify = record_bool(record, "tls_verify", true, span)?;
        let ca_certificate = match record.get("ca_certificate") {
            Some(value) => Some(self.host_path(&value_to_path(value, "ca_certificate", span)?)),
            None => self.ssl_cert_file_from_env(),
        };
        Ok(NetCallOptions {
            pool,
            tls_verify,
            ca_certificate,
        })
    }

    pub(in crate::runtime::eval) fn net_request_from_record(
        &mut self,
        record: RecordMap,
        span: Span,
    ) -> Result<NetRequest, RuntimeError> {
        let body = net_body_from_record(self, &record, span)?;
        Ok(NetRequest {
            method: record_str(&record, "method", None, span)?,
            url: record_str(&record, "url", None, span)?,
            headers: record_headers(&record, span)?,
            body,
            timeout: record_duration(&record, "timeout", span)?,
            connect_timeout: record_duration(&record, "connect_timeout", span)?,
            redirects: record_nonnegative_usize(&record, "redirects", 3, span)?,
            fail_status: record_bool(&record, "fail_status", false, span)?,
            max_body_bytes: record_positive_u64(&record, "max_body_bytes", 10 * 1024 * 1024, span)?,
        })
    }

    pub(in crate::runtime::eval) fn net_download_from_record(
        &mut self,
        record: RecordMap,
        span: Span,
    ) -> Result<NetDownload, RuntimeError> {
        let dest = record_path(&record, "dest", span)?;
        Ok(NetDownload {
            url: record_str(&record, "url", None, span)?,
            dest: self.host_path(&dest),
            headers: record_headers(&record, span)?,
            timeout: record_duration(&record, "timeout", span)?,
            connect_timeout: record_duration(&record, "connect_timeout", span)?,
            redirects: record_nonnegative_usize(&record, "redirects", 3, span)?,
            fail_status: record_bool(&record, "fail_status", false, span)?,
            max_body_bytes: record_optional_positive_u64(&record, "max_body_bytes", span)?,
            atomic: record_bool(&record, "atomic", true, span)?,
            overwrite: record_bool(&record, "overwrite", false, span)?,
        })
    }

    pub(in crate::runtime::eval) fn net_upload_from_record(
        &mut self,
        record: RecordMap,
        span: Span,
    ) -> Result<NetUpload, RuntimeError> {
        let source = record_path(&record, "source", span)?;
        Ok(NetUpload {
            method: record_str(&record, "method", Some("PUT"), span)?,
            url: record_str(&record, "url", None, span)?,
            source: self.host_path(&source),
            headers: record_headers(&record, span)?,
            timeout: record_duration(&record, "timeout", span)?,
            connect_timeout: record_duration(&record, "connect_timeout", span)?,
            redirects: record_nonnegative_usize(&record, "redirects", 3, span)?,
            fail_status: record_bool(&record, "fail_status", false, span)?,
            max_body_bytes: record_positive_u64(&record, "max_body_bytes", 10 * 1024 * 1024, span)?,
        })
    }

    pub(in crate::runtime::eval) fn ssl_cert_file_from_env(&self) -> Option<std::path::PathBuf> {
        self.env
            .get_owned(b"SSL_CERT_FILE".as_slice())
            .map(|value| std::path::PathBuf::from(OsString::from_vec(value)))
    }
}
