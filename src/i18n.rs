#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    pub fn detect() -> Self {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                let lower = val.trim().to_lowercase();
                if !lower.is_empty() && lower != "c" && lower != "posix" {
                    if lower.starts_with("de") {
                        return Lang::De;
                    }
                    return Lang::En;
                }
            }
        }
        Lang::En
    }
}

pub struct Strings {
    pub search_placeholder: &'static str,
    pub no_apps_found: &'static str,
    pub default_folder_name: &'static str,
    pub remove_from_folder: &'static str,
    pub new_folder_with_app: &'static str,
    pub open_folder: &'static str,
    pub rename_folder: &'static str,
    pub dissolve_folder: &'static str,
    pub settings: &'static str,
    pub background_opacity: &'static str,
    pub icon_size: &'static str,
    pub columns: &'static str,
    pub rows: &'static str,
    pub spacing: &'static str,
    pub reset_defaults: &'static str,
    pub config_hint: &'static str,
    pub auto_steam_folder: &'static str,
    lang: Lang,
}

impl Strings {
    pub fn from_system() -> Self {
        match Lang::detect() {
            Lang::De => Self {
                search_placeholder: "Suchen",
                no_apps_found: "Keine Anwendungen gefunden",
                default_folder_name: "Ordner",
                remove_from_folder: "Aus Ordner entfernen",
                new_folder_with_app: "Neuer Ordner mit dieser App",
                open_folder: "Öffnen",
                rename_folder: "Umbenennen",
                dissolve_folder: "Ordner auflösen",
                settings: "Einstellungen",
                background_opacity: "Hintergrund-Deckkraft",
                icon_size: "Icon-Größe",
                columns: "Spalten",
                rows: "Zeilen",
                spacing: "Abstand",
                reset_defaults: "Standard wiederherstellen",
                config_hint: "Konfig: ~/.config/hyperpad/config.conf",
                auto_steam_folder: "Steam-Spiele in Ordner sammeln",
                lang: Lang::De,
            },
            Lang::En => Self {
                search_placeholder: "Search",
                no_apps_found: "No Applications Found",
                default_folder_name: "Folder",
                remove_from_folder: "Remove from folder",
                new_folder_with_app: "New folder with this app",
                open_folder: "Open",
                rename_folder: "Rename",
                dissolve_folder: "Dissolve folder",
                settings: "Settings",
                background_opacity: "Background opacity",
                icon_size: "Icon size",
                columns: "Columns",
                rows: "Rows",
                spacing: "Spacing",
                reset_defaults: "Reset to defaults",
                config_hint: "Config: ~/.config/hyperpad/config.conf",
                auto_steam_folder: "Group Steam games in folder",
                lang: Lang::En,
            },
        }
    }

    pub fn move_to_folder(&self, folder_name: &str) -> String {
        match self.lang {
            Lang::De => format!("In „{folder_name}“ verschieben"),
            Lang::En => format!("Move to \"{folder_name}\""),
        }
    }
}
