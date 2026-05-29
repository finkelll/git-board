mod app;
mod cache;
mod cli;
mod columns;
mod config;
mod gh;
mod model;
mod panel;
mod timefmt;
mod ui;

use anyhow::Result;

fn main() -> Result<()> {
    let args = cli::Cli::parse_args();
    let settings = config::Settings::load(args)?;
    app::run(settings)
}
