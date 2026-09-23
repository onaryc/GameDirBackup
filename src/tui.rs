use file_tree_json::{build_file_node, to_tree, FileNode, RootNode, TraversalMode};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Terminal,
};
use std::{collections::HashSet, io::stdout, path::Path, time::Duration};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode { Flat, Tree }
impl ViewMode { fn label(self) -> &'static str { match self { Self::Flat => "Flat", Self::Tree => "Tree" } } }

#[derive(Clone)]
struct DisplayItem { name: String, path: String, is_directory: bool, depth: usize }

struct App {
    flat: RootNode,
    tree: RootNode,
    mode: ViewMode,
    expanded: HashSet<String>,
    display_items: Vec<DisplayItem>,
    dirty: bool,
    selected: usize,
    scroll_offset: usize,
    horizontal_offset: usize,
    max_line_width: usize,
    viewport_height: usize,
    viewport_width: usize,
    list_area: Rect,
}

impl App {
    fn new(path: &Path) -> Result<Self, std::io::Error> {
        let flat = build_file_node(path, TraversalMode::Parallel, false)?;
        let tree = to_tree(&flat);
        let mut app = Self {
            flat, tree, mode: ViewMode::Tree, expanded: HashSet::new(),
            display_items: Vec::new(), dirty: true, selected: 0,
            scroll_offset: 0, horizontal_offset: 0, max_line_width: 0,
            viewport_height: 1, viewport_width: 1, list_area: Rect::default(),
        };
        app.ensure_display_items();
        Ok(app)
    }

    fn ensure_display_items(&mut self) {
        if !self.dirty { return; }
        let selected_path = self.display_items.get(self.selected).map(|i| i.path.clone());
        self.display_items = match self.mode {
            ViewMode::Flat => self.flat.nodes.iter().map(|n| DisplayItem {
                name: n.name.clone(), path: n.path.clone(), is_directory: n.is_directory, depth: n.depth,
            }).collect(),
            ViewMode::Tree => {
                let mut items = Vec::new();
                Self::push_tree(&self.tree.nodes, &self.expanded, &mut items);
                items
            }
        };
        if self.mode == ViewMode::Flat { self.display_items.sort_by(|a, b| a.path.cmp(&b.path)); }
        self.max_line_width = self.display_items.iter().map(|i| self.item_width(i)).max().unwrap_or(0);
        self.selected = selected_path.and_then(|p| self.display_items.iter().position(|i| i.path == p))
            .unwrap_or(self.selected).min(self.display_items.len().saturating_sub(1));
        self.dirty = false;
        self.clamp_scroll();
        self.clamp_horizontal_scroll();
        self.ensure_selected_visible();
    }

    fn push_tree(nodes: &[FileNode], expanded: &HashSet<String>, out: &mut Vec<DisplayItem>) {
        for n in nodes {
            out.push(DisplayItem { name: n.name.clone(), path: n.path.clone(), is_directory: n.is_directory, depth: n.depth });
            if n.is_directory && expanded.contains(&n.path) { Self::push_tree(&n.children, expanded, out); }
        }
    }

    fn item_width(&self, item: &DisplayItem) -> usize {
        let prefix = 2;
        match self.mode {
            ViewMode::Tree => item.depth.saturating_sub(1) * 2 + prefix + item.name.chars().count(),
            ViewMode::Flat => prefix + item.path.chars().count(),
        }
    }

    fn mark_dirty(&mut self) { self.dirty = true; }
    fn toggle_mode(&mut self) { self.mode = match self.mode { ViewMode::Flat => ViewMode::Tree, ViewMode::Tree => ViewMode::Flat }; self.mark_dirty(); }

    fn toggle_expand_at(&mut self, index: usize) {
        if self.mode != ViewMode::Tree { return; }
        if let Some(item) = self.display_items.get(index).filter(|i| i.is_directory) {
            if !self.expanded.insert(item.path.clone()) { self.expanded.remove(&item.path); }
            self.mark_dirty();
        }
    }
    fn expand_selected(&mut self) { if self.mode == ViewMode::Tree { if let Some(i) = self.display_items.get(self.selected).filter(|i| i.is_directory) { if self.expanded.insert(i.path.clone()) { self.mark_dirty(); } } } }
    fn collapse_selected(&mut self) { if self.mode == ViewMode::Tree { if let Some(i) = self.display_items.get(self.selected).filter(|i| i.is_directory) { if self.expanded.remove(&i.path) { self.mark_dirty(); } } } }
    fn move_down(&mut self) { if self.selected + 1 < self.display_items.len() { self.selected += 1; self.ensure_selected_visible(); } }
    fn move_up(&mut self) { self.selected = self.selected.saturating_sub(1); self.ensure_selected_visible(); }
    fn page(&mut self, down: bool) { let n = self.viewport_height.max(1); self.selected = if down { (self.selected + n).min(self.display_items.len().saturating_sub(1)) } else { self.selected.saturating_sub(n) }; self.ensure_selected_visible(); }
    fn scroll_by(&mut self, delta: isize) { let max = self.display_items.len().saturating_sub(self.viewport_height.max(1)); self.scroll_offset = ((self.scroll_offset as isize + delta).max(0) as usize).min(max); }
    fn scroll_horizontal_by(&mut self, delta: isize) { let max = self.max_line_width.saturating_sub(self.viewport_width.max(1)); self.horizontal_offset = ((self.horizontal_offset as isize + delta).max(0) as usize).min(max); }
    fn clamp_scroll(&mut self) { self.scroll_offset = self.scroll_offset.min(self.display_items.len().saturating_sub(self.viewport_height.max(1))); }
    fn clamp_horizontal_scroll(&mut self) { self.horizontal_offset = self.horizontal_offset.min(self.max_line_width.saturating_sub(self.viewport_width.max(1))); }
    fn ensure_selected_visible(&mut self) { if self.selected < self.scroll_offset { self.scroll_offset = self.selected; } else if self.selected >= self.scroll_offset + self.viewport_height.max(1) { self.scroll_offset = self.selected + 1 - self.viewport_height.max(1); } }

    // Clicks in the dedicated scrollbar column/row are intentionally ignored.
    fn handle_click(&mut self, column: u16, row: u16) {
        if !self.list_area.contains((column, row)) { return; }
        let index = self.scroll_offset + (row - self.list_area.y) as usize;
        if index < self.display_items.len() { self.selected = index; self.toggle_expand_at(index); }
    }

    fn render_item(&self, item: &DisplayItem) -> ListItem<'static> {
        let (prefix, style) = if item.is_directory {
            (if self.mode == ViewMode::Tree && self.expanded.contains(&item.path) { "📂 " } else { "📁 " }, Style::default().fg(Color::Blue))
        } else { ("📄 ", Style::default().fg(Color::Green)) };
        let content = match self.mode {
            ViewMode::Tree => format!("{}{}{}", "  ".repeat(item.depth.saturating_sub(1)), prefix, item.name),
            ViewMode::Flat => format!("{}{}", prefix, item.path),
        };
        let content: String = content.chars().skip(self.horizontal_offset).collect();
        ListItem::new(Line::from(Span::styled(content, style)))
    }

    fn info_text(&self) -> Text<'static> {
        Text::from(vec![
            Line::from("Contrôles:".bold()),
            Line::from("  Q/Esc: Quitter   Tab: Flat/Tree   ↑/↓: Naviguer   PgUp/PgDn: Scroll rapide"),
            Line::from("  →/Entrée: Déplier   ←: Replier   Clic gauche: Sélection/Déplier"),
            Line::from("  Molette: Scroll vertical   Maj+←/→: Scroll horizontal"),
            Line::from(""),
            Line::from(format!("Mode: {}", self.mode.label()).fg(Color::Yellow)),
            Line::from(format!("Total: {} dossiers, {} fichiers", self.tree.dirs_nb, self.tree.files_nb)),
            Line::from(format!("Affichés: {} nœuds ({}/{})", self.display_items.len(), (self.selected + 1).min(self.display_items.len()), self.display_items.len().max(1))),
        ])
    }
}

fn run_app(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;
    let mut app = App::new(path)?;
    let result = event_loop(&mut terminal, &mut app);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    result
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        app.ensure_display_items();
        terminal.draw(|f| ui(f, app))?;
        if event::poll(Duration::from_millis(16))? { match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => break,
                KeyCode::Tab => app.toggle_mode(), KeyCode::Down => app.move_down(), KeyCode::Up => app.move_up(),
                KeyCode::PageDown => app.page(true), KeyCode::PageUp => app.page(false), KeyCode::Home => { app.selected = 0; app.ensure_selected_visible(); },
                KeyCode::End => { app.selected = app.display_items.len().saturating_sub(1); app.ensure_selected_visible(); },
                KeyCode::Left if k.modifiers.contains(KeyModifiers::SHIFT) => app.scroll_horizontal_by(-4),
                KeyCode::Right if k.modifiers.contains(KeyModifiers::SHIFT) => app.scroll_horizontal_by(4),
                KeyCode::Right => app.expand_selected(), KeyCode::Left => app.collapse_selected(),
                KeyCode::Enter => { let i = app.selected; app.toggle_expand_at(i); }, _ => {}
            },
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => app.handle_click(m.column, m.row),
                MouseEventKind::Down(MouseButton::Right) => break,
                MouseEventKind::ScrollUp => app.scroll_by(-3), MouseEventKind::ScrollDown => app.scroll_by(3),
                MouseEventKind::ScrollLeft => app.scroll_horizontal_by(-4), MouseEventKind::ScrollRight => app.scroll_horizontal_by(4), _ => {}
            }, _ => {}
        }}
    }
    Ok(())
}

fn ui(f: &mut ratatui::prelude::Frame, app: &mut App) {
    let area = f.area();
    let outer = Block::default().title(format!(" File Node Visualizer — {} ", app.mode.label()).bold()).borders(Borders::ALL).border_style(Style::default().fg(Color::White));
    f.render_widget(outer, area);
    let inner = Rect { x: area.x + 1, y: area.y + 1, width: area.width.saturating_sub(2), height: area.height.saturating_sub(2) };
    let layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(1), Constraint::Length(7)]).split(inner);
    let panel = layout[0];
    let panel_inner = Rect { x: panel.x + 1, y: panel.y + 1, width: panel.width.saturating_sub(2), height: panel.height.saturating_sub(2) };
    // Reserve explicit cells for scrollbars instead of drawing them over List's content.
    let content = Rect { x: panel_inner.x, y: panel_inner.y, width: panel_inner.width.saturating_sub(1), height: panel_inner.height.saturating_sub(1) };
    let vbar = Rect { x: content.x + content.width, y: content.y, width: 1, height: content.height };
    let hbar = Rect { x: content.x, y: content.y + content.height, width: content.width, height: 1 };
    app.list_area = content;
    app.viewport_height = content.height as usize;
    app.viewport_width = content.width as usize;
    app.clamp_scroll(); app.clamp_horizontal_scroll();
    let end = (app.scroll_offset + app.viewport_height).min(app.display_items.len());
    let items: Vec<_> = app.display_items[app.scroll_offset..end].iter().map(|i| app.render_item(i)).collect();
    let mut state = ListState::default();
    if app.selected >= app.scroll_offset && app.selected < end { state.select(Some(app.selected - app.scroll_offset)); }
    f.render_stateful_widget(List::new(items).highlight_style(Style::default().bg(Color::DarkGray).fg(Color::Black)).highlight_symbol("> "), content, &mut state);
    let mut vs = ScrollbarState::new(app.display_items.len()).position(app.scroll_offset).viewport_content_length(app.viewport_height);
    f.render_stateful_widget(Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(None).end_symbol(None).style(Style::default().fg(Color::Cyan)), vbar, &mut vs);
    if app.max_line_width > app.viewport_width {
        let mut hs = ScrollbarState::new(app.max_line_width).position(app.horizontal_offset).viewport_content_length(app.viewport_width);
        f.render_stateful_widget(Scrollbar::new(ScrollbarOrientation::HorizontalBottom).begin_symbol(None).end_symbol(None).style(Style::default().fg(Color::Cyan)), hbar, &mut hs);
    }
    f.render_widget(Paragraph::new(app.info_text()).block(Block::default().title(" Informations ".bold()).borders(Borders::ALL).border_style(Style::default().fg(Color::Magenta))).wrap(Wrap::default()), layout[1]);
}

fn print_usage() {
    println!("Usage: file_tree_tui [PATH]\n\nArguments:\n  PATH  Directory path to visualize (default: current directory)");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") { print_usage(); return Ok(()); }
    let path = if args.len() > 1 && !args[1].starts_with('-') { Path::new(&args[1]) } else { Path::new(".") };
    if !path.exists() { eprintln!("Erreur: Le chemin '{}' n'existe pas", path.display()); std::process::exit(1); }
    run_app(path)
}
