use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub columns: usize,
    pub rows: usize,
    pub icon_size: i32,
    pub spacing: i32,
    pub background_opacity: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            columns: 7,
            rows: 4,
            icon_size: 72,
            spacing: 16,
            background_opacity: 0.25,
        }
    }
}

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
            })?;
        Some(base.join("hyperpad").join("config.conf"))
    }

    pub fn tile_size(&self) -> i32 {
        self.icon_size + 54
    }

    pub fn apps_per_page(&self) -> usize {
        self.columns * self.rows
    }

    pub fn load() -> Self {
        let mut cfg = Self::default();
        let Some(path) = Self::config_path() else {
            return cfg;
        };

        let Ok(contents) = fs::read_to_string(&path) else {
            // Create default config file on first run
            cfg.save();
            return cfg;
        };

        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let Some((key, val)) = trimmed.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let val = val.trim();
            match key {
                "columns" => {
                    if let Ok(v) = val.parse::<usize>() {
                        cfg.columns = v.clamp(3, 12);
                    }
                }
                "rows" => {
                    if let Ok(v) = val.parse::<usize>() {
                        cfg.rows = v.clamp(2, 8);
                    }
                }
                "icon_size" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.icon_size = v.clamp(36, 128);
                    }
                }
                "spacing" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.spacing = v.clamp(4, 48);
                    }
                }
                "background_opacity" => {
                    if let Ok(v) = val.parse::<f64>() {
                        cfg.background_opacity = v.clamp(0.0, 0.95);
                    }
                }
                _ => {}
            }
        }
        cfg
    }

    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let content = format!(
            "# launchpad-rs Konfiguration\n\
             # Änderungen werden beim nächsten Öffnen (oder direkt über das Zahnrad-Menü) übernommen.\n\n\
             columns = {}\n\
             rows = {}\n\
             icon_size = {}\n\
             spacing = {}\n\
             background_opacity = {:.2}\n",
            self.columns, self.rows, self.icon_size, self.spacing, self.background_opacity
        );
        let _ = fs::write(&path, content);
    }
}
