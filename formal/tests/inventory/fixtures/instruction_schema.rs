instruction_schema! {
    BuildValueRow => ValueView [Value] {
        Empty => ValueEmpty = 0 [0; Operands; FrameApply];
        Pair {
            left: BuildExprId,
            right: Option<BuildExprId>,
        } => ValuePair = 1 [EFFECT_HOST | EFFECT_TRACE; Scheduled; FrameControl; EagerAtom];
    }
}
