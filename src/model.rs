use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub attempt: Option<u64>,
    pub conclusion: Option<String>,
    pub created_at: DateTime<Utc>,
    pub database_id: u64,
    pub display_title: String,
    pub event: String,
    pub head_branch: String,
    pub head_sha: Option<String>,
    pub name: String,
    pub number: Option<u64>,
    #[serde(default)]
    pub failure_data: Vec<String>,
    #[serde(default)]
    pub failure_reason: Option<String>,
    pub pr_number: Option<u64>,
    pub started_at: Option<DateTime<Utc>>,
    pub status: String,
    pub updated_at: DateTime<Utc>,
    pub url: Option<String>,
    pub workflow_database_id: Option<u64>,
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

    pub fn is_pull_request_event(&self) -> bool {
        self.event.starts_with("pull_request")
    }

    pub fn status_label(&self) -> &str {
        self.conclusion
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&self.status)
    }

    pub fn failure_label(&self) -> Option<&str> {
        match self.status_label() {
            "failure" | "cancelled" | "timed_out" | "startup_failure" | "action_required" => {
                Some(self.status_label())
            }
            _ => None,
        }
    }
}
