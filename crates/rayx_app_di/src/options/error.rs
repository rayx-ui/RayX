#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptionsError {
    Missing { type_name: &'static str },
}

impl std::fmt::Display for OptionsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { type_name } => {
                write!(formatter, "options for `{type_name}` are not registered")
            }
        }
    }
}

impl std::error::Error for OptionsError {}
