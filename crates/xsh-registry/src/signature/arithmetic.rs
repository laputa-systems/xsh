#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ArithmeticOperator { Add, Subtract, Multiply, Divide, Remainder }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArithmeticDomain { Integer, Int, Float, Str, Duration, List, Set }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArithmeticResult { Int, Float, Str, Duration, ListConcatenation, SetDifference }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ArithmeticSignature {
    Integer,
    Float,
    StringConcatenation,
    ListConcatenation,
    SetDifference,
    DurationPair,
    DurationScale,
    IntegerScale,
    DurationRatio,
}

impl ArithmeticSignature {
    pub const ALL: [Self; 9] = [Self::Integer, Self::Float, Self::StringConcatenation, Self::ListConcatenation, Self::SetDifference, Self::DurationPair, Self::DurationScale, Self::IntegerScale, Self::DurationRatio];

    pub const fn accepts_operator(self, operator: ArithmeticOperator) -> bool {
        use ArithmeticOperator as O;
        match self {
            Self::Integer => true,
            Self::Float => !matches!(operator, O::Remainder),
            Self::StringConcatenation | Self::ListConcatenation => matches!(operator, O::Add),
            Self::SetDifference => matches!(operator, O::Subtract),
            Self::DurationPair => matches!(operator, O::Add | O::Subtract),
            Self::DurationScale => matches!(operator, O::Multiply | O::Divide),
            Self::IntegerScale => matches!(operator, O::Multiply),
            Self::DurationRatio => matches!(operator, O::Divide),
        }
    }

    pub const fn domains(self) -> (ArithmeticDomain, ArithmeticDomain) {
        use ArithmeticDomain as D;
        match self {
            Self::Integer => (D::Integer, D::Integer),
            Self::Float => (D::Float, D::Float),
            Self::StringConcatenation => (D::Str, D::Str),
            Self::ListConcatenation => (D::List, D::List),
            Self::SetDifference => (D::Set, D::Set),
            Self::DurationPair => (D::Duration, D::Duration),
            Self::DurationScale => (D::Duration, D::Int),
            Self::IntegerScale => (D::Int, D::Duration),
            Self::DurationRatio => (D::Duration, D::Duration),
        }
    }

    pub const fn result(self) -> ArithmeticResult {
        match self {
            Self::Integer | Self::DurationRatio => ArithmeticResult::Int,
            Self::Float => ArithmeticResult::Float,
            Self::StringConcatenation => ArithmeticResult::Str,
            Self::ListConcatenation => ArithmeticResult::ListConcatenation,
            Self::SetDifference => ArithmeticResult::SetDifference,
            Self::DurationPair | Self::DurationScale | Self::IntegerScale => ArithmeticResult::Duration,
        }
    }
}
