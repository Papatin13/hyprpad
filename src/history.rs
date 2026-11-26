use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default)]
pub struct UsageStats {
    pub count: u64,
    pub last_used: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AppHistory {
    entries: HashMap<String, UsageStats>,
}

impl AppHistory {
    fn history_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
            })?;
        Some(base.join("hyperpad").join("history.tsv"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::history_path() else {
            return Self::default();
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            return Self::default();
        };

        let mut entries = HashMap::new();
        for line in contents.lines() {
            let mut parts = line.split('\t');
            let (Some(id), Some(count_str), Some(last_str)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if let (Ok(count), Ok(last_used)) = (count_str.parse::<u64>(), last_str.parse::<u64>()) {
                entries.insert(id.to_string(), UsageStats { count, last_used });
            }
        }
        Self { entries }
    }

    pub fn get(&self, app_id: &str) -> UsageStats {
        self.entries.get(app_id).cloned().unwrap_or_default()
    }

    pub fn record(&mut self, app_id: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let sanitized_id = app_id.replace(['\t', '\n', '\r'], " ");
        let entry = self.entries.entry(sanitized_id).or_default();
        entry.count = entry.count.saturating_add(1);
        entry.last_used = now;
        self.save();
    }

    fn save(&self) {
        let Some(path) = Self::history_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut out = String::new();
        for (id, stats) in &self.entries {
            out.push_str(&format!("{id}\t{}\t{}\n", stats.count, stats.last_used));
        }
        let _ = fs::write(&path, out);
    }
}
