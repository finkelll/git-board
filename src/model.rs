use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub author: Option<PullRequestAuthor>,
    pub base_ref_name: String,
    pub created_at: DateTime<Utc>,
    pub head_ref_name: String,
    pub is_draft: bool,
    pub number: u64,
    pub title: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PullRequestAuthor {
    pub login: String,
}

impl PullRequest {
    pub fn status_label(&self) -> &'static str {
        if self.is_draft {
            "draft"
        } else {
            "open"
        }
    }

    pub fn author_login(&self) -> &str {
        self.author
            .as_ref()
            .map(|author| author.login.as_str())
            .unwrap_or("unknown")
    }
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
