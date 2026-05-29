use clap::{ArgAction, Parser};

#[derive(Debug, Clone, Parser)]
#[command(name = "git-board")]
#[command(about = "Live GitHub Actions dashboard for the terminal")]
pub struct Cli {
    #[arg(short = 'R', long = "repo", value_name = "OWNER/REPO")]
    pub repo: Option<String>,

    #[arg(
        long = "interval",
        value_name = "DURATION",
        help = "Refresh interval, for example 15s or 1m"
    )]
    pub interval: Option<String>,

    #[arg(long = "limit", value_name = "N")]
    pub limit: Option<usize>,

    #[arg(
        long = "columns",
        value_name = "LIST",
        help = "Comma-separated columns"
    )]
    pub columns: Option<String>,

    #[arg(short = 'b', long = "branch")]
    pub branch: Option<String>,

    #[arg(short = 'w', long = "workflow")]
    pub workflow: Option<String>,

    #[arg(short = 's', long = "status")]
    pub status: Option<String>,

    #[arg(short = 'e', long = "event")]
    pub event: Option<String>,

    #[arg(
        long = "cursor-auto-hide",
        action = ArgAction::SetTrue,
        conflicts_with = "no_cursor_auto_hide",
        help = "Enable auto-hiding the row navigation cursor"
    )]
    pub cursor_auto_hide: bool,

    #[arg(
        long = "no-cursor-auto-hide",
        action = ArgAction::SetTrue,
        help = "Disable auto-hiding the row navigation cursor"
    )]
    pub no_cursor_auto_hide: bool,

    #[arg(
        long = "cursor-hide-after",
        value_name = "DURATION",
        help = "Time until the navigation cursor hides, for example 5s"
    )]
    pub cursor_hide_after: Option<String>,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
