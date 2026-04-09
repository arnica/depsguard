// Dependabot scanner: checks cooldown.default-days in .github/dependabot.yml.

use std::path::Path;

use super::config::read_dependabot_entries;
use super::detect::get_delay_days;
use super::types::{CheckStatus, Recommendation};

pub fn scan(path: &Path) -> Vec<Recommendation> {
    let days = get_delay_days();
    let entries = read_dependabot_entries(path);
    if entries.is_empty() {
        return Vec::new();
    }
    let single = entries.len() == 1;
    let mut recs = Vec::new();
    for entry in &entries {
        let status = match entry.cooldown_default_days {
            Some(d) if d >= days => CheckStatus::Ok,
            Some(d) => CheckStatus::WrongValue(d.to_string()),
            None => CheckStatus::Missing,
        };
        let key = if single {
            "cooldown.default-days".into()
        } else {
            format!(
                "cooldown.default-days ({}: {})",
                entry.ecosystem, entry.directory
            )
        };
        recs.push(Recommendation {
            key,
            description: format!("Delay updates by {days} days"),
            expected: days.to_string(),
            status,
        });
    }
    recs
}
