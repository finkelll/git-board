use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub conclusion: Option<String>,
    pub created_at: DateTime<Utc>,
    pub database_id: u64,
    pub display_title: String,
    pub event: String,
    pub head_branch: String,
    pub name: String,
    pub started_at: Option<DateTime<Utc>>,
    pub status: String,
    pub updated_at: DateTime<Utc>,
    pub workflow_name: String,
}

impl Run {
    pub fn workflow_label(&self) -> &str {
        if self.workflow_name.is_empty() {
            &self.name
        } else {
            &self.workflow_name
        }
    }

    pub fn status_label(&self) -> &str {
        self.conclusion
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&self.status)
    }
}
