use anyhow::{bail, Result};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Status,
    Title,
    Workflow,
    Branch,
    Event,
    Id,
    Elapsed,
    Age,
}

impl Column {
    pub fn default_columns() -> Vec<Self> {
        vec![
            Self::Status,
            Self::Title,
            Self::Workflow,
            Self::Branch,
            Self::Event,
            Self::Id,
            Self::Elapsed,
            Self::Age,
        ]
    }

    pub fn header(self) -> &'static str {
        match self {
            Self::Status => "STATUS",
            Self::Title => "TITLE",
            Self::Workflow => "WORKFLOW",
            Self::Branch => "BRANCH",
            Self::Event => "EVENT",
            Self::Id => "ID",
            Self::Elapsed => "ELAPSED",
            Self::Age => "AGE",
        }
    }

    pub fn width(self) -> u16 {
        match self {
            Self::Status => 8,
            Self::Title => 50,
            Self::Workflow => 18,
            Self::Branch => 34,
            Self::Event => 16,
            Self::Id => 14,
            Self::Elapsed => 10,
            Self::Age => 24,
        }
    }

    pub fn all_names() -> &'static str {
        "status,title,workflow,branch,event,id,elapsed,age"
    }
}

impl fmt::Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Status => "status",
            Self::Title => "title",
            Self::Workflow => "workflow",
            Self::Branch => "branch",
            Self::Event => "event",
            Self::Id => "id",
            Self::Elapsed => "elapsed",
            Self::Age => "age",
        };
        f.write_str(name)
    }
}

impl FromStr for Column {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "status" => Ok(Self::Status),
            "title" => Ok(Self::Title),
            "workflow" => Ok(Self::Workflow),
            "branch" => Ok(Self::Branch),
            "event" => Ok(Self::Event),
            "id" => Ok(Self::Id),
            "elapsed" => Ok(Self::Elapsed),
            "age" => Ok(Self::Age),
            other => bail!(
                "unknown column '{other}'. Valid columns: {}",
                Self::all_names()
            ),
        }
    }
}

pub fn parse_columns_csv(value: &str) -> Result<Vec<Column>> {
    let columns = value
        .split(',')
        .map(Column::from_str)
        .collect::<Result<Vec<_>>>()?;

    if columns.is_empty() {
        bail!("at least one column must be configured");
    }

    Ok(columns)
}

pub fn parse_columns_list(values: &[String]) -> Result<Vec<Column>> {
    let columns = values
        .iter()
        .map(|value| Column::from_str(value))
        .collect::<Result<Vec<_>>>()?;

    if columns.is_empty() {
        bail!("at least one column must be configured");
    }

    Ok(columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_columns_in_order() {
        assert_eq!(
            parse_columns_csv("title,status,id").unwrap(),
            vec![Column::Title, Column::Status, Column::Id]
        );
    }

    #[test]
    fn rejects_unknown_columns() {
        assert!(parse_columns_csv("status,banana").is_err());
    }
}
