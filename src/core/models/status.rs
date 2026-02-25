use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusIndicator {
    Operational,
    Minor,
    Major,
    Critical,
    Maintenance,
    Unknown,
}

impl std::fmt::Display for StatusIndicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Operational => write!(f, "Operational"),
            Self::Minor => write!(f, "Minor Issue"),
            Self::Major => write!(f, "Major Issue"),
            Self::Critical => write!(f, "Critical"),
            Self::Maintenance => write!(f, "Maintenance"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    pub indicator: StatusIndicator,
    pub description: Option<String>,
}
