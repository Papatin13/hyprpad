use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub app_ids: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FolderStore {
    pub folders: Vec<Folder>,
}

impl FolderStore {
    fn folders_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
            })?;
        Some(base.join("hyperpad").join("folders.tsv"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::folders_path() else {
            return Self::default();
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            return Self::default();
        };

        let mut folders = Vec::new();
        for line in contents.lines() {
            let mut parts = line.split('\t');
            let (Some(id), Some(name), Some(apps_str)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let app_ids: Vec<String> = apps_str
                .split('|')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            if !app_ids.is_empty() {
                folders.push(Folder {
                    id: id.to_string(),
                    name: name.to_string(),
                    app_ids,
                });
            }
        }
        Self { folders }
    }

    pub fn save(&self) {
        let Some(path) = Self::folders_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut out = String::new();
        for f in &self.folders {
            let clean_name = f.name.replace(['\t', '\n', '\r'], " ");
            let apps = f
                .app_ids
                .iter()
                .map(|id| id.replace(['\t', '\n', '\r', '|'], " "))
                .collect::<Vec<_>>()
                .join("|");
            out.push_str(&format!("{}\t{}\t{}\n", f.id, clean_name, apps));
        }
        let _ = fs::write(&path, out);
    }

    /// Returns a set of all app IDs currently inside any folder.
    pub fn assigned_app_ids(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        for f in &self.folders {
            for id in &f.app_ids {
                set.insert(id.clone());
            }
        }
        set
    }

    /// Create a new folder with the given initial app IDs. Returns the new folder ID.
    pub fn create_folder(&mut self, name: &str, app_ids: Vec<String>) -> String {
        // Remove these apps from any existing folders first
        for id in &app_ids {
            self.remove_app_internal(id);
        }

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let id = format!("folder-{ts}");
        self.folders.push(Folder {
            id: id.clone(),
            name: name.to_string(),
            app_ids,
        });
        self.cleanup_empty();
        self.save();
        id
    }

    /// Add an app to an existing folder.
    pub fn add_app_to_folder(&mut self, folder_id: &str, app_id: &str) {
        self.remove_app_internal(app_id);
        if let Some(folder) = self.folders.iter_mut().find(|f| f.id == folder_id) {
            if !folder.app_ids.iter().any(|id| id == app_id) {
                folder.app_ids.push(app_id.to_string());
            }
        }
        self.cleanup_empty();
        self.save();
    }

    /// Remove an app from whichever folder it belongs to.
    pub fn remove_app(&mut self, app_id: &str) {
        self.remove_app_internal(app_id);
        self.cleanup_empty();
        self.save();
    }

    /// Rename a folder.
    pub fn rename_folder(&mut self, folder_id: &str, new_name: &str) {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Some(folder) = self.folders.iter_mut().find(|f| f.id == folder_id) {
            folder.name = trimmed.to_string();
        }
        self.save();
    }

    /// Dissolve a folder (returns all apps to the root grid).
    pub fn dissolve_folder(&mut self, folder_id: &str) {
        self.folders.retain(|f| f.id != folder_id);
        self.save();
    }

    /// Collect all unassigned Steam game IDs into the dedicated "Steam" folder.
    pub fn sync_steam_folder(&mut self, steam_app_ids: &[String]) {
        if steam_app_ids.is_empty() {
            return;
        }
        let assigned = self.assigned_app_ids();
        let to_add: Vec<String> = steam_app_ids
            .iter()
            .filter(|id| !assigned.contains(*id))
            .cloned()
            .collect();

        if to_add.is_empty() {
            return;
        }

        if let Some(folder) = self.folders.iter_mut().find(|f| f.id == "auto-steam") {
            folder.app_ids.extend(to_add);
        } else {
            self.folders.push(Folder {
                id: "auto-steam".to_string(),
                name: "Steam".to_string(),
                app_ids: to_add,
            });
        }
        self.save();
    }

    fn remove_app_internal(&mut self, app_id: &str) {
        for f in &mut self.folders {
            f.app_ids.retain(|id| id != app_id);
        }
    }

    fn cleanup_empty(&mut self) {
        self.folders.retain(|f| !f.app_ids.is_empty());
    }
}
