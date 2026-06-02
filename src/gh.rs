use crate::config::{Filters, Settings};
use crate::model::{PullRequest, Run};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

const JSON_FIELDS: &str = concat!(
    "attempt,conclusion,createdAt,databaseId,displayTitle,event,headBranch,",
    "headSha,name,number,startedAt,status,updatedAt,workflowDatabaseId,workflowName"
);
const PR_JSON_FIELDS: &str =
    "author,baseRefName,createdAt,headRefName,isDraft,number,title,updatedAt";
const RUN_PR_JSON_FIELDS: &str = "headRefName,number";

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
    let mut runs = parse_runs(&output)?;
    populate_run_pr_numbers(repo, &mut runs, runner)?;
    Ok(runs)
}

pub fn fetch_pull_requests(
    repo: &str,
    settings: &Settings,
    runner: &impl CommandRunner,
) -> Result<Vec<PullRequest>> {
    let args = pr_list_args(repo, settings.limit);
    let output = runner.run("gh", &args)?;
    parse_pull_requests(&output)
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

pub fn open_pull_request(repo: &str, number: u64, runner: &impl CommandRunner) -> Result<()> {
    let args = vec![
        "pr".to_string(),
        "view".to_string(),
        number.to_string(),
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

fn pr_list_args(repo: &str, limit: usize) -> Vec<String> {
    vec![
        "pr".to_string(),
        "list".to_string(),
        "--repo".to_string(),
        repo.to_string(),
        "--state".to_string(),
        "open".to_string(),
        "--search".to_string(),
        "sort:updated-desc".to_string(),
        "--limit".to_string(),
        limit.to_string(),
        "--json".to_string(),
        PR_JSON_FIELDS.to_string(),
    ]
}

fn push_filter(args: &mut Vec<String>, flag: &str, value: &Option<String>) {
    if let Some(value) = value {
        args.push(flag.to_string());
        args.push(value.to_string());
    }
}

fn populate_run_pr_numbers(
    repo: &str,
    runs: &mut [Run],
    runner: &impl CommandRunner,
) -> Result<()> {
    if !runs.iter().any(Run::is_pull_request_event) {
        return Ok(());
    }

    let args = run_pr_list_args(repo, runs.len());
    let output = runner.run("gh", &args)?;
    let pull_requests = parse_run_pull_requests(&output)?;
    let mut pull_request_by_branch = HashMap::new();
    for pull_request in pull_requests {
        pull_request_by_branch
            .entry(pull_request.head_ref_name)
            .or_insert(pull_request.number);
    }

    for run in runs
        .iter_mut()
        .filter(|run| run.is_pull_request_event() && run.pr_number.is_none())
    {
        run.pr_number = pull_request_by_branch.get(&run.head_branch).copied();
    }

    Ok(())
}

fn run_pr_list_args(repo: &str, run_count: usize) -> Vec<String> {
    vec![
        "pr".to_string(),
        "list".to_string(),
        "--repo".to_string(),
        repo.to_string(),
        "--state".to_string(),
        "all".to_string(),
        "--search".to_string(),
        "sort:updated-desc".to_string(),
        "--limit".to_string(),
        run_count.max(100).to_string(),
        "--json".to_string(),
        RUN_PR_JSON_FIELDS.to_string(),
    ]
}

fn parse_runs(output: &str) -> Result<Vec<Run>> {
    serde_json::from_str(output).context("failed to parse gh run list JSON")
}

fn parse_run_pull_requests(output: &str) -> Result<Vec<RunPullRequest>> {
    serde_json::from_str(output).context("failed to parse gh workflow run pull requests JSON")
}

fn parse_pull_requests(output: &str) -> Result<Vec<PullRequest>> {
    serde_json::from_str(output).context("failed to parse gh pr list JSON")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunPullRequest {
    head_ref_name: String,
    number: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Filters;
    use std::cell::RefCell;

    struct FakeRunner {
        calls: RefCell<Vec<Vec<String>>>,
        outputs: RefCell<Vec<String>>,
    }

    impl FakeRunner {
        fn new(outputs: Vec<&str>) -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                outputs: RefCell::new(outputs.into_iter().rev().map(ToString::to_string).collect()),
            }
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, _program: &str, args: &[String]) -> Result<String> {
            self.calls.borrow_mut().push(args.to_vec());
            self.outputs
                .borrow_mut()
                .pop()
                .context("fake runner had no output")
        }
    }

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

    #[test]
    fn parses_run_pull_requests() {
        let pull_requests =
            parse_run_pull_requests(r#"[{"headRefName":"feature","number":330}]"#).unwrap();

        assert_eq!(pull_requests[0].head_ref_name, "feature");
        assert_eq!(pull_requests[0].number, 330);
    }

    #[test]
    fn fetch_runs_populates_pr_numbers_for_pr_events() {
        let runner = FakeRunner::new(vec![
            r#"[{
                "conclusion":null,
                "createdAt":"2026-05-29T14:00:00Z",
                "databaseId":123,
                "displayTitle":"feat: dashboard",
                "event":"pull_request",
                "headBranch":"feature",
                "headSha":"abc",
                "name":"verify",
                "number":1,
                "startedAt":"2026-05-29T14:00:03Z",
                "status":"in_progress",
                "updatedAt":"2026-05-29T14:01:03Z",
                "workflowDatabaseId":456,
                "workflowName":"verify"
            }]"#,
            r#"[{"headRefName":"feature","number":330}]"#,
        ]);
        let settings = Settings {
            repo: None,
            interval: std::time::Duration::from_secs(15),
            limit: 20,
            columns: Vec::new(),
            pr_columns: Vec::new(),
            filters: Filters::default(),
            cursor: crate::config::CursorSettings {
                auto_hide: true,
                hide_after: std::time::Duration::from_secs(5),
            },
            global: false,
            layout: crate::config::DashboardLayout::InProgress,
        };

        let runs = fetch_runs("owner/repo", &settings, &runner).unwrap();

        assert_eq!(runs[0].pr_number, Some(330));
        let calls = runner.calls.borrow();
        assert!(calls[1].windows(2).any(|pair| pair == ["pr", "list"]));
        assert!(calls[1].windows(2).any(|pair| pair == ["--state", "all"]));
        assert!(calls[1]
            .windows(2)
            .any(|pair| pair == ["--json", RUN_PR_JSON_FIELDS]));
    }

    #[test]
    fn builds_pr_list_args_for_open_prs() {
        let args = pr_list_args("owner/repo", 25);

        assert!(args.windows(2).any(|pair| pair == ["--repo", "owner/repo"]));
        assert!(args.windows(2).any(|pair| pair == ["--state", "open"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--search", "sort:updated-desc"]));
        assert!(args.windows(2).any(|pair| pair == ["--limit", "25"]));
    }

    #[test]
    fn parses_pull_requests() {
        let json = r#"[{
            "author":{"login":"octocat"},
            "baseRefName":"main",
            "createdAt":"2026-05-29T14:00:00Z",
            "headRefName":"feature",
            "isDraft":false,
            "number":42,
            "title":"Add screen",
            "updatedAt":"2026-05-29T15:00:00Z"
        }]"#;

        let prs = parse_pull_requests(json).unwrap();
        assert_eq!(prs[0].number, 42);
        assert_eq!(prs[0].author_login(), "octocat");
    }
}
