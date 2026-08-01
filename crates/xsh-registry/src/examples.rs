//! On-disk XSH snippets attached to the public API reference.
//!
//! The registry owns which API item receives an example; the example source
//! itself lives in docs/snippets/api/ so it can be read, edited, and reused
//! as XSH rather than being hidden in Rust string literals.

pub(crate) fn source(id: &str) -> Option<String> {
    let source = match id {
        "module.archive" | "module.archive.tar_list" => {
            include_str!("../../../docs/snippets/api/archive-tar-list.xsh")
        }
        "module.bytes" => include_str!("../../../docs/snippets/api/bytes-base64.xsh"),
        "module.fs" | "module.fs.read_text" | "method.Path.read_text" => {
            include_str!("../../../docs/snippets/api/fs-read-text.xsh")
        }
        "module.json" | "module.json.read" => {
            include_str!("../../../docs/snippets/api/json-read.xsh")
        }
        "module.process" | "module.process.run" => {
            include_str!("../../../docs/snippets/api/process-run.xsh")
        }
        "module.json.decode" => include_str!("../../../docs/snippets/api/json-decode.xsh"),
        "module.json.write" => include_str!("../../../docs/snippets/api/json-write.xsh"),
        "module.fs.write" | "method.Path.write" => {
            include_str!("../../../docs/snippets/api/fs-write.xsh")
        }
        "module.fs.tempdir" => include_str!("../../../docs/snippets/api/fs-tempdir.xsh"),
        "module.path.absolute" => include_str!("../../../docs/snippets/api/path-absolute.xsh"),
        "module.process.command" => {
            include_str!("../../../docs/snippets/api/process-command.xsh")
        }
        "module.process.spawn" => include_str!("../../../docs/snippets/api/process-spawn.xsh"),
        "module.module.load" => include_str!("../../../docs/snippets/api/module-load.xsh"),
        "module.patch.apply" => include_str!("../../../docs/snippets/api/patch-apply.xsh"),
        "method.Path.resolve" => include_str!("../../../docs/snippets/api/path-resolve.xsh"),
        "method.Bytes.base64" => {
            include_str!("../../../docs/snippets/api/bytes-base64.xsh")
        }
        "method.Bytes.utf8" => include_str!("../../../docs/snippets/api/bytes-utf8.xsh"),
        "method.Stream.collect" => {
            include_str!("../../../docs/snippets/api/stream-collect.xsh")
        }
        "method.Str.trim" => include_str!("../../../docs/snippets/api/str-trim.xsh"),
        "method.List.join" => include_str!("../../../docs/snippets/api/list-join.xsh"),
        "method.Result.context" => {
            include_str!("../../../docs/snippets/api/result-context.xsh")
        }
        "record.ArchiveEntry" => {
            include_str!("../../../docs/snippets/api/record-archive-entry.xsh")
        }
        "record.FsEntry" => include_str!("../../../docs/snippets/api/record-fs-entry.xsh"),
        "record.NetResponse" => {
            include_str!("../../../docs/snippets/api/record-net-response.xsh")
        }
        "language.run.statement-position" => {
            include_str!("../../../docs/snippets/api/run-statement.xsh")
        }
        "language.run.value-position" => include_str!("../../../docs/snippets/api/run-value.xsh"),
        "language.run.status" => include_str!("../../../docs/snippets/api/run-status.xsh"),
        "language.run.text" => include_str!("../../../docs/snippets/api/run-text.xsh"),
        "language.run.capture---text" => {
            include_str!("../../../docs/snippets/api/run-capture-text.xsh")
        }
        "language.effect.fs" => include_str!("../../../docs/snippets/api/effect-fs.xsh"),
        "language.effect.error" => include_str!("../../../docs/snippets/api/effect-error.xsh"),
        "language.stream.par-map" => {
            include_str!("../../../docs/snippets/api/stream-par-map.xsh")
        }
        "language.stream.where" => include_str!("../../../docs/snippets/api/stream-where.xsh"),
        "language.core.procs" => include_str!("../../../docs/snippets/api/core-procs.xsh"),
        "language.core.pure-functions" => {
            include_str!("../../../docs/snippets/api/core-pure-functions.xsh")
        }
        "language.core.results" => include_str!("../../../docs/snippets/api/core-results.xsh"),
        "language.core.postfix-question" => {
            include_str!("../../../docs/snippets/api/core-postfix-question.xsh")
        }
        "language.core.records" => include_str!("../../../docs/snippets/api/core-records.xsh"),
        "language.core.statements" => {
            include_str!("../../../docs/snippets/api/core-statements.xsh")
        }
        "language.core.command-interpolation" => {
            include_str!("../../../docs/snippets/api/core-command-interpolation.xsh")
        }
        "language.core.path-literals" => {
            include_str!("../../../docs/snippets/api/core-path-literals.xsh")
        }
        _ => return None,
    };
    Some(source.trim_end().to_string())
}
