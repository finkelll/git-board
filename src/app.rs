use crate::cache::{self, GlobalCacheSession, GlobalCacheStatus, GlobalCacheWorker};
use crate::columns::{parse_columns_csv, parse_pr_columns_csv};
use crate::config::{CursorSettings, DashboardLayout, Settings};
use crate::gh::{self, SystemRunner};
use crate::model::{PullRequest, Run};
use crate::panel::{ConfigRow, Panel, PanelKind};
use crate::ui;
use anyhow::Result;
use chrono::{DateTime, Local};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use serde::{Deserialize, Serialize};
use std::io;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

pub fn run(settings: Settings) -> Result<()> {
    let runner = SystemRunner;
    let repo = gh::resolve_repo(&settings, &runner)?;
    let global_cache = if settings.global {
        Some(GlobalCacheSession::register(&repo)?)
    } else {
        None
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, repo, settings, global_cache);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

#[derive(Debug)]
pub struct State {
    pub repo: String,
    pub screen: Screen,
    pub runs: Vec<Run>,
    pub pull_requests: Vec<PullRequest>,
    pub selected_run: usize,
    pub selected_pr: usize,
    pub runs_scroll: usize,
    pub prs_scroll: usize,
    pub cursor_visible: bool,
    pub last_check: Option<DateTime<Local>>,
    pub loading: bool,
    pub tick: u64,
    pub error: Option<String>,
    pub panel: Option<Panel>,
    pub auth_prompt_focus: usize,
    pub auth_prompt_dismissed: bool,
    pub auth_login_requested: bool,
    pub destroy_prompt_focus: usize,
    pub destroy_massive_focus: usize,
    pub config_draft: Option<ConfigDraft>,
    pub config_focus: usize,
    pub config_text_cursor: usize,
    pub quick_look_scroll: usize,
    pub keys_scroll: usize,
    pub run_sort: RunSort,
    pub settings: Settings,
    pub global_cache_status: Option<GlobalCacheStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Runs,
    PullRequests,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunSort {
    TimeDescSuccessesLast,
    TimeDesc,
}

impl RunSort {
    pub fn next(self) -> Self {
        match self {
            Self::TimeDescSuccessesLast => Self::TimeDesc,
            Self::TimeDesc => Self::TimeDescSuccessesLast,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::TimeDescSuccessesLast => "time desc, successes last",
            Self::TimeDesc => "time desc",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigDraft {
    pub layout: DashboardLayout,
    pub interval: Duration,
    pub limit: usize,
    pub columns: String,
    pub pr_columns: String,
    pub cursor: CursorSettings,
}

impl ConfigDraft {
    fn from_state(state: &State) -> Self {
        Self {
            layout: state.settings.layout,
            interval: state.settings.interval,
            limit: state.settings.limit,
            columns: state
                .settings
                .columns
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
            pr_columns: state
                .settings
                .pr_columns
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
            cursor: state.settings.cursor.clone(),
        }
    }
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    repo: String,
    settings: Settings,
    mut global_cache: Option<GlobalCacheSession>,
) -> Result<()> {
    let mut state = State {
        repo,
        screen: Screen::Runs,
        runs: Vec::new(),
        pull_requests: Vec::new(),
        selected_run: 0,
        selected_pr: 0,
        runs_scroll: 0,
        prs_scroll: 0,
        cursor_visible: false,
        last_check: None,
        loading: false,
        tick: 0,
        error: None,
        panel: None,
        auth_prompt_focus: 0,
        auth_prompt_dismissed: false,
        auth_login_requested: false,
        destroy_prompt_focus: 1,
        destroy_massive_focus: 1,
        config_draft: None,
        config_focus: 0,
        config_text_cursor: 0,
        quick_look_scroll: 0,
        keys_scroll: 0,
        run_sort: RunSort::TimeDescSuccessesLast,
        settings,
        global_cache_status: global_cache.as_ref().and_then(|cache| cache.status().ok()),
    };
    maybe_open_auth_prompt(&mut state);

    let mut next_refresh = Instant::now();
    let mut last_tick = Instant::now();
    let mut last_navigation_at: Option<Instant> = None;
    let mut pending: Option<Receiver<FetchResult>> = None;

    loop {
        if last_tick.elapsed() >= Duration::from_millis(120) {
            state.tick = state.tick.wrapping_add(1);
            last_tick = Instant::now();
        }

        if state.cursor_visible && state.settings.cursor.auto_hide {
            if let Some(last_navigation_at) = last_navigation_at {
                if last_navigation_at.elapsed() >= state.settings.cursor.hide_after {
                    state.cursor_visible = false;
                }
            }
        }

        if let Some(cache) = &mut global_cache {
            match cache.maintain() {
                Ok(status) => state.global_cache_status = Some(status),
                Err(error) => set_error(&mut state, error.to_string()),
            }
        }

        if pending.is_none() && Instant::now() >= next_refresh {
            pending = Some(spawn_fetch(
                state.repo.clone(),
                state.settings.clone(),
                global_cache.as_ref().map(GlobalCacheSession::worker),
            ));
            state.loading = true;
        }

        if let Some(rx) = &pending {
            match rx.try_recv() {
                Ok(result) => {
                    state.loading = false;
                    state.last_check = Some(Local::now());
                    match result {
                        Ok(runs) => {
                            state.runs = runs.runs;
                            state.pull_requests = runs.pull_requests;
                            clear_error(&mut state);
                            state.auth_prompt_dismissed = false;
                            clamp_selection(&mut state);
                        }
                        Err(error) => set_error(&mut state, error.to_string()),
                    }
                    pending = None;
                    next_refresh = Instant::now() + state.settings.interval;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    state.loading = false;
                    state.last_check = Some(Local::now());
                    set_error(&mut state, "refresh worker disconnected".to_string());
                    pending = None;
                    next_refresh = Instant::now() + state.settings.interval;
                }
            }
        }

        terminal.draw(|frame| ui::draw(frame, &mut state))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if handle_key(key, &mut state, &mut next_refresh, &mut last_navigation_at) {
                        break;
                    }
                }
            }
        }

        if state.auth_login_requested {
            run_gh_auth_login(terminal, &mut state, &mut next_refresh)?;
        }
    }

    Ok(())
}

fn handle_key(
    key: KeyEvent,
    state: &mut State,
    next_refresh: &mut Instant,
    last_navigation_at: &mut Option<Instant>,
) -> bool {
    if state.panel.is_some() {
        return handle_panel_key(key, state, next_refresh, last_navigation_at);
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => true,
        KeyCode::Char('k') => {
            state.keys_scroll = 0;
            state.panel = Some(Panel::keys());
            false
        }
        KeyCode::Char(' ') => {
            state.quick_look_scroll = 0;
            state.panel = Some(Panel::quick_look());
            false
        }
        KeyCode::Char('c') => {
            open_config_panel(state);
            false
        }
        KeyCode::Char('d') => {
            open_destroy_panel(state);
            false
        }
        KeyCode::Char('r') => {
            *next_refresh = Instant::now();
            false
        }
        KeyCode::Char('l') => {
            state.settings.layout = state.settings.layout.next();
            clamp_selection(state);
            false
        }
        KeyCode::Char('s') => {
            state.run_sort = state.run_sort.next();
            clamp_selection(state);
            false
        }
        KeyCode::Tab => {
            state.screen = match state.screen {
                Screen::Runs => Screen::PullRequests,
                Screen::PullRequests => Screen::Runs,
            };
            state.cursor_visible = true;
            *last_navigation_at = Some(Instant::now());
            false
        }
        KeyCode::Char('h') => {
            toggle_cursor_auto_hide(state, last_navigation_at);
            false
        }
        KeyCode::Up => {
            if state.cursor_visible {
                let selected = selected_mut(state);
                *selected = selected.saturating_sub(1);
            }
            state.cursor_visible = true;
            *last_navigation_at = Some(Instant::now());
            false
        }
        KeyCode::Down => {
            let len = selected_len(state);
            let cursor_visible = state.cursor_visible;
            let selected = selected_mut(state);
            if cursor_visible && *selected + 1 < len {
                *selected += 1;
            }
            state.cursor_visible = true;
            *last_navigation_at = Some(Instant::now());
            false
        }
        KeyCode::Enter => {
            match state.screen {
                Screen::Runs => {
                    if let Some(run) = selected_run(state) {
                        let repo = state.repo.clone();
                        let id = run.database_id;
                        thread::spawn(move || {
                            let _ = gh::open_run(&repo, id, &SystemRunner);
                        });
                    }
                }
                Screen::PullRequests => {
                    if let Some(pr) = state.pull_requests.get(state.selected_pr) {
                        let repo = state.repo.clone();
                        let number = pr.number;
                        thread::spawn(move || {
                            let _ = gh::open_pull_request(&repo, number, &SystemRunner);
                        });
                    }
                }
            }
            false
        }
        _ => false,
    }
}

fn handle_panel_key(
    key: KeyEvent,
    state: &mut State,
    next_refresh: &mut Instant,
    _last_navigation_at: &mut Option<Instant>,
) -> bool {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            if state
                .panel
                .is_some_and(|panel| panel.kind == PanelKind::AuthLogin)
            {
                state.auth_prompt_dismissed = true;
            }
            state.panel = None;
            state.config_draft = None;
            false
        }
        _ => match state.panel.map(|panel| panel.kind) {
            Some(PanelKind::Config) => {
                handle_config_key(key, state, next_refresh);
                false
            }
            Some(PanelKind::QuickLook) => {
                handle_quick_look_key(key, state);
                false
            }
            Some(PanelKind::Keys) => {
                handle_keys_key(key, state);
                false
            }
            Some(PanelKind::AuthLogin) => {
                handle_auth_login_key(key, state);
                false
            }
            Some(PanelKind::Destroy) => handle_destroy_key(key, state),
            Some(PanelKind::DestroyMassiveConfirm) => handle_destroy_massive_key(key, state),
            None => false,
        },
    }
}

fn handle_keys_key(key: KeyEvent, state: &mut State) {
    match key.code {
        KeyCode::Up => {
            state.keys_scroll = state.keys_scroll.saturating_sub(1);
        }
        KeyCode::Down => {
            state.keys_scroll = state.keys_scroll.saturating_add(1);
        }
        _ => {}
    }
}

fn handle_destroy_key(key: KeyEvent, state: &mut State) -> bool {
    match key.code {
        KeyCode::Left => {
            state.destroy_prompt_focus = state.destroy_prompt_focus.saturating_sub(1);
            false
        }
        KeyCode::Right | KeyCode::Tab => {
            state.destroy_prompt_focus = (state.destroy_prompt_focus + 1).min(2);
            false
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => destroy_repo_clients(state),
        KeyCode::Char('n') | KeyCode::Char('N') => {
            close_destroy_panel(state);
            false
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            open_destroy_massive_confirm_panel(state);
            false
        }
        KeyCode::Enter => match state.destroy_prompt_focus {
            0 => destroy_repo_clients(state),
            1 => {
                close_destroy_panel(state);
                false
            }
            _ => {
                open_destroy_massive_confirm_panel(state);
                false
            }
        },
        _ => false,
    }
}

fn handle_destroy_massive_key(key: KeyEvent, state: &mut State) -> bool {
    match key.code {
        KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
            state.destroy_massive_focus = usize::from(state.destroy_massive_focus == 0);
            false
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => destroy_all_clients(state),
        KeyCode::Char('n') | KeyCode::Char('N') => {
            close_destroy_panel(state);
            false
        }
        KeyCode::Enter => {
            if state.destroy_massive_focus == 0 {
                destroy_all_clients(state)
            } else {
                close_destroy_panel(state);
                false
            }
        }
        _ => false,
    }
}

fn open_destroy_panel(state: &mut State) {
    state.destroy_prompt_focus = 1;
    state.panel = Some(Panel::destroy());
}

fn open_destroy_massive_confirm_panel(state: &mut State) {
    state.destroy_massive_focus = 1;
    state.panel = Some(Panel::destroy_massive_confirm());
}

fn close_destroy_panel(state: &mut State) {
    state.panel = None;
}

fn destroy_repo_clients(state: &mut State) -> bool {
    match cache::repo_client_pids(&state.repo).and_then(kill_other_pids) {
        Ok(()) => true,
        Err(error) => {
            set_error(state, error.to_string());
            false
        }
    }
}

fn destroy_all_clients(state: &mut State) -> bool {
    match cache::all_client_pids().and_then(kill_other_pids) {
        Ok(()) => true,
        Err(error) => {
            set_error(state, error.to_string());
            false
        }
    }
}

fn kill_other_pids(pids: Vec<u32>) -> Result<()> {
    let current_pid = std::process::id();
    let targets = pids
        .into_iter()
        .filter(|pid| *pid != current_pid)
        .map(|pid| pid.to_string())
        .collect::<Vec<_>>();

    if !targets.is_empty() {
        let status = Command::new("kill").args(&targets).status()?;
        if !status.success() {
            anyhow::bail!("kill exited with status {status}");
        }
    }

    Ok(())
}

fn handle_auth_login_key(key: KeyEvent, state: &mut State) {
    match key.code {
        KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
            state.auth_prompt_focus = usize::from(state.auth_prompt_focus == 0);
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            request_auth_login(state);
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            dismiss_auth_prompt(state);
        }
        KeyCode::Enter => {
            if state.auth_prompt_focus == 0 {
                request_auth_login(state);
            } else {
                dismiss_auth_prompt(state);
            }
        }
        _ => {}
    }
}

fn request_auth_login(state: &mut State) {
    state.panel = None;
    state.auth_prompt_dismissed = true;
    state.auth_login_requested = true;
}

fn dismiss_auth_prompt(state: &mut State) {
    state.panel = None;
    state.auth_prompt_dismissed = true;
}

fn handle_quick_look_key(key: KeyEvent, state: &mut State) {
    match key.code {
        KeyCode::Char(' ') => {
            state.panel = None;
        }
        KeyCode::Up => {
            state.quick_look_scroll = state.quick_look_scroll.saturating_sub(1);
        }
        KeyCode::Down => {
            state.quick_look_scroll = state.quick_look_scroll.saturating_add(1);
        }
        _ => {}
    }
}

fn handle_config_key(key: KeyEvent, state: &mut State, next_refresh: &mut Instant) {
    match key.code {
        KeyCode::Up => {
            state.config_focus = state.config_focus.saturating_sub(1);
            sync_text_cursor_to_focus(state);
        }
        KeyCode::Down => {
            if state.config_focus + 1 < ConfigRow::len() {
                state.config_focus += 1;
            }
            sync_text_cursor_to_focus(state);
        }
        KeyCode::Left if ConfigRow::from_index(state.config_focus).is_text_field() => {
            state.config_text_cursor = state.config_text_cursor.saturating_sub(1);
        }
        KeyCode::Right if ConfigRow::from_index(state.config_focus).is_text_field() => {
            move_text_cursor_right(state);
        }
        KeyCode::Left | KeyCode::Char('-') => edit_config_row(state, -1),
        KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => edit_config_row(state, 1),
        KeyCode::Enter => {
            apply_config_draft(state, next_refresh);
        }
        KeyCode::Backspace => edit_config_text(state, None),
        KeyCode::Char(ch) if ConfigRow::from_index(state.config_focus).is_text_field() => {
            edit_config_text(state, Some(ch))
        }
        _ => {}
    }
}

fn edit_config_row(state: &mut State, direction: i32) {
    let focused = ConfigRow::from_index(state.config_focus);
    let Some(draft) = state.config_draft.as_mut() else {
        return;
    };

    match ConfigRow::from_index(state.config_focus) {
        ConfigRow::Interval => {
            draft.interval = adjust_seconds(draft.interval, direction, 5, 3600);
        }
        ConfigRow::Layout => {
            draft.layout = draft.layout.next();
        }
        ConfigRow::Limit => {
            draft.limit = adjust_usize(draft.limit, direction, 1, 200);
        }
        ConfigRow::CursorAutoHide => draft.cursor.auto_hide = !draft.cursor.auto_hide,
        ConfigRow::CursorHideAfter => {
            draft.cursor.hide_after = adjust_seconds(draft.cursor.hide_after, direction, 1, 300);
        }
        ConfigRow::Columns => {}
        ConfigRow::PrColumns => {}
    }

    if focused.is_text_field() {
        clear_error(state);
    }
}

fn toggle_cursor_auto_hide(state: &mut State, last_navigation_at: &mut Option<Instant>) {
    state.settings.cursor.auto_hide = !state.settings.cursor.auto_hide;
    if state.settings.cursor.auto_hide && state.cursor_visible {
        *last_navigation_at = Some(Instant::now());
    }
}

fn edit_config_text(state: &mut State, ch: Option<char>) {
    let focused = ConfigRow::from_index(state.config_focus);
    let Some(draft) = state.config_draft.as_mut() else {
        return;
    };

    let value = match focused {
        ConfigRow::Columns => &mut draft.columns,
        ConfigRow::PrColumns => &mut draft.pr_columns,
        _ => return,
    };

    match ch {
        Some(ch) if !ch.is_control() => {
            insert_char(value, state.config_text_cursor, ch);
            state.config_text_cursor += 1;
        }
        None => {
            if state.config_text_cursor > 0 {
                state.config_text_cursor -= 1;
                remove_char(value, state.config_text_cursor);
            }
        }
        _ => {}
    }
}

fn apply_config_draft(state: &mut State, next_refresh: &mut Instant) {
    let Some(draft) = state.config_draft.clone() else {
        return;
    };

    let columns = match parse_columns_csv(&draft.columns) {
        Ok(columns) => columns,
        Err(error) => {
            set_error(state, error.to_string());
            return;
        }
    };
    let pr_columns = match parse_pr_columns_csv(&draft.pr_columns) {
        Ok(columns) => columns,
        Err(error) => {
            set_error(state, error.to_string());
            return;
        }
    };

    state.settings.interval = draft.interval;
    state.settings.layout = draft.layout;
    state.settings.limit = draft.limit;
    state.settings.columns = columns;
    state.settings.pr_columns = pr_columns;
    state.settings.cursor = draft.cursor;
    state.panel = None;
    state.config_draft = None;
    clear_error(state);

    *next_refresh = Instant::now();
}

fn set_error(state: &mut State, error: String) {
    maybe_open_auth_prompt(state);
    state.error = Some(error);
}

fn clear_error(state: &mut State) {
    state.error = None;
}

fn maybe_open_auth_prompt(state: &mut State) {
    if state.auth_prompt_dismissed || state.panel.is_some() || gh_is_authenticated() {
        return;
    }

    state.auth_prompt_focus = 0;
    state.panel = Some(Panel::auth_login());
}

fn gh_is_authenticated() -> bool {
    Command::new("gh")
        .args(["auth", "status", "-h", "github.com"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn run_gh_auth_login(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut State,
    next_refresh: &mut Instant,
) -> Result<()> {
    state.auth_login_requested = false;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let status = Command::new("gh")
        .args(["auth", "login", "-h", "github.com"])
        .status();

    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    terminal.clear()?;

    match status {
        Ok(status) if status.success() => {
            clear_error(state);
            state.auth_prompt_dismissed = false;
            *next_refresh = Instant::now();
        }
        Ok(status) => set_error(state, format!("gh auth login exited with status {status}")),
        Err(error) => set_error(state, format!("failed to run gh auth login: {error}")),
    }

    Ok(())
}

fn open_config_panel(state: &mut State) {
    state.config_focus = 0;
    state.config_draft = Some(ConfigDraft::from_state(state));
    state.config_text_cursor = state.config_draft.as_ref().map_or(0, |draft| {
        draft_text_for_focus(draft, state.config_focus)
            .chars()
            .count()
    });
    state.panel = Some(Panel::config());
}

fn sync_text_cursor_to_focus(state: &mut State) {
    if ConfigRow::from_index(state.config_focus).is_text_field() {
        state.config_text_cursor = state.config_draft.as_ref().map_or(0, |draft| {
            draft_text_for_focus(draft, state.config_focus)
                .chars()
                .count()
        });
    }
}

fn move_text_cursor_right(state: &mut State) {
    let max = state.config_draft.as_ref().map_or(0, |draft| {
        draft_text_for_focus(draft, state.config_focus)
            .chars()
            .count()
    });
    if state.config_text_cursor < max {
        state.config_text_cursor += 1;
    }
}

fn insert_char(value: &mut String, char_index: usize, ch: char) {
    let byte_index = byte_index_for_char(value, char_index);
    value.insert(byte_index, ch);
}

fn remove_char(value: &mut String, char_index: usize) {
    let start = byte_index_for_char(value, char_index);
    let end = byte_index_for_char(value, char_index + 1);
    if start < end {
        value.replace_range(start..end, "");
    }
}

fn byte_index_for_char(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .map(|(index, _)| index)
        .nth(char_index)
        .unwrap_or(value.len())
}

fn adjust_seconds(value: Duration, direction: i32, min: u64, max: u64) -> Duration {
    let seconds = value.as_secs();
    let next = if direction < 0 {
        seconds.saturating_sub(1)
    } else {
        seconds.saturating_add(1)
    };
    Duration::from_secs(next.clamp(min, max))
}

fn adjust_usize(value: usize, direction: i32, min: usize, max: usize) -> usize {
    let next = if direction < 0 {
        value.saturating_sub(1)
    } else {
        value.saturating_add(1)
    };
    next.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn run(
        database_id: u64,
        workflow_name: &str,
        head_branch: &str,
        conclusion: Option<&str>,
        status: &str,
        created_minute: u32,
    ) -> Run {
        let created_at = Utc
            .with_ymd_and_hms(2026, 6, 2, 10, created_minute, 0)
            .unwrap();
        Run {
            conclusion: conclusion.map(ToString::to_string),
            created_at,
            database_id,
            display_title: format!("run {database_id}"),
            event: "push".to_string(),
            head_branch: head_branch.to_string(),
            name: workflow_name.to_string(),
            pr_number: None,
            started_at: Some(created_at),
            status: status.to_string(),
            updated_at: created_at,
            workflow_name: workflow_name.to_string(),
        }
    }

    #[test]
    fn failure_is_unresolved_without_following_start_or_success() {
        let runs = vec![run(1, "verify", "main", Some("failure"), "completed", 10)];

        assert!(run_visible_in_layout(
            &runs[0],
            &runs,
            DashboardLayout::InProgress
        ));
    }

    #[test]
    fn newer_start_resolves_same_workflow_branch_failure() {
        let runs = vec![
            run(2, "verify", "main", None, "in_progress", 12),
            run(1, "verify", "main", Some("failure"), "completed", 10),
        ];

        assert!(!unresolved_failure(&runs[1], &runs));
    }

    #[test]
    fn newer_success_resolves_same_workflow_branch_failure() {
        let runs = vec![
            run(2, "verify", "main", Some("success"), "completed", 12),
            run(1, "verify", "main", Some("failure"), "completed", 10),
        ];

        assert!(!unresolved_failure(&runs[1], &runs));
    }

    #[test]
    fn newer_failed_start_resolves_older_same_workflow_branch_failure() {
        let runs = vec![
            run(2, "verify", "main", Some("failure"), "completed", 12),
            run(1, "verify", "main", Some("failure"), "completed", 10),
        ];

        assert!(!unresolved_failure(&runs[1], &runs));
        assert!(unresolved_failure(&runs[0], &runs));
    }

    #[test]
    fn in_progress_layout_shows_latest_workflow_success() {
        let runs = vec![run(1, "verify", "main", Some("success"), "completed", 10)];

        assert!(run_visible_in_layout(
            &runs[0],
            &runs,
            DashboardLayout::InProgress
        ));
    }

    #[test]
    fn newer_workflow_entry_hides_older_success() {
        let runs = vec![
            run(2, "verify", "main", None, "in_progress", 12),
            run(1, "verify", "main", Some("success"), "completed", 10),
        ];

        assert!(!run_visible_in_layout(
            &runs[1],
            &runs,
            DashboardLayout::InProgress
        ));
    }

    #[test]
    fn newer_workflow_entry_on_other_branch_hides_older_success() {
        let runs = vec![
            run(2, "verify", "feature", Some("failure"), "completed", 12),
            run(1, "verify", "main", Some("success"), "completed", 10),
        ];

        assert!(!run_visible_in_layout(
            &runs[1],
            &runs,
            DashboardLayout::InProgress
        ));
    }

    #[test]
    fn in_progress_layout_sorts_successes_last() {
        let runs = vec![
            run(
                1,
                "latest-success",
                "main",
                Some("success"),
                "completed",
                14,
            ),
            run(2, "running", "main", None, "in_progress", 12),
            run(3, "failed", "main", Some("failure"), "completed", 10),
        ];

        assert_eq!(
            sorted_visible_run_indices(
                &runs,
                DashboardLayout::InProgress,
                RunSort::TimeDescSuccessesLast
            ),
            vec![1, 2, 0]
        );
    }

    #[test]
    fn time_desc_sort_does_not_push_successes_last() {
        let runs = vec![
            run(
                1,
                "latest-success",
                "main",
                Some("success"),
                "completed",
                14,
            ),
            run(2, "running", "main", None, "in_progress", 12),
            run(3, "failed", "main", Some("failure"), "completed", 10),
        ];

        assert_eq!(
            sorted_visible_run_indices(&runs, DashboardLayout::InProgress, RunSort::TimeDesc),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn all_layout_sorts_by_time_desc() {
        let runs = vec![
            run(1, "old", "main", Some("success"), "completed", 10),
            run(2, "new", "main", Some("failure"), "completed", 12),
        ];

        assert_eq!(
            sorted_visible_run_indices(&runs, DashboardLayout::All, RunSort::TimeDescSuccessesLast),
            vec![1, 0]
        );
    }

    #[test]
    fn different_branch_success_does_not_resolve_failure() {
        let runs = vec![
            run(2, "verify", "feature", Some("success"), "completed", 12),
            run(1, "verify", "main", Some("failure"), "completed", 10),
        ];

        assert!(unresolved_failure(&runs[1], &runs));
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct DashboardData {
    runs: Vec<Run>,
    pull_requests: Vec<PullRequest>,
}

type FetchResult = anyhow::Result<DashboardData>;

fn spawn_fetch(
    repo: String,
    settings: Settings,
    global_cache: Option<GlobalCacheWorker>,
) -> Receiver<FetchResult> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = fetch_dashboard(&repo, &settings, global_cache);
        let _ = tx.send(result);
    });
    rx
}

fn fetch_dashboard(
    repo: &str,
    settings: &Settings,
    global_cache: Option<GlobalCacheWorker>,
) -> FetchResult {
    let Some(cache) = global_cache else {
        return fetch_dashboard_from_github(repo, settings);
    };

    if !cache.is_owner() {
        return cache.read();
    }

    let data = fetch_dashboard_from_github(repo, settings)?;
    cache.write(&data)?;
    Ok(data)
}

fn fetch_dashboard_from_github(repo: &str, settings: &Settings) -> FetchResult {
    gh::fetch_runs(repo, settings, &SystemRunner).and_then(|runs| {
        gh::fetch_pull_requests(repo, settings, &SystemRunner).map(|pull_requests| DashboardData {
            runs,
            pull_requests,
        })
    })
}

fn clamp_selection(state: &mut State) {
    let visible_runs = visible_run_indices(state);
    if visible_runs.is_empty() {
        state.selected_run = 0;
        state.runs_scroll = 0;
    } else if state.selected_run >= visible_runs.len() {
        state.selected_run = visible_runs.len() - 1;
    }

    if state.pull_requests.is_empty() {
        state.selected_pr = 0;
        state.prs_scroll = 0;
    } else if state.selected_pr >= state.pull_requests.len() {
        state.selected_pr = state.pull_requests.len() - 1;
    }

    state.runs_scroll = state.runs_scroll.min(visible_runs.len().saturating_sub(1));
    state.prs_scroll = state
        .prs_scroll
        .min(state.pull_requests.len().saturating_sub(1));
}

fn selected_mut(state: &mut State) -> &mut usize {
    match state.screen {
        Screen::Runs => &mut state.selected_run,
        Screen::PullRequests => &mut state.selected_pr,
    }
}

fn selected_len(state: &State) -> usize {
    match state.screen {
        Screen::Runs => visible_run_indices(state).len(),
        Screen::PullRequests => state.pull_requests.len(),
    }
}

pub fn selected_run(state: &State) -> Option<&Run> {
    visible_run_indices(state)
        .get(state.selected_run)
        .and_then(|index| state.runs.get(*index))
}

pub fn visible_run_indices(state: &State) -> Vec<usize> {
    sorted_visible_run_indices(&state.runs, state.settings.layout, state.run_sort)
}

fn sorted_visible_run_indices(runs: &[Run], layout: DashboardLayout, sort: RunSort) -> Vec<usize> {
    let mut indices = runs
        .iter()
        .enumerate()
        .filter_map(|(index, run)| run_visible_in_layout(run, runs, layout).then_some(index))
        .collect::<Vec<_>>();

    indices.sort_by(|left, right| compare_runs_for_sort(&runs[*left], &runs[*right], layout, sort));
    indices
}

fn compare_runs_for_sort(
    left: &Run,
    right: &Run,
    layout: DashboardLayout,
    sort: RunSort,
) -> std::cmp::Ordering {
    let left_success = run_succeeded(left);
    let right_success = run_succeeded(right);

    if layout == DashboardLayout::InProgress
        && sort == RunSort::TimeDescSuccessesLast
        && left_success != right_success
    {
        return left_success.cmp(&right_success);
    }

    right
        .created_at
        .cmp(&left.created_at)
        .then_with(|| right.database_id.cmp(&left.database_id))
}

fn run_visible_in_layout(run: &Run, runs: &[Run], layout: DashboardLayout) -> bool {
    match layout {
        DashboardLayout::All => true,
        DashboardLayout::InProgress => {
            run_in_progress(run)
                || unresolved_failure(run, runs)
                || latest_workflow_success(run, runs)
        }
    }
}

fn run_in_progress(run: &Run) -> bool {
    matches!(
        run.status.as_str(),
        "queued" | "in_progress" | "requested" | "waiting" | "pending"
    )
}

fn unresolved_failure(run: &Run, runs: &[Run]) -> bool {
    if !run_failed(run) {
        return false;
    }

    !runs.iter().any(|candidate| {
        same_workflow_branch(run, candidate)
            && candidate.created_at > run.created_at
            && (run_started(candidate) || run_succeeded(candidate))
    })
}

fn run_failed(run: &Run) -> bool {
    matches!(
        run.status_label(),
        "failure" | "cancelled" | "timed_out" | "startup_failure" | "action_required"
    )
}

fn run_succeeded(run: &Run) -> bool {
    run.status_label() == "success"
}

fn latest_workflow_success(run: &Run, runs: &[Run]) -> bool {
    run_succeeded(run)
        && !runs
            .iter()
            .any(|candidate| same_workflow(run, candidate) && candidate.created_at > run.created_at)
}

fn run_started(run: &Run) -> bool {
    run.started_at.is_some()
}

fn same_workflow_branch(left: &Run, right: &Run) -> bool {
    left.workflow_label() == right.workflow_label() && left.head_branch == right.head_branch
}

fn same_workflow(left: &Run, right: &Run) -> bool {
    left.workflow_label() == right.workflow_label()
}

fn draft_text_for_focus(draft: &ConfigDraft, focus: usize) -> &str {
    match ConfigRow::from_index(focus) {
        ConfigRow::Columns => &draft.columns,
        ConfigRow::PrColumns => &draft.pr_columns,
        _ => "",
    }
}
