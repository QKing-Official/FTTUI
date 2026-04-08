use std::{
    collections::HashMap,
    fs,
    io,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Local;
use color_eyre::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    prelude::*,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Terminal,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// Windows fix
#[cfg(windows)]
const DOUBLE_KEY_FIX: bool = true;
#[cfg(not(windows))]
const DOUBLE_KEY_FIX: bool = false;

mod flexible {
    use serde::{Deserialize, Deserializer};
    pub fn u64_flex<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        use serde::de::Error;
        let v = serde_json::Value::deserialize(d)?;
        match v {
            serde_json::Value::Number(n) => n
                .as_u64()
                .or_else(|| n.as_f64().map(|f| f as u64))
                .ok_or_else(|| D::Error::custom("bad number")),
            serde_json::Value::String(s) => s.parse::<u64>().map_err(D::Error::custom),
            serde_json::Value::Null => Ok(0),
            _ => Err(D::Error::custom("expected number")),
        }
    }
    pub fn f64_as_u64<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        use serde::de::Error;
        let v = serde_json::Value::deserialize(d)?;
        match v {
            serde_json::Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0) as u64),
            serde_json::Value::String(s) => {
                s.parse::<f64>().map(|f| f as u64).map_err(D::Error::custom)
            }
            serde_json::Value::Null => Ok(0),
            _ => Err(D::Error::custom("expected number")),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Config {
    refresh_seconds: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self { refresh_seconds: 30 }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir().unwrap().join("fttui/config.json")
}
fn load_config() -> Config {
    let path = config_path();
    if !path.exists() {
        let cfg = Config::default();
        fs::create_dir_all(path.parent().unwrap()).ok();
        fs::write(&path, serde_json::to_vec_pretty(&cfg).unwrap()).ok();
        return cfg;
    }
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
const HOT: &str = "http://ftpdb.jam06452.uk/api/hot";
const WEEK: &str = "http://ftpdb.jam06452.uk/api/top_this_week";
const ALL: &str = "http://ftpdb.jam06452.uk/api/top_all_time";
const RANDOM: &str = "http://ftpdb.jam06452.uk/api/random_projects?filter=stat_hot_score";

#[derive(Serialize, Deserialize, Default, Clone)]
struct Project {
    id: Option<String>,
    title: Option<String>,
    banner_url: Option<String>,
    #[serde(default, deserialize_with = "flexible::u64_flex")]
    total_hours: u64,
    #[serde(default, alias = "stat_total_likes", deserialize_with = "flexible::f64_as_u64")]
    total_likes: u64,
    user_id: Option<String>,
    display_name: Option<String>,
    avatar_url: Option<String>,
    slack_id: Option<String>,
    #[serde(default, alias = "stat_hot_score", deserialize_with = "flexible::f64_as_u64")]
    hot_score: u64,
    #[serde(default)]
    likes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ProjectDetail {
    id: String,
    title: String,
    banner_url: Option<String>,
    total_hours: u64,
    total_likes: u64,
    user_id: String,
    display_name: String,
    avatar_url: Option<String>,
    slack_id: Option<String>,
    description: Option<String>,
    repo_url: Option<String>,
    demo_url: Option<String>,
    ship_status: Option<String>,
}

#[derive(Deserialize)]
struct ProjectInfoRaw(ProjectInfoPart, ProjectExtraPart);

#[derive(Deserialize)]
struct ProjectInfoPart {
    #[serde(default)]
    id: String,
    title: String,
    banner_url: Option<String>,
    #[serde(deserialize_with = "flexible::u64_flex")]
    total_hours: u64,
    #[serde(deserialize_with = "flexible::u64_flex")]
    total_likes: u64,
    user_id: String,
    display_name: String,
    avatar_url: Option<String>,
    slack_id: Option<String>,
}

#[derive(Deserialize)]
struct ProjectExtraPart {
    description: Option<String>,
    repo_url: Option<String>,
    demo_url: Option<String>,
    ship_status: Option<String>,
}

#[derive(Default, Clone)]
struct ApiCache {
    hot: Vec<Project>,
    week: Vec<Project>,
    all: Vec<Project>,
    random: Vec<Project>,
    details: HashMap<String, ProjectDetail>,
}

type SharedState = Arc<RwLock<ApiCache>>;

#[derive(Default)]
struct UIState {
    selected_panel: usize,
    selected_indices: [usize; 4],
    scroll_offsets: [usize; 4],
    detail_mode: bool,
    detail_project_id: Option<String>,
    detail_info: Option<ProjectDetail>,
    detail_loading: bool,
    detail_error: bool,
    detail_error_body: Option<String>,
    detail_scroll: usize,
    clock: String,
}

fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap().join("fttui")
}
fn cache_file(name: &str) -> PathBuf {
    cache_dir().join(format!("{name}.json"))
}

fn read_cache_sync(name: &str) -> Vec<Project> {
    fs::read_to_string(cache_file(name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_cache_sync(name: &str, projects: &[Project]) {
    fs::create_dir_all(cache_dir()).ok();
    if let Ok(bytes) = serde_json::to_vec(projects) {
        fs::write(cache_file(name), bytes).ok();
    }
}

async fn preload_from_disk(state: &SharedState) {
    let (hot, week, all, random) = tokio::join!(
        tokio::task::spawn_blocking(|| read_cache_sync("hot")),
        tokio::task::spawn_blocking(|| read_cache_sync("week")),
        tokio::task::spawn_blocking(|| read_cache_sync("all")),
        tokio::task::spawn_blocking(|| read_cache_sync("random")),
    );
    let mut s = state.write().await;
    if let Ok(v) = hot {
        if !v.is_empty() {
            s.hot = v;
        }
    }
    if let Ok(v) = week {
        if !v.is_empty() {
            s.week = v;
        }
    }
    if let Ok(v) = all {
        if !v.is_empty() {
            s.all = v;
        }
    }
    if let Ok(v) = random {
        if !v.is_empty() {
            s.random = v;
        }
    }
}

fn parse_projects(bytes: &[u8]) -> Option<Vec<Project>> {
    if let Ok(mut arr) = serde_json::from_slice::<Vec<Project>>(bytes) {
        arr.sort_unstable_by(|a, b| b.hot_score.cmp(&a.hot_score));
        return Some(arr);
    }
    if let Ok(map) = serde_json::from_slice::<HashMap<String, Project>>(bytes) {
        let mut projects: Vec<Project> = map.into_values().collect();
        projects.sort_unstable_by(|a, b| b.hot_score.cmp(&a.hot_score));
        return Some(projects);
    }
    None
}

async fn fetch_list(client: &reqwest::Client, url: &str, name: &str) -> Vec<Project> {
    let name = name.to_string();
    let bytes = match client.get(url).send().await {
        Ok(resp) => match resp.bytes().await {
            Ok(b) => b,
            Err(_) => {
                let n = name.clone();
                return tokio::task::spawn_blocking(move || read_cache_sync(&n))
                    .await
                    .unwrap_or_default();
            }
        },
        Err(_) => {
            let n = name.clone();
            return tokio::task::spawn_blocking(move || read_cache_sync(&n))
                .await
                .unwrap_or_default();
        }
    };

    match parse_projects(&bytes) {
        Some(projects) => {
            let existing_len = {
                let n = name.clone();
                tokio::task::spawn_blocking(move || read_cache_sync(&n).len())
                    .await
                    .unwrap_or(0)
            };
            if existing_len != projects.len() {
                let to_write = projects.clone();
                tokio::task::spawn_blocking(move || write_cache_sync(&name, &to_write))
                    .await
                    .ok();
            }
            projects
        }
        None => tokio::task::spawn_blocking(move || read_cache_sync(&name))
            .await
            .unwrap_or_default(),
    }
}

async fn update_all(client: Arc<reqwest::Client>, state: &SharedState) {
    let (hot, week, all, random) = tokio::join!(
        fetch_list(&client, HOT, "hot"),
        fetch_list(&client, WEEK, "week"),
        fetch_list(&client, ALL, "all"),
        fetch_list(&client, RANDOM, "random"),
    );
    let mut s = state.write().await;
    s.hot = hot;
    s.week = week;
    s.all = all;
    s.random = random;
}

async fn updater(client: Arc<reqwest::Client>, state: SharedState, refresh: u64) {
    loop {
        update_all(client.clone(), &state).await;
        tokio::time::sleep(Duration::from_secs(refresh)).await;
    }
}

async fn fetch_project_detail(client: &reqwest::Client, id: &str) -> Result<ProjectDetail, String> {
    let url = format!("http://ftpdb.jam06452.uk/api/project_info/{}", id);
    let bytes = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    if let Ok(raw) = serde_json::from_slice::<ProjectInfoRaw>(&bytes) {
        let (info, extra) = (raw.0, raw.1);
        return Ok(ProjectDetail {
            id: if info.id.is_empty() {
                id.to_string()
            } else {
                info.id
            },
            title: info.title,
            banner_url: info.banner_url,
            total_hours: info.total_hours,
            total_likes: info.total_likes,
            user_id: info.user_id,
            display_name: info.display_name,
            avatar_url: info.avatar_url,
            slack_id: info.slack_id,
            description: extra.description,
            repo_url: extra.repo_url,
            demo_url: extra.demo_url,
            ship_status: extra.ship_status,
        });
    }

    #[derive(Deserialize)]
    struct Full {
        #[serde(default)]
        id: String,
        title: String,
        banner_url: Option<String>,
        #[serde(deserialize_with = "flexible::u64_flex")]
        total_hours: u64,
        #[serde(deserialize_with = "flexible::u64_flex")]
        total_likes: u64,
        user_id: String,
        display_name: String,
        avatar_url: Option<String>,
        slack_id: Option<String>,
        description: Option<String>,
        repo_url: Option<String>,
        demo_url: Option<String>,
        ship_status: Option<String>,
    }
    if let Ok(mut arr) = serde_json::from_slice::<Vec<Full>>(&bytes) {
        if let Some(f) = arr.drain(..).next() {
            return Ok(ProjectDetail {
                id: if f.id.is_empty() {
                    id.to_string()
                } else {
                    f.id
                },
                title: f.title,
                banner_url: f.banner_url,
                total_hours: f.total_hours,
                total_likes: f.total_likes,
                user_id: f.user_id,
                display_name: f.display_name,
                avatar_url: f.avatar_url,
                slack_id: f.slack_id,
                description: f.description,
                repo_url: f.repo_url,
                demo_url: f.demo_url,
                ship_status: f.ship_status,
            });
        }
    }

    if let Ok(single) = serde_json::from_slice::<ProjectInfoPart>(&bytes) {
        return Ok(ProjectDetail {
            id: if single.id.is_empty() {
                id.to_string()
            } else {
                single.id
            },
            title: single.title,
            banner_url: single.banner_url,
            total_hours: single.total_hours,
            total_likes: single.total_likes,
            user_id: single.user_id,
            display_name: single.display_name,
            avatar_url: single.avatar_url,
            slack_id: single.slack_id,
            description: None,
            repo_url: None,
            demo_url: None,
            ship_status: None,
        });
    }

    let raw_str = String::from_utf8_lossy(&bytes);
    let e1 = serde_json::from_slice::<ProjectInfoRaw>(&bytes)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    let e2 = serde_json::from_slice::<Vec<serde_json::Value>>(&bytes)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    Err(format!(
        "shape1_err: {}\nshape2_arr_err: {}\nraw: {}",
        e1, e2, raw_str
    ))
}

fn lines_per_project(p: &Project) -> usize {
    if p.display_name.is_some() {
        3
    } else {
        2
    }
}

fn visible_project_count(projects: &[Project], offset: usize, height: usize) -> usize {
    let mut used = 0usize;
    let mut count = 0usize;
    for p in projects.iter().skip(offset) {
        let need = lines_per_project(p);
        if used + need > height {
            break;
        }
        used += need;
        count += 1;
    }
    count
}

fn render_panel<'a>(
    title: &'a str,
    projects: &'a [Project],
    is_selected: bool,
    selected_index: Option<usize>,
    scroll_offset: usize,
    height: u16,
) -> Paragraph<'a> {
    let mut lines: Vec<Line> = Vec::new();
    let mut line_count = 0usize;

    for (i, p) in projects.iter().enumerate().skip(scroll_offset) {
        let needed = lines_per_project(p);
        if line_count + needed > height as usize {
            break;
        }
        line_count += needed;

        let hl = Some(i) == selected_index;
        let base = if hl {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
            Style::default()
        };

        let title_str = p.title.as_deref().unwrap_or("No Title");
        let hours = p.total_hours;
        let likes = if p.total_likes > 0 {
            p.total_likes
        } else {
            p.likes.unwrap_or(0)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{} ", title_str), base.fg(Color::Yellow)),
            Span::styled(format!("[{}h, {} likes]", hours, likes), base.fg(Color::Gray)),
        ]));
        if let Some(user) = &p.display_name {
            lines.push(Line::from(Span::styled(
                format!("  by {}", user),
                Style::default().fg(Color::Cyan),
            )));
        }
        lines.push(Line::from(Span::raw("")));
    }

    let mut block = Block::default().title(title).borders(Borders::ALL);
    if is_selected {
        block = block.border_style(Style::default().fg(Color::Green));
    }
    Paragraph::new(lines).block(block).wrap(Wrap { trim: true })
}

struct PopupWidget {
    paragraph: Paragraph<'static>,
    popup_area: Rect,
    scroll: u16,
}

impl Widget for PopupWidget {
    fn render(self, _area: Rect, buf: &mut Buffer) {
        Clear.render(self.popup_area, buf);
        self.paragraph.scroll((self.scroll, 0)).render(self.popup_area, buf);
    }
}

fn render_detail_popup(
    area: Rect,
    project: &ProjectDetail,
    loading: bool,
    error: bool,
    error_body: Option<&str>,
    scroll: usize,
) -> PopupWidget {
    let popup_area = centered_rect(60, 70, area);

    let content: Vec<Line<'static>> = if loading {
        vec![Line::from("Loading project details...")]
    } else if error {
        let mut lines = vec![
            Line::from(Span::styled(
                "Failed to load project details.",
                Style::default().fg(Color::Red),
            )),
            Line::from(""),
        ];
        if let Some(body) = error_body {
            lines.push(Line::from(Span::styled(
                "Server response:",
                Style::default().fg(Color::Yellow),
            )));
            for chunk in wrap_text(body, 58) {
                lines.push(Line::from(Span::raw(chunk)));
            }
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            "↑/↓  scroll    Esc/Backspace/Enter  close",
            Style::default().fg(Color::DarkGray),
        )));
        lines
    } else {
        let mut lines: Vec<Line<'static>> = vec![
            Line::from(Span::styled(
                format!("Title:  {}", project.title),
                Style::default().fg(Color::Yellow),
            )),
            Line::from(Span::raw(format!("Hours:  {}", project.total_hours))),
            Line::from(Span::raw(format!("Likes:  {}", project.total_likes))),
            Line::from(Span::raw(format!("Author: {}", project.display_name))),
        ];
        if let Some(desc) = &project.description {
            lines.push(Line::from(Span::raw("")));
            for chunk in wrap_text(desc, 58) {
                lines.push(Line::from(Span::raw(chunk)));
            }
        }
        if let Some(repo) = &project.repo_url {
            lines.push(Line::from(Span::raw("")));
            lines.push(Line::from(Span::styled("Repo:", Style::default().fg(Color::Cyan))));
            lines.push(Line::from(Span::raw(repo.clone())));
        }
        if let Some(demo) = &project.demo_url {
            lines.push(Line::from(Span::raw("")));
            lines.push(Line::from(Span::styled("Demo:", Style::default().fg(Color::Cyan))));
            lines.push(Line::from(Span::raw(demo.clone())));
        }
        if let Some(status) = &project.ship_status {
            lines.push(Line::from(Span::raw("")));
            lines.push(Line::from(Span::raw(format!("Ship status: {}", status))));
        }
        lines.push(Line::from(Span::raw("")));
        lines.push(Line::from(Span::styled(
            "↑/↓  scroll    Esc/Backspace/Enter  close",
            Style::default().fg(Color::DarkGray),
        )));
        lines
    };

    let paragraph = Paragraph::new(content)
        .block(Block::default().title("Project Details").borders(Borders::ALL))
        .wrap(Wrap { trim: false });

    PopupWidget {
        paragraph,
        popup_area,
        scroll: scroll as u16,
    }
}

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() <= max_width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current.clone());
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

fn ui(frame: &mut Frame, state: &SharedState, ui_state: &Arc<RwLock<UIState>>) {
    let ui = match ui_state.try_read() {
        Ok(u) => u,
        Err(_) => return,
    };
    let data = match state.try_read() {
        Ok(d) => d,
        Err(_) => return,
    };

    let areas = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Fill(1),
    ])
    .split(frame.area());

    frame.render_widget(
        Block::default()
            .title(format!(
                " FlavorTown Project Viewer TUI (FTTUI)  |  API by jam06452  |  {}  |  q quit  r refresh  Tab panel  Enter details",
                ui.clock
            ))
            .borders(Borders::ALL),
        areas[0],
    );

    let top = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(areas[1]);
    let bottom = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(areas[2]);

    let panel_areas = [top[0], top[1], bottom[0], bottom[1]];
    let panel_heights = panel_areas.map(|a| a.height.saturating_sub(2));
    let panel_titles = ["Hot projects", "Top this week", "Top all time", "Random projects"];
    let lists: [&[Project]; 4] = [&data.hot, &data.week, &data.all, &data.random];

    for i in 0..4 {
        let selected_idx = if ui.selected_panel == i {
            Some(ui.selected_indices[i])
        } else {
            None
        };
        frame.render_widget(
            render_panel(
                panel_titles[i],
                lists[i],
                ui.selected_panel == i,
                selected_idx,
                ui.scroll_offsets[i],
                panel_heights[i],
            ),
            panel_areas[i],
        );
    }

    if ui.detail_mode {
        let default_detail = ProjectDetail::default();
        let detail_widget = if let Some(detail) = &ui.detail_info {
            render_detail_popup(frame.area(), detail, false, false, None, ui.detail_scroll)
        } else if ui.detail_loading {
            render_detail_popup(frame.area(), &default_detail, true, false, None, 0)
        } else if ui.detail_error {
            render_detail_popup(
                frame.area(),
                &default_detail,
                false,
                true,
                ui.detail_error_body.as_deref(),
                ui.detail_scroll,
            )
        } else {
            return;
        };
        frame.render_widget(detail_widget, frame.area());
    }
}

async fn handle_input(
    key: KeyCode,
    client: &Arc<reqwest::Client>,
    state: &SharedState,
    ui_state: &Arc<RwLock<UIState>>,
) -> Result<()> {
    let mut ui = ui_state.write().await;

    if ui.detail_mode {
        match key {
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Esc | KeyCode::Backspace => {
                ui.detail_mode = false;
                ui.detail_info = None;
                ui.detail_loading = false;
                ui.detail_error = false;
                ui.detail_error_body = None;
                ui.detail_project_id = None;
                ui.detail_scroll = 0;
            }
            KeyCode::Up | KeyCode::Char('w') => {
                ui.detail_scroll = ui.detail_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('s') => {
                ui.detail_scroll += 1;
            }
            _ => {}
        }
        return Ok(());
    }

    match key {
        KeyCode::Tab => {
            ui.selected_panel = (ui.selected_panel + 1) % 4;
        }
        KeyCode::Up | KeyCode::Char('w') => {
            let panel = ui.selected_panel;
            let len = {
                let d = state.read().await;
                get_list_len(&d, panel)
            };
            if len > 0 && ui.selected_indices[panel] > 0 {
                ui.selected_indices[panel] -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('s') => {
            let panel = ui.selected_panel;
            let len = {
                let d = state.read().await;
                get_list_len(&d, panel)
            };
            if len > 0 && ui.selected_indices[panel] < len - 1 {
                ui.selected_indices[panel] += 1;
            }
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            let panel = ui.selected_panel;
            let idx = ui.selected_indices[panel];

            let maybe_id: Option<String> = {
                let data = state.read().await;
                let list = get_list(&data, panel);
                list.get(idx).and_then(|p| p.id.clone())
            };

            if let Some(id) = maybe_id {
                let cached = {
                    let data = state.read().await;
                    data.details.get(&id).cloned()
                };

                if let Some(detail) = cached {
                    ui.detail_mode = true;
                    ui.detail_project_id = Some(id);
                    ui.detail_info = Some(detail);
                    ui.detail_loading = false;
                    ui.detail_error = false;
                    ui.detail_error_body = None;
                    ui.detail_scroll = 0;
                } else {
                    ui.detail_mode = true;
                    ui.detail_project_id = Some(id.clone());
                    ui.detail_loading = true;
                    ui.detail_error = false;
                    ui.detail_error_body = None;
                    ui.detail_info = None;
                    ui.detail_scroll = 0;

                    let ui_clone = ui_state.clone();
                    let state_clone = state.clone();
                    let client_clone = client.clone();
                    tokio::spawn(async move {
                        let result = fetch_project_detail(&client_clone, &id).await;
                        let mut g = ui_clone.write().await;
                        g.detail_loading = false;
                        match result {
                            Ok(d) => {
                                state_clone.write().await.details.insert(id, d.clone());
                                g.detail_info = Some(d);
                            }
                            Err(raw) => {
                                g.detail_error = true;
                                g.detail_error_body = Some(raw);
                            }
                        }
                    });
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn get_list_len(data: &ApiCache, panel: usize) -> usize {
    match panel {
        0 => data.hot.len(),
        1 => data.week.len(),
        2 => data.all.len(),
        3 => data.random.len(),
        _ => 0,
    }
}

fn get_list<'a>(data: &'a ApiCache, panel: usize) -> &'a [Project] {
    match panel {
        0 => &data.hot,
        1 => &data.week,
        2 => &data.all,
        3 => &data.random,
        _ => &[],
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let config = load_config();
    let state = Arc::new(RwLock::new(ApiCache::default()));
    let ui_state = Arc::new(RwLock::new(UIState::default()));

    let client = Arc::new(reqwest::Client::new());

    preload_from_disk(&state).await;

    tokio::spawn(updater(client.clone(), state.clone(), config.refresh_seconds));

    {
        let ui_clock = ui_state.clone();
        tokio::spawn(async move {
            loop {
                {
                    let mut ui = ui_clock.write().await;
                    ui.clock = Local::now().format("%d-%m-%Y %H:%M:%S").to_string();
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, client, state, ui_state).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    res
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    client: Arc<reqwest::Client>,
    state: SharedState,
    ui_state: Arc<RwLock<UIState>>,
) -> Result<()> {
    let tick_rate = Duration::from_millis(16);
    let mut last_tick = Instant::now();

    loop {
        {
            let mut ui = ui_state.write().await;
            let data = state.read().await;
            let full_rect = terminal.get_frame().area();

            let rows = Layout::vertical([
                Constraint::Length(3),
                Constraint::Fill(1),
                Constraint::Fill(1),
            ])
            .split(full_rect);
            let top_h = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);
            let bottom_h =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[2]);
            let panel_rects = [top_h[0], top_h[1], bottom_h[0], bottom_h[1]];

            for panel in 0..4usize {
                let projects = get_list(&data, panel);
                let avail_h = panel_rects[panel].height.saturating_sub(2) as usize;
                let selected = ui.selected_indices[panel];
                let scroll = &mut ui.scroll_offsets[panel];

                if *scroll >= projects.len() {
                    *scroll = projects.len().saturating_sub(1);
                }
                if selected < *scroll {
                    *scroll = selected;
                    continue;
                }
                let visible = visible_project_count(projects, *scroll, avail_h);
                let last_visible = scroll.saturating_add(visible).saturating_sub(1);
                if selected > last_visible {
                    while *scroll < projects.len() {
                        let v = visible_project_count(projects, *scroll, avail_h);
                        let last = scroll.saturating_add(v).saturating_sub(1);
                        if selected <= last {
                            break;
                        }
                        *scroll += 1;
                    }
                }
            }
        }

        terminal.draw(|f| ui(f, &state, &ui_state))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('r') => {
                            let c = client.clone();
                            let s = state.clone();
                            tokio::spawn(async move {
                                update_all(c, &s).await;
                            });
                        }
                        other => {
                            handle_input(other, &client, &state, &ui_state).await?;
                        }
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    Ok(())
}