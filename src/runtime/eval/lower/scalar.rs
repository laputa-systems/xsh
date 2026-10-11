use super::{
    Arc, AssignOp, BinaryOp, BuildBoolId, BuildBoolRow, BuildExprId, BuildExprRow, BuildIntId,
    BuildIntRow, BuildPatternRow, BuildStmtId, BuildStmtRow, CompactLowerConstructProbe,
    LoweredStrPredicate, LoweredValue, ScanBytes, ScanCheck, ScanCondition, Span,
};

impl CompactLowerConstructProbe<'_, '_> {
    pub(super) fn lower_int_expr_candidate(&self, expr: &BuildExprId) -> Option<BuildIntId> {
        if self
            .scratch
            .borrow()
            .non_int_binary_expressions
            .contains(&expr.index())
        {
            return None;
        }
        let row = {
            let scratch = self.scratch.borrow();
            scratch.expressions[expr.index()].clone()
        };
        match &row {
            BuildExprRow::Int(value) => Some(push_build_row!(self, int, BuildIntRow::Int(*value))),
            BuildExprRow::Param(slot) => Some(push_build_row!(self, int, BuildIntRow::Slot(*slot))),
            BuildExprRow::Binary {
                op, left, right, ..
            } if matches!(
                op,
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem
            ) =>
            {
                Some(push_build_row!(
                    self,
                    int,
                    BuildIntRow::Binary {
                        op: *op,
                        left: self.lower_int_expr_candidate(left)?,
                        right: self.lower_int_expr_candidate(right)?,
                    }
                ))
            }
            BuildExprRow::StrByteLen { receiver, span } => {
                let receiver_row = {
                    let scratch = self.scratch.borrow();
                    scratch.expressions[receiver.index()].clone()
                };
                match receiver_row {
                    BuildExprRow::Param(slot) => Some(push_build_row!(
                        self,
                        int,
                        BuildIntRow::StrByteLenSlot { slot, span: *span }
                    )),
                    _ => None,
                }
            }
            BuildExprRow::Method {
                receiver,
                name,
                args,
                span,
            } if name.as_str() == "count_lines" && args.is_empty() => {
                let receiver_row = {
                    let scratch = self.scratch.borrow();
                    scratch.expressions[receiver.index()].clone()
                };
                match receiver_row {
                    BuildExprRow::Param(slot) => Some(push_build_row!(
                        self,
                        int,
                        BuildIntRow::StrCountLinesSlot { slot, span: *span }
                    )),
                    _ => None,
                }
            }
            BuildExprRow::MatchExpr { value, arms, .. } => {
                // Optional fallback lowers to an exhaustive null/present match.
                // Fuse only a literal alternative; effects retain lazy evaluation.
                let [
                    (null_pattern, None, fallback),
                    (present_pattern, None, present),
                ] = arms.as_slice()
                else {
                    return None;
                };
                let scratch = self.scratch.borrow();
                if !matches!(
                    scratch.patterns[null_pattern.index()],
                    BuildPatternRow::Literal(LoweredValue::Null)
                ) {
                    return None;
                }
                let BuildPatternRow::Bind { slot: present_slot } =
                    scratch.patterns[present_pattern.index()]
                else {
                    return None;
                };
                if !matches!(scratch.expressions[present.index()], BuildExprRow::Param(slot) if slot == present_slot)
                {
                    return None;
                }
                let BuildExprRow::StrByteAt {
                    receiver,
                    index,
                    span,
                } = scratch.expressions[value.index()].clone()
                else {
                    return None;
                };
                let BuildExprRow::Param(slot) = scratch.expressions[receiver.index()] else {
                    return None;
                };
                drop(scratch);
                let default = self.lowered_inert_int_literal(*fallback)?;
                Some(push_build_row!(
                    self,
                    int,
                    BuildIntRow::StrByteAtSlot {
                        slot,
                        index: self.lower_int_expr_candidate(&index)?,
                        default: if default == -1 {
                            None
                        } else {
                            Some(push_build_row!(self, int, BuildIntRow::Int(default)))
                        },
                        span,
                    }
                ))
            }
            _ => None,
        }
    }

    pub(super) fn lower_bool_expr_candidate(&self, expr: &BuildExprId) -> Option<BuildBoolId> {
        let row = {
            let scratch = self.scratch.borrow();
            scratch.expressions[expr.index()].clone()
        };
        match &row {
            BuildExprRow::Bool(value) => {
                Some(push_build_row!(self, bool, BuildBoolRow::Bool(*value)))
            }
            BuildExprRow::Param(slot) => {
                Some(push_build_row!(self, bool, BuildBoolRow::Slot(*slot)))
            }
            BuildExprRow::Binary {
                op,
                left,
                right,
                span,
            } => match op {
                BinaryOp::In | BinaryOp::NotIn => {
                    let receiver_row = self.scratch.borrow().expressions[right.index()].clone();
                    let BuildExprRow::Param(slot) = receiver_row else {
                        return None;
                    };
                    let needle_row = self.scratch.borrow().expressions[left.index()].clone();
                    let candidate = if let BuildExprRow::Str(needle) = needle_row {
                        push_build_row!(
                            self,
                            bool,
                            BuildBoolRow::StrContainsSlot {
                                slot,
                                needle,
                                span: *span
                            }
                        )
                    } else {
                        let needle = self.lowered_literal_value(left)?;
                        push_build_row!(
                            self,
                            bool,
                            BuildBoolRow::ContainsSlot {
                                slot,
                                needle,
                                span: *span
                            }
                        )
                    };
                    Some(if *op == BinaryOp::NotIn {
                        push_build_row!(self, bool, BuildBoolRow::Not(candidate))
                    } else {
                        candidate
                    })
                }
                BinaryOp::And => Some(push_build_row!(
                    self,
                    bool,
                    BuildBoolRow::And(
                        self.lower_bool_expr_candidate(left)?,
                        self.lower_bool_expr_candidate(right)?,
                    )
                )),
                BinaryOp::Or => Some(push_build_row!(
                    self,
                    bool,
                    BuildBoolRow::Or(
                        self.lower_bool_expr_candidate(left)?,
                        self.lower_bool_expr_candidate(right)?,
                    )
                )),
                BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge => {
                    if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                        if self.lowered_empty_string_literal(right)
                            && let Some((slot, span)) = self.lowered_trim_slot(left)
                        {
                            let candidate = push_build_row!(
                                self,
                                bool,
                                BuildBoolRow::TrimEmptySlot { slot, span }
                            );
                            return Some(if *op == BinaryOp::Eq {
                                candidate
                            } else {
                                push_build_row!(self, bool, BuildBoolRow::Not(candidate))
                            });
                        }
                        if self.lowered_empty_string_literal(left)
                            && let Some((slot, span)) = self.lowered_trim_slot(right)
                        {
                            let candidate = push_build_row!(
                                self,
                                bool,
                                BuildBoolRow::TrimEmptySlot { slot, span }
                            );
                            return Some(if *op == BinaryOp::Eq {
                                candidate
                            } else {
                                push_build_row!(self, bool, BuildBoolRow::Not(candidate))
                            });
                        }
                        if let Some(value) = self.lowered_bool_literal(right) {
                            let candidate = self.lower_bool_expr_candidate(left)?;
                            return Some(
                                if (*op == BinaryOp::Eq && value) || (*op == BinaryOp::Ne && !value)
                                {
                                    candidate
                                } else {
                                    push_build_row!(self, bool, BuildBoolRow::Not(candidate))
                                },
                            );
                        }
                        if let Some(value) = self.lowered_bool_literal(left) {
                            let candidate = self.lower_bool_expr_candidate(right)?;
                            return Some(
                                if (*op == BinaryOp::Eq && value) || (*op == BinaryOp::Ne && !value)
                                {
                                    candidate
                                } else {
                                    push_build_row!(self, bool, BuildBoolRow::Not(candidate))
                                },
                            );
                        }
                    }
                    if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                        let left_row = self.scratch.borrow().expressions[left.index()].clone();
                        if let BuildExprRow::Param(slot) = left_row
                            && let Some(value) = self.lowered_literal_value(right)
                        {
                            return Some(push_build_row!(
                                self,
                                bool,
                                BuildBoolRow::LiteralCompareSlot {
                                    op: *op,
                                    slot,
                                    value,
                                }
                            ));
                        }
                        let right_row = self.scratch.borrow().expressions[right.index()].clone();
                        if let BuildExprRow::Param(slot) = right_row
                            && let Some(value) = self.lowered_literal_value(left)
                        {
                            return Some(push_build_row!(
                                self,
                                bool,
                                BuildBoolRow::LiteralCompareSlot {
                                    op: *op,
                                    slot,
                                    value,
                                }
                            ));
                        }
                    }
                    let left = self.lower_int_expr_candidate(left)?;
                    let right = self.lower_int_expr_candidate(right)?;
                    if self.lowered_int_expr_needs_type_context(&left)
                        || self.lowered_int_expr_needs_type_context(&right)
                    {
                        return None;
                    }
                    Some(push_build_row!(
                        self,
                        bool,
                        BuildBoolRow::IntCompare {
                            op: *op,
                            left,
                            right,
                        }
                    ))
                }
                _ => None,
            },
            BuildExprRow::StrPredicate {
                receiver,
                predicate,
                needle,
                span,
            } => {
                let needle = self.lowered_needle_bytes(needle)?;
                let receiver_row = self.scratch.borrow().expressions[receiver.index()].clone();
                if let BuildExprRow::Param(slot) = receiver_row {
                    return Some(push_build_row!(
                        self,
                        bool,
                        BuildBoolRow::StrPredicateSlot {
                            slot,
                            predicate: *predicate,
                            needle,
                            span: *span,
                        }
                    ));
                }
                if let Some((slot, trim_span)) = self.lowered_trim_slot(receiver) {
                    return Some(push_build_row!(
                        self,
                        bool,
                        BuildBoolRow::TrimStrPredicateSlot {
                            slot,
                            predicate: *predicate,
                            needle,
                            span: trim_span,
                        }
                    ));
                }
                None
            }
            _ => None,
        }
    }

    fn lowered_inert_int_literal(&self, expr: BuildExprId) -> Option<i64> {
        match self.scratch.borrow().expressions[expr.index()].clone() {
            BuildExprRow::Int(value) => Some(value),
            BuildExprRow::Binary {
                op: BinaryOp::Sub,
                left,
                right,
                ..
            } => {
                let scratch = self.scratch.borrow();
                match (
                    &scratch.expressions[left.index()],
                    &scratch.expressions[right.index()],
                ) {
                    (BuildExprRow::Int(0), BuildExprRow::Int(value)) => value.checked_neg(),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn lowered_literal_value(&self, expr: &BuildExprId) -> Option<LoweredValue> {
        let row = self.scratch.borrow().expressions[expr.index()].clone();
        match &row {
            BuildExprRow::Null => Some(LoweredValue::Null),
            BuildExprRow::Unit => Some(LoweredValue::Unit),
            BuildExprRow::Int(value) => Some(LoweredValue::Int(*value)),
            BuildExprRow::Float(value) => Some(LoweredValue::Float(*value)),
            BuildExprRow::Duration(value) => Some(LoweredValue::Duration(value.clone())),
            BuildExprRow::Bool(value) => Some(LoweredValue::Bool(*value)),
            BuildExprRow::Str(value) => Some(LoweredValue::Str(value.clone())),
            BuildExprRow::Bytes(value) => Some(LoweredValue::Bytes(value.clone())),
            _ => None,
        }
    }

    pub(super) fn lowered_int_expr_needs_type_context(&self, expr: &BuildIntId) -> bool {
        let row = self.scratch.borrow().ints[expr.index()].clone();
        match &row {
            BuildIntRow::Slot(_) => true,
            BuildIntRow::Int(_)
            | BuildIntRow::Binary { .. }
            | BuildIntRow::StrByteLenSlot { .. }
            | BuildIntRow::StrCountLinesSlot { .. }
            | BuildIntRow::StrByteAtSlot { .. } => false,
        }
    }

    fn lowered_empty_string_literal(&self, expr: &BuildExprId) -> bool {
        let row = self.scratch.borrow().expressions[expr.index()].clone();
        matches!(&row, BuildExprRow::Str(value) if value.is_empty())
            || matches!(&row, BuildExprRow::Bytes(value) if value.is_empty())
    }

    /// Extract a literal `Str` or `Bytes` needle as bytes, for the byte-level
    /// predicate fast paths. `Str` needles use their UTF-8 bytes, which makes
    /// byte `starts_with`/`ends_with`/`contains` equivalent to the `Str` ops.
    fn lowered_needle_bytes(&self, expr: &BuildExprId) -> Option<Arc<[u8]>> {
        let row = self.scratch.borrow().expressions[expr.index()].clone();
        match &row {
            BuildExprRow::Str(value) => Some(value.as_bytes().into()),
            BuildExprRow::Bytes(value) => Some(value.clone()),
            _ => None,
        }
    }

    fn lowered_trim_slot(&self, expr: &BuildExprId) -> Option<(usize, Span)> {
        let row = self.scratch.borrow().expressions[expr.index()].clone();
        let BuildExprRow::Method {
            receiver,
            name,
            args,
            span,
        } = &row
        else {
            return None;
        };
        if name.as_str() != "trim" || !args.is_empty() {
            return None;
        }
        let receiver = self.scratch.borrow().expressions[receiver.index()].clone();
        let BuildExprRow::Param(slot) = receiver else {
            return None;
        };
        Some((slot, *span))
    }

    fn lowered_bool_literal(&self, expr: &BuildExprId) -> Option<bool> {
        let row = self.scratch.borrow().expressions[expr.index()].clone();
        match &row {
            BuildExprRow::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub(super) fn lowered_bool_expr_needs_type_context(&self, expr: &BuildBoolId) -> bool {
        let row = self.scratch.borrow().bools[expr.index()].clone();
        match &row {
            BuildBoolRow::Slot(_) => true,
            BuildBoolRow::Not(inner) => self.lowered_bool_expr_needs_type_context(inner),
            _ => false,
        }
    }

    /// Returns whether a lowered statement list is *guaranteed* to hit an explicit
    /// `Return`/propagation on every control-flow path. This decides whether the
    /// compact lowerer must append an implicit `Return ok unit` for unit/Result[Unit]
    /// procs (and whether value-returning procs are well-formed). It must be
    /// CONSERVATIVE: the runtime never treats a bare tail `Expr` statement as a
    /// return (it yields `StmtFlow::None` for a non-error value), and an `if`
    /// Try to lower a ForStrLines body into a `ScanLines` node for faster
    /// execution. Returns `Some(ScanLines)` if the body matches the simple scanner
    /// pattern: an optional `let trimmed = line.trim()` followed by an `IfBool`
    /// where every branch is a counter increment.
    pub(super) fn try_lower_scan_lines(
        &self,
        text: &BuildExprId,
        line_slot: usize,
        body: &[BuildStmtId],
        span: Span,
    ) -> Option<BuildStmtId> {
        let text_row = self.scratch.borrow().expressions[text.index()].clone();
        let text_slot = match text_row {
            BuildExprRow::Param(slot) => slot,
            _ => return None,
        };
        let (if_stmt, trimmed_slot) = match body {
            [if_stmt] => (*if_stmt, None),
            [trim_stmt, if_stmt] => {
                let trimmed = self.scratch.borrow().statements[trim_stmt.index()].clone();
                let BuildStmtRow::Let { slot, value } = trimmed else {
                    return None;
                };
                let expression = self.scratch.borrow().expressions[value.index()].clone();
                let BuildExprRow::Method {
                    receiver,
                    name,
                    args,
                    ..
                } = expression
                else {
                    return None;
                };
                if name.as_str() != "trim" || !args.is_empty() {
                    return None;
                }
                if !matches!(
                    self.scratch.borrow().expressions[receiver.index()],
                    BuildExprRow::Param(param) if param == line_slot
                ) {
                    return None;
                }
                (*if_stmt, Some(slot))
            }
            _ => return None,
        };
        let mut checks = Vec::new();
        if !self.collect_scan_checks(if_stmt, trimmed_slot, &mut checks) {
            return None;
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::ScanLines {
                text_slot,
                line_slot,
                checks,
                span,
            }
        ))
    }

    pub(super) fn try_lower_scan_bytes(
        &self,
        line_slot: usize,
        body: &[BuildStmtId],
        span: Span,
    ) -> Option<Vec<BuildStmtId>> {
        if let Some(lowered) = self.try_lower_scan_bytes_direct(line_slot, body, span) {
            return Some(lowered);
        }
        let mut changed = false;
        let mut lowered = Vec::with_capacity(body.len());
        for stmt in body {
            let row = self.scratch.borrow().statements[stmt.index()].clone();
            let replacement = match row {
                BuildStmtRow::If {
                    branches,
                    else_body,
                } => {
                    let mut branch_changed = false;
                    let branches = branches
                        .into_iter()
                        .map(|(condition, branch)| {
                            if let Some(branch) =
                                self.try_lower_scan_bytes(line_slot, &branch, span)
                            {
                                branch_changed = true;
                                (condition, branch)
                            } else {
                                (condition, branch)
                            }
                        })
                        .collect();
                    let else_body = else_body.and_then(|branch| {
                        self.try_lower_scan_bytes(line_slot, &branch, span)
                            .inspect(|_| branch_changed = true)
                            .or(Some(branch))
                    });
                    branch_changed.then(|| {
                        push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::If {
                                branches,
                                else_body,
                            }
                        )
                    })
                }
                BuildStmtRow::IfBool {
                    branches,
                    else_body,
                } => {
                    let mut branch_changed = false;
                    let branches = branches
                        .into_iter()
                        .map(|(condition, branch)| {
                            if let Some(branch) =
                                self.try_lower_scan_bytes(line_slot, &branch, span)
                            {
                                branch_changed = true;
                                (condition, branch)
                            } else {
                                (condition, branch)
                            }
                        })
                        .collect();
                    let else_body = else_body.and_then(|branch| {
                        self.try_lower_scan_bytes(line_slot, &branch, span)
                            .inspect(|_| branch_changed = true)
                            .or(Some(branch))
                    });
                    branch_changed.then(|| {
                        push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::IfBool {
                                branches,
                                else_body,
                            }
                        )
                    })
                }
                _ => None,
            };
            if let Some(stmt) = replacement {
                changed = true;
                lowered.push(stmt);
            } else {
                lowered.push(*stmt);
            }
        }
        changed.then_some(lowered)
    }

    fn try_lower_scan_bytes_direct(
        &self,
        line_slot: usize,
        body: &[BuildStmtId],
        span: Span,
    ) -> Option<Vec<BuildStmtId>> {
        let (while_index, while_id) = body.iter().enumerate().find_map(|(index, stmt)| {
            matches!(
                self.scratch.borrow().statements[stmt.index()],
                BuildStmtRow::WhileBool { .. } | BuildStmtRow::While { .. }
            )
            .then_some((index, *stmt))
        })?;
        let (index_slot, line_len_slot, loop_body) =
            match self.scratch.borrow().statements[while_id.index()].clone() {
                BuildStmtRow::WhileBool { condition, body } => {
                    let BuildBoolRow::IntCompare {
                        op: BinaryOp::Lt,
                        left,
                        right,
                    } = self.scratch.borrow().bools[condition.index()].clone()
                    else {
                        return None;
                    };
                    (
                        self.scan_bytes_int_slot(left)?,
                        self.scan_bytes_int_slot(right)?,
                        body,
                    )
                }
                BuildStmtRow::While { condition, body } => {
                    let BuildExprRow::Binary {
                        op: BinaryOp::Lt,
                        left,
                        right,
                        ..
                    } = self.scratch.borrow().expressions[condition.index()].clone()
                    else {
                        return None;
                    };
                    (
                        self.scan_bytes_expr_slot(left)?,
                        self.scan_bytes_expr_slot(right)?,
                        body,
                    )
                }
                _ => return None,
            };
        if loop_body.len() != 3 {
            return None;
        }
        let (ch_slot, next_slot) = match (
            self.scratch.borrow().statements[loop_body[0].index()].clone(),
            self.scratch.borrow().statements[loop_body[1].index()].clone(),
        ) {
            (
                BuildStmtRow::LetInt {
                    slot: ch_slot,
                    value: byte_value,
                },
                BuildStmtRow::LetInt {
                    slot: next_slot,
                    value: next_value,
                },
            ) if self.scan_bytes_byte_at(byte_value, line_slot, index_slot, 0)
                && self.scan_bytes_byte_at(next_value, line_slot, index_slot, 1) =>
            {
                (ch_slot, next_slot)
            }
            (
                BuildStmtRow::Let {
                    slot: ch_slot,
                    value: byte_value,
                },
                BuildStmtRow::Let {
                    slot: next_slot,
                    value: next_value,
                },
            ) if self.scan_bytes_byte_at_expr(byte_value, line_slot, index_slot, 0)
                && self.scan_bytes_byte_at_expr(next_value, line_slot, index_slot, 1) =>
            {
                (ch_slot, next_slot)
            }
            _ => return None,
        };
        let control = self.scratch.borrow().statements[loop_body[2].index()].clone();
        if let BuildStmtRow::If {
            branches,
            else_body: Some(_),
        } = control.clone()
        {
            if branches.len() != 5 {
                return None;
            }
            let block_depth_slot =
                self.scan_bytes_expr_compare_slot(branches[0].0, BinaryOp::Gt, Some(0), None)?;
            let in_string_slot = self.scan_bytes_expr_slot(branches[1].0)?;
            if !self.scan_bytes_expr_quote_condition(branches[2].0, ch_slot)
                || !self.scan_bytes_expr_pair_condition(branches[3].0, ch_slot, next_slot, 47, 47)
                || !self.scan_bytes_expr_pair_condition(branches[4].0, ch_slot, next_slot, 47, 42)
            {
                return None;
            }
            let comment_seen_slot = self.scan_bytes_expr_true_assignment(&branches[0].1)?;
            let code_seen_slot = self.scan_bytes_expr_true_assignment(&branches[1].1)?;
            let escaped_slot = self.scan_bytes_expr_nested_slot(&branches[1].1)?;
            let string_delim_slot =
                self.scan_bytes_expr_delimiter_assignment(&branches[2].1, ch_slot)?;
            let config = ScanBytes {
                line_slot,
                block_depth_slot,
                code_seen_slot,
                comment_seen_slot,
                in_string_slot,
                string_delim_slot,
                escaped_slot,
                nested: false,
                span,
            };
            let scan = push_build_row!(self, stmt, BuildStmtRow::ScanBytes { config });
            let mut lowered = body.to_vec();
            lowered[while_index] = scan;
            return Some(lowered);
        }
        let BuildStmtRow::IfBool {
            branches,
            else_body,
        } = control
        else {
            return None;
        };
        if branches.len() != 5 || else_body.is_none() {
            return None;
        }
        let block_depth_slot =
            self.scan_bytes_compare_slot(branches[0].0, BinaryOp::Gt, Some(0), None)?;
        let in_string_slot = self.scan_bytes_bool_slot(branches[1].0)?;
        if !self.scan_bytes_quote_condition(branches[2].0, ch_slot)
            || !self.scan_bytes_pair_condition(branches[3].0, ch_slot, next_slot, 47, 47)
            || !self.scan_bytes_pair_condition(branches[4].0, ch_slot, next_slot, 47, 42)
        {
            return None;
        }
        let comment_seen_slot = self.scan_bytes_true_assignment(&branches[0].1)?;
        let code_seen_slot = self.scan_bytes_true_assignment(&branches[1].1)?;
        let escaped_slot = self.scan_bytes_nested_bool_slot(&branches[1].1)?;
        let string_delim_slot = self.scan_bytes_delimiter_assignment(&branches[2].1, ch_slot)?;
        if line_len_slot == index_slot
            || block_depth_slot == code_seen_slot
            || block_depth_slot == comment_seen_slot
            || in_string_slot == escaped_slot
        {
            return None;
        }
        let config = ScanBytes {
            line_slot,
            block_depth_slot,
            code_seen_slot,
            comment_seen_slot,
            in_string_slot,
            string_delim_slot,
            escaped_slot,
            nested: false,
            span,
        };
        let scan = push_build_row!(self, stmt, BuildStmtRow::ScanBytes { config });
        let mut lowered = body.to_vec();
        lowered[while_index] = scan;
        Some(lowered)
    }

    fn scan_bytes_int_slot(&self, value: BuildIntId) -> Option<usize> {
        match self.scratch.borrow().ints[value.index()] {
            BuildIntRow::Slot(slot) => Some(slot),
            _ => None,
        }
    }

    fn scan_bytes_expr_slot(&self, value: BuildExprId) -> Option<usize> {
        match self.scratch.borrow().expressions[value.index()] {
            BuildExprRow::Param(slot) => Some(slot),
            _ => None,
        }
    }

    fn scan_bytes_byte_at(
        &self,
        value: BuildIntId,
        line_slot: usize,
        index_slot: usize,
        offset: i64,
    ) -> bool {
        let BuildIntRow::StrByteAtSlot {
            slot,
            index,
            default,
            ..
        } = self.scratch.borrow().ints[value.index()].clone()
        else {
            return false;
        };
        if slot != line_slot || default.is_some() {
            return false;
        }
        match (self.scratch.borrow().ints[index.index()].clone(), offset) {
            (BuildIntRow::Slot(slot), 0) => slot == index_slot,
            (
                BuildIntRow::Binary {
                    op: BinaryOp::Add,
                    left,
                    right,
                },
                1,
            ) => {
                self.scan_bytes_int_slot(left) == Some(index_slot)
                    && matches!(
                        self.scratch.borrow().ints[right.index()],
                        BuildIntRow::Int(1)
                    )
            }
            _ => false,
        }
    }

    fn scan_bytes_byte_at_expr(
        &self,
        value: BuildExprId,
        line_slot: usize,
        index_slot: usize,
        offset: i64,
    ) -> bool {
        let BuildExprRow::StrByteAt {
            receiver, index, ..
        } = self.scratch.borrow().expressions[value.index()].clone()
        else {
            return false;
        };
        if self.scan_bytes_expr_slot(receiver) != Some(line_slot) {
            return false;
        }
        match (
            self.scratch.borrow().expressions[index.index()].clone(),
            offset,
        ) {
            (BuildExprRow::Param(slot), 0) => slot == index_slot,
            (
                BuildExprRow::Binary {
                    op: BinaryOp::Add,
                    left,
                    right,
                    ..
                },
                1,
            ) => {
                self.scan_bytes_expr_slot(left) == Some(index_slot)
                    && matches!(
                        self.scratch.borrow().expressions[right.index()],
                        BuildExprRow::Int(1)
                    )
            }
            _ => false,
        }
    }

    fn scan_bytes_expr_compare_slot(
        &self,
        value: BuildExprId,
        expected_op: BinaryOp,
        expected_right: Option<i64>,
        expected_left: Option<usize>,
    ) -> Option<usize> {
        let BuildExprRow::Binary {
            op, left, right, ..
        } = self.scratch.borrow().expressions[value.index()].clone()
        else {
            return None;
        };
        if op != expected_op {
            return None;
        }
        if let Some(expected_right) = expected_right
            && !matches!(self.scratch.borrow().expressions[right.index()], BuildExprRow::Int(value) if value == expected_right)
        {
            return None;
        }
        let slot = self.scan_bytes_expr_slot(left)?;
        if expected_left.is_some_and(|expected| expected != slot) {
            return None;
        }
        Some(slot)
    }

    fn scan_bytes_expr_pair_condition(
        &self,
        value: BuildExprId,
        left_slot: usize,
        right_slot: usize,
        left_value: i64,
        right_value: i64,
    ) -> bool {
        let BuildExprRow::Binary {
            op: BinaryOp::And,
            left,
            right,
            ..
        } = self.scratch.borrow().expressions[value.index()].clone()
        else {
            return false;
        };
        self.scan_bytes_expr_compare_slot(left, BinaryOp::Eq, Some(left_value), Some(left_slot))
            .is_some_and(|_| {
                self.scan_bytes_expr_compare_slot(
                    right,
                    BinaryOp::Eq,
                    Some(right_value),
                    Some(right_slot),
                )
                .is_some()
            })
    }

    fn scan_bytes_expr_quote_condition(&self, value: BuildExprId, ch_slot: usize) -> bool {
        let mut values = Vec::new();
        self.scan_bytes_expr_quote_values(value, ch_slot, &mut values);
        values.sort_unstable();
        values == [34, 39, 96]
    }

    fn scan_bytes_expr_quote_values(
        &self,
        value: BuildExprId,
        ch_slot: usize,
        values: &mut Vec<i64>,
    ) {
        match self.scratch.borrow().expressions[value.index()].clone() {
            BuildExprRow::Binary {
                op: BinaryOp::Or,
                left,
                right,
                ..
            } => {
                self.scan_bytes_expr_quote_values(left, ch_slot, values);
                self.scan_bytes_expr_quote_values(right, ch_slot, values);
            }
            BuildExprRow::Binary {
                op: BinaryOp::Eq,
                left,
                right,
                ..
            } if self.scan_bytes_expr_slot(left) == Some(ch_slot) => {
                if let BuildExprRow::Int(value) = self.scratch.borrow().expressions[right.index()] {
                    values.push(value);
                }
            }
            _ => {}
        }
    }

    fn scan_bytes_expr_true_assignment(&self, statements: &[BuildStmtId]) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            match self.scratch.borrow().statements[stmt.index()].clone() {
                BuildStmtRow::Assign {
                    check: None,
                    slot,
                    op: AssignOp::Set,
                    value,
                    ..
                } => matches!(
                    self.scratch.borrow().expressions[value.index()],
                    BuildExprRow::Bool(true)
                )
                .then_some(slot),
                BuildStmtRow::AssignBool { slot, value } => matches!(
                    self.scratch.borrow().bools[value.index()],
                    BuildBoolRow::Bool(true)
                )
                .then_some(slot),
                _ => None,
            }
        })
    }

    fn scan_bytes_expr_nested_slot(&self, statements: &[BuildStmtId]) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            let BuildStmtRow::If { branches, .. } =
                self.scratch.borrow().statements[stmt.index()].clone()
            else {
                return None;
            };
            branches
                .first()
                .and_then(|(condition, _)| self.scan_bytes_expr_slot(*condition))
        })
    }

    fn scan_bytes_expr_delimiter_assignment(
        &self,
        statements: &[BuildStmtId],
        ch_slot: usize,
    ) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            let BuildStmtRow::Assign {
                check: None,
                slot,
                op: AssignOp::Set,
                value,
                ..
            } = self.scratch.borrow().statements[stmt.index()].clone()
            else {
                return None;
            };
            (self.scan_bytes_expr_slot(value) == Some(ch_slot)).then_some(slot)
        })
    }

    fn scan_bytes_bool_slot(&self, value: BuildBoolId) -> Option<usize> {
        match self.scratch.borrow().bools[value.index()] {
            BuildBoolRow::Slot(slot) => Some(slot),
            _ => None,
        }
    }

    fn scan_bytes_compare_slot(
        &self,
        value: BuildBoolId,
        expected_op: BinaryOp,
        expected_right: Option<i64>,
        expected_left: Option<usize>,
    ) -> Option<usize> {
        let BuildBoolRow::IntCompare { op, left, right } =
            self.scratch.borrow().bools[value.index()].clone()
        else {
            return None;
        };
        if op != expected_op {
            return None;
        }
        if let Some(expected_right) = expected_right
            && !matches!(self.scratch.borrow().ints[right.index()], BuildIntRow::Int(value) if value == expected_right)
        {
            return None;
        }
        let slot = self.scan_bytes_int_slot(left)?;
        if expected_left.is_some_and(|expected| expected != slot) {
            return None;
        }
        Some(slot)
    }

    fn scan_bytes_pair_condition(
        &self,
        value: BuildBoolId,
        left_slot: usize,
        right_slot: usize,
        left_value: i64,
        right_value: i64,
    ) -> bool {
        let BuildBoolRow::And(left, right) = self.scratch.borrow().bools[value.index()].clone()
        else {
            return false;
        };
        self.scan_bytes_compare_slot(left, BinaryOp::Eq, Some(left_value), Some(left_slot))
            .is_some_and(|_| {
                self.scan_bytes_compare_slot(
                    right,
                    BinaryOp::Eq,
                    Some(right_value),
                    Some(right_slot),
                )
                .is_some()
            })
    }

    fn scan_bytes_quote_condition(&self, value: BuildBoolId, ch_slot: usize) -> bool {
        let BuildBoolRow::Or(left, right) = self.scratch.borrow().bools[value.index()].clone()
        else {
            return false;
        };
        let mut values = Vec::new();
        self.scan_bytes_quote_values(left, ch_slot, &mut values);
        self.scan_bytes_quote_values(right, ch_slot, &mut values);
        values.sort_unstable();
        values == [34, 39, 96]
    }

    fn scan_bytes_quote_values(&self, value: BuildBoolId, ch_slot: usize, values: &mut Vec<i64>) {
        match self.scratch.borrow().bools[value.index()].clone() {
            BuildBoolRow::Or(left, right) => {
                self.scan_bytes_quote_values(left, ch_slot, values);
                self.scan_bytes_quote_values(right, ch_slot, values);
            }
            _ => {
                let BuildBoolRow::IntCompare {
                    op: BinaryOp::Eq,
                    left,
                    right,
                } = self.scratch.borrow().bools[value.index()].clone()
                else {
                    return;
                };
                if self.scan_bytes_int_slot(left) == Some(ch_slot)
                    && let BuildIntRow::Int(value) = self.scratch.borrow().ints[right.index()]
                {
                    values.push(value);
                }
            }
        }
    }

    fn scan_bytes_true_assignment(&self, statements: &[BuildStmtId]) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            let BuildStmtRow::AssignBool { slot, value } =
                self.scratch.borrow().statements[stmt.index()].clone()
            else {
                return None;
            };
            matches!(
                self.scratch.borrow().bools[value.index()],
                BuildBoolRow::Bool(true)
            )
            .then_some(slot)
        })
    }

    fn scan_bytes_nested_bool_slot(&self, statements: &[BuildStmtId]) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            let BuildStmtRow::IfBool { branches, .. } =
                self.scratch.borrow().statements[stmt.index()].clone()
            else {
                return None;
            };
            branches
                .first()
                .and_then(|(condition, _)| self.scan_bytes_bool_slot(*condition))
        })
    }

    fn scan_bytes_delimiter_assignment(
        &self,
        statements: &[BuildStmtId],
        ch_slot: usize,
    ) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            let BuildStmtRow::AssignInt {
                slot,
                op: AssignOp::Set,
                value,
                ..
            } = self.scratch.borrow().statements[stmt.index()].clone()
            else {
                return None;
            };
            (self.scan_bytes_int_slot(value) == Some(ch_slot)).then_some(slot)
        })
    }

    fn collect_scan_checks(
        &self,
        stmt: BuildStmtId,
        trimmed_slot: Option<usize>,
        checks: &mut Vec<ScanCheck>,
    ) -> bool {
        let BuildStmtRow::IfBool {
            branches,
            else_body,
        } = self.scratch.borrow().statements[stmt.index()].clone()
        else {
            return false;
        };
        for (condition, branch_body) in &branches {
            if branch_body.len() != 1 {
                return false;
            }
            let assignment = self.scratch.borrow().statements[branch_body[0].index()].clone();
            let counter_slot = match assignment {
                BuildStmtRow::Assign {
                    check: None,
                    slot,
                    op: AssignOp::Add,
                    value,
                    ..
                } if matches!(
                    self.scratch.borrow().expressions[value.index()],
                    BuildExprRow::Int(1)
                ) =>
                {
                    slot
                }
                BuildStmtRow::AssignInt {
                    slot,
                    op: AssignOp::Add,
                    value,
                    ..
                } if matches!(
                    self.scratch.borrow().ints[value.index()],
                    BuildIntRow::Int(1)
                ) =>
                {
                    slot
                }
                _ => return false,
            };
            let condition = self.scratch.borrow().bools[condition.index()].clone();
            let scan_condition = match condition {
                BuildBoolRow::TrimEmptySlot { .. } => ScanCondition::TrimEmpty,
                BuildBoolRow::LiteralCompareSlot {
                    op: BinaryOp::Eq,
                    slot,
                    value: LoweredValue::Bytes(value),
                } if trimmed_slot == Some(slot) && value.is_empty() => ScanCondition::TrimEmpty,
                BuildBoolRow::TrimStrPredicateSlot {
                    predicate: LoweredStrPredicate::StartsWith,
                    needle,
                    ..
                } => ScanCondition::TrimStartsWith(needle.to_vec()),
                BuildBoolRow::StrPredicateSlot {
                    predicate: LoweredStrPredicate::StartsWith,
                    slot,
                    needle,
                    ..
                } => {
                    if trimmed_slot == Some(slot) {
                        ScanCondition::TrimStartsWith(needle.to_vec())
                    } else {
                        ScanCondition::StartsWith(needle.to_vec())
                    }
                }
                _ => return false,
            };
            checks.push(ScanCheck {
                condition: scan_condition,
                counter_slot,
            });
        }
        let Some(else_body) = else_body else {
            return true;
        };
        if else_body.len() != 1 {
            return false;
        }
        self.collect_scan_checks(else_body[0], trimmed_slot, checks)
    }
}
