#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Keys,
    Config,
    QuickLook,
    AuthLogin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigRow {
    Interval,
    Limit,
    CursorAutoHide,
    CursorHideAfter,
    Columns,
    PrColumns,
}

impl ConfigRow {
    pub const ROWS: [Self; 6] = [
        Self::Interval,
        Self::Limit,
        Self::CursorAutoHide,
        Self::CursorHideAfter,
        Self::Columns,
        Self::PrColumns,
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
            Self::PrColumns => "PR columns",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Interval => "←/→ or -/+ adjusts by 1s",
            Self::Limit => "←/→ or -/+ adjusts by 1",
            Self::CursorAutoHide => "←/→ toggles",
            Self::CursorHideAfter => "←/→ or -/+ adjusts by 1s",
            Self::Columns => "type to edit, ←/→ moves cursor",
            Self::PrColumns => "type to edit, ←/→ moves cursor",
        }
    }

    pub fn editable(self) -> bool {
        true
    }

    pub fn is_text_field(self) -> bool {
        matches!(self, Self::Columns | Self::PrColumns)
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

    pub fn quick_look() -> Self {
        Self {
            kind: PanelKind::QuickLook,
        }
    }

    pub fn auth_login() -> Self {
        Self {
            kind: PanelKind::AuthLogin,
        }
    }

    pub fn title(self) -> &'static str {
        match self.kind {
            PanelKind::Keys => "Key Commands",
            PanelKind::Config => "Config",
            PanelKind::QuickLook => "Quick Look",
            PanelKind::AuthLogin => "GitHub Authentication",
        }
    }

    pub fn lines(self) -> &'static [(&'static str, &'static str)] {
        match self.kind {
            PanelKind::Keys => &[
                ("r", "Refresh immediately"),
                ("h", "Toggle cursor auto-hide"),
                ("⇥", "Switch between Actions and pull request screens"),
                ("c", "Open config panel"),
                ("k", "Show key commands"),
                ("␣", "Quick look at selected row"),
                ("↑ / ↓", "Show or move the navigation cursor"),
                ("↵", "Open selected run or pull request in browser"),
            ],
            PanelKind::Config | PanelKind::QuickLook | PanelKind::AuthLogin => &[],
        }
    }
}
