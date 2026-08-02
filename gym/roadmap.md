# Gym roadmap

## Initial environment

The first gym is deliberately narrow. The agent receives the task prompt,
task-neutral rules, and the user-facing handbook from `runtime/` in a minimal
Alpine container.
It can invoke xsh and xsht, use the BusyBox applets and task-specific utilities
provided by the image, and work in `/work`. The first eval uses a transparent
command-line oracle instead of a checked-in harness.

The host-side outer controller is documented in the repository-level `GYM.md`.
It invokes a full-access Pi judge between isolated trials so handbook changes
are attributed by hash and replayed by the next inner run.

## Pure-XSH gym

The intended next environment removes the BusyBox action surface. The agent
must achieve goals using only XSH, with no shell, editor, awk, fd, sort, or
other host utility available to its solution process. The evaluator may still
run outside the agent boundary to provide inputs and compare results.

Before enabling that mode, define how an agent creates or edits its submitted
source when it cannot invoke an editor, how task inputs are mounted, and which
filesystem roots XSH may access. The capability boundary must be explicit
enough that a task cannot accidentally depend on ambient host state.

Future tasks should keep the same discipline: one small user-facing handbook,
task-specific restrictions, deterministic outputs, and an external acceptance
oracle whose implementation is not available to the candidate program.
