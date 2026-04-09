#[derive(Debug)]
pub enum ParseError {
    InvalidFormat(String),
    MissingField(String),
    OutOfRange { field: String, value: i64 },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormat(msg) => write!(f, "invalid format: {msg}"),
            Self::MissingField(name) => write!(f, "missing field: {name}"),
            Self::OutOfRange { field, value } => {
                write!(f, "field {field} out of range: {value}")
            }
        }
    }
}
