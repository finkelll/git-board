#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Keys,
    Config,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigRow {
    Interval,
    Limit,
    CursorAutoHide,
    CursorHideAfter,
    Columns,
}

impl ConfigRow {
    pub const ROWS: [Self; 5] = [
        Self::Interval,
        Self::Limit,
        Self::CursorAutoHide,
        Self::CursorHideAfter,
        Self::Columns,
    ];

    pub fn from_index(index: usize) -> Self {
        Self::ROWS[index.min(Self::ROWS.len() - 1)]
    }

    pub fn len() -> usize {
        Self::ROWS.len()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Interval => "interval",
            Self::Limit => "limit",
            Self::CursorAutoHide => "cursor",
            Self::CursorHideAfter => "hide after",
            Self::Columns => "columns",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Interval => "Left/Right or -/+ adjusts by 1s",
            Self::Limit => "Left/Right or -/+ adjusts by 1",
            Self::CursorAutoHide => "Left/Right toggles",
            Self::CursorHideAfter => "Left/Right or -/+ adjusts by 1s",
            Self::Columns => "type to edit, Left/Right moves cursor",
        }
    }

    pub fn editable(self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Panel {
    pub kind: PanelKind,
}

impl Panel {
    pub fn keys() -> Self {
        Self {
            kind: PanelKind::Keys,
        }
    }

    pub fn config() -> Self {
        Self {
            kind: PanelKind::Config,
        }
    }

    pub fn title(self) -> &'static str {
        match self.kind {
            PanelKind::Keys => "Key Commands",
            PanelKind::Config => "Config",
        }
    }

    pub fn lines(self) -> &'static [(&'static str, &'static str)] {
        match self.kind {
            PanelKind::Keys => &[
                ("r", "Refresh immediately"),
                ("h", "Toggle cursor auto-hide"),
                ("c", "Open config panel"),
                ("k", "Show key commands"),
                ("Up / Down", "Show or move the navigation cursor"),
                ("Enter", "Open selected workflow run in browser"),
            ],
            PanelKind::Config => &[],
        }
    }
}
