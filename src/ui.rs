use crate::app::{selected_run, visible_run_indices, RunSort, Screen, State};
use crate::columns::{Column, PrColumn};
use crate::model::{PullRequest, Run};
use crate::panel::{ConfigRow, Panel, PanelKind};
use crate::timefmt;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;
use std::time::Duration;

#[derive(Debug)]
struct TitleStatus {
    text: String,
    stale_age: Option<Duration>,
}

pub fn draw(frame: &mut Frame<'_>, state: &mut State) {
    let area = frame.area();
    let title = match state.screen {
        Screen::Runs => "=== GitHub Actions ===",
        Screen::PullRequests => "=== Open Pull Requests ===",
    };
    let title = title_with_global_status(title, state);
    let last_check = state
        .last_check
        .map(|time| time.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
        .unwrap_or_else(|| "not checked yet".to_string());
    let refresh = refresh_indicator(state);
    let inline_header = title.text.chars().count()
        + stale_title_text(&title).chars().count()
        + last_check.chars().count()
        + refresh.chars().count()
        + 4
        <= usize::from(area.width);

    let has_error = state.error.is_some();
    let error_height = if has_error { 1 } else { 0 };
    let (table_area, error_area, footer_area) = if inline_header {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(5),
                Constraint::Length(error_height),
                Constraint::Length(1),
            ])
            .split(area);
        frame.render_widget(header_line(&title, &last_check, refresh, true), chunks[0]);
        (chunks[1], chunks[2], chunks[3])
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(5),
                Constraint::Length(error_height),
                Constraint::Length(1),
            ])
            .split(area);
        frame.render_widget(header_line(&title, &last_check, refresh, false), chunks[0]);
        frame.render_widget(check_line(&last_check, refresh), chunks[1]);
        (chunks[2], chunks[3], chunks[4])
    };

    match state.screen {
        Screen::Runs => render_runs_table(frame, state, table_area),
        Screen::PullRequests => render_pull_requests_table(frame, state, table_area),
    }

    let footer = footer(state);
    if has_error {
        frame.render_widget(error_line(state), error_area);
    }
    frame.render_widget(footer, footer_area);

    if let Some(panel) = state.panel {
        render_panel(frame, panel, state);
    }
}

fn title_with_global_status(title: &str, state: &State) -> TitleStatus {
    if !state.settings.global {
        return TitleStatus {
            text: format!("{title} 🐺"),
            stale_age: None,
        };
    }

    let Some(status) = state.global_cache_status else {
        return TitleStatus {
            text: title.to_string(),
            stale_age: None,
        };
    };

    let clients = if status.client_count == 1 {
        "1 client".to_string()
    } else {
        format!("{} clients", status.client_count)
    };
    let stale_age = status
        .cache_age
        .filter(|age| *age > state.settings.interval.mul_f64(1.2));

    let text = if status.is_owner {
        format!("{title} 🐓 [{clients}]")
    } else {
        format!("{title} 🐥 [{clients}]")
    };

    TitleStatus { text, stale_age }
}

fn header_line(
    title: &TitleStatus,
    last_check: &str,
    refresh: &str,
    include_check: bool,
) -> Line<'static> {
    let mut spans = vec![Span::styled(
        title.text.clone(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )];
    spans.extend(stale_title_spans(title));

    if include_check {
        spans.push(Span::raw("  "));
        spans.extend(check_spans(last_check, refresh));
    }

    Line::from(spans)
}

fn stale_title_spans(title: &TitleStatus) -> Vec<Span<'static>> {
    title.stale_age.map_or_else(Vec::new, |age| {
        vec![
            Span::raw(" "),
            Span::styled(
                format!("STALE {}", humantime::format_duration(age)),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]
    })
}

fn stale_title_text(title: &TitleStatus) -> String {
    title.stale_age.map_or_else(String::new, |age| {
        format!(" STALE {}", humantime::format_duration(age))
    })
}

fn check_line(last_check: &str, refresh: &str) -> Line<'static> {
    Line::from(check_spans(last_check, refresh))
}

fn check_spans(last_check: &str, refresh: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            last_check.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(refresh.to_string(), Style::default().fg(Color::Yellow)),
    ]
}

fn render_runs_table(frame: &mut Frame<'_>, state: &mut State, area: Rect) {
    let widths = state
        .settings
        .columns
        .iter()
        .map(|column| Constraint::Length(column.width()))
        .collect::<Vec<_>>();
    let visible = visible_run_indices(state);
    let header = table_header(state.settings.columns.iter().map(|column| column.header()));
    let rows = visible.iter().enumerate().filter_map(|(index, run_index)| {
        let run = state.runs.get(*run_index)?;
        let selected = state.cursor_visible && index == state.selected_run;
        Some(
            Row::new(
                state
                    .settings
                    .columns
                    .iter()
                    .map(|column| run_cell_for(*column, run))
                    .collect::<Vec<_>>(),
            )
            .style(row_style(selected)),
        )
    });
    render_table(
        frame,
        rows,
        widths,
        header,
        state.cursor_visible.then_some(state.selected_run),
        &mut state.runs_scroll,
        area,
    );
}

fn render_pull_requests_table(frame: &mut Frame<'_>, state: &mut State, area: Rect) {
    let widths = state
        .settings
        .pr_columns
        .iter()
        .map(|column| Constraint::Length(column.width()))
        .collect::<Vec<_>>();
    let header = table_header(
        state
            .settings
            .pr_columns
            .iter()
            .map(|column| column.header()),
    );
    let rows = state
        .pull_requests
        .iter()
        .enumerate()
        .map(|(index, pull_request)| {
            let selected = state.cursor_visible && index == state.selected_pr;
            Row::new(
                state
                    .settings
                    .pr_columns
                    .iter()
                    .map(|column| pull_request_cell_for(*column, pull_request))
                    .collect::<Vec<_>>(),
            )
            .style(row_style(selected))
        });
    render_table(
        frame,
        rows,
        widths,
        header,
        state.cursor_visible.then_some(state.selected_pr),
        &mut state.prs_scroll,
        area,
    );
}

fn table_header<'a>(labels: impl Iterator<Item = &'a str>) -> Row<'static> {
    Row::new(
        labels
            .map(|label| {
                Cell::from(label.to_string()).style(
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::UNDERLINED),
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn render_table<'a>(
    frame: &mut Frame<'_>,
    rows: impl Iterator<Item = Row<'a>>,
    widths: Vec<Constraint>,
    header: Row<'static>,
    selected: Option<usize>,
    scroll: &mut usize,
    area: Rect,
) {
    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(2)
        .block(Block::default().borders(Borders::NONE));
    let mut table_state = TableState::new()
        .with_offset(*scroll)
        .with_selected(selected);
    frame.render_stateful_widget(table, area, &mut table_state);
    *scroll = table_state.offset();
}

fn row_style(selected: bool) -> Style {
    if selected {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default()
    }
}

fn run_cell_for(column: Column, run: &Run) -> Cell<'static> {
    match column {
        Column::Status => status_cell(run),
        Column::Title => Cell::from(truncate_owned(&run.display_title, 48)).style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Column::Workflow => Cell::from(truncate_owned(run.workflow_label(), 18))
            .style(Style::default().fg(Color::White)),
        Column::Branch => Cell::from(truncate_owned(&run.head_branch, 32))
            .style(Style::default().fg(Color::White)),
        Column::Event => {
            Cell::from(truncate_owned(&run.event, 16)).style(Style::default().fg(Color::White))
        }
        Column::PullRequest => Cell::from(
            run.pr_number
                .map(|number| format!("#{number}"))
                .unwrap_or_default(),
        )
        .style(Style::default().fg(Color::Cyan)),
        Column::Id => {
            Cell::from(run.database_id.to_string()).style(Style::default().fg(Color::Cyan))
        }
        Column::Elapsed => Cell::from(timefmt::elapsed(
            run.started_at,
            run.updated_at,
            &run.status,
        ))
        .style(Style::default().fg(Color::White)),
        Column::Age => {
            Cell::from(timefmt::age(run.created_at)).style(Style::default().fg(Color::DarkGray))
        }
    }
}

fn pull_request_cell_for(column: PrColumn, pull_request: &PullRequest) -> Cell<'static> {
    match column {
        PrColumn::Status => pr_status_cell(pull_request),
        PrColumn::Title => Cell::from(truncate_owned(&pull_request.title, 48)).style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        PrColumn::Author => Cell::from(truncate_owned(pull_request.author_login(), 18))
            .style(Style::default().fg(Color::White)),
        PrColumn::Branch => Cell::from(truncate_owned(&pull_request.head_ref_name, 32))
            .style(Style::default().fg(Color::White)),
        PrColumn::Base => Cell::from(truncate_owned(&pull_request.base_ref_name, 18))
            .style(Style::default().fg(Color::White)),
        PrColumn::Number => {
            Cell::from(format!("#{}", pull_request.number)).style(Style::default().fg(Color::Cyan))
        }
        PrColumn::Age => Cell::from(timefmt::age(pull_request.created_at))
            .style(Style::default().fg(Color::DarkGray)),
        PrColumn::Updated => Cell::from(timefmt::age(pull_request.updated_at))
            .style(Style::default().fg(Color::DarkGray)),
    }
}

fn status_cell(run: &Run) -> Cell<'static> {
    let label = run.status_label();
    let (symbol, color) = match label {
        "success" => ("✓", Color::Green),
        "failure" | "cancelled" | "timed_out" | "startup_failure" | "action_required" => {
            ("✗", Color::Red)
        }
        "queued" | "in_progress" | "requested" | "waiting" | "pending" => ("*", Color::Yellow),
        "skipped" | "neutral" | "stale" => ("-", Color::DarkGray),
        _ if run.status != "completed" => ("*", Color::Yellow),
        _ => ("?", Color::Magenta),
    };
    Cell::from(symbol).style(Style::default().fg(color).add_modifier(Modifier::BOLD))
}

fn pr_status_cell(pull_request: &PullRequest) -> Cell<'static> {
    let (symbol, color) = match pull_request.status_label() {
        "draft" => ("D", Color::Yellow),
        _ => ("O", Color::Green),
    };
    Cell::from(symbol).style(Style::default().fg(color).add_modifier(Modifier::BOLD))
}

fn footer(state: &State) -> Line<'static> {
    let mut spans = vec![
        Span::styled("repo ", Style::default().fg(Color::DarkGray)),
        Span::styled(state.repo.clone(), Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled("interval ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            humantime::format_duration(state.settings.interval).to_string(),
            Style::default().fg(Color::White),
        ),
        Span::raw("  "),
        Span::styled("layout ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            state.settings.layout.to_string(),
            Style::default().fg(Color::White),
        ),
        Span::raw("  "),
    ];

    spans.push(Span::styled(
        match state.screen {
            Screen::Runs => {
                "⇥ PRs  l layout  r refresh  c config  k keys  ␣ quick look  ↑/↓ select  ↵ open run  q/ESC quit"
            }
            Screen::PullRequests => {
                "⇥ Actions  l layout  r refresh  c config  k keys  ␣ quick look  ↑/↓ select  ↵ open PR  q/ESC quit"
            }
        },
        Style::default().fg(Color::DarkGray),
    ));

    Line::from(spans)
}

fn error_line(state: &State) -> Line<'static> {
    let error = state.error.as_deref().unwrap_or_default();
    Line::from(vec![
        Span::styled("error ", Style::default().fg(Color::DarkGray)),
        Span::styled(truncate_owned(error, 140), Style::default().fg(Color::Red)),
    ])
}

fn refresh_indicator(state: &State) -> &'static str {
    if !state.loading {
        return " ";
    }

    match state.tick % 4 {
        0 => "|",
        1 => "/",
        2 => "-",
        _ => "\\",
    }
}

fn render_panel(frame: &mut Frame<'_>, panel: Panel, state: &mut State) {
    let area = centered_rect(72, 58, frame.area());
    frame.render_widget(Clear, area);

    let lines = panel_lines(panel, state);
    let footer = panel_footer(panel);
    let block = Block::default()
        .title(panel.title())
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block.style(Style::default().bg(Color::Black)), area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let visible_lines = usize::from(chunks[0].height);
    let max_scroll = lines.len().saturating_sub(visible_lines);
    let scroll = match panel.kind {
        PanelKind::Config => config_panel_scroll(state, visible_lines, max_scroll),
        PanelKind::QuickLook => {
            state.quick_look_scroll = state.quick_look_scroll.min(max_scroll);
            state.quick_look_scroll
        }
        PanelKind::Keys => {
            state.keys_scroll = state.keys_scroll.min(max_scroll);
            state.keys_scroll
        }
        PanelKind::AuthLogin | PanelKind::Destroy | PanelKind::DestroyMassiveConfirm => 0,
    };
    let body = Paragraph::new(lines)
        .style(Style::default().bg(Color::Black))
        .scroll((scroll as u16, 0))
        .wrap(Wrap { trim: true });
    frame.render_widget(body, chunks[0]);
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().bg(Color::Black)),
        chunks[1],
    );
}

fn config_panel_scroll(state: &State, visible_lines: usize, max_scroll: usize) -> usize {
    if visible_lines == 0 {
        return 0;
    }

    let focus = state.config_focus;
    let scroll = focus.saturating_add(1).saturating_sub(visible_lines);
    scroll.min(max_scroll)
}

fn panel_lines(panel: Panel, state: &State) -> Vec<Line<'static>> {
    if panel.kind == PanelKind::Keys {
        return keys_lines(state);
    }

    let mut lines = panel
        .lines()
        .iter()
        .map(|(key, command)| command_line(key, command))
        .collect::<Vec<_>>();

    if lines.is_empty() {
        lines = match panel.kind {
            PanelKind::Config => config_lines(state),
            PanelKind::QuickLook => quick_look_lines(state),
            PanelKind::AuthLogin => auth_login_lines(state),
            PanelKind::Destroy => destroy_lines(state),
            PanelKind::DestroyMassiveConfirm => destroy_massive_confirm_lines(state),
            PanelKind::Keys => keys_lines(state),
        };
    }

    lines
}

fn keys_lines(state: &State) -> Vec<Line<'static>> {
    let mut lines = Panel::keys()
        .lines()
        .iter()
        .map(|(key, command)| command_line(key, command))
        .collect::<Vec<_>>();

    lines.push(Line::from(""));
    lines.push(value_line("sort", sort_description(state.run_sort)));
    lines
}

fn sort_description(sort: RunSort) -> &'static str {
    sort.label()
}

fn auth_login_lines(state: &State) -> Vec<Line<'static>> {
    vec![
        Line::from("GitHub authentication failed."),
        Line::from("Run gh auth login for this terminal session?"),
        Line::from(""),
        Line::from(vec![
            auth_choice_span("YES", state.auth_prompt_focus == 0),
            Span::raw("  "),
            auth_choice_span("NO", state.auth_prompt_focus == 1),
        ]),
    ]
}

fn destroy_lines(state: &State) -> Vec<Line<'static>> {
    vec![
        attention_line(),
        Line::from("Close git-board sessions?"),
        Line::from(""),
        Line::from(vec![
            prompt_choice_span("YES", state.destroy_prompt_focus == 0),
            Span::raw("  "),
            prompt_choice_span("NO", state.destroy_prompt_focus == 1),
            Span::raw("  "),
            prompt_choice_span("MASSIVE", state.destroy_prompt_focus == 2),
        ]),
        Line::from(""),
        Line::from(destroy_option_description(state.destroy_prompt_focus)),
    ]
}

fn destroy_massive_confirm_lines(state: &State) -> Vec<Line<'static>> {
    vec![
        attention_line(),
        Line::from("Destroy all git-board sessions?"),
        Line::from(""),
        Line::from(vec![
            prompt_choice_span("YES", state.destroy_massive_focus == 0),
            Span::raw("  "),
            prompt_choice_span("NO", state.destroy_massive_focus == 1),
        ]),
        Line::from(""),
        Line::from(destroy_massive_description(state.destroy_massive_focus)),
    ]
}

fn attention_line() -> Line<'static> {
    Line::from(Span::styled(
        "ATTENTION",
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    ))
}

fn destroy_option_description(focus: usize) -> &'static str {
    match focus {
        0 => "Close every git-board client for this repository, including this one.",
        1 => "Do nothing and close this panel.",
        _ => "Open an additional confirmation before closing every git-board client.",
    }
}

fn destroy_massive_description(focus: usize) -> &'static str {
    match focus {
        0 => "Close every registered git-board client across all repositories.",
        _ => "Do nothing and close this panel.",
    }
}

fn auth_choice_span(label: &str, focused: bool) -> Span<'static> {
    prompt_choice_span(label, focused)
}

fn prompt_choice_span(label: &str, focused: bool) -> Span<'static> {
    let style = if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    };

    Span::styled(format!(" {label} "), style)
}

fn quick_look_lines(state: &State) -> Vec<Line<'static>> {
    match state.screen {
        Screen::Runs => selected_run(state).map_or_else(
            || vec![value_line("status", "No run selected")],
            run_detail_lines,
        ),
        Screen::PullRequests => state.pull_requests.get(state.selected_pr).map_or_else(
            || vec![value_line("status", "No pull request selected")],
            pull_request_detail_lines,
        ),
    }
}

fn run_detail_lines(run: &Run) -> Vec<Line<'static>> {
    vec![
        value_line("status", run.status_label()),
        value_line("conclusion", run.conclusion.as_deref().unwrap_or("")),
        value_line("title", &run.display_title),
        value_line("workflow", run.workflow_label()),
        value_line("branch", &run.head_branch),
        value_line("event", &run.event),
        value_line(
            "pr",
            &run.pr_number
                .map(|number| format!("#{number}"))
                .unwrap_or_default(),
        ),
        value_line("id", &run.database_id.to_string()),
        value_line("created", &run.created_at.to_rfc3339()),
        value_line(
            "started",
            &run.started_at
                .map_or_else(String::new, |time| time.to_rfc3339()),
        ),
        value_line("updated", &run.updated_at.to_rfc3339()),
        value_line(
            "elapsed",
            &timefmt::elapsed(run.started_at, run.updated_at, &run.status),
        ),
        value_line("age", &timefmt::age(run.created_at)),
    ]
}

fn pull_request_detail_lines(pull_request: &PullRequest) -> Vec<Line<'static>> {
    vec![
        value_line("status", pull_request.status_label()),
        value_line("draft", if pull_request.is_draft { "yes" } else { "no" }),
        value_line("title", &pull_request.title),
        value_line("author", pull_request.author_login()),
        value_line("branch", &pull_request.head_ref_name),
        value_line("base", &pull_request.base_ref_name),
        value_line("number", &format!("#{}", pull_request.number)),
        value_line("created", &pull_request.created_at.to_rfc3339()),
        value_line("updated", &pull_request.updated_at.to_rfc3339()),
        value_line("age", &timefmt::age(pull_request.created_at)),
    ]
}

fn value_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<12}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(Color::White)),
    ])
}

fn config_lines(state: &State) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    lines.extend(
        ConfigRow::ROWS
            .iter()
            .enumerate()
            .map(|(index, row)| config_row_line(*row, index == state.config_focus, state)),
    );

    lines
}

fn config_row_line(row: ConfigRow, focused: bool, state: &State) -> Line<'static> {
    let row_style = if focused {
        Style::default().fg(Color::White).bg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    let label_style = if row.editable() {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut spans = vec![Span::styled(format!("{:<12}", row.label()), label_style)];

    if row == ConfigRow::Columns && focused {
        spans.extend(text_field_spans(
            &config_value(row, state),
            state.config_text_cursor,
        ));
    } else {
        spans.push(Span::styled(
            format!("{:<42}", truncate_owned(&config_value(row, state), 40)),
            row_style,
        ));
    }

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        row.hint(),
        Style::default().fg(Color::DarkGray),
    ));

    Line::from(spans)
}

fn config_value(row: ConfigRow, state: &State) -> String {
    let draft = state.config_draft.as_ref();
    match row {
        ConfigRow::Layout => draft
            .map_or(state.settings.layout, |draft| draft.layout)
            .to_string(),
        ConfigRow::Interval => humantime::format_duration(
            draft.map_or(state.settings.interval, |draft| draft.interval),
        )
        .to_string(),
        ConfigRow::Limit => draft
            .map_or(state.settings.limit, |draft| draft.limit)
            .to_string(),
        ConfigRow::CursorAutoHide => {
            let auto_hide = draft.map_or(state.settings.cursor.auto_hide, |draft| {
                draft.cursor.auto_hide
            });
            if auto_hide {
                "auto-hide enabled".to_string()
            } else {
                "auto-hide disabled".to_string()
            }
        }
        ConfigRow::CursorHideAfter => {
            humantime::format_duration(draft.map_or(state.settings.cursor.hide_after, |draft| {
                draft.cursor.hide_after
            }))
            .to_string()
        }
        ConfigRow::Columns => draft.map_or_else(
            || {
                state
                    .settings
                    .columns
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            },
            |draft| draft.columns.clone(),
        ),
        ConfigRow::PrColumns => draft.map_or_else(
            || {
                state
                    .settings
                    .pr_columns
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            },
            |draft| draft.pr_columns.clone(),
        ),
    }
}

fn panel_footer(panel: Panel) -> Line<'static> {
    let text = match panel.kind {
        PanelKind::Config => "↑/↓ selects  ←/→ or -/+ edit  q/ESC cancel  ↵ accept",
        PanelKind::Keys => "↑/↓ scroll  q / ESC closes this panel",
        PanelKind::QuickLook => "↑/↓ scroll  ␣ / q / ESC closes this panel",
        PanelKind::AuthLogin => "←/→ choose  ↵ confirm  Y yes  N no  ESC closes this panel",
        PanelKind::Destroy => {
            "←/→ choose  ↵ confirm  Y yes  N no  M massive  ESC closes this panel"
        }
        PanelKind::DestroyMassiveConfirm => {
            "←/→ choose  ↵ confirm  Y yes  N no  ESC closes this panel"
        }
    };
    Line::from(Span::styled(text, Style::default().fg(Color::DarkGray)))
}

fn text_field_spans(value: &str, cursor: usize) -> Vec<Span<'static>> {
    const FIELD_WIDTH: usize = 42;
    let chars = value.chars().collect::<Vec<_>>();
    let cursor = cursor.min(chars.len());
    let max_visible = FIELD_WIDTH.saturating_sub(1);
    let start = cursor.saturating_sub(max_visible);
    let end = (start + max_visible).min(chars.len());
    let before = chars[start..cursor].iter().collect::<String>();
    let cursor_char = chars.get(cursor).copied().unwrap_or(' ');
    let after = if cursor < end {
        chars[cursor + 1..end].iter().collect::<String>()
    } else {
        String::new()
    };
    let used = before.chars().count() + 1 + after.chars().count();
    let padding = " ".repeat(FIELD_WIDTH.saturating_sub(used));

    vec![
        Span::styled(
            before,
            Style::default().fg(Color::White).bg(Color::DarkGray),
        ),
        Span::styled(
            cursor_char.to_string(),
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ),
        Span::styled(
            format!("{after}{padding}"),
            Style::default().fg(Color::White).bg(Color::DarkGray),
        ),
    ]
}

fn command_line(key: &str, command: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{key:<12}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(command.to_string(), Style::default().fg(Color::White)),
    ])
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn truncate_owned(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut out = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() && max_chars > 1 {
        out.truncate(out.len().saturating_sub(1));
        out.push('…');
    }
    out
}
