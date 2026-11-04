use crate::source::Span;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug)]
pub(crate) struct LoweredDeclaration {
    pub(crate) statement: usize,
    pub(crate) span: Span,
    pub(crate) slot: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EmittedDeclaration {
    pub(crate) statement: usize,
    pub(crate) function: u32,
    pub(crate) instruction: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LoweredParameter {
    pub(crate) slot: usize,
}

#[derive(Default)]
struct Trace {
    lowered: Vec<LoweredDeclaration>,
    emitted: Vec<EmittedDeclaration>,
    parameters: Vec<LoweredParameter>,
}

thread_local! {
    static TRACE: RefCell<Trace> = RefCell::new(Trace::default());
}

pub(crate) fn clear() {
    TRACE.with(|trace| *trace.borrow_mut() = Trace::default());
}

pub(super) fn lowered(statement: usize, span: Span, slot: usize) {
    TRACE.with(|trace| trace.borrow_mut().lowered.push(LoweredDeclaration { statement, span, slot }));
}

pub(super) fn emitted(statement: usize, function: u32, instruction: usize) {
    TRACE.with(|trace| trace.borrow_mut().emitted.push(EmittedDeclaration { statement, function, instruction }));
}

pub(super) fn parameter(slot: usize) {
    TRACE.with(|trace| trace.borrow_mut().parameters.push(LoweredParameter { slot }));
}

pub(crate) fn recorded() -> (Vec<LoweredDeclaration>, Vec<EmittedDeclaration>, Vec<LoweredParameter>) {
    TRACE.with(|trace| {
        let trace = trace.borrow();
        (trace.lowered.clone(), trace.emitted.clone(), trace.parameters.clone())
    })
}
