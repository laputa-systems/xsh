You are solving a task in an intentionally isolated XSH environment. Produce
the requested artifact in `/work` and optimize for a small, deterministic,
directly verifiable solution.

## Environment

You are running as `root` in a minimal Alpine Linux container with `/work` as
the working directory. The task workspace is mounted at `/work`; it contains
this file, `handbook.md`, the selected task file, and the files you create.
The image may also provide copies of the reference files before the workspace
is mounted.

The fixed base environment contains BusyBox applets, `xsh`, `xsht`, `curl`, and
the system CA certificates. The task-specific image may add a named utility
such as `fd`; do not assume any other packages. There is no compiler,
toolchain, Git checkout, other language runtime, or repository source tree.
HTTPS access is available through `curl` when a task permits network access,
but external services are not an implicit source of task data.

Do not read or rely on hidden host paths, repository source files, generated
documentation, package registries, or implementation details. Use `xsht api`
to discover XSH behavior and the files mounted in `/work` to understand the
task.

Do not modify the handbook, task prompt, roadmap, or evaluation inputs. Write
only the requested artifact unless the task explicitly says otherwise.

## Tool and solution rules

- Use `xsht api` for exact XSH signatures, effects, return types, and
  contracts.
- Use `xsht check` early and after substantive changes.
- Use `xsht fmt` and `xsht lint` as part of the normal development loop.
- BusyBox utilities may be used for local inspection and editing when the task
  allows them. A utility used by the evaluator or for local observation is not
  automatically permitted inside the submitted XSH program.
- Follow task-specific restrictions on subprocesses, filesystem access,
  network access, nondeterminism, and hard-coded results.
- Keep stdout limited to the task’s required output. Send no progress or debug
  text to stdout.
- Do not hard-code an oracle’s current result when the task asks for a
  general computation.


## Task Review

At the end of the task, after the requested artifact is in place, open
`/work/review.md` and fill it out in place. The file is a review template:
keep its section headings (`## XSH language proposals` and
`## xsht friction`) and replace the placeholder entries under each section
with your actual review. It is an expected deliverable like any other task
output; the evaluator checks that it exists and keeps the template's
sections.

The review is an honest engineering report of this session. Write `None.`
under a section when you have nothing for it; do not invent entries.
