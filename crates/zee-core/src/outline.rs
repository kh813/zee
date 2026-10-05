use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineNode {
    pub title: String,
    pub level: usize,
    pub line: usize, // 0-based line index in buffer
    pub children: Vec<OutlineNode>,
    pub is_expanded: bool,
}

impl OutlineNode {
    pub fn new(title: String, level: usize, line: usize) -> Self {
        Self {
            title,
            level,
            line,
            children: Vec::new(),
            is_expanded: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FlatOutlineItem {
    pub title: String,
    pub level: usize,
    pub depth: usize,
    pub line: usize,
    pub is_expanded: bool,
    pub has_children: bool,
    pub node_id: usize, // index in flat list or identifier
}

/// Parses Markdown content and returns a hierarchical tree of headings.
pub fn parse_markdown_outline(content: &str) -> Vec<OutlineNode> {
    let mut root_nodes: Vec<OutlineNode> = Vec::new();
    let mut stack: Vec<OutlineNode> = Vec::new();
    let mut in_code_block = false;

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        if let Some((level, title)) = parse_heading_line(trimmed) {
            let node = OutlineNode::new(title, level, line_idx);

            while let Some(top) = stack.last() {
                if top.level >= level {
                    let popped = stack.pop().unwrap();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(popped);
                    } else {
                        root_nodes.push(popped);
                    }
                } else {
                    break;
                }
            }
            stack.push(node);
        }
    }

    while let Some(popped) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(popped);
        } else {
            root_nodes.push(popped);
        }
    }

    root_nodes
}

fn parse_heading_line(line: &str) -> Option<(usize, String)> {
    if !line.starts_with('#') {
        return None;
    }
    let count = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&count) {
        let rest = line[count..].trim();
        if !rest.is_empty() {
            return Some((count, rest.to_string()));
        }
    }
    None
}

/// Flattens an outline tree according to expanded/collapsed state.
pub fn flatten_outline(nodes: &[OutlineNode], depth: usize, out: &mut Vec<FlatOutlineItem>) {
    for node in nodes {
        let has_children = !node.children.is_empty();
        out.push(FlatOutlineItem {
            title: node.title.clone(),
            level: node.level,
            depth,
            line: node.line,
            is_expanded: node.is_expanded,
            has_children,
            node_id: out.len(),
        });
        if has_children && node.is_expanded {
            flatten_outline(&node.children, depth + 1, out);
        }
    }
}

/// Extracts outline for a given language / extension, utilizing installed WASM plugins
/// first, with built-in fallbacks.
/// Builds a hierarchical OutlineNode tree from a flat list of (level, title, line) items.
pub fn build_tree_from_levels(items: Vec<(usize, String, usize)>) -> Vec<OutlineNode> {
    let mut root_nodes: Vec<OutlineNode> = Vec::new();
    let mut stack: Vec<OutlineNode> = Vec::new();

    for (level, title, line) in items {
        let node = OutlineNode::new(title, level, line);

        while let Some(top) = stack.last() {
            if top.level >= level {
                let popped = stack.pop().unwrap();
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(popped);
                } else {
                    root_nodes.push(popped);
                }
            } else {
                break;
            }
        }
        stack.push(node);
    }

    while let Some(popped) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(popped);
        } else {
            root_nodes.push(popped);
        }
    }

    root_nodes
}

/// Parses Rust source code and extracts functions, structs, enums, impls, traits, and modules.
pub fn parse_rust_outline(content: &str) -> Vec<OutlineNode> {
    let mut items = Vec::new();
    let mut in_block_comment = false;

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if in_block_comment {
            if let Some(end) = trimmed.find("*/") {
                in_block_comment = false;
                if trimmed[end + 2..].trim().is_empty() {
                    continue;
                }
            } else {
                continue;
            }
        }
        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
                continue;
            }
        }
        if trimmed.starts_with("//") {
            continue;
        }

        let leading_spaces = line.chars().take_while(|c| c.is_whitespace()).count();
        let indent_level = if leading_spaces >= 4 { 2 } else { 1 };

        let norm = trimmed.strip_prefix("pub ").unwrap_or(trimmed);
        let norm = norm.strip_prefix("pub(crate) ").unwrap_or(norm);
        let norm = norm.strip_prefix("async ").unwrap_or(norm);

        if norm.starts_with("fn ") {
            let name = norm[3..].split('(').next().unwrap_or("").trim();
            if !name.is_empty() {
                items.push((indent_level, format!("fn {}()", name), line_idx));
            }
        } else if norm.starts_with("struct ") {
            let name = norm[7..].split(&['{', ';', '('][..]).next().unwrap_or("").trim();
            if !name.is_empty() {
                items.push((1, format!("struct {}", name), line_idx));
            }
        } else if norm.starts_with("enum ") {
            let name = norm[5..].split('{').next().unwrap_or("").trim();
            if !name.is_empty() {
                items.push((1, format!("enum {}", name), line_idx));
            }
        } else if norm.starts_with("impl ") {
            let body = norm[5..].split('{').next().unwrap_or("").trim();
            if !body.is_empty() {
                items.push((1, format!("impl {}", body), line_idx));
            }
        } else if norm.starts_with("trait ") {
            let name = norm[6..].split(&['{', ':'][..]).next().unwrap_or("").trim();
            if !name.is_empty() {
                items.push((1, format!("trait {}", name), line_idx));
            }
        } else if norm.starts_with("mod ") {
            let name = norm[4..].split(&['{', ';'][..]).next().unwrap_or("").trim();
            if !name.is_empty() {
                items.push((1, format!("mod {}", name), line_idx));
            }
        }
    }

    build_tree_from_levels(items)
}

/// Parses Python source code and extracts classes, functions, and methods based on indentation.
pub fn parse_python_outline(content: &str) -> Vec<OutlineNode> {
    let mut items = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        let leading_spaces = line.len() - trimmed.len();
        let level = (leading_spaces / 4) + 1;

        let norm = trimmed.strip_prefix("async ").unwrap_or(trimmed);
        if norm.starts_with("def ") {
            let name = norm[4..].split('(').next().unwrap_or("").trim();
            if !name.is_empty() {
                items.push((level, format!("def {}()", name), line_idx));
            }
        } else if norm.starts_with("class ") {
            let name = norm[6..].split(&['(', ':'][..]).next().unwrap_or("").trim();
            if !name.is_empty() {
                items.push((level, format!("class {}", name), line_idx));
            }
        }
    }

    build_tree_from_levels(items)
}

/// Parses Go source code and extracts functions, methods, and type definitions.
pub fn parse_go_outline(content: &str) -> Vec<OutlineNode> {
    let mut items = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("func ") {
            let rest = &trimmed[5..];
            if rest.starts_with('(') {
                // Method: func (r *Receiver) MethodName(...)
                if let Some(close_paren) = rest.find(')') {
                    let recv = &rest[1..close_paren];
                    let after_recv = rest[close_paren + 1..].trim();
                    let name = after_recv.split('(').next().unwrap_or("").trim();
                    if !name.is_empty() {
                        items.push((1, format!("({}) {}", recv, name), line_idx));
                    }
                }
            } else {
                let name = rest.split('(').next().unwrap_or("").trim();
                if !name.is_empty() {
                    items.push((1, format!("func {}()", name), line_idx));
                }
            }
        } else if trimmed.starts_with("type ") {
            let parts: Vec<&str> = trimmed[5..].split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[0];
                let kind = parts[1];
                items.push((1, format!("type {} {}", name, kind), line_idx));
            }
        }
    }

    build_tree_from_levels(items)
}

/// Parses JSON content and extracts top-level and section keys.
pub fn parse_json_outline(content: &str) -> Vec<OutlineNode> {
    let mut items = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('"') {
            if let Some(end_quote) = trimmed[1..].find('"') {
                let key = &trimmed[1..1 + end_quote];
                let after = trimmed[1 + end_quote + 1..].trim();
                if after.starts_with(':') {
                    let leading_spaces = line.chars().take_while(|c| c.is_whitespace()).count();
                    let level = (leading_spaces / 2).max(1);
                    let is_container = after[1..].trim().starts_with('{') || after[1..].trim().starts_with('[');
                    let display = if is_container {
                        format!("\"{}\": {{…}}", key)
                    } else {
                        format!("\"{}\"", key)
                    };
                    items.push((level, display, line_idx));
                }
            }
        }
    }

    build_tree_from_levels(items)
}

/// Parses HTML content and extracts heading tags and title.
pub fn parse_html_outline(content: &str) -> Vec<OutlineNode> {
    let mut items = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();

        if let Some(pos) = lower.find("<title>") {
            let after = &trimmed[pos + 7..];
            let title = after.split("</").next().unwrap_or("").trim();
            if !title.is_empty() {
                items.push((1, format!("<title> {}", title), line_idx));
            }
        }

        for h in 1..=6 {
            let tag = format!("<h{}", h);
            if let Some(pos) = lower.find(&tag) {
                let after_tag = &trimmed[pos + 3..];
                if let Some(close_bracket) = after_tag.find('>') {
                    let content = &after_tag[close_bracket + 1..];
                    let text = content.split("</").next().unwrap_or("").trim();
                    if !text.is_empty() {
                        items.push((h, format!("H{}: {}", h, text), line_idx));
                    }
                }
            }
        }
    }

    build_tree_from_levels(items)
}

/// Parses CSS content and extracts selectors and @media queries.
pub fn parse_css_outline(content: &str) -> Vec<OutlineNode> {
    let mut items = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("/*") || trimmed.starts_with('*') || trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('@') && trimmed.contains('{') {
            let name = trimmed.split('{').next().unwrap_or("").trim();
            items.push((1, name.to_string(), line_idx));
        } else if trimmed.ends_with('{') {
            let selector = trimmed.trim_end_matches('{').trim();
            if !selector.is_empty() && !selector.starts_with('@') {
                let leading_spaces = line.chars().take_while(|c| c.is_whitespace()).count();
                let level = if leading_spaces > 0 { 2 } else { 1 };
                items.push((level, selector.to_string(), line_idx));
            }
        }
    }

    build_tree_from_levels(items)
}

/// Extracts outline for a given language / extension, utilizing installed WASM plugins
/// first, with built-in fallbacks for Markdown, Rust, Python, Go, JSON, HTML, and CSS.
pub fn extract_outline(
    plugin_manager: Option<&mut crate::plugin::PluginManager>,
    lang_or_ext: &str,
    content: &str,
) -> Vec<OutlineNode> {
    if let Some(pm) = plugin_manager {
        if let Some(nodes) = pm.parse_outline(lang_or_ext, content) {
            if !nodes.is_empty() {
                return nodes;
            }
        }
    }

    let ext = lang_or_ext.to_lowercase();
    match ext.as_str() {
        "md" | "markdown" => parse_markdown_outline(content),
        "rs" | "rust" => parse_rust_outline(content),
        "py" | "python" => parse_python_outline(content),
        "go" | "golang" => parse_go_outline(content),
        "json" => parse_json_outline(content),
        "html" | "htm" => parse_html_outline(content),
        "css" => parse_css_outline(content),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_outline_parsing() {
        let md = r#"# Main Title
Intro text

## Section 1
Content 1

```markdown
### Code block heading (should be ignored)
```

### Subsection 1.1
Content 1.1

## Section 2
Content 2
"#;
        let outline = parse_markdown_outline(md);
        assert_eq!(outline.len(), 1);
        assert_eq!(outline[0].title, "Main Title");
        assert_eq!(outline[0].line, 0);
        assert_eq!(outline[0].children.len(), 2);

        let s1 = &outline[0].children[0];
        assert_eq!(s1.title, "Section 1");
        assert_eq!(s1.line, 3);
        assert_eq!(s1.children.len(), 1);
        assert_eq!(s1.children[0].title, "Subsection 1.1");
        assert_eq!(s1.children[0].line, 10);

        let s2 = &outline[0].children[1];
        assert_eq!(s2.title, "Section 2");
        assert_eq!(s2.line, 13);
    }

    #[test]
    fn test_flatten_outline() {
        let mut n1 = OutlineNode::new("Root".to_string(), 1, 0);
        let n2 = OutlineNode::new("Child".to_string(), 2, 5);
        n1.children.push(n2);

        let mut flat = Vec::new();
        flatten_outline(&[n1.clone()], 0, &mut flat);
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].title, "Root");
        assert_eq!(flat[1].title, "Child");
        assert_eq!(flat[1].depth, 1);

        // Test collapsed
        n1.is_expanded = false;
        let mut flat_collapsed = Vec::new();
        flatten_outline(&[n1], 0, &mut flat_collapsed);
        assert_eq!(flat_collapsed.len(), 1);
    }

    #[test]
    fn test_rust_outline_parsing() {
        let code = r#"
mod foo;

pub struct Bar {
    x: i32,
}

pub enum Baz {
    One,
}

impl Bar {
    pub fn new() -> Self {
        Self { x: 0 }
    }
}

pub async fn run() {
}
"#;
        let nodes = parse_rust_outline(code);
        let titles: Vec<&str> = nodes.iter().map(|n| n.title.as_str()).collect();
        assert!(titles.contains(&"mod foo"));
        assert!(titles.contains(&"struct Bar"));
        assert!(titles.contains(&"enum Baz"));
        assert!(titles.contains(&"impl Bar"));
        assert!(titles.contains(&"fn run()"));
        // impl Bar should contain fn new() as child
        let bar_impl = nodes.iter().find(|n| n.title == "impl Bar").unwrap();
        assert_eq!(bar_impl.children.len(), 1);
        assert_eq!(bar_impl.children[0].title, "fn new()");
    }

    #[test]
    fn test_python_outline_parsing() {
        let code = r#"
class MyService:
    def __init__(self):
        pass

    async def start(self):
        pass

def standalone_func():
    pass
"#;
        let nodes = parse_python_outline(code);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].title, "class MyService");
        assert_eq!(nodes[0].children.len(), 2);
        assert_eq!(nodes[0].children[0].title, "def __init__()");
        assert_eq!(nodes[0].children[1].title, "def start()");
        assert_eq!(nodes[1].title, "def standalone_func()");
    }

    #[test]
    fn test_go_outline_parsing() {
        let code = r#"
type Server struct {}

func NewServer() *Server {
    return &Server{}
}

func (s *Server) Start() error {
    return nil
}
"#;
        let nodes = parse_go_outline(code);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].title, "type Server struct");
        assert_eq!(nodes[1].title, "func NewServer()");
        assert_eq!(nodes[2].title, "(s *Server) Start");
    }

    #[test]
    fn test_json_outline_parsing() {
        let json = r#"{
  "name": "zee",
  "dependencies": {
    "serde": "1.0"
  }
}"#;
        let nodes = parse_json_outline(json);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].title, "\"name\"");
        assert_eq!(nodes[1].title, "\"dependencies\": {…}");
    }

    #[test]
    fn test_html_and_css_outline_parsing() {
        let html = r#"<!DOCTYPE html>
<html>
<head><title>Test Page</title></head>
<body>
  <h1>Welcome</h1>
  <h2>Subheading</h2>
</body>
</html>"#;
        let html_nodes = parse_html_outline(html);
        assert_eq!(html_nodes.len(), 2); // <title> and <h1> are root nodes
        assert_eq!(html_nodes[0].title, "<title> Test Page");
        assert_eq!(html_nodes[1].title, "H1: Welcome");
        assert_eq!(html_nodes[1].children.len(), 1);
        assert_eq!(html_nodes[1].children[0].title, "H2: Subheading");

        let css = r#"
@media (max-width: 600px) {
  .nav {
    display: none;
  }
}

.button {
  color: red;
}
"#;
        let css_nodes = parse_css_outline(css);
        assert_eq!(css_nodes.len(), 2);
        assert_eq!(css_nodes[0].title, "@media (max-width: 600px)");
        assert_eq!(css_nodes[1].title, ".button");
    }
}

