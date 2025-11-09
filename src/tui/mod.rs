use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{stdout, Stdout},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use crate::{cli_config, theme::Theme};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Local};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Frame, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use serde_yaml::Value;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
struct FolderEntry {
    path: PathBuf,
    name: String,
    depth: usize,
    parent: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct NoteEntry {
    path: PathBuf,
    name: String,
    modified: Option<DateTime<Local>>,
    tags: Vec<String>,
}

impl NoteEntry {
    fn formatted_modified(&self) -> Option<String> {
        self.modified
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Folders,
    Notes,
    Viewer,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Focus::Folders => Focus::Notes,
            Focus::Notes => Focus::Viewer,
            Focus::Viewer => Focus::Folders,
        }
    }

    fn prev(self) -> Self {
        match self {
            Focus::Folders => Focus::Viewer,
            Focus::Notes => Focus::Folders,
            Focus::Viewer => Focus::Notes,
        }
    }
}

enum AppAction {
    Continue,
    Quit,
    Open { editor: String, note: PathBuf },
}

pub struct AppState {
    vault_path: PathBuf,
    theme: Theme,
    editor_command: Option<String>,
    folders: Vec<FolderEntry>,
    folder_index: HashMap<PathBuf, usize>,
    expanded: HashSet<PathBuf>,
    selected_folder: PathBuf,
    notes_cache: HashMap<PathBuf, Vec<NoteEntry>>,
    selected_note: Option<usize>,
    focus: Focus,
    note_preview: String,
    base_status: String,
    status: String,
}

impl AppState {
    fn new(vault_path: PathBuf, theme: Theme, editor_command: Option<String>) -> Result<Self> {
        let folders = build_folder_entries(&vault_path)?;
        let mut folder_index = HashMap::new();
        for (idx, folder) in folders.iter().enumerate() {
            folder_index.insert(folder.path.clone(), idx);
        }

        let expanded = initialize_expanded_folders(&folders, &vault_path);

        let selected_folder = folders
            .first()
            .map(|f| f.path.clone())
            .unwrap_or_else(|| vault_path.clone());

        let mut notes_cache = HashMap::new();
        let mut selected_note = None;
        ensure_notes_loaded(&mut notes_cache, &selected_folder)?;
        if let Some(notes) = notes_cache.get(&selected_folder) {
            if !notes.is_empty() {
                selected_note = Some(0);
            }
        }

        let mut app = Self {
            vault_path,
            theme,
            editor_command,
            folders,
            folder_index,
            expanded,
            selected_folder,
            notes_cache,
            selected_note,
            focus: Focus::Folders,
            note_preview: String::new(),
            base_status: String::new(),
            status: String::new(),
        };
        let base_status = app.default_status_message();
        app.base_status = base_status.clone();
        app.status = base_status;
        app.refresh_note_preview();
        Ok(app)
    }

    fn current_folder_entry(&self) -> Option<&FolderEntry> {
        self.folder_index
            .get(&self.selected_folder)
            .and_then(|idx| self.folders.get(*idx))
    }

    fn has_folder_children(&self, path: &Path) -> bool {
        self.folders
            .iter()
            .any(|folder| folder.parent.as_ref().map(|p| p == path).unwrap_or(false))
    }

    fn is_folder_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }

    fn visible_folders(&self) -> Vec<&FolderEntry> {
        self.folders
            .iter()
            .filter(|folder| self.is_folder_visible(folder))
            .collect()
    }

    fn is_folder_visible(&self, folder: &FolderEntry) -> bool {
        match &folder.parent {
            None => true,
            Some(parent) => {
                if !self.expanded.contains(parent) {
                    return false;
                }
                if let Some(idx) = self.folder_index.get(parent) {
                    if let Some(parent_entry) = self.folders.get(*idx) {
                        return self.is_folder_visible(parent_entry);
                    }
                }
                true
            }
        }
    }

    fn visible_folder_index(&self) -> Option<usize> {
        let visible = self.visible_folders();
        visible
            .iter()
            .position(|folder| folder.path == self.selected_folder)
    }

    fn move_folder_selection(&mut self, delta: isize) -> Result<()> {
        let visible = self.visible_folders();
        if visible.is_empty() {
            return Ok(());
        }
        let current_index = self.visible_folder_index().unwrap_or(0);
        let len = visible.len() as isize;
        let next_index = (current_index as isize + delta).clamp(0, len - 1) as usize;
        let next_path = visible[next_index].path.clone();
        if next_path != self.selected_folder {
            self.select_folder(next_path)?;
        }
        Ok(())
    }

    fn select_folder(&mut self, path: PathBuf) -> Result<()> {
        ensure_notes_loaded(&mut self.notes_cache, &path)?;
        self.selected_folder = path.clone();
        let notes = self.notes_cache.get(&path);
        self.selected_note = notes.and_then(|entries| (!entries.is_empty()).then_some(0));
        self.refresh_note_preview();
        Ok(())
    }

    fn expand_selected_folder(&mut self) {
        self.expanded.insert(self.selected_folder.clone());
    }

    fn collapse_selected_folder(&mut self) -> Result<()> {
        if self.expanded.remove(&self.selected_folder) {
            return Ok(());
        }
        if let Some(entry) = self.current_folder_entry() {
            if let Some(parent) = &entry.parent {
                self.select_folder(parent.clone())?;
            }
        }
        Ok(())
    }

    fn notes_for_selected_folder(&self) -> &[NoteEntry] {
        self.notes_cache
            .get(&self.selected_folder)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    fn move_note_selection(&mut self, delta: isize) {
        let notes = self.notes_for_selected_folder();
        if notes.is_empty() {
            self.selected_note = None;
            self.note_preview.clear();
            return;
        }
        let current = self.selected_note.unwrap_or(0) as isize;
        let max = notes.len() as isize - 1;
        let next = (current + delta).clamp(0, max) as usize;
        self.selected_note = Some(next);
        self.refresh_note_preview();
    }

    fn selected_note_entry(&self) -> Option<&NoteEntry> {
        self.selected_note
            .and_then(|idx| self.notes_for_selected_folder().get(idx))
    }

    fn selected_note_path(&self) -> Option<PathBuf> {
        self.selected_note_entry().map(|note| note.path.clone())
    }

    fn prepare_open_action(&mut self) -> Result<Option<AppAction>> {
        let Some(path) = self.selected_note_path() else {
            self.set_status("Select a note to open");
            return Ok(None);
        };

        let editor = match self.editor_command.clone() {
            Some(command) => command,
            None => match cli_config::resolve_editor() {
                Ok(command) => {
                    self.editor_command = Some(command.clone());
                    command
                }
                Err(err) => {
                    self.set_status(err.to_string());
                    return Ok(None);
                }
            },
        };

        Ok(Some(AppAction::Open { editor, note: path }))
    }

    fn refresh_after_external_edit(&mut self, note_path: &Path) -> Result<()> {
        self.notes_cache.remove(&self.selected_folder);
        ensure_notes_loaded(&mut self.notes_cache, &self.selected_folder)?;

        if let Some(entries) = self.notes_cache.get(&self.selected_folder) {
            if let Some(idx) = entries.iter().position(|note| note.path == note_path) {
                self.selected_note = Some(idx);
            } else if entries.is_empty() {
                self.selected_note = None;
            } else {
                self.selected_note = Some(0);
            }
        } else {
            self.selected_note = None;
        }

        self.refresh_note_preview();
        Ok(())
    }

    fn refresh_note_preview(&mut self) {
        if let Some(path) = self.selected_note_path() {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    self.note_preview = content;
                }
                Err(err) => {
                    self.note_preview = format!("Failed to read note {}: {}", path.display(), err);
                }
            }
        } else {
            self.note_preview = String::from("Select a note to preview");
        }
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }

    fn reset_status(&mut self) {
        self.status = self.base_status.clone();
    }

    fn default_status_message(&self) -> String {
        let vault_name = self
            .vault_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.vault_path.to_string_lossy().into_owned());
        format!(
            "Vault: {} • ↑/↓ navigate • ←/→ fold • Enter open • Tab switch panel • q quit",
            vault_name
        )
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<AppAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(AppAction::Continue);
        }
        match key.code {
            KeyCode::Char('q') => return Ok(AppAction::Quit),
            KeyCode::Tab => {
                self.focus = self.focus.next();
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
            }
            KeyCode::Up => match self.focus {
                Focus::Folders => {
                    if let Err(err) = self.move_folder_selection(-1) {
                        self.set_status(err.to_string());
                    }
                }
                Focus::Notes => self.move_note_selection(-1),
                Focus::Viewer => {}
            },
            KeyCode::Down => match self.focus {
                Focus::Folders => {
                    if let Err(err) = self.move_folder_selection(1) {
                        self.set_status(err.to_string());
                    }
                }
                Focus::Notes => self.move_note_selection(1),
                Focus::Viewer => {}
            },
            KeyCode::Left => {
                if matches!(self.focus, Focus::Folders) {
                    if let Err(err) = self.collapse_selected_folder() {
                        self.set_status(err.to_string());
                    }
                }
            }
            KeyCode::Right => {
                if matches!(self.focus, Focus::Folders) {
                    self.expand_selected_folder();
                }
            }
            KeyCode::Enter => match self.focus {
                Focus::Folders => {
                    self.expand_selected_folder();
                    self.focus = Focus::Notes;
                }
                Focus::Notes | Focus::Viewer => {
                    if let Some(action) = self.prepare_open_action()? {
                        return Ok(action);
                    }
                }
            },
            KeyCode::Char('e') | KeyCode::Char('o') => {
                if let Some(action) = self.prepare_open_action()? {
                    return Ok(action);
                }
            }
            KeyCode::Char('n') | KeyCode::Char('d') => {
                self.set_status("Action not implemented yet");
            }
            KeyCode::Char('/') => {
                self.set_status("Search is not implemented yet");
            }
            KeyCode::Esc => {
                self.focus = Focus::Folders;
                self.reset_status();
            }
            _ => {}
        }
        Ok(AppAction::Continue)
    }
}

fn initialize_expanded_folders(folders: &[FolderEntry], vault_path: &Path) -> HashSet<PathBuf> {
    let mut expanded = HashSet::new();
    expanded.insert(vault_path.to_path_buf());
    for entry in folders.iter().filter(|entry| entry.depth <= 1) {
        expanded.insert(entry.path.clone());
    }
    expanded
}

fn build_folder_entries(vault_path: &Path) -> Result<Vec<FolderEntry>> {
    let mut entries = Vec::new();
    let walker = WalkDir::new(vault_path).into_iter();
    for entry in walker.filter_entry(|e| should_visit_dir(e)) {
        let entry = entry?;
        if entry.file_type().is_dir() {
            let depth = entry.depth();
            let path = entry.path().to_path_buf();
            let name = if depth == 0 {
                vault_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| vault_path.to_string_lossy().into_owned())
            } else {
                entry.file_name().to_string_lossy().into_owned()
            };
            let parent = path.parent().map(|p| p.to_path_buf());
            entries.push(FolderEntry {
                path,
                name,
                depth,
                parent,
            });
        }
    }
    Ok(entries)
}

fn should_visit_dir(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    if entry.file_type().is_dir() {
        let name = entry.file_name().to_string_lossy();
        return !name.starts_with('.') && name != "node_modules";
    }
    true
}

fn ensure_notes_loaded(cache: &mut HashMap<PathBuf, Vec<NoteEntry>>, folder: &Path) -> Result<()> {
    if cache.contains_key(folder) {
        return Ok(());
    }
    let notes = read_notes(folder)?;
    cache.insert(folder.to_path_buf(), notes);
    Ok(())
}

fn read_notes(folder: &Path) -> Result<Vec<NoteEntry>> {
    let mut entries = Vec::new();
    if folder.is_dir() {
        for entry in
            fs::read_dir(folder).with_context(|| format!("failed to read {}", folder.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if is_markdown(&path) {
                entries.push(build_note_entry(path)?);
            }
        }
    }
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(entries)
}

fn build_note_entry(path: PathBuf) -> Result<NoteEntry> {
    let metadata = fs::metadata(&path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    let modified = metadata.modified().ok().map(DateTime::<Local>::from);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();

    let content = fs::read_to_string(&path).unwrap_or_default();
    let tags = extract_tags(&content);

    Ok(NoteEntry {
        path,
        name,
        modified,
        tags,
    })
}

fn extract_tags(content: &str) -> Vec<String> {
    let mut lines = content.lines();
    match lines.next() {
        Some(line) if line.trim() == "---" => {}
        _ => return Vec::new(),
    }

    let mut front_matter = String::new();
    for line in lines.by_ref() {
        if line.trim() == "---" {
            break;
        }
        front_matter.push_str(line);
        front_matter.push('\n');
    }

    if front_matter.is_empty() {
        return Vec::new();
    }

    let Ok(value) = serde_yaml::from_str::<Value>(&front_matter) else {
        return Vec::new();
    };

    match value.get("tags") {
        Some(Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|item| item.as_str().map(|s| s.to_string()))
            .collect(),
        Some(Value::String(tag)) => vec![tag.clone()],
        _ => Vec::new(),
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

pub fn run(vault_path: PathBuf) -> Result<()> {
    let (theme, editor_command) = match cli_config::read() {
        Ok(cfg) => (cfg.theme.resolve(), cfg.editor.clone()),
        Err(_) => (Theme::default(), None),
    };

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let res = run_app(&mut terminal, vault_path, theme, editor_command);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    vault_path: PathBuf,
    theme: Theme,
    editor_command: Option<String>,
) -> Result<()> {
    let mut app = AppState::new(vault_path, theme, editor_command)?;

    loop {
        terminal.draw(|f| draw(f, &app))?;

        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(key) => match app.handle_key(key)? {
                    AppAction::Quit => break,
                    AppAction::Continue => {}
                    AppAction::Open { editor, note } => {
                        suspend_terminal(terminal)?;
                        let launch_result = launch_editor(&editor, &note);
                        resume_terminal(terminal)?;

                        match launch_result {
                            Ok(()) => {
                                if let Err(err) = app.refresh_after_external_edit(&note) {
                                    app.set_status(err.to_string());
                                } else {
                                    let display = note
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|| note.display().to_string());
                                    app.set_status(format!("Opened {display} with {editor}"));
                                }
                            }
                            Err(err) => {
                                app.set_status(err.to_string());
                            }
                        }
                    }
                },
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    Ok(())
}

fn suspend_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn resume_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    terminal.hide_cursor()?;
    terminal.clear()?;
    Ok(())
}

fn launch_editor(editor: &str, note: &Path) -> Result<()> {
    let status = Command::new(editor)
        .arg(note)
        .status()
        .with_context(|| format!("failed to execute editor `{editor}`"))?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("Editor exited with status {status}"))
    }
}

fn draw(frame: &mut Frame, app: &AppState) {
    let full = frame.size();
    frame.render_widget(
        Block::default().style(Style::default().bg(app.theme.background)),
        full,
    );

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(full);

    let body_area = vertical[0];
    let status_area = vertical[1];

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(body_area);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(columns[0]);

    render_folders(frame, left[0], app);
    render_notes(frame, left[1], app);
    render_viewer(frame, columns[1], app);
    render_status(frame, status_area, app);
}

fn render_folders(frame: &mut Frame, area: Rect, app: &AppState) {
    let title = Line::from("Folders");
    let mut items: Vec<ListItem> = Vec::new();
    let theme = &app.theme;
    for folder in app.visible_folders() {
        let indent_level = folder.depth.saturating_sub(1);
        let indent = "  ".repeat(indent_level);
        let has_children = app.has_folder_children(&folder.path);
        let symbol = if has_children {
            if app.is_folder_expanded(&folder.path) {
                "▼ "
            } else {
                "▶ "
            }
        } else {
            "  "
        };
        let text = format!("{indent}{symbol}{}", folder.name);
        items.push(ListItem::new(Line::from(Span::styled(
            text,
            Style::default().fg(theme.folder).bg(theme.background),
        ))));
    }

    let mut state = ListState::default();
    if let Some(selected) = app.visible_folder_index() {
        state.select(Some(selected));
    }

    let highlight = Style::default()
        .fg(theme.accent)
        .bg(theme.background)
        .add_modifier(Modifier::BOLD);

    let block_style = if app.focus == Focus::Folders {
        Style::default().fg(theme.accent).bg(theme.background)
    } else {
        Style::default().bg(theme.background)
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(block_style),
        )
        .highlight_style(highlight);

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_notes(frame: &mut Frame, area: Rect, app: &AppState) {
    let notes = app.notes_for_selected_folder();
    let theme = &app.theme;
    let mut items = Vec::new();
    for note in notes {
        let mut spans = vec![Span::styled(
            note.name.clone(),
            Style::default().fg(theme.note).bg(theme.background),
        )];
        if let Some(modified) = note.formatted_modified() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                modified,
                Style::default().fg(theme.modified).bg(theme.background),
            ));
        }
        if !note.tags.is_empty() {
            let tag_text = format!("  #{}", note.tags.join(" #"));
            spans.push(Span::styled(
                tag_text,
                Style::default().fg(theme.tag).bg(theme.background),
            ));
        }
        items.push(ListItem::new(Line::from(spans)));
    }

    if items.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "(no notes)",
            Style::default().fg(theme.note).bg(theme.background),
        ))));
    }

    let mut state = ListState::default();
    if let Some(selected) = app.selected_note {
        state.select(Some(selected));
    }

    let highlight = Style::default()
        .fg(theme.accent)
        .bg(theme.background)
        .add_modifier(Modifier::BOLD);

    let block_style = if app.focus == Focus::Notes {
        Style::default().fg(theme.accent).bg(theme.background)
    } else {
        Style::default().bg(theme.background)
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Notes")
                .style(block_style),
        )
        .highlight_style(highlight);

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_viewer(frame: &mut Frame, area: Rect, app: &AppState) {
    let theme = &app.theme;
    let block_style = if app.focus == Focus::Viewer {
        Style::default().fg(theme.accent).bg(theme.background)
    } else {
        Style::default().bg(theme.background)
    };

    let paragraph = Paragraph::new(app.note_preview.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Preview")
                .style(block_style),
        )
        .style(Style::default().fg(theme.note).bg(theme.background));

    frame.render_widget(paragraph, area);
}

fn render_status(frame: &mut Frame, area: Rect, app: &AppState) {
    let theme = &app.theme;
    let paragraph = Paragraph::new(app.status.as_str())
        .style(Style::default().fg(theme.note).bg(theme.background));
    frame.render_widget(paragraph, area);
}
