use crate::config::{Filters, Settings};
use crate::model::Run;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::process::Command;

const JSON_FIELDS: &str = concat!(
    "attempt,conclusion,createdAt,databaseId,displayTitle,event,headBranch,",
    "headSha,name,number,startedAt,status,updatedAt,workflowDatabaseId,workflowName"
);

pub trait CommandRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<String>;
}

pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<String> {
        let output = Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("failed to execute {program}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            bail!(
                "{program} exited with status {}{}",
                output.status,
                if stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {stderr}")
                }
            );
        }

        String::from_utf8(output.stdout).context("gh output was not valid UTF-8")
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoView {
    name_with_owner: String,
}

pub fn resolve_repo(settings: &Settings, runner: &impl CommandRunner) -> Result<String> {
    if let Some(repo) = &settings.repo {
        return Ok(repo.clone());
    }

    let args = vec![
        "repo".to_string(),
        "view".to_string(),
        "--json".to_string(),
        "nameWithOwner".to_string(),
    ];
    let output = runner
        .run("gh", &args)
        .context("failed to infer repository. Run from a GitHub repo or pass --repo owner/name")?;
    let repo: RepoView =
        serde_json::from_str(&output).context("failed to parse gh repo view JSON")?;

    if repo.name_with_owner.trim().is_empty() {
        bail!("failed to infer repository. Run from a GitHub repo or pass --repo owner/name");
    }

    Ok(repo.name_with_owner)
}

pub fn fetch_runs(
    repo: &str,
    settings: &Settings,
    runner: &impl CommandRunner,
) -> Result<Vec<Run>> {
    let args = run_list_args(repo, settings.limit, &settings.filters);
    let output = runner.run("gh", &args)?;
    parse_runs(&output)
}

pub fn open_run(repo: &str, id: u64, runner: &impl CommandRunner) -> Result<()> {
    let args = vec![
        "run".to_string(),
        "view".to_string(),
        id.to_string(),
        "--repo".to_string(),
        repo.to_string(),
        "--web".to_string(),
    ];
    runner.run("gh", &args).map(|_| ())
}

fn run_list_args(repo: &str, limit: usize, filters: &Filters) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "list".to_string(),
        "--repo".to_string(),
        repo.to_string(),
        "--limit".to_string(),
        limit.to_string(),
        "--json".to_string(),
        JSON_FIELDS.to_string(),
    ];

    push_filter(&mut args, "--branch", &filters.branch);
    push_filter(&mut args, "--workflow", &filters.workflow);
    push_filter(&mut args, "--status", &filters.status);
    push_filter(&mut args, "--event", &filters.event);

    args
}

fn push_filter(args: &mut Vec<String>, flag: &str, value: &Option<String>) {
    if let Some(value) = value {
        args.push(flag.to_string());
        args.push(value.to_string());
    }
}

fn parse_runs(output: &str) -> Result<Vec<Run>> {
    serde_json::from_str(output).context("failed to parse gh run list JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Filters;

    #[test]
    fn builds_run_list_args_with_filters() {
        let args = run_list_args(
            "owner/repo",
            30,
            &Filters {
                branch: Some("main".to_string()),
                workflow: Some("verify".to_string()),
                status: Some("success".to_string()),
                event: None,
            },
        );

        assert!(args.windows(2).any(|pair| pair == ["--repo", "owner/repo"]));
        assert!(args.windows(2).any(|pair| pair == ["--limit", "30"]));
        assert!(args.windows(2).any(|pair| pair == ["--branch", "main"]));
        assert!(args.windows(2).any(|pair| pair == ["--workflow", "verify"]));
        assert!(args.windows(2).any(|pair| pair == ["--status", "success"]));
    }

    #[test]
    fn parses_runs() {
        let json = r#"[{
            "conclusion":"success",
            "createdAt":"2026-05-29T14:00:00Z",
            "databaseId":123,
            "displayTitle":"feat: dashboard",
            "event":"push",
            "headBranch":"main",
            "headSha":"abc",
            "name":"verify",
            "number":1,
            "startedAt":"2026-05-29T14:00:03Z",
            "status":"completed",
            "updatedAt":"2026-05-29T14:01:03Z",
            "workflowDatabaseId":456,
            "workflowName":"verify"
        }]"#;

        let runs = parse_runs(json).unwrap();
        assert_eq!(runs[0].database_id, 123);
        assert_eq!(runs[0].workflow_label(), "verify");
    }
}
