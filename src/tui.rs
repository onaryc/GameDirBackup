use file_tree_json::{build_file_node, to_tree, FileNode, RootNode, TraversalMode};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Terminal,
};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::collections::HashSet;
use std::io::stdout;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Flat,
    Tree,
}

impl ViewMode {
    fn label(self) -> &'static str {
        match self {
            ViewMode::Flat => "Flat",
            ViewMode::Tree => "Tree",
        }
    }
}

/// Représentation légère d'un `FileNode` pour l'affichage : on ne garde que
/// ce dont le rendu a besoin, pour limiter le coût des reconstructions.
#[derive(Clone)]
struct DisplayItem {
    name: String,
    path: String,
    is_directory: bool,
    depth: usize,
}

struct App {
    flat: RootNode,
    tree: RootNode,
    mode: ViewMode,
    /// Chemins des dossiers actuellement dépliés (mode Tree uniquement).
    expanded: HashSet<String>,
    /// Liste affichée courante, reconstruite uniquement quand `dirty` est vrai.
    display_items: Vec<DisplayItem>,
    dirty: bool,

    selected: usize,
    scroll_offset: usize,
    horizontal_offset: usize,
    /// Largeur (en caractères) de la ligne la plus longue de `display_items`,
    /// recalculée uniquement lors d'une reconstruction (`ensure_display_items`).
    max_line_width: usize,

    viewport_height: usize,
    viewport_width: usize,
    list_area_y: u16,
    list_area_height: u16,
}

impl App {
    fn new(path: &Path) -> Result<Self, std::io::Error> {
        let flat = build_file_node(path, TraversalMode::Parallel, false)?;
        let tree = to_tree(&flat);

        let mut app = Self {
            flat,
            tree,
            mode: ViewMode::Tree,
            expanded: HashSet::new(),
            display_items: Vec::new(),
            dirty: true,
            selected: 0,
            scroll_offset: 0,
            horizontal_offset: 0,
            max_line_width: 0,
            viewport_height: 24,
            viewport_width: 80,
            list_area_y: 0,
            list_area_height: 0,
        };
        app.ensure_display_items();
        Ok(app)
    }

    // --- Construction de la liste affichée ------------------------------

    fn ensure_display_items(&mut self) {
        if !self.dirty {
            return;
        }

        // On retient le chemin sélectionné pour tenter de le retrouver après
        // reconstruction (expand/collapse ou changement de mode).
        let previously_selected_path = self
            .display_items
            .get(self.selected)
            .map(|item| item.path.clone());

        self.display_items = match self.mode {
            ViewMode::Flat => self.build_flat_display_items(),
            ViewMode::Tree => {
                let mut items = Vec::new();
                Self::push_tree_display_items(&self.tree.nodes, &self.expanded, &mut items);
                items
            }
        };
        self.dirty = false;

        self.max_line_width = self
            .display_items
            .iter()
            .map(|item| Self::item_width(self.mode, item))
            .max()
            .unwrap_or(0);

        // On retrouve la même sélection si possible, sinon on borne l'index.
        self.selected = previously_selected_path
            .and_then(|path| self.display_items.iter().position(|item| item.path == path))
            .unwrap_or(self.selected)
            .min(self.display_items.len().saturating_sub(1));

        self.ensure_selected_visible();
        self.clamp_horizontal_scroll();
    }

    /// Largeur (en caractères) qu'occupera `item` une fois formaté pour
    /// l'affichage — calculée arithmétiquement (sans allouer de String) pour
    /// pouvoir être appliquée à tous les éléments sans coût significatif.
    /// Doit rester cohérente avec le formatage réel fait par `render_item`.
    fn item_width(mode: ViewMode, item: &DisplayItem) -> usize {
        const PREFIX_WIDTH: usize = 2; // icône + espace
        match mode {
            ViewMode::Tree => {
                let indent = item.depth.saturating_sub(1) * 2;
                indent + PREFIX_WIDTH + item.name.chars().count()
            }
            ViewMode::Flat => PREFIX_WIDTH + item.path.chars().count(),
        }
    }

    fn build_flat_display_items(&self) -> Vec<DisplayItem> {
        let mut items: Vec<DisplayItem> = self
            .flat
            .nodes
            .iter()
            .map(|node| DisplayItem {
                name: node.name.clone(),
                path: node.path.clone(),
                is_directory: node.is_directory,
                depth: node.depth,
            })
            .collect();
        items.sort_by(|a, b| a.path.cmp(&b.path));
        items
    }

    fn push_tree_display_items(nodes: &[FileNode], expanded: &HashSet<String>, out: &mut Vec<DisplayItem>) {
        for node in nodes {
            out.push(DisplayItem {
                name: node.name.clone(),
                path: node.path.clone(),
                is_directory: node.is_directory,
                depth: node.depth,
            });

            if node.is_directory && expanded.contains(&node.path) {
                Self::push_tree_display_items(&node.children, expanded, out);
            }
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    // --- Actions ----------------------------------------------------------

    fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            ViewMode::Flat => ViewMode::Tree,
            ViewMode::Tree => ViewMode::Flat,
        };
        self.mark_dirty();
    }

    fn toggle_expand_at(&mut self, index: usize) {
        if self.mode != ViewMode::Tree {
            return;
        }
        if let Some(item) = self.display_items.get(index) {
            if !item.is_directory {
                return;
            }
            if self.expanded.contains(&item.path) {
                self.expanded.remove(&item.path);
            } else {
                self.expanded.insert(item.path.clone());
            }
            self.mark_dirty();
        }
    }

    fn expand_selected(&mut self) {
        if self.mode != ViewMode::Tree {
            return;
        }
        if let Some(item) = self.display_items.get(self.selected) {
            if item.is_directory && !self.expanded.contains(&item.path) {
                self.expanded.insert(item.path.clone());
                self.mark_dirty();
            }
        }
    }

    fn collapse_selected(&mut self) {
        if self.mode != ViewMode::Tree {
            return;
        }
        if let Some(item) = self.display_items.get(self.selected) {
            if item.is_directory && self.expanded.contains(&item.path) {
                self.expanded.remove(&item.path);
                self.mark_dirty();
            }
        }
    }

    // --- Navigation ---------------------------------------------------------

    fn move_down(&mut self) {
        if self.selected + 1 < self.display_items.len() {
            self.selected += 1;
            self.ensure_selected_visible();
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.ensure_selected_visible();
    }

    fn page_down(&mut self) {
        let step = self.viewport_height.max(1);
        self.selected = (self.selected + step).min(self.display_items.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    fn page_up(&mut self) {
        let step = self.viewport_height.max(1);
        self.selected = self.selected.saturating_sub(step);
        self.ensure_selected_visible();
    }

    fn go_to_start(&mut self) {
        self.selected = 0;
        self.ensure_selected_visible();
    }

    fn go_to_end(&mut self) {
        self.selected = self.display_items.len().saturating_sub(1);
        self.ensure_selected_visible();
    }

    fn scroll_by(&mut self, delta: isize) {
        let max_offset = self.display_items.len().saturating_sub(self.viewport_height.max(1));
        let new_offset = (self.scroll_offset as isize + delta).max(0) as usize;
        self.scroll_offset = new_offset.min(max_offset);
    }

    fn clamp_scroll(&mut self) {
        let max_offset = self.display_items.len().saturating_sub(self.viewport_height.max(1));
        self.scroll_offset = self.scroll_offset.min(max_offset);
    }

    fn scroll_horizontal_by(&mut self, delta: isize) {
        let max_offset = self.max_line_width.saturating_sub(self.viewport_width.max(1));
        let new_offset = (self.horizontal_offset as isize + delta).max(0) as usize;
        self.horizontal_offset = new_offset.min(max_offset);
    }

    fn clamp_horizontal_scroll(&mut self) {
        let max_offset = self.max_line_width.saturating_sub(self.viewport_width.max(1));
        self.horizontal_offset = self.horizontal_offset.min(max_offset);
    }

    /// Recale la vue pour que la sélection soit visible. À appeler
    /// uniquement après un déplacement de sélection (clavier/clic/reconstruction
    /// de la liste) — jamais à chaque frame, sinon ça annule un scroll molette
    /// qui n'a pas déplacé la sélection.
    fn ensure_selected_visible(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.viewport_height > 0 && self.selected >= self.scroll_offset + self.viewport_height {
            self.scroll_offset = self.selected + 1 - self.viewport_height;
        }
    }

    fn handle_click(&mut self, row: u16) {
        if row < self.list_area_y || row >= self.list_area_y + self.list_area_height {
            return;
        }
        let index = self.scroll_offset + (row - self.list_area_y) as usize;
        if index >= self.display_items.len() {
            return;
        }
        self.selected = index;
        self.toggle_expand_at(index);
    }

    // --- Rendu ---------------------------------------------------------------

    /// Ne construit des `ListItem` que pour la tranche visible : le coût par
    /// frame est proportionnel à `viewport_height`, jamais au nombre total
    /// de fichiers/dossiers.
    fn visible_list_items(&self) -> Vec<ListItem<'static>> {
        let end = (self.scroll_offset + self.viewport_height).min(self.display_items.len());
        self.display_items[self.scroll_offset..end]
            .iter()
            .map(|item| self.render_item(item))
            .collect()
    }

    fn render_item(&self, item: &DisplayItem) -> ListItem<'static> {
        let (prefix, style) = if item.is_directory {
            let icon = if self.mode == ViewMode::Tree && self.expanded.contains(&item.path) {
                "📂 "
            } else {
                "📁 "
            };
            (icon, Style::default().fg(Color::Blue))
        } else {
            ("📄 ", Style::default().fg(Color::Green))
        };

        let content = match self.mode {
            ViewMode::Tree => {
                let indent = "  ".repeat(item.depth.saturating_sub(1));
                format!("{}{}{}", indent, prefix, item.name)
            }
            ViewMode::Flat => format!("{}{}", prefix, item.path),
        };

        let visible_content: String = if self.horizontal_offset > 0 {
            content.chars().skip(self.horizontal_offset).collect()
        } else {
            content
        };

        ListItem::new(Line::from(Span::styled(visible_content, style)))
    }

    fn info_text(&self) -> Text<'static> {
        let mut lines = Vec::new();

        lines.push(Line::from("Contrôles:".bold()));
        lines.push(Line::from(
            "  Q/Esc: Quitter   Tab: Flat/Tree   ↑/↓: Naviguer   PgUp/PgDn: Scroll rapide",
        ));
        lines.push(Line::from(
            "  →/Entrée: Déplier   ←: Replier   Clic gauche: Sélection/Déplier",
        ));
        lines.push(Line::from(
            "  Molette: Scroll vertical   Maj+←/→: Scroll horizontal",
        ));
        lines.push(Line::from(""));

        lines.push(Line::from(format!("Mode: {}", self.mode.label()).fg(Color::Yellow)));
        lines.push(Line::from(format!(
            "Total: {} dossiers, {} fichiers",
            self.tree.dirs_nb, self.tree.files_nb
        )));
        lines.push(Line::from(format!(
            "Affichés: {} nœuds ({}/{})",
            self.display_items.len(),
            (self.selected + 1).min(self.display_items.len()),
            self.display_items.len().max(1)
        )));

        Text::from(lines)
    }
}

fn run_app(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(path)?;

    let result = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;

    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        app.ensure_display_items();

        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key_event) => {
                    if key_event.kind == KeyEventKind::Press {
                        match key_event.code {
                            KeyCode::Esc | KeyCode::Char('q') => break,
                            KeyCode::Tab => app.toggle_mode(),
                            KeyCode::Down => app.move_down(),
                            KeyCode::Up => app.move_up(),
                            KeyCode::PageDown => app.page_down(),
                            KeyCode::PageUp => app.page_up(),
                            KeyCode::Home => app.go_to_start(),
                            KeyCode::End => app.go_to_end(),
                            KeyCode::Left if key_event.modifiers.contains(KeyModifiers::SHIFT) => {
                                app.scroll_horizontal_by(-4)
                            }
                            KeyCode::Right if key_event.modifiers.contains(KeyModifiers::SHIFT) => {
                                app.scroll_horizontal_by(4)
                            }
                            KeyCode::Right => app.expand_selected(),
                            KeyCode::Left => app.collapse_selected(),
                            KeyCode::Enter => {
                                let selected = app.selected;
                                app.toggle_expand_at(selected);
                            }
                            _ => {}
                        }
                    }
                }
                Event::Mouse(mouse_event) => match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        app.handle_click(mouse_event.row);
                    }
                    MouseEventKind::Down(MouseButton::Right) => break,
                    MouseEventKind::ScrollUp => app.scroll_by(-3),
                    MouseEventKind::ScrollDown => app.scroll_by(3),
                    MouseEventKind::ScrollLeft => app.scroll_horizontal_by(-4),
                    MouseEventKind::ScrollRight => app.scroll_horizontal_by(4),
                    _ => {}
                },
                _ => {}
            }
        }
    }

    Ok(())
}

fn ui(f: &mut ratatui::prelude::Frame, app: &mut App) {
    let area = f.area();

    let block = Block::default()
        .title(" File Node Visualizer ".bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White));
    f.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(7)])
        .split(inner);

    app.list_area_y = layout[0].y + 1; // +1 : la bordure du haut occupe la 1re ligne
    app.list_area_height = layout[0].height.saturating_sub(2);
    app.viewport_height = app.list_area_height as usize;
    app.viewport_width = layout[0].width.saturating_sub(2) as usize; // moins les bordures gauche/droite
    app.clamp_scroll();
    app.clamp_horizontal_scroll();

    let list_title = format!(
        " {} ({}/{}) ",
        app.mode.label(),
        (app.selected + 1).min(app.display_items.len().max(1)),
        app.display_items.len()
    );
    let list_block = Block::default()
        .title(list_title.bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let items = app.visible_list_items();
    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::Black))
        .highlight_symbol("> ");

    let mut list_state = ListState::default();
    let selected_is_visible =
        app.selected >= app.scroll_offset && app.selected < app.scroll_offset + app.viewport_height;
    list_state.select(selected_is_visible.then(|| app.selected - app.scroll_offset));
    f.render_stateful_widget(list, layout[0], &mut list_state);

    // Scrollbar verticale, affichée sur la bordure droite du bloc liste.
    // `viewport_content_length` est fixé explicitement à la hauteur réelle
    // de la zone visible (sans les bordures) : sans ça, ratatui la déduit du
    // Rect complet passé au rendu (bordures comprises), ce qui fausse à la
    // fois la taille du curseur et sa correspondance avec la vue réelle.
    let mut v_scrollbar_state = ScrollbarState::new(app.display_items.len())
        .position(app.scroll_offset)
        .viewport_content_length(app.viewport_height);
    let v_scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .style(Style::default().fg(Color::Cyan));
    f.render_stateful_widget(v_scrollbar, layout[0], &mut v_scrollbar_state);

    // Scrollbar horizontale : seulement si le contenu dépasse réellement la
    // largeur visible. Sinon un curseur "plein" occuperait toute la barre
    // (rien à faire défiler), ce qui donnait l'impression d'une barre bien
    // plus grosse que la verticale.
    if app.max_line_width > app.viewport_width {
        let mut h_scrollbar_state = ScrollbarState::new(app.max_line_width)
            .position(app.horizontal_offset)
            .viewport_content_length(app.viewport_width);
        let h_scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
            .begin_symbol(None)
            .end_symbol(None)
            .style(Style::default().fg(Color::Cyan));
        f.render_stateful_widget(h_scrollbar, layout[0], &mut h_scrollbar_state);
    }

    let info_block = Block::default()
        .title(" Informations ".bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let info_paragraph = Paragraph::new(app.info_text())
        .block(info_block)
        .wrap(Wrap::default());

    f.render_widget(info_paragraph, layout[1]);
}

fn print_usage() {
    println!("Usage: file_tree_tui [PATH]");
    println!();
    println!("Arguments:");
    println!("  PATH  Directory path to visualize (default: current directory)");
    println!();
    println!("Controls:");
    println!("  Q/Esc: Quit                Tab: Switch Flat/Tree mode");
    println!("  Up/Down: Navigate          PageUp/PageDown: Fast scroll");
    println!("  Right/Enter: Expand        Left: Collapse");
    println!("  Left click: Select/Toggle  Right click: Quit");
    println!("  Mouse wheel: Vertical scroll   Shift+Left/Right: Horizontal scroll");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return Ok(());
    }

    let path = if args.len() > 1 && !args[1].starts_with('-') {
        Path::new(&args[1])
    } else {
        Path::new(".")
    };

    if !path.exists() {
        eprintln!("Erreur: Le chemin '{}' n'existe pas", path.display());
        std::process::exit(1);
    }

    run_app(path)
}
