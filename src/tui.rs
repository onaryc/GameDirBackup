use file_tree_json::{build_flat_tree, build_tree, TreeNode, FlatNode};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::stdout;
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
enum DisplayMode {
    Tree,
    Flat,
}

#[derive(Clone, Debug)]
struct TreeState {
    node: TreeNode,
    expanded: bool,
    children: Vec<TreeState>,
}

impl TreeState {
    fn from_tree(node: TreeNode) -> Self {
        let children = node.children.clone().into_iter()
            .map(TreeState::from_tree)
            .collect();

        Self {
            node,
            expanded: true,
            children,
        }
    }

    fn toggle_expand(&mut self) {
        self.expanded = !self.expanded;
    }

    fn get_visible_items(&self, items: &mut Vec<(TreeState, usize)>, depth: usize) {
        items.push((self.clone(), depth));

        if self.expanded {
            for child in &self.children {
                child.get_visible_items(items, depth + 1);
            }
        }
    }

    fn find_at_position(&mut self, position: usize, current_index: &mut usize) -> Option<&mut TreeState> {
        if *current_index == position {
            return Some(self);
        }

        *current_index += 1;

        if self.expanded {
            for child in &mut self.children {
                if let Some(found) = child.find_at_position(position, current_index) {
                    return Some(found);
                }
            }
        }

        None
    }
}

struct App {
    tree_state: Option<TreeState>,
    flat: Option<Vec<FlatNode>>,
    mode: DisplayMode,
    scroll: usize,
    visible_items: Vec<(TreeState, usize)>,
}

impl App {
    fn new(path: &Path) -> Result<Self, std::io::Error> {
        let tree = build_tree(path).ok();
        let flat = build_flat_tree(path).ok();

        let tree_state = tree.map(TreeState::from_tree);

        let mut app = Self {
            tree_state: tree_state.clone(),
            flat,
            mode: DisplayMode::Tree,
            scroll: 0,
            visible_items: Vec::new(),
        };

        if let Some(ref _tree_state) = app.tree_state {
            app.update_visible_items();
        }

        Ok(app)
    }

    fn update_visible_items(&mut self) {
        self.visible_items.clear();
        if let Some(ref tree_state) = self.tree_state {
            tree_state.get_visible_items(&mut self.visible_items, 0);
        }
    }

    fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            DisplayMode::Tree => DisplayMode::Flat,
            DisplayMode::Flat => DisplayMode::Tree,
        };
        self.scroll = 0;
    }

    fn get_items(&self) -> Vec<ListItem<'_>> {
        match self.mode {
            DisplayMode::Tree => self.get_tree_items(),
            DisplayMode::Flat => self.get_flat_items(),
        }
    }

    fn get_tree_items(&self) -> Vec<ListItem<'_>> {
        let mut items = Vec::new();
        for (node_state, depth) in &self.visible_items {
            let indent = "  ".repeat(*depth);
            let prefix = if node_state.node.is_directory {
                if node_state.expanded {
                    "📁 "
                } else {
                    "📂 "
                }
            } else {
                "📄 "
            };

            let content = format!("{}{}{}", indent, prefix, node_state.node.name);
            let style = if node_state.node.is_directory {
                Style::default().fg(Color::Blue)
            } else {
                Style::default().fg(Color::Green)
            };

            items.push(ListItem::new(Line::from(Span::styled(content, style))));
        }
        items
    }

    fn get_flat_items(&self) -> Vec<ListItem<'_>> {
        let mut items = Vec::new();
        if let Some(flat_nodes) = &self.flat {
            for node in flat_nodes {
                let prefix = if node.is_directory {
                    "📁 "
                } else {
                    "📄 "
                };

                let parent_display = node.parent.as_deref().unwrap_or("none");
                let content = format!("{} {} (parent: {})", prefix, node.name, parent_display);
                let style = if node.is_directory {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default().fg(Color::Green)
                };

                items.push(ListItem::new(Line::from(Span::styled(content, style))));
            }
        }
        items
    }

    fn get_info_text(&self) -> Text<'_> {
        let mut lines = Vec::new();

        lines.push(Line::from("Controles:".bold()));
        lines.push(Line::from("  TAB: Changer de mode (Arborescent/Plat)"));
        lines.push(Line::from("  Q/Clic droit: Quitter"));
        lines.push(Line::from("  ↑/↓: Naviguer"));
        lines.push(Line::from("  Clic gauche: Déplier/Replier (mode arborescent)"));
        lines.push(Line::from(""));

        match self.mode {
            DisplayMode::Tree => {
                lines.push(Line::from("Mode: Arborescent".fg(Color::Yellow)));
                if let Some(tree) = &self.tree_state {
                    let total_files = Self::count_files(&tree.node);
                    let total_dirs = Self::count_dirs(&tree.node);
                    let visible_count = self.visible_items.len();
                    lines.push(Line::from(format!("Total: {} dossiers, {} fichiers", total_dirs, total_files)));
                    lines.push(Line::from(format!("Affichés: {} éléments", visible_count)));
                }
            }
            DisplayMode::Flat => {
                lines.push(Line::from("Mode: Plat".fg(Color::Yellow)));
                if let Some(flat) = &self.flat {
                    let total_files = flat.iter().filter(|n| !n.is_directory).count();
                    let total_dirs = flat.iter().filter(|n| n.is_directory).count();
                    lines.push(Line::from(format!("Total: {} dossiers, {} fichiers", total_dirs, total_files)));
                }
            }
        }

        Text::from(lines)
    }

    fn count_files(node: &TreeNode) -> usize {
        let mut count = if !node.is_directory { 1 } else { 0 };
        for child in &node.children {
            count += Self::count_files(child);
        }
        count
    }

    fn count_dirs(node: &TreeNode) -> usize {
        let mut count = if node.is_directory { 1 } else { 0 };
        for child in &node.children {
            count += Self::count_dirs(child);
        }
        count
    }

    fn handle_click(&mut self, row: u16) {
        if self.mode == DisplayMode::Tree {
            let list_height = self.get_items().len() as u16;
            if row > 0 && row <= list_height {
                let index = (row - 1) as usize + self.scroll;
                if index < self.visible_items.len() {
                    if let Some(node) = self.find_tree_node_by_index(index) {
                        node.toggle_expand();
                        self.update_visible_items();
                    }
                }
            }
        }
    }

    fn find_tree_node_by_index(&mut self, index: usize) -> Option<&mut TreeState> {
        if let Some(ref mut tree_state) = self.tree_state {
            let mut current_index = 0;
            tree_state.find_at_position(index, &mut current_index)
        } else {
            None
        }
    }
}

fn run_app(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(path)?;

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => break,
                        KeyCode::Tab => app.toggle_mode(),
                        KeyCode::Down => {
                            let items_count = app.get_items().len();
                            if app.scroll < items_count.saturating_sub(1) {
                                app.scroll += 1;
                            }
                        }
                        KeyCode::Up => {
                            if app.scroll > 0 {
                                app.scroll -= 1;
                            }
                        }
                        KeyCode::Enter => {
                            if app.mode == DisplayMode::Tree {
                                let current_index = app.scroll;
                                if current_index < app.visible_items.len() {
                                    if let Some(node) = app.find_tree_node_by_index(current_index) {
                                        node.toggle_expand();
                                        app.update_visible_items();
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            if let Event::Mouse(mouse_event) = event::read()? {
                match mouse_event.kind {
                    MouseEventKind::Down(button) => {
                        match button {
                            MouseButton::Left => {
                                app.handle_click(mouse_event.row);
                            }
                            MouseButton::Right => {
                                break;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(())
}

fn ui(f: &mut ratatui::prelude::Frame, app: &mut App) {
    let area = f.area();

    let block = Block::default()
        .title(" File Tree Visualizer ".bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White));

    f.render_widget(block, area);

    let inner = Rect {
        x: 1,
        y: 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(7),
        ])
        .split(inner);

    let title = match app.mode {
        DisplayMode::Tree => " Mode Arborescent (clic pour déployer) ",
        DisplayMode::Flat => " Mode Plat ",
    };

    let list_block = Block::default()
        .title(title.bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let items = app.get_items();
    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::Black))
        .highlight_symbol("> ");

    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(Some(app.scroll));
    f.render_stateful_widget(list, layout[1], &mut list_state);

    let info_block = Block::default()
        .title(" Informations ".bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let info_paragraph = Paragraph::new(app.get_info_text())
        .block(info_block)
        .wrap(Wrap::default());

    f.render_widget(info_paragraph, layout[2]);
}

fn print_usage() {
    println!("Usage: file_tree_tui [PATH]");
    println!();
    println!("Arguments:");
    println!("  PATH  Directory path to visualize (default: current directory)");
    println!();
    println!("Controls:");
    println!("  TAB: Toggle between Tree and Flat modes");
    println!("  Q/Clic droit: Quit");
    println!("  ↑/↓: Navigate");
    println!("  ENTER/Clic gauche: Expand/Collapse nodes (Tree mode)");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
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