use crate::columns::{parse_columns_csv, parse_pr_columns_csv};
use crate::config::{CursorSettings, Settings};
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
use std::io;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

pub fn run(settings: Settings) -> Result<()> {
    let runner = SystemRunner;
    let repo = gh::resolve_repo(&settings, &runner)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, repo, settings);

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
    pub config_draft: Option<ConfigDraft>,
    pub config_focus: usize,
    pub config_text_cursor: usize,
    pub quick_look_scroll: usize,
    pub settings: Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Runs,
    PullRequests,
}

#[derive(Debug, Clone)]
pub struct ConfigDraft {
    pub interval: Duration,
    pub limit: usize,
    pub columns: String,
    pub pr_columns: String,
    pub cursor: CursorSettings,
}

impl ConfigDraft {
    fn from_state(state: &State) -> Self {
        Self {
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
        config_draft: None,
        config_focus: 0,
        config_text_cursor: 0,
        quick_look_scroll: 0,
        settings,
    };

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

        if pending.is_none() && Instant::now() >= next_refresh {
            pending = Some(spawn_fetch(state.repo.clone(), state.settings.clone()));
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
                            state.error = None;
                            clamp_selection(&mut state);
                        }
                        Err(error) => state.error = Some(error.to_string()),
                    }
                    pending = None;
                    next_refresh = Instant::now() + state.settings.interval;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    state.loading = false;
                    state.last_check = Some(Local::now());
                    state.error = Some("refresh worker disconnected".to_string());
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
        KeyCode::Char('r') => {
            *next_refresh = Instant::now();
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
                    if let Some(run) = state.runs.get(state.selected_run) {
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
            Some(PanelKind::Keys) | None => false,
        },
    }
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
        state.error = None;
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
            state.error = Some(error.to_string());
            return;
        }
    };
    let pr_columns = match parse_pr_columns_csv(&draft.pr_columns) {
        Ok(columns) => columns,
        Err(error) => {
            state.error = Some(error.to_string());
            return;
        }
    };

    state.settings.interval = draft.interval;
    state.settings.limit = draft.limit;
    state.settings.columns = columns;
    state.settings.pr_columns = pr_columns;
    state.settings.cursor = draft.cursor;
    state.panel = None;
    state.config_draft = None;
    state.error = None;

    *next_refresh = Instant::now();
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

#[derive(Debug)]
struct DashboardData {
    runs: Vec<Run>,
    pull_requests: Vec<PullRequest>,
}

type FetchResult = anyhow::Result<DashboardData>;

fn spawn_fetch(repo: String, settings: Settings) -> Receiver<FetchResult> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = gh::fetch_runs(&repo, &settings, &SystemRunner).and_then(|runs| {
            gh::fetch_pull_requests(&repo, &settings, &SystemRunner).map(|pull_requests| {
                DashboardData {
                    runs,
                    pull_requests,
                }
            })
        });
        let _ = tx.send(result);
    });
    rx
}

fn clamp_selection(state: &mut State) {
    if state.runs.is_empty() {
        state.selected_run = 0;
        state.runs_scroll = 0;
    } else if state.selected_run >= state.runs.len() {
        state.selected_run = state.runs.len() - 1;
    }

    if state.pull_requests.is_empty() {
        state.selected_pr = 0;
        state.prs_scroll = 0;
    } else if state.selected_pr >= state.pull_requests.len() {
        state.selected_pr = state.pull_requests.len() - 1;
    }

    state.runs_scroll = state.runs_scroll.min(state.runs.len().saturating_sub(1));
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
        Screen::Runs => state.runs.len(),
        Screen::PullRequests => state.pull_requests.len(),
    }
}

fn draft_text_for_focus(draft: &ConfigDraft, focus: usize) -> &str {
    match ConfigRow::from_index(focus) {
        ConfigRow::Columns => &draft.columns,
        ConfigRow::PrColumns => &draft.pr_columns,
        _ => "",
    }
}
