#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryOp {
    ResultFallback,
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    NotIn,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    /// `|` on two sets.
    Union,
    /// `&` on two sets.
    Intersect,
}
