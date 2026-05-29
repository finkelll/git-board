use chrono::{DateTime, Utc};

pub fn elapsed(
    started_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    status: &str,
) -> String {
    let Some(started_at) = started_at else {
        return "-".to_string();
    };

    let end = if status == "completed" {
        updated_at
    } else {
        Utc::now()
    };
    let seconds = end.signed_duration_since(started_at).num_seconds().max(0);
    compact_duration(seconds)
}

pub fn age(created_at: DateTime<Utc>) -> String {
    let seconds = Utc::now()
        .signed_duration_since(created_at)
        .num_seconds()
        .max(0);

    match seconds {
        0..=59 => "less than a minute ago".to_string(),
        60..=89 => "about 1 minute ago".to_string(),
        90..=3599 => format!("about {} minutes ago", seconds / 60),
        3600..=5399 => "about 1 hour ago".to_string(),
        5400..=86_399 => format!("about {} hours ago", seconds / 3600),
        86_400..=129_599 => "about 1 day ago".to_string(),
        _ => format!("about {} days ago", seconds / 86_400),
    }
}

fn compact_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{hours}h{minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m{secs}s")
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn formats_age() {
        assert_eq!(
            age(Utc::now() - Duration::seconds(30)),
            "less than a minute ago"
        );
        assert!(age(Utc::now() - Duration::minutes(22)).contains("22 minutes"));
    }
}
