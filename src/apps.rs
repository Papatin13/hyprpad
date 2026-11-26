use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use gio::prelude::*;
use gtk4::gdk;
use gtk4::prelude::*;

use crate::history::AppHistory;

#[derive(Clone)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<gio::Icon>,
    pub info: gio::AppInfo,
}

impl AppEntry {
    /// Load all visible desktop applications sorted by usage frequency, recency, then alphabetically.
    pub fn load_all(history: &AppHistory) -> Vec<Self> {
        let mut apps: Vec<AppEntry> = gio::AppInfo::all()
            .into_iter()
            .filter(|app| app.should_show())
            .map(|app| {
                let name = app.name().to_string();
                let id = app
                    .id()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| name.clone());
                let description = app
                    .description()
                    .map(|d| d.to_string())
                    .unwrap_or_default();
                let icon = app.icon();
                AppEntry {
                    id,
                    name,
                    description,
                    icon,
                    info: app,
                }
            })
            .collect();

        apps.sort_by(|a, b| {
            let sa = history.get(&a.id);
            let sb = history.get(&b.id);
            sb.count
                .cmp(&sa.count)
                .then_with(|| sb.last_used.cmp(&sa.last_used))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        apps
    }

    /// Filter and rank applications by fuzzy match score combined with usage history.
    pub fn filter(apps: &[AppEntry], query: &str, history: &AppHistory) -> Vec<AppEntry> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return apps.to_vec();
        }

        let matcher = SkimMatcherV2::default();
        let mut scored: Vec<(i64, u64, u64, AppEntry)> = apps
            .iter()
            .filter_map(|app| {
                let name_score = matcher.fuzzy_match(&app.name, trimmed);
                let desc_score = matcher
                    .fuzzy_match(&app.description, trimmed)
                    .map(|s| s / 2);

                let base_score = match (name_score, desc_score) {
                    (Some(n), Some(d)) => Some(n.max(d)),
                    (Some(n), None) => Some(n),
                    (None, Some(d)) => Some(d),
                    (None, None) => None,
                };

                base_score.map(|score| {
                    let stats = history.get(&app.id);
                    let freq_bonus = if stats.count > 0 {
                        ((stats.count as f64 + 1.0).ln() * 25.0) as i64
                    } else {
                        0
                    };
                    (
                        score + freq_bonus,
                        stats.count,
                        stats.last_used,
                        app.clone(),
                    )
                })
            })
            .collect();

        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.3.name.to_lowercase().cmp(&b.3.name.to_lowercase()))
        });

        scored.into_iter().map(|(_, _, _, app)| app).collect()
    }

    /// Launch the application using GDK's AppLaunchContext for Wayland activation tokens.
    pub fn launch(&self) -> Result<(), gtk4::glib::Error> {
        let context = gdk::Display::default().map(|display| display.app_launch_context());
        self.info.launch(&[], context.as_ref())
    }
}
