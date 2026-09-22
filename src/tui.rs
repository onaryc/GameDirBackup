use file_tree_json::{build_tree, TreeNode};
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
use std::sync::Arc;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Debug)]
struct LazyTreeNode {
    name: String,
    path: String,
    is_directory: bool,
    children: Vec<Arc<LazyTreeNode>>,
}

impl LazyTreeNode {
    fn from_tree_node(node: TreeNode) -> Self {
        let children = node.children.into_iter()
            .map(|child| Arc::new(Self::from_tree_node(child)))
            .collect();

        Self {
            name: node.name,
            path: node.path,
            is_directory: node.is_directory,
            children,
        }
    }
}

struct App {
    root_node: Option<Arc<LazyTreeNode>>,
    scroll: usize,
    viewport_height: usize,
    list_area_y: u16,
    list_area_height: u16,
    loaded_nodes: HashMap<String, Arc<LazyTreeNode>>,
    all_paths: Vec<String>,
    expanded_paths: HashSet<String>,
    cached_tree_items: Option<Vec<ListItem<'static>>>,
    last_scroll: usize,
    last_expanded_hash: u64,
}

impl App {
    fn new(path: &Path) -> Result<Self, std::io::Error> {
        let root_node = match build_tree(path) {
            Ok(tree) => {
                Some(Arc::new(LazyTreeNode::from_tree_node(tree)))
            }
            Err(e) => {
                eprintln!("Error building tree: {}", e);
                None
            }
        };

        let all_paths = if let Some(ref root_node) = root_node {
            let mut paths = Vec::new();
            Self::collect_all_paths(&root_node, &mut paths);
            paths
        } else {
            Vec::new()
        };

        let mut loaded_nodes = HashMap::new();
        if let Some(ref root_node) = root_node {
            loaded_nodes.insert(root_node.path.clone(), root_node.clone());
        }

        Ok(Self {
            root_node,
            scroll: 0,
            viewport_height: 24,
            list_area_y: 0,
            list_area_height: 0,
            loaded_nodes,
            all_paths,
            expanded_paths: HashSet::new(),
            cached_tree_items: None,
            last_scroll: 0,
            last_expanded_hash: 0,
        })
    }

    fn collect_all_paths(node: &LazyTreeNode, paths: &mut Vec<String>) {
        paths.push(node.path.clone());
        for child in &node.children {
            Self::collect_all_paths(child, paths);
        }
    }

    fn get_visible_tree_items(&self) -> Vec<(Arc<LazyTreeNode>, usize)> {
        let mut visible_items = Vec::new();
        if let Some(ref root_node) = self.root_node {
            let mut stack = VecDeque::new();
            stack.push_back((root_node.clone(), 0));

            while let Some((node, depth)) = stack.pop_front() {
                let loaded_node = if self.loaded_nodes.contains_key(&node.path) {
                    self.loaded_nodes[&node.path].clone()
                } else {
                    node
                };

                visible_items.push((loaded_node.clone(), depth));

                if self.expanded_paths.contains(&loaded_node.path) {
                    for child in &loaded_node.children {
                        stack.push_back((child.clone(), depth + 1));
                    }
                }
            }
        }
        visible_items
    }

    fn load_node_if_needed(&mut self, path: &str) {
        if !self.loaded_nodes.contains_key(path) {
            let path_obj = Path::new(path);
            if let Ok(tree) = build_tree(path_obj) {
                let new_node = Arc::new(LazyTreeNode::from_tree_node(tree));
                self.loaded_nodes.insert(path.to_string(), new_node);
            }
        }
    }

    fn ensure_visible_nodes_loaded(&mut self) {
        if self.scroll % 3 != 0 && self.scroll != 0 {
            return;
        }

        let visible_items = self.get_visible_tree_items();
        for (node, _) in visible_items {
            self.load_node_if_needed(&node.path);
        }
    }

    fn toggle_expand(&mut self, node_path: &str) {
        if self.expanded_paths.contains(node_path) {
            self.expanded_paths.remove(node_path);
        } else {
            self.expanded_paths.insert(node_path.to_string());
        }
        self.cached_tree_items = None;
    }

    fn get_expanded_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        for path in &self.expanded_paths {
            path.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn get_items(&mut self) -> Vec<ListItem<'static>> {
        self.get_tree_items()
    }

    fn get_tree_items(&mut self) -> Vec<ListItem<'static>> {
        let current_hash = self.get_expanded_hash();
        if self.last_scroll == self.scroll && self.last_expanded_hash == current_hash {
            return self.cached_tree_items.clone().unwrap_or_default();
        }

        let visible_items = self.get_visible_tree_items();
        let mut items = Vec::new();

        for (node, depth) in visible_items {
            let indent = "  ".repeat(depth);
            let prefix = if node.is_directory {
                if self.expanded_paths.contains(&node.path) {
                    "📂 "
                } else {
                    "📁 "
                }
            } else {
                "📄 "
            };

            let content = format!("{}{}{}", indent, prefix, node.name);
            let style = if node.is_directory {
                Style::default().fg(Color::Blue)
            } else {
                Style::default().fg(Color::Green)
            };

            items.push(ListItem::new(Line::from(Span::styled(content, style))));
        }

        self.cached_tree_items = Some(items.clone());
        self.last_scroll = self.scroll;
        self.last_expanded_hash = current_hash;

        items
    }

    fn get_info_text(&self) -> Text<'_> {
        let mut lines = Vec::new();

        lines.push(Line::from("Contrôles:".bold()));
        lines.push(Line::from("  Q/Clic droit: Quitter"));
        lines.push(Line::from("  ↑/↓: Naviguer"));
        lines.push(Line::from("  PageUp/PageDown: Scroll rapide"));
        lines.push(Line::from("  Entrée/Clic gauche: Déplier/Replier"));
        lines.push(Line::from(""));

        lines.push(Line::from("Mode: Arborescent".fg(Color::Yellow)));
        if let Some(root) = &self.root_node {
            let total_files = Self::count_files_in_lazy(root);
            let total_dirs = Self::count_dirs_in_lazy(root);
            let loaded_count = self.loaded_nodes.len();
            lines.push(Line::from(format!("Total: {} dossiers, {} fichiers", total_dirs, total_files)));
            lines.push(Line::from(format!("Chargés: {}/{} nœuds", loaded_count, self.all_paths.len())));
        }

        Text::from(lines)
    }

    fn count_files_in_lazy(node: &LazyTreeNode) -> usize {
        let mut count = if !node.is_directory { 1 } else { 0 };
        for child in &node.children {
            count += Self::count_files_in_lazy(child);
        }
        count
    }

    fn count_dirs_in_lazy(node: &LazyTreeNode) -> usize {
        let mut count = if node.is_directory { 1 } else { 0 };
        for child in &node.children {
            count += Self::count_dirs_in_lazy(child);
        }
        count
    }

    fn handle_click(&mut self, row: u16, _column: u16) {
        if row >= self.list_area_y && row < self.list_area_y + self.list_area_height {
            let relative_row = (row - self.list_area_y) as usize;
            let index = relative_row + self.scroll;
            let visible_items = self.get_visible_tree_items();
            if index < visible_items.len() {
                let node = &visible_items[index].0;
                if node.is_directory {
                    self.load_node_if_needed(&node.path);
                    self.toggle_expand(&node.path);
                }
            }
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
        terminal.draw(|f| {
            let size = f.area();
            app.viewport_height = size.height as usize;
            ui(f, &mut app)
        })?;

        app.ensure_visible_nodes_loaded();

        // let root_node = match build_tree(path) {
        //     Ok(tree) => {
        //         Some(Arc::new(LazyTreeNode::from_tree_node(tree)))
        //     }
        //     Err(e) => {
        //         eprintln!("Error building tree: {}", e);
        //         None
        //     }
        // };
        if event::poll(std::time::Duration::from_millis(16))? {
            match event::read()? {
                // Event::FocusGained => null,
                // Event::FocusLost => null,
                Event::Key(key_event) => {
                    if key_event.kind == KeyEventKind::Press {
                        match key_event.code {
                            KeyCode::Esc => break,
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
                            KeyCode::PageDown => {
                                let items_count = app.get_items().len();
                                app.scroll = std::cmp::min(app.scroll + app.viewport_height / 2, items_count.saturating_sub(1));
                                app.ensure_visible_nodes_loaded();
                            }
                            KeyCode::PageUp => {
                                app.scroll = app.scroll.saturating_sub(app.viewport_height / 2);
                            }
                            KeyCode::Enter => {
                                let visible_items = app.get_visible_tree_items();
                                if app.scroll < visible_items.len() {
                                    let node = &visible_items[app.scroll].0;
                                    if node.is_directory {
                                        app.load_node_if_needed(&node.path);
                                        app.toggle_expand(&node.path);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                },
                Event::Mouse(mouse_event) => {
                    match mouse_event.kind {
                        MouseEventKind::Down(button) => {
                            match button {
                                MouseButton::Left => {
                                    app.handle_click(mouse_event.row, mouse_event.column);
                                }
                                MouseButton::Right => {
                                    break;
                                }
                                _ => {}
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            if app.scroll > 0 {
                                app.scroll = app.scroll.saturating_sub(3);
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            let items_count = app.get_items().len();
                            if app.scroll < items_count.saturating_sub(1) {
                                app.scroll = std::cmp::min(app.scroll + 3, items_count.saturating_sub(1));
                                app.ensure_visible_nodes_loaded();
                            }
                        }
                        _ => {}
                    }
                },
                // Event::Paste(data) => null,
                // Event::Resize(width, height) => null,
                _ => {}
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
            Constraint::Min(1),
            Constraint::Length(7),
        ])
        .split(inner);

    app.list_area_y = inner.y + layout[0].y;
    app.list_area_height = layout[0].height;

    let list_block = Block::default()
        .title(" Mode Arborescent (clic pour déployer) ".bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let items = app.get_items();
    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::Black))
        .highlight_symbol("> ");

    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(Some(app.scroll));
    f.render_stateful_widget(list, layout[0], &mut list_state);

    let info_block = Block::default()
        .title(" Informations ".bold())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let info_paragraph = Paragraph::new(app.get_info_text())
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
    println!("  Q/Right click: Quit");
    println!("  ↑/↓: Navigate (PageUp/PageDown for fast scroll)");
    println!("  ENTER/Left click: Expand/Collapse nodes");
    println!("  Mouse Wheel: Fast scrolling");
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