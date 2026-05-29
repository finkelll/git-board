use crate::app::{Screen, State};
use crate::columns::{Column, PrColumn};
use crate::model::{PullRequest, Run};
use crate::panel::{ConfigRow, Panel};
use crate::timefmt;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap};
use ratatui::Frame;

pub fn draw(frame: &mut Frame<'_>, state: &State) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let header = Line::from(Span::styled(
        match state.screen {
            Screen::Runs => "=== GitHub Actions ===",
            Screen::PullRequests => "=== Open Pull Requests ===",
        },
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(header, chunks[0]);

    let last_check = state
        .last_check
        .map(|time| time.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
        .unwrap_or_else(|| "not checked yet".to_string());
    let check_line = Line::from(vec![
        Span::styled(
            last_check,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(refresh_indicator(state), Style::default().fg(Color::Yellow)),
    ]);
    frame.render_widget(check_line, chunks[1]);

    match state.screen {
        Screen::Runs => render_runs_table(frame, state, chunks[2]),
        Screen::PullRequests => render_pull_requests_table(frame, state, chunks[2]),
    }

    let footer = footer(state);
    frame.render_widget(footer, chunks[3]);

    if let Some(panel) = state.panel {
        render_panel(frame, panel, state);
    }
}

fn render_runs_table(frame: &mut Frame<'_>, state: &State, area: Rect) {
    let widths = state
        .settings
        .columns
        .iter()
        .map(|column| Constraint::Length(column.width()))
        .collect::<Vec<_>>();
    let header = table_header(state.settings.columns.iter().map(|column| column.header()));
    let rows = state.runs.iter().enumerate().map(|(index, run)| {
        let selected = state.cursor_visible && index == state.selected_run;
        Row::new(
            state
                .settings
                .columns
                .iter()
                .map(|column| run_cell_for(*column, run))
                .collect::<Vec<_>>(),
        )
        .style(row_style(selected))
    });
    render_table(frame, rows, widths, header, area);
}

fn render_pull_requests_table(frame: &mut Frame<'_>, state: &State, area: Rect) {
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
    render_table(frame, rows, widths, header, area);
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
    area: Rect,
) {
    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(2)
        .block(Block::default().borders(Borders::NONE));
    frame.render_widget(table, area);
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
    ];

    if let Some(error) = &state.error {
        spans.push(Span::styled(
            truncate_owned(error, 80),
            Style::default().fg(Color::Red),
        ));
        spans.push(Span::raw("  "));
    }

    spans.push(Span::styled(
        match state.screen {
            Screen::Runs => {
                "TAB PRs  r refresh  c config  k keys  ↑/↓ select  ENTER open run  q/ESC quit"
            }
            Screen::PullRequests => {
                "TAB Actions  r refresh  c config  k keys  ↑/↓ select  ENTER open PR  q/ESC quit"
            }
        },
        Style::default().fg(Color::DarkGray),
    ));

    Line::from(spans)
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

fn render_panel(frame: &mut Frame<'_>, panel: Panel, state: &State) {
    let area = centered_rect(72, 58, frame.area());
    frame.render_widget(Clear, area);

    let lines = panel_lines(panel, state);

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(panel.title())
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().bg(Color::Black))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn panel_lines(panel: Panel, state: &State) -> Vec<Line<'static>> {
    let mut lines = panel
        .lines()
        .iter()
        .map(|(key, command)| command_line(key, command))
        .collect::<Vec<_>>();

    if lines.is_empty() {
        lines = config_lines(state);
    }

    lines.push(Line::from(""));
    lines.push(panel_footer(panel));
    lines
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
        crate::panel::PanelKind::Config => {
            "↑/↓ selects  ←/→ or -/+ edit  q/ESC cancel  ENTER accept"
        }
        crate::panel::PanelKind::Keys => "q / ESC closes this panel",
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
