# Chapter 15: Why Not XSH

This guide has shown where XSH is strong: orchestration, typed host boundaries,
structured failures, files, processes, streams, data formats, tests, and traces.

This final chapter names the other side of that promise. XSH is designed for a
specific tier of work. When the work is orchestration, XSH. When the work is
something else, use the right tool and let XSH invoke it.

## Fine-Grained Concurrency

XSH has parallel streams and process-level parallelism. That covers most
concurrency in systems scripting: fan-out builds, parallel file walks, and
concurrent service probes.

If the problem is thousands of concurrent connections, coroutines, event loops,
or shared mutable state across green threads, that is an application runtime's
job. Use Go, Erlang, Rust's async ecosystem, or another runtime built for that
shape. XSH's unit of concurrency is the process and the pipeline, not the task
or the fiber.

That boundary is also an implementation principle. XSH does not need a bytecode
VM, green-thread scheduler, or async task runtime to be good glue. Its runtime
walks checked source structure, coordinates explicit process and stream work,
and records what happened. The expensive work should usually happen in the
programs XSH invokes or in focused host APIs.

## Long-Lived Application Services

Init scripts and service supervisors are XSH's territory. The service itself
usually is not.

A daemon maintaining complex in-memory state, a server handling thousands of
requests per second, or a system with its own internal event loop wants a
compiled runtime with mature memory management and profiling tooling. XSH fills
the gap between bash and a full application runtime.

## Interactive Terminal Applications

XSH produces output and transforms data. If the deliverable is an interactive
application, such as a file browser, status dashboard, or full-screen UI, XSH is
the wrong level. That work needs a language with a TUI library and an event
loop.

## Specialized Tools

XSH's relationship to tools outside its tier is not competitive. A script that
decides to run `python3 train.py` is doing its job correctly. A script that
reimplements gradient descent is doing the wrong job.

XSH orchestrates. Specialized tools do their work. The boundary between them
should be visible in the source: a `run` call, a typed result coming back, an
explicit handoff.

## The Rule Of Thumb

Use XSH when the problem is coordinating processes, files, paths, data formats,
system state, and expected failures. Reach for another language when the core
problem is an application runtime, an event loop, a complex in-memory service,
or a specialized algorithm.
