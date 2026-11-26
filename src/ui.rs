use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CenterBox, CssProvider,
    DragSource, DropTarget, Entry, EventControllerKey, EventControllerScroll,
    EventControllerScrollFlags, GestureClick, Grid, Image, Label, Orientation, Overlay, PickFlags,
    Popover, PropagationPhase, Scale, Stack, StackTransitionType, Switch, WidgetPaintable,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::apps::AppEntry;
use crate::config::Config;
use crate::folders::{Folder, FolderStore};
use crate::history::AppHistory;
use crate::i18n::Strings;

#[derive(Clone)]
enum GridItem {
    App(AppEntry),
    Folder(Folder),
}

#[derive(Clone, Copy)]
struct LayoutMetrics {
    scale: f64,
    icon_size: i32,
    tile_size: i32,
    spacing: i32,
    cols: usize,
    apps_per_page: usize,
}

struct LauncherState {
    all_apps: Vec<AppEntry>,
    display_items: RefCell<Vec<GridItem>>,
    history: RefCell<AppHistory>,
    folders: RefCell<FolderStore>,
    config: RefCell<Config>,
    strings: Strings,
    dynamic_css: CssProvider,
    monitor_size: Cell<(i32, i32)>,
    current_page: Cell<usize>,
    total_pages: Cell<usize>,
    selected_index: Cell<usize>,
    buttons: RefCell<Vec<Button>>,
    dot_buttons: RefCell<Vec<Button>>,
    last_scroll: RefCell<Instant>,
    open_folder_id: RefCell<Option<String>>,
    root_box: GtkBox,
    stack: Stack,
    dots_box: GtkBox,
    search_entry: Entry,
    folder_overlay: GtkBox,
    window: ApplicationWindow,
}

fn initial_monitor_size() -> (i32, i32) {
    if let Some(display) = gdk::Display::default() {
        let monitors = display.monitors();
        if let Some(obj) = monitors.item(0) {
            if let Ok(mon) = obj.downcast::<gdk::Monitor>() {
                let geo = mon.geometry();
                if geo.width() > 0 && geo.height() > 0 {
                    return (geo.width(), geo.height());
                }
            }
        }
    }
    (1920, 1080)
}

fn compute_layout_for(cfg: &Config, (mw, mh): (i32, i32)) -> LayoutMetrics {
    let scale = if cfg.auto_scale {
        let screen_scale = (mw as f64 / 1920.0).min(mh as f64 / 1080.0);
        let base_tile = (cfg.icon_size + 54) as f64;
        let cols = cfg.columns as f64;
        let rows = cfg.rows as f64;
        let sp = cfg.spacing as f64;

        let fit_w = (mw as f64 * 0.88) / (cols * base_tile + (cols - 1.0) * sp);
        let fit_h = (mh as f64 * 0.76) / (rows * base_tile + (rows - 1.0) * sp);
        let fit_scale = fit_w.min(fit_h);

        screen_scale.clamp(0.55, 1.65).min(fit_scale).max(0.45)
    } else {
        1.0
    };

    LayoutMetrics {
        scale,
        icon_size: ((cfg.icon_size as f64) * scale).round() as i32,
        tile_size: (((cfg.icon_size + 54) as f64) * scale).round() as i32,
        spacing: ((cfg.spacing as f64) * scale).round().max(4.0) as i32,
        cols: cfg.columns,
        apps_per_page: cfg.apps_per_page(),
    }
}

pub fn build_ui(app: &Application) {
    let strings = Strings::from_system();
    let config = Config::load();
    let mon_size = initial_monitor_size();
    let initial_layout = compute_layout_for(&config, mon_size);
    let dynamic_css = load_css(&config, &initial_layout);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Hyperpad")
        .build();

    window.add_css_class("launchpad-window");

    // Configure Wayland Layer Shell for Hyprland
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("hyperpad"));
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_exclusive_zone(-1);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);

    let history = AppHistory::load();
    let mut folders = FolderStore::load();
    let all_apps = AppEntry::load_all(&history);
    if config.auto_steam_folder {
        let steam_ids: Vec<String> = all_apps
            .iter()
            .filter(|a| a.is_steam_game)
            .map(|a| a.id.clone())
            .collect();
        folders.sync_steam_folder(&steam_ids);
    }

    // Root vertical layout
    let root_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(20)
        .hexpand(true)
        .vexpand(true)
        .margin_top(48)
        .margin_bottom(36)
        .margin_start(64)
        .margin_end(64)
        .build();

    // Top search bar container
    let search_container = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();
    search_container.add_css_class("search-box");

    let search_icon = Image::from_icon_name("system-search-symbolic");
    search_icon.set_pixel_size(16);
    search_icon.add_css_class("search-icon");

    let search_entry = Entry::builder()
        .placeholder_text(strings.search_placeholder)
        .hexpand(true)
        .build();
    search_entry.add_css_class("search-entry");

    search_container.append(&search_icon);
    search_container.append(&search_entry);

    // Top-right settings gear button
    let settings_btn = Button::builder()
        .icon_name("emblem-system-symbolic")
        .tooltip_text(strings.settings)
        .halign(Align::End)
        .valign(Align::Center)
        .build();
    settings_btn.add_css_class("settings-btn");

    let header_spacer = GtkBox::builder().width_request(36).build();

    let header_bar = CenterBox::new();
    header_bar.set_start_widget(Some(&header_spacer));
    header_bar.set_center_widget(Some(&search_container));
    header_bar.set_end_widget(Some(&settings_btn));
    header_bar.set_hexpand(true);

    // Center paginated stack
    let stack = Stack::builder()
        .transition_type(StackTransitionType::SlideLeftRight)
        .transition_duration(240)
        .hexpand(true)
        .vexpand(true)
        .hhomogeneous(true)
        .vhomogeneous(true)
        .build();

    // Bottom page indicator dots
    let dots_box = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .halign(Align::Center)
        .valign(Align::Center)
        .height_request(24)
        .build();

    root_box.append(&header_bar);
    root_box.append(&stack);
    root_box.append(&dots_box);

    // Folder modal overlay
    let folder_overlay = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Fill)
        .valign(Align::Fill)
        .hexpand(true)
        .vexpand(true)
        .visible(false)
        .build();
    folder_overlay.add_css_class("folder-backdrop");

    let main_overlay = Overlay::new();
    main_overlay.set_child(Some(&root_box));
    main_overlay.add_overlay(&folder_overlay);

    window.set_child(Some(&main_overlay));

    let state = Rc::new(LauncherState {
        all_apps,
        display_items: RefCell::new(Vec::new()),
        history: RefCell::new(history),
        folders: RefCell::new(folders),
        config: RefCell::new(config),
        strings,
        dynamic_css,
        monitor_size: Cell::new(mon_size),
        current_page: Cell::new(0),
        total_pages: Cell::new(1),
        selected_index: Cell::new(0),
        buttons: RefCell::new(Vec::new()),
        dot_buttons: RefCell::new(Vec::new()),
        last_scroll: RefCell::new(Instant::now() - Duration::from_secs(1)),
        open_folder_id: RefCell::new(None),
        root_box: root_box.clone(),
        stack: stack.clone(),
        dots_box: dots_box.clone(),
        search_entry: search_entry.clone(),
        folder_overlay: folder_overlay.clone(),
        window: window.clone(),
    });

    // Settings gear button handler
    {
        let state_s = Rc::clone(&state);
        let btn_weak = settings_btn.downgrade();
        settings_btn.connect_clicked(move |_| {
            if let Some(btn) = btn_weak.upgrade() {
                show_settings_popover(&state_s, &btn);
            }
        });
    }

    // Initial grid population
    refresh_grid(&state);

    // Search input listener
    {
        let state = Rc::clone(&state);
        search_entry.connect_changed(move |_| {
            if state.open_folder_id.borrow().is_some() {
                close_folder(&state);
            }
            state.current_page.set(0);
            state.selected_index.set(0);
            refresh_grid(&state);
        });
    }

    // Scroll wheel / trackpad swipe to change pages
    {
        let state = Rc::clone(&state);
        let scroll_ctrl = EventControllerScroll::new(EventControllerScrollFlags::BOTH_AXES);
        scroll_ctrl.connect_scroll(move |_, dx, dy| {
            if state.open_folder_id.borrow().is_some() {
                return glib::Propagation::Proceed;
            }
            let now = Instant::now();
            if now.duration_since(*state.last_scroll.borrow()) < Duration::from_millis(260) {
                return glib::Propagation::Stop;
            }

            let delta = if dx.abs() > dy.abs() { dx } else { dy };
            if delta.abs() < 0.15 {
                return glib::Propagation::Proceed;
            }

            let cur = state.current_page.get();
            let total = state.total_pages.get();

            if delta > 0.0 && cur + 1 < total {
                *state.last_scroll.borrow_mut() = now;
                go_to_page(&state, cur + 1);
            } else if delta < 0.0 && cur > 0 {
                *state.last_scroll.borrow_mut() = now;
                go_to_page(&state, cur - 1);
            }

            glib::Propagation::Stop
        });
        window.add_controller(scroll_ctrl);
    }

    // Click on empty background in root_box to close Launchpad
    {
        let root_weak = root_box.downgrade();
        let state = Rc::clone(&state);
        let click = GestureClick::new();
        click.set_propagation_phase(PropagationPhase::Bubble);
        click.connect_released(move |_, _, x, y| {
            if state.open_folder_id.borrow().is_some() {
                return;
            }
            let Some(root) = root_weak.upgrade() else {
                return;
            };
            if let Some(picked) = root.pick(x, y, PickFlags::DEFAULT) {
                let mut cur = Some(picked);
                while let Some(w) = cur {
                    if w.has_css_class("app-button")
                        || w.has_css_class("search-box")
                        || w.has_css_class("page-dot")
                        || w.has_css_class("settings-btn")
                    {
                        return;
                    }
                    cur = w.parent();
                }
            }
            state.window.close();
        });
        root_box.add_controller(click);
    }

    // Click on folder backdrop outside modal to close folder
    {
        let overlay_weak = folder_overlay.downgrade();
        let state = Rc::clone(&state);
        let click = GestureClick::new();
        click.set_propagation_phase(PropagationPhase::Bubble);
        click.connect_released(move |_, _, x, y| {
            let Some(ov) = overlay_weak.upgrade() else {
                return;
            };
            if let Some(picked) = ov.pick(x, y, PickFlags::DEFAULT) {
                let mut cur = Some(picked);
                while let Some(w) = cur {
                    if w.has_css_class("folder-modal") {
                        return;
                    }
                    cur = w.parent();
                }
            }
            close_folder(&state);
        });
        folder_overlay.add_controller(click);
    }

    // Keyboard navigation & shortcuts
    {
        let state = Rc::clone(&state);
        let key_ctrl = EventControllerKey::new();
        key_ctrl.set_propagation_phase(PropagationPhase::Capture);
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            if state.open_folder_id.borrow().is_some() {
                if key == gdk::Key::Escape {
                    close_folder(&state);
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Proceed;
            }

            match key {
                gdk::Key::Escape => {
                    state.window.close();
                    return glib::Propagation::Stop;
                }
                gdk::Key::Return | gdk::Key::KP_Enter => {
                    let items = state.display_items.borrow();
                    let idx = state.selected_index.get();
                    match items.get(idx).cloned() {
                        Some(GridItem::App(app)) => {
                            launch_and_close(&state, &app);
                        }
                        Some(GridItem::Folder(folder)) => {
                            drop(items);
                            open_folder(&state, &folder.id, false);
                        }
                        None => {}
                    }
                    return glib::Propagation::Stop;
                }
                gdk::Key::Page_Down => {
                    let cur = state.current_page.get();
                    let total = state.total_pages.get();
                    if cur + 1 < total {
                        go_to_page(&state, cur + 1);
                    }
                    return glib::Propagation::Stop;
                }
                gdk::Key::Page_Up => {
                    let cur = state.current_page.get();
                    if cur > 0 {
                        go_to_page(&state, cur - 1);
                    }
                    return glib::Propagation::Stop;
                }
                gdk::Key::Right => {
                    if state.search_entry.has_focus() && !state.search_entry.text().is_empty() {
                        return glib::Propagation::Proceed;
                    }
                    move_selection(&state, 1);
                    return glib::Propagation::Stop;
                }
                gdk::Key::Left => {
                    if state.search_entry.has_focus() && !state.search_entry.text().is_empty() {
                        return glib::Propagation::Proceed;
                    }
                    move_selection(&state, -1);
                    return glib::Propagation::Stop;
                }
                gdk::Key::Down => {
                    let cols = state.config.borrow().columns as isize;
                    move_selection(&state, cols);
                    return glib::Propagation::Stop;
                }
                gdk::Key::Up => {
                    let cols = state.config.borrow().columns as isize;
                    move_selection(&state, -cols);
                    return glib::Propagation::Stop;
                }
                _ => {
                    if !state.search_entry.has_focus() {
                        if let Some(ch) = key.to_unicode() {
                            if !ch.is_control() {
                                state.search_entry.grab_focus();
                                let mut pos = state.search_entry.text().chars().count() as i32;
                                state.search_entry.insert_text(&ch.to_string(), &mut pos);
                                state.search_entry.set_position(pos);
                                return glib::Propagation::Stop;
                            }
                        }
                    }
                }
            }

            glib::Propagation::Proceed
        });
        window.add_controller(key_ctrl);
    }

    window.present();
    if let Some(surface) = window.surface() {
        let state_m = Rc::clone(&state);
        surface.connect_enter_monitor(move |_, monitor| {
            let geo = monitor.geometry();
            let new_size = (geo.width(), geo.height());
            if new_size.0 > 0 && new_size.1 > 0 && state_m.monitor_size.get() != new_size {
                state_m.monitor_size.set(new_size);
                sync_layout(&state_m);
            }
        });
    }
    sync_layout(&state);
    search_entry.grab_focus();
}

fn current_layout(state: &LauncherState) -> LayoutMetrics {
    compute_layout_for(&state.config.borrow(), state.monitor_size.get())
}

fn sync_layout(state: &Rc<LauncherState>) {
    let layout = current_layout(state);
    let cfg = state.config.borrow();
    apply_dynamic_css(&state.dynamic_css, &cfg, &layout);

    let mt = (48.0 * layout.scale).round() as i32;
    let mb = (36.0 * layout.scale).round() as i32;
    let mx = (64.0 * layout.scale).round() as i32;
    state.root_box.set_margin_top(mt.max(16));
    state.root_box.set_margin_bottom(mb.max(14));
    state.root_box.set_margin_start(mx.max(20));
    state.root_box.set_margin_end(mx.max(20));
    state
        .root_box
        .set_spacing((20.0 * layout.scale).round().max(10.0) as i32);

    drop(cfg);
    refresh_grid(state);
}

fn load_css(config: &Config, layout: &LayoutMetrics) -> CssProvider {
    let provider = CssProvider::new();
    provider.load_from_data(include_str!("style.css"));

    let dynamic = CssProvider::new();
    apply_dynamic_css(&dynamic, config, layout);

    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 10,
        );
        gtk4::style_context_add_provider_for_display(
            &display,
            &dynamic,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 11,
        );
    }
    dynamic
}

fn apply_dynamic_css(dynamic: &CssProvider, config: &Config, layout: &LayoutMetrics) {
    let font_size = (13.0 * layout.scale).round().clamp(10.0, 18.0) as i32;
    let search_w = (300.0 * layout.scale).round().clamp(220.0, 440.0) as i32;
    let folder_pad = (8.0 * layout.scale).round().clamp(4.0, 12.0) as i32;
    let css = format!(
        "window.launchpad-window, window.launchpad-window.background {{ \
            background-color: rgba(12, 14, 20, {:.2}); \
         }} \
         .folder-icon-box {{ \
            min-width: {}px; \
            min-height: {}px; \
            padding: {}px; \
         }} \
         .app-label {{ \
            font-size: {}px; \
         }} \
         .search-box {{ \
            min-width: {}px; \
         }}",
        config.background_opacity,
        layout.icon_size,
        layout.icon_size,
        folder_pad,
        font_size,
        search_w
    );
    dynamic.load_from_data(&css);
}

fn launch_and_close(state: &Rc<LauncherState>, app: &AppEntry) {
    state.history.borrow_mut().record(&app.id);
    if let Err(err) = app.launch() {
        eprintln!("Failed to launch {}: {}", app.name, err);
    }
    state.window.close();
}

fn build_display_items(state: &LauncherState) -> Vec<GridItem> {
    let query = state.search_entry.text();
    let history = state.history.borrow();

    if !query.trim().is_empty() {
        return AppEntry::filter(&state.all_apps, &query, &history)
            .into_iter()
            .map(GridItem::App)
            .collect();
    }

    let folders = state.folders.borrow();
    let assigned = folders.assigned_app_ids();

    // Combine top-level apps and folders, ordered by frecency (max count & recency)
    let mut ranked: Vec<(u64, u64, String, GridItem)> = Vec::new();

    for app in &state.all_apps {
        if !assigned.contains(&app.id) {
            let stats = history.get(&app.id);
            ranked.push((
                stats.count,
                stats.last_used,
                app.name.to_lowercase(),
                GridItem::App(app.clone()),
            ));
        }
    }

    for folder in &folders.folders {
        let mut max_count = 0u64;
        let mut max_last = 0u64;
        for id in &folder.app_ids {
            let stats = history.get(id);
            max_count = max_count.max(stats.count);
            max_last = max_last.max(stats.last_used);
        }
        ranked.push((
            max_count,
            max_last,
            folder.name.to_lowercase(),
            GridItem::Folder(folder.clone()),
        ));
    }

    ranked.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    ranked.into_iter().map(|(_, _, _, item)| item).collect()
}

fn refresh_grid(state: &Rc<LauncherState>) {
    let items = build_display_items(state);
    *state.display_items.borrow_mut() = items;

    while let Some(child) = state.stack.first_child() {
        state.stack.remove(&child);
    }
    while let Some(child) = state.dots_box.first_child() {
        state.dots_box.remove(&child);
    }
    state.buttons.borrow_mut().clear();
    state.dot_buttons.borrow_mut().clear();

    let items = state.display_items.borrow();
    if items.is_empty() {
        state.total_pages.set(1);
        let empty_label = Label::new(Some(state.strings.no_apps_found));
        empty_label.add_css_class("empty-label");
        empty_label.set_halign(Align::Center);
        empty_label.set_valign(Align::Center);
        state.stack.add_named(&empty_label, Some("empty"));
        return;
    }

    let layout = current_layout(state);
    let (cols, apps_per_page, spacing, tile_size) =
        (layout.cols, layout.apps_per_page, layout.spacing, layout.tile_size);

    let chunks: Vec<&[GridItem]> = items.chunks(apps_per_page).collect();
    let num_pages = chunks.len();
    state.total_pages.set(num_pages);

    for (page_idx, chunk) in chunks.iter().enumerate() {
        let grid = Grid::builder()
            .row_homogeneous(true)
            .column_homogeneous(true)
            .row_spacing(spacing)
            .column_spacing(spacing)
            .halign(Align::Center)
            .valign(Align::Center)
            .build();

        for (i, item) in chunk.iter().enumerate() {
            let row = (i / cols) as i32;
            let col = (i % cols) as i32;
            let global_idx = page_idx * apps_per_page + i;

            let btn = match item {
                GridItem::App(app_entry) => create_app_button(state, app_entry, None),
                GridItem::Folder(folder) => create_folder_button(state, folder),
            };
            if global_idx == state.selected_index.get() {
                btn.add_css_class("selected-app");
            }
            state.buttons.borrow_mut().push(btn.clone());
            grid.attach(&btn, col, row, 1, 1);
        }

        if chunk.len() < cols {
            for col in chunk.len()..cols {
                let spacer = GtkBox::builder()
                    .width_request(tile_size)
                    .height_request(tile_size)
                    .build();
                grid.attach(&spacer, col as i32, 0, 1, 1);
            }
        }

        let page_name = format!("page-{page_idx}");
        state.stack.add_named(&grid, Some(&page_name));
    }

    if num_pages > 1 {
        for page_idx in 0..num_pages {
            let dot = Button::new();
            dot.add_css_class("page-dot");
            if page_idx == state.current_page.get() {
                dot.add_css_class("active");
            }

            let state_clone = Rc::clone(state);
            dot.connect_clicked(move |_| {
                go_to_page(&state_clone, page_idx);
            });

            state.dots_box.append(&dot);
            state.dot_buttons.borrow_mut().push(dot);
        }
    }

    let cur = state.current_page.get().min(num_pages.saturating_sub(1));
    state.current_page.set(cur);
    let clamped_sel = state
        .selected_index
        .get()
        .min(items.len().saturating_sub(1));
    state.selected_index.set(clamped_sel);
    state.stack.set_visible_child_name(&format!("page-{cur}"));
}

fn create_app_button(
    state: &Rc<LauncherState>,
    app_entry: &AppEntry,
    inside_folder_id: Option<&str>,
) -> Button {
    let vbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();

    let layout = current_layout(state);
    let (icon_size, tile_size) = (layout.icon_size, layout.tile_size);

    let image = match &app_entry.icon {
        Some(gicon) => Image::from_gicon(gicon),
        None => Image::from_icon_name("application-x-executable"),
    };
    image.set_pixel_size(icon_size);
    image.add_css_class("app-icon");

    let label = Label::builder()
        .label(&app_entry.name)
        .max_width_chars(14)
        .ellipsize(pango::EllipsizeMode::End)
        .justify(gtk4::Justification::Center)
        .halign(Align::Center)
        .build();
    label.add_css_class("app-label");

    vbox.append(&image);
    vbox.append(&label);

    let btn = Button::builder()
        .child(&vbox)
        .width_request(tile_size)
        .height_request(tile_size)
        .has_frame(false)
        .build();
    btn.add_css_class("app-button");

    if !app_entry.description.is_empty() {
        btn.set_tooltip_text(Some(&app_entry.description));
    }

    // Left-click: launch app
    {
        let state_clone = Rc::clone(state);
        let app_clone = app_entry.clone();
        btn.connect_clicked(move |_| {
            launch_and_close(&state_clone, &app_clone);
        });
    }

    // Drag & Drop support on root grid
    if inside_folder_id.is_none() {
        let drag_source = DragSource::new();
        drag_source.set_actions(gdk::DragAction::MOVE);
        let app_id_drag = app_entry.id.clone();
        drag_source.connect_prepare(move |_, _, _| {
            Some(gdk::ContentProvider::for_value(&app_id_drag.to_value()))
        });
        let img_weak = image.downgrade();
        drag_source.connect_drag_begin(move |src, _| {
            if let Some(img) = img_weak.upgrade() {
                let paintable = WidgetPaintable::new(Some(&img));
                src.set_icon(Some(&paintable), 36, 36);
            }
        });
        btn.add_controller(drag_source);

        // Drop another app onto this app -> create a new folder with both
        let drop_target = DropTarget::new(String::static_type(), gdk::DragAction::MOVE);
        let btn_weak = btn.downgrade();
        drop_target.connect_enter(move |_, _, _| {
            if let Some(b) = btn_weak.upgrade() {
                b.add_css_class("drop-target");
            }
            gdk::DragAction::MOVE
        });
        let btn_weak_leave = btn.downgrade();
        drop_target.connect_leave(move |_| {
            if let Some(b) = btn_weak_leave.upgrade() {
                b.remove_css_class("drop-target");
            }
        });
        let state_drop = Rc::clone(state);
        let target_app_id = app_entry.id.clone();
        let btn_weak_drop = btn.downgrade();
        drop_target.connect_drop(move |_, value, _, _| {
            if let Some(b) = btn_weak_drop.upgrade() {
                b.remove_css_class("drop-target");
            }
            if let Ok(dragged_id) = value.get::<String>() {
                if dragged_id != target_app_id {
                    let folder_id = state_drop
                        .folders
                        .borrow_mut()
                        .create_folder(state_drop.strings.default_folder_name, vec![target_app_id.clone(), dragged_id]);
                    refresh_grid(&state_drop);
                    open_folder(&state_drop, &folder_id, true);
                    return true;
                }
            }
            false
        });
        btn.add_controller(drop_target);
    }

    // Right-click context menu
    {
        let state_ctx = Rc::clone(state);
        let app_id = app_entry.id.clone();
        let folder_ctx = inside_folder_id.map(|s| s.to_string());
        let btn_weak = btn.downgrade();
        let right_click = GestureClick::new();
        right_click.set_button(3);
        right_click.connect_released(move |_, _, _, _| {
            let Some(b) = btn_weak.upgrade() else {
                return;
            };
            show_app_context_menu(&state_ctx, &b, &app_id, folder_ctx.as_deref());
        });
        btn.add_controller(right_click);
    }

    btn
}

fn create_folder_button(state: &Rc<LauncherState>, folder: &Folder) -> Button {
    let vbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();

    let icon_box = Grid::builder()
        .row_homogeneous(true)
        .column_homogeneous(true)
        .row_spacing(6)
        .column_spacing(6)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();
    icon_box.add_css_class("folder-icon-box");

    let layout = current_layout(state);
    let (mini_size, tile_size) = ((layout.icon_size / 3).max(14), layout.tile_size);

    // Show up to 4 mini app icons in a 2x2 grid
    for (idx, app_id) in folder.app_ids.iter().take(4).enumerate() {
        let row = (idx / 2) as i32;
        let col = (idx % 2) as i32;
        let mini_img = if let Some(app) = state.all_apps.iter().find(|a| &a.id == app_id) {
            match &app.icon {
                Some(gicon) => Image::from_gicon(gicon),
                None => Image::from_icon_name("application-x-executable"),
            }
        } else {
            Image::from_icon_name("application-x-executable")
        };
        mini_img.set_pixel_size(mini_size);
        icon_box.attach(&mini_img, col, row, 1, 1);
    }

    let label = Label::builder()
        .label(&folder.name)
        .max_width_chars(14)
        .ellipsize(pango::EllipsizeMode::End)
        .justify(gtk4::Justification::Center)
        .halign(Align::Center)
        .build();
    label.add_css_class("app-label");

    vbox.append(&icon_box);
    vbox.append(&label);

    let btn = Button::builder()
        .child(&vbox)
        .width_request(tile_size)
        .height_request(tile_size)
        .has_frame(false)
        .build();
    btn.add_css_class("app-button");

    // Left-click: open folder overlay
    {
        let state_clone = Rc::clone(state);
        let folder_id = folder.id.clone();
        btn.connect_clicked(move |_| {
            open_folder(&state_clone, &folder_id, false);
        });
    }

    // Drop target: drag an app into this folder
    {
        let drop_target = DropTarget::new(String::static_type(), gdk::DragAction::MOVE);
        let btn_weak = btn.downgrade();
        drop_target.connect_enter(move |_, _, _| {
            if let Some(b) = btn_weak.upgrade() {
                b.add_css_class("drop-target");
            }
            gdk::DragAction::MOVE
        });
        let btn_weak_leave = btn.downgrade();
        drop_target.connect_leave(move |_| {
            if let Some(b) = btn_weak_leave.upgrade() {
                b.remove_css_class("drop-target");
            }
        });
        let state_drop = Rc::clone(state);
        let folder_id = folder.id.clone();
        let btn_weak_drop = btn.downgrade();
        drop_target.connect_drop(move |_, value, _, _| {
            if let Some(b) = btn_weak_drop.upgrade() {
                b.remove_css_class("drop-target");
            }
            if let Ok(dragged_id) = value.get::<String>() {
                state_drop
                    .folders
                    .borrow_mut()
                    .add_app_to_folder(&folder_id, &dragged_id);
                refresh_grid(&state_drop);
                return true;
            }
            false
        });
        btn.add_controller(drop_target);
    }

    // Right-click context menu on folder
    {
        let state_ctx = Rc::clone(state);
        let folder_id = folder.id.clone();
        let btn_weak = btn.downgrade();
        let right_click = GestureClick::new();
        right_click.set_button(3);
        right_click.connect_released(move |_, _, _, _| {
            let Some(b) = btn_weak.upgrade() else {
                return;
            };
            show_folder_context_menu(&state_ctx, &b, &folder_id);
        });
        btn.add_controller(right_click);
    }

    btn
}

fn show_app_context_menu(
    state: &Rc<LauncherState>,
    parent: &Button,
    app_id: &str,
    inside_folder_id: Option<&str>,
) {
    let popover = Popover::new();
    popover.set_parent(parent);
    popover.add_css_class("context-popover");
    popover.set_has_arrow(false);

    let menu_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .build();

    if let Some(folder_id) = inside_folder_id {
        let remove_btn = Button::with_label(state.strings.remove_from_folder);
        remove_btn.add_css_class("context-menu-btn");
        let state_c = Rc::clone(state);
        let app_c = app_id.to_string();
        let folder_c = folder_id.to_string();
        let pop_weak = popover.downgrade();
        remove_btn.connect_clicked(move |_| {
            if let Some(p) = pop_weak.upgrade() {
                p.popdown();
            }
            state_c.folders.borrow_mut().remove_app(&app_c);
            refresh_grid(&state_c);
            // Re-open folder if it still exists, otherwise close
            let still_exists = state_c
                .folders
                .borrow()
                .folders
                .iter()
                .any(|f| f.id == folder_c);
            if still_exists {
                open_folder(&state_c, &folder_c, false);
            } else {
                close_folder(&state_c);
            }
        });
        menu_box.append(&remove_btn);
    } else {
        let new_folder_btn = Button::with_label(state.strings.new_folder_with_app);
        new_folder_btn.add_css_class("context-menu-btn");
        let state_c = Rc::clone(state);
        let app_c = app_id.to_string();
        let pop_weak = popover.downgrade();
        new_folder_btn.connect_clicked(move |_| {
            if let Some(p) = pop_weak.upgrade() {
                p.popdown();
            }
            let fid = state_c
                .folders
                .borrow_mut()
                .create_folder(state_c.strings.default_folder_name, vec![app_c.clone()]);
            refresh_grid(&state_c);
            open_folder(&state_c, &fid, true);
        });
        menu_box.append(&new_folder_btn);

        let folders = state.folders.borrow().folders.clone();
        for folder in folders {
            let move_btn = Button::with_label(&state.strings.move_to_folder(&folder.name));
            move_btn.add_css_class("context-menu-btn");
            let state_c = Rc::clone(state);
            let app_c = app_id.to_string();
            let fid = folder.id.clone();
            let pop_weak = popover.downgrade();
            move_btn.connect_clicked(move |_| {
                if let Some(p) = pop_weak.upgrade() {
                    p.popdown();
                }
                state_c
                    .folders
                    .borrow_mut()
                    .add_app_to_folder(&fid, &app_c);
                refresh_grid(&state_c);
            });
            menu_box.append(&move_btn);
        }
    }

    popover.set_child(Some(&menu_box));
    popover.popup();
}

fn show_folder_context_menu(state: &Rc<LauncherState>, parent: &Button, folder_id: &str) {
    let popover = Popover::new();
    popover.set_parent(parent);
    popover.add_css_class("context-popover");
    popover.set_has_arrow(false);

    let menu_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .build();

    let open_btn = Button::with_label(state.strings.open_folder);
    open_btn.add_css_class("context-menu-btn");
    {
        let state_c = Rc::clone(state);
        let fid = folder_id.to_string();
        let pop_weak = popover.downgrade();
        open_btn.connect_clicked(move |_| {
            if let Some(p) = pop_weak.upgrade() {
                p.popdown();
            }
            open_folder(&state_c, &fid, false);
        });
    }
    menu_box.append(&open_btn);

    let rename_btn = Button::with_label(state.strings.rename_folder);
    rename_btn.add_css_class("context-menu-btn");
    {
        let state_c = Rc::clone(state);
        let fid = folder_id.to_string();
        let pop_weak = popover.downgrade();
        rename_btn.connect_clicked(move |_| {
            if let Some(p) = pop_weak.upgrade() {
                p.popdown();
            }
            open_folder(&state_c, &fid, true);
        });
    }
    menu_box.append(&rename_btn);

    let dissolve_btn = Button::with_label(state.strings.dissolve_folder);
    dissolve_btn.add_css_class("context-menu-btn");
    dissolve_btn.add_css_class("destructive");
    {
        let state_c = Rc::clone(state);
        let fid = folder_id.to_string();
        let pop_weak = popover.downgrade();
        dissolve_btn.connect_clicked(move |_| {
            if let Some(p) = pop_weak.upgrade() {
                p.popdown();
            }
            state_c.folders.borrow_mut().dissolve_folder(&fid);
            refresh_grid(&state_c);
        });
    }
    menu_box.append(&dissolve_btn);

    popover.set_child(Some(&menu_box));
    popover.popup();
}

fn open_folder(state: &Rc<LauncherState>, folder_id: &str, focus_title: bool) {
    let folder = {
        let folders = state.folders.borrow();
        folders
            .folders
            .iter()
            .find(|f| f.id == folder_id)
            .cloned()
    };
    let Some(folder) = folder else {
        return;
    };

    *state.open_folder_id.borrow_mut() = Some(folder.id.clone());

    while let Some(child) = state.folder_overlay.first_child() {
        state.folder_overlay.remove(&child);
    }

    let modal = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(20)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();
    modal.add_css_class("folder-modal");

    // Editable folder title
    let title_entry = Entry::builder()
        .text(&folder.name)
        .halign(Align::Center)
        .build();
    gtk4::prelude::EditableExt::set_alignment(&title_entry, 0.5);
    title_entry.add_css_class("folder-title-entry");

    {
        let state_c = Rc::clone(state);
        let fid = folder.id.clone();
        title_entry.connect_activate(move |entry| {
            state_c.folders.borrow_mut().rename_folder(&fid, &entry.text());
            refresh_grid(&state_c);
            state_c.folder_overlay.grab_focus();
        });
    }
    {
        let state_c = Rc::clone(state);
        let fid = folder.id.clone();
        title_entry.connect_changed(move |entry| {
            state_c.folders.borrow_mut().rename_folder(&fid, &entry.text());
        });
    }

    // Folder apps grid (up to 4 columns)
    let grid = Grid::builder()
        .row_homogeneous(true)
        .column_homogeneous(true)
        .row_spacing(14)
        .column_spacing(14)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();

    let folder_cols = 4usize;
    let folder_apps: Vec<AppEntry> = folder
        .app_ids
        .iter()
        .filter_map(|id| state.all_apps.iter().find(|a| &a.id == id).cloned())
        .collect();

    for (i, app_entry) in folder_apps.iter().enumerate() {
        let row = (i / folder_cols) as i32;
        let col = (i % folder_cols) as i32;
        let btn = create_app_button(state, app_entry, Some(&folder.id));
        grid.attach(&btn, col, row, 1, 1);
    }

    modal.append(&title_entry);
    modal.append(&grid);

    // Center modal vertically inside backdrop
    let center_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Center)
        .valign(Align::Center)
        .hexpand(true)
        .vexpand(true)
        .build();
    center_box.append(&modal);

    state.folder_overlay.append(&center_box);
    state.folder_overlay.set_visible(true);

    if focus_title {
        title_entry.grab_focus();
        title_entry.select_region(0, -1);
    }
}

fn close_folder(state: &Rc<LauncherState>) {
    *state.open_folder_id.borrow_mut() = None;
    state.folder_overlay.set_visible(false);
    while let Some(child) = state.folder_overlay.first_child() {
        state.folder_overlay.remove(&child);
    }
    refresh_grid(state);
    state.search_entry.grab_focus();
}

fn go_to_page(state: &Rc<LauncherState>, target_page: usize) {
    let total = state.total_pages.get();
    if target_page >= total {
        return;
    }

    state.current_page.set(target_page);
    state
        .stack
        .set_visible_child_name(&format!("page-{target_page}"));

    for (idx, dot) in state.dot_buttons.borrow().iter().enumerate() {
        if idx == target_page {
            dot.add_css_class("active");
        } else {
            dot.remove_css_class("active");
        }
    }

    let apps_per_page = state.config.borrow().apps_per_page();
    let new_selected = target_page * apps_per_page;
    update_selection(state, new_selected);
}

fn move_selection(state: &Rc<LauncherState>, delta: isize) {
    let len = state.display_items.borrow().len();
    if len == 0 {
        return;
    }

    let apps_per_page = state.config.borrow().apps_per_page();
    let cur = state.selected_index.get() as isize;
    let next = (cur + delta).clamp(0, (len - 1) as isize) as usize;
    let target_page = next / apps_per_page;

    if target_page != state.current_page.get() {
        state.current_page.set(target_page);
        state
            .stack
            .set_visible_child_name(&format!("page-{target_page}"));
        for (idx, dot) in state.dot_buttons.borrow().iter().enumerate() {
            if idx == target_page {
                dot.add_css_class("active");
            } else {
                dot.remove_css_class("active");
            }
        }
    }

    update_selection(state, next);
}

fn update_selection(state: &Rc<LauncherState>, new_index: usize) {
    let buttons = state.buttons.borrow();
    if buttons.is_empty() {
        return;
    }
    let clamped = new_index.min(buttons.len() - 1);
    let old = state.selected_index.get();
    if let Some(old_btn) = buttons.get(old) {
        old_btn.remove_css_class("selected-app");
    }
    if let Some(new_btn) = buttons.get(clamped) {
        new_btn.add_css_class("selected-app");
    }
    state.selected_index.set(clamped);
}

fn make_slider_row(
    label_text: &str,
    min: f64,
    max: f64,
    step: f64,
    initial: f64,
    format_val: fn(f64) -> String,
) -> (GtkBox, Scale, Label) {
    let container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .build();

    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .hexpand(true)
        .build();

    let title = Label::builder()
        .label(label_text)
        .halign(Align::Start)
        .hexpand(true)
        .build();
    title.add_css_class("settings-label");

    let value_lbl = Label::builder()
        .label(&format_val(initial))
        .halign(Align::End)
        .build();
    value_lbl.add_css_class("settings-value");

    header.append(&title);
    header.append(&value_lbl);

    let scale = Scale::with_range(Orientation::Horizontal, min, max, step);
    scale.set_value(initial);
    scale.set_draw_value(false);
    scale.set_hexpand(true);

    container.append(&header);
    container.append(&scale);
    (container, scale, value_lbl)
}

fn show_settings_popover(state: &Rc<LauncherState>, parent: &Button) {
    let popover = Popover::new();
    popover.set_parent(parent);
    popover.add_css_class("settings-popover");
    popover.set_has_arrow(false);

    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(10)
        .build();

    let title = Label::builder()
        .label(state.strings.settings)
        .halign(Align::Start)
        .build();
    title.add_css_class("settings-title");
    root.append(&title);

    let cfg = state.config.borrow().clone();

    // 1. Background opacity (blur visibility)
    let (op_row, op_scale, op_lbl) = make_slider_row(
        state.strings.background_opacity,
        0.0,
        0.85,
        0.05,
        cfg.background_opacity,
        |v| format!("{}%", (v * 100.0).round() as i32),
    );
    {
        let state_c = Rc::clone(state);
        let lbl = op_lbl.clone();
        op_scale.connect_value_changed(move |s| {
            let val = (s.value() * 20.0).round() / 20.0;
            lbl.set_label(&format!("{}%", (val * 100.0).round() as i32));
            {
                let mut c = state_c.config.borrow_mut();
                c.background_opacity = val;
                c.save();
            }
            let layout = current_layout(&state_c);
            apply_dynamic_css(&state_c.dynamic_css, &state_c.config.borrow(), &layout);
        });
    }
    root.append(&op_row);

    // 2. Icon size
    let (icon_row, icon_scale, icon_lbl) = make_slider_row(
        state.strings.icon_size,
        44.0,
        112.0,
        4.0,
        cfg.icon_size as f64,
        |v| format!("{} px", v.round() as i32),
    );
    {
        let state_c = Rc::clone(state);
        let lbl = icon_lbl.clone();
        icon_scale.connect_value_changed(move |s| {
            let val = s.value().round() as i32;
            lbl.set_label(&format!("{val} px"));
            {
                let mut c = state_c.config.borrow_mut();
                c.icon_size = val;
                c.save();
            }
            sync_layout(&state_c);
        });
    }
    root.append(&icon_row);

    // 3. Columns
    let (cols_row, cols_scale, cols_lbl) = make_slider_row(
        state.strings.columns,
        4.0,
        10.0,
        1.0,
        cfg.columns as f64,
        |v| format!("{}", v.round() as usize),
    );
    {
        let state_c = Rc::clone(state);
        let lbl = cols_lbl.clone();
        cols_scale.connect_value_changed(move |s| {
            let val = s.value().round() as usize;
            lbl.set_label(&format!("{val}"));
            {
                let mut c = state_c.config.borrow_mut();
                c.columns = val;
                c.save();
            }
            sync_layout(&state_c);
        });
    }
    root.append(&cols_row);

    // 4. Rows
    let (rows_row, rows_scale, rows_lbl) = make_slider_row(
        state.strings.rows,
        2.0,
        6.0,
        1.0,
        cfg.rows as f64,
        |v| format!("{}", v.round() as usize),
    );
    {
        let state_c = Rc::clone(state);
        let lbl = rows_lbl.clone();
        rows_scale.connect_value_changed(move |s| {
            let val = s.value().round() as usize;
            lbl.set_label(&format!("{val}"));
            {
                let mut c = state_c.config.borrow_mut();
                c.rows = val;
                c.save();
            }
            sync_layout(&state_c);
        });
    }
    root.append(&rows_row);

    // 5. Spacing
    let (sp_row, sp_scale, sp_lbl) = make_slider_row(
        state.strings.spacing,
        8.0,
        36.0,
        2.0,
        cfg.spacing as f64,
        |v| format!("{} px", v.round() as i32),
    );
    {
        let state_c = Rc::clone(state);
        let lbl = sp_lbl.clone();
        sp_scale.connect_value_changed(move |s| {
            let val = s.value().round() as i32;
            lbl.set_label(&format!("{val} px"));
            {
                let mut c = state_c.config.borrow_mut();
                c.spacing = val;
                c.save();
            }
            sync_layout(&state_c);
        });
    }
    root.append(&sp_row);

    // 6. Auto screen scaling switch
    let scale_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(12)
        .hexpand(true)
        .margin_top(4)
        .margin_bottom(2)
        .build();
    let scale_lbl = Label::builder()
        .label(state.strings.auto_scale)
        .halign(Align::Start)
        .hexpand(true)
        .build();
    scale_lbl.add_css_class("settings-label");
    let scale_switch = Switch::builder()
        .active(cfg.auto_scale)
        .valign(Align::Center)
        .build();
    {
        let state_c = Rc::clone(state);
        scale_switch.connect_state_set(move |_, active| {
            {
                let mut c = state_c.config.borrow_mut();
                c.auto_scale = active;
                c.save();
            }
            sync_layout(&state_c);
            glib::Propagation::Proceed
        });
    }
    scale_row.append(&scale_lbl);
    scale_row.append(&scale_switch);
    root.append(&scale_row);

    // 7. Auto Steam folder switch
    let steam_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(12)
        .hexpand(true)
        .margin_top(2)
        .margin_bottom(4)
        .build();
    let steam_lbl = Label::builder()
        .label(state.strings.auto_steam_folder)
        .halign(Align::Start)
        .hexpand(true)
        .build();
    steam_lbl.add_css_class("settings-label");
    let steam_switch = Switch::builder()
        .active(cfg.auto_steam_folder)
        .valign(Align::Center)
        .build();
    {
        let state_c = Rc::clone(state);
        steam_switch.connect_state_set(move |_, active| {
            {
                let mut c = state_c.config.borrow_mut();
                c.auto_steam_folder = active;
                c.save();
            }
            if active {
                let steam_ids: Vec<String> = state_c
                    .all_apps
                    .iter()
                    .filter(|a| a.is_steam_game)
                    .map(|a| a.id.clone())
                    .collect();
                state_c.folders.borrow_mut().sync_steam_folder(&steam_ids);
            } else {
                state_c.folders.borrow_mut().dissolve_folder("auto-steam");
            }
            refresh_grid(&state_c);
            glib::Propagation::Proceed
        });
    }
    steam_row.append(&steam_lbl);
    steam_row.append(&steam_switch);
    root.append(&steam_row);

    // Reset button & config path hint
    let reset_btn = Button::with_label(state.strings.reset_defaults);
    reset_btn.add_css_class("settings-reset-btn");
    {
        let state_c = Rc::clone(state);
        let op_s = op_scale.clone();
        let ic_s = icon_scale.clone();
        let co_s = cols_scale.clone();
        let ro_s = rows_scale.clone();
        let sp_s = sp_scale.clone();
        let sc_sw = scale_switch.clone();
        let st_sw = steam_switch.clone();
        reset_btn.connect_clicked(move |_| {
            let def = Config::default();
            {
                let mut c = state_c.config.borrow_mut();
                *c = def.clone();
                c.save();
            }
            op_s.set_value(def.background_opacity);
            ic_s.set_value(def.icon_size as f64);
            co_s.set_value(def.columns as f64);
            ro_s.set_value(def.rows as f64);
            sp_s.set_value(def.spacing as f64);
            sc_sw.set_active(def.auto_scale);
            st_sw.set_active(def.auto_steam_folder);
            sync_layout(&state_c);
        });
    }
    root.append(&reset_btn);

    let path_lbl = Label::builder()
        .label(state.strings.config_hint)
        .halign(Align::Start)
        .build();
    path_lbl.add_css_class("settings-path");
    root.append(&path_lbl);

    popover.set_child(Some(&root));
    popover.popup();
}
