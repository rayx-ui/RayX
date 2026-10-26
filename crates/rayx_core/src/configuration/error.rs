use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionsFinalizeStage {
    Normalize,
    Validate,
}

impl fmt::Display for OptionsFinalizeStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normalize => formatter.write_str("normalize"),
            Self::Validate => formatter.write_str("validate"),
        }
    }
}

#[derive(Debug)]
pub struct OptionsFinalizeError {
    stage: OptionsFinalizeStage,
    message: String,
}

impl OptionsFinalizeError {
    pub fn normalization(message: impl Into<String>) -> Self {
        Self {
            stage: OptionsFinalizeStage::Normalize,
            message: message.into(),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            stage: OptionsFinalizeStage::Validate,
            message: message.into(),
        }
    }

    pub fn stage(&self) -> OptionsFinalizeStage {
        self.stage
    }
}

impl From<String> for OptionsFinalizeError {
    fn from(message: String) -> Self {
        Self::validation(message)
    }
}

impl From<&str> for OptionsFinalizeError {
    fn from(message: &str) -> Self {
        Self::validation(message)
    }
}

impl fmt::Display for OptionsFinalizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for OptionsFinalizeError {}

#[derive(Debug)]
pub enum ConfigurationError {
    OptionsFinalize {
        section: String,
        type_name: &'static str,
        stage: OptionsFinalizeStage,
        source: Box<OptionsFinalizeError>,
    },
    OptionsPlanConflict {
        type_name: &'static str,
        existing_section: String,
        requested_section: String,
    },
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OptionsFinalize {
                section,
                type_name,
                stage,
                source,
            } => write!(
                formatter,
                "failed to {stage} options `{type_name}` for section `{section}`: {source}"
            ),
            Self::OptionsPlanConflict {
                type_name,
                existing_section,
                requested_section,
            } => write!(
                formatter,
                "options plan conflict for `{type_name}`: existing section `{existing_section}`, requested section `{requested_section}`"
            ),
        }
    }
}

impl std::error::Error for ConfigurationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::OptionsFinalize { source, .. } => Some(source.as_ref()),
            Self::OptionsPlanConflict { .. } => None,
        }
    }
}
