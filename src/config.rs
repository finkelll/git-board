use crate::cli::Cli;
use crate::columns::{
    parse_columns_csv, parse_columns_list, parse_pr_columns_csv, parse_pr_columns_list, Column,
    PrColumn,
};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::env;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

const DEFAULT_INTERVAL: Duration = Duration::from_secs(15);
const MIN_INTERVAL: Duration = Duration::from_secs(5);
const DEFAULT_LIMIT: usize = 20;
const DEFAULT_CURSOR_AUTO_HIDE: bool = true;
const DEFAULT_CURSOR_HIDE_AFTER: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct Settings {
    pub repo: Option<String>,
    pub interval: Duration,
    pub limit: usize,
    pub columns: Vec<Column>,
    pub pr_columns: Vec<PrColumn>,
    pub filters: Filters,
    pub cursor: CursorSettings,
    pub global: bool,
    pub layout: DashboardLayout,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    pub branch: Option<String>,
    pub workflow: Option<String>,
    pub status: Option<String>,
    pub event: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorSettings {
    pub auto_hide: bool,
    pub hide_after: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardLayout {
    InProgress,
    All,
}

impl DashboardLayout {
    pub const VALUES: [Self; 2] = [Self::InProgress, Self::All];

    pub fn label(self) -> &'static str {
        match self {
            Self::InProgress => "in progress",
            Self::All => "all",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::VALUES
            .iter()
            .position(|layout| *layout == self)
            .unwrap_or(0);
        Self::VALUES[(index + 1) % Self::VALUES.len()]
    }
}

impl fmt::Display for DashboardLayout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

impl FromStr for DashboardLayout {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "in-progress" | "in progress" | "progress" => Ok(Self::InProgress),
            "all" => Ok(Self::All),
            _ => bail!("layout must be one of: in-progress, all"),
        }
    }
}

const DEFAULT_LAYOUT: DashboardLayout = DashboardLayout::All;

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    repo: Option<String>,
    interval: Option<String>,
    limit: Option<usize>,
    layout: Option<String>,
    columns: Option<Vec<String>>,
    pr_columns: Option<Vec<String>>,
    filters: Option<FileFilters>,
    cursor: Option<FileCursor>,
}

#[derive(Debug, Default, Deserialize)]
struct FileFilters {
    branch: Option<String>,
    workflow: Option<String>,
    status: Option<String>,
    event: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FileCursor {
    auto_hide: Option<bool>,
    hide_after: Option<String>,
}

impl Settings {
    pub fn load(args: Cli) -> Result<Self> {
        let file = load_config_file()?;
        let cli_cursor_auto_hide = cli_cursor_auto_hide(&args);

        let interval = match args.interval.or(file.interval) {
            Some(value) => parse_interval(&value)?,
            None => DEFAULT_INTERVAL,
        };
        if interval < MIN_INTERVAL {
            bail!("interval must be at least 5s");
        }

        let columns = match (args.columns, file.columns) {
            (Some(value), _) => parse_columns_csv(&value)?,
            (None, Some(values)) => parse_columns_list(&values)?,
            (None, None) => Column::default_columns(),
        };
        let pr_columns = match (args.pr_columns, file.pr_columns) {
            (Some(value), _) => parse_pr_columns_csv(&value)?,
            (None, Some(values)) => parse_pr_columns_list(&values)?,
            (None, None) => PrColumn::default_columns(),
        };

        let file_filters = file.filters.unwrap_or_default();
        let file_cursor = file.cursor.unwrap_or_default();
        let cursor = CursorSettings {
            auto_hide: cli_cursor_auto_hide
                .or(file_cursor.auto_hide)
                .unwrap_or(DEFAULT_CURSOR_AUTO_HIDE),
            hide_after: match args.cursor_hide_after.or(file_cursor.hide_after) {
                Some(value) => parse_interval(&value)?,
                None => DEFAULT_CURSOR_HIDE_AFTER,
            },
        };

        Ok(Self {
            repo: non_empty(args.repo).or_else(|| non_empty(file.repo)),
            interval,
            limit: args.limit.or(file.limit).unwrap_or(DEFAULT_LIMIT),
            columns,
            pr_columns,
            filters: Filters {
                branch: non_empty(args.branch).or_else(|| non_empty(file_filters.branch)),
                workflow: non_empty(args.workflow).or_else(|| non_empty(file_filters.workflow)),
                status: non_empty(args.status).or_else(|| non_empty(file_filters.status)),
                event: non_empty(args.event).or_else(|| non_empty(file_filters.event)),
            },
            cursor,
            global: !args.independent,
            layout: match args.layout.or(file.layout) {
                Some(value) => value.parse()?,
                None => DEFAULT_LAYOUT,
            },
        })
    }
}

fn cli_cursor_auto_hide(args: &Cli) -> Option<bool> {
    if args.cursor_auto_hide {
        Some(true)
    } else if args.no_cursor_auto_hide {
        Some(false)
    } else {
        None
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn parse_interval(value: &str) -> Result<Duration> {
    humantime::parse_duration(value).with_context(|| format!("invalid interval '{value}'"))
}

fn load_config_file() -> Result<FileConfig> {
    let Some(path) = config_path() else {
        return Ok(FileConfig::default());
    };

    if !path.exists() {
        return Ok(FileConfig::default());
    }

    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("failed to parse config {}", path.display()))
}

pub fn config_path() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|dir| dir.join("git-board").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_empty_filters() {
        assert_eq!(non_empty(Some("  ".to_string())), None);
        assert_eq!(
            non_empty(Some(" main ".to_string())),
            Some("main".to_string())
        );
    }

    #[test]
    fn parses_duration() {
        assert_eq!(parse_interval("10s").unwrap(), Duration::from_secs(10));
    }

    #[test]
    fn defaults_cursor_auto_hide() {
        let cursor = CursorSettings {
            auto_hide: DEFAULT_CURSOR_AUTO_HIDE,
            hide_after: DEFAULT_CURSOR_HIDE_AFTER,
        };

        assert_eq!(cursor.auto_hide, true);
        assert_eq!(cursor.hide_after, Duration::from_secs(5));
    }

    #[test]
    fn defaults_layout_all() {
        assert_eq!(DEFAULT_LAYOUT, DashboardLayout::All);
    }

    #[test]
    fn parses_dashboard_layout() {
        assert_eq!(
            "in-progress".parse::<DashboardLayout>().unwrap(),
            DashboardLayout::InProgress
        );
        assert_eq!(
            "all".parse::<DashboardLayout>().unwrap(),
            DashboardLayout::All
        );
        assert!("failures".parse::<DashboardLayout>().is_err());
    }
}
