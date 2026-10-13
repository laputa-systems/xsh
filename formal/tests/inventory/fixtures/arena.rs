#[repr(u8)]
pub enum ArenaExprTag {
    Null,
    BoolFalse,
    Call,
}

pub enum Type {
    Any,
    List(Box<Type>),
    Result { ok: Box<Type>, error: Box<Type> },
}
