use serde::{Deserialize, Serialize};
use regex::Regex;
use std::ops::Range;
use rayon::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Keyword,
    TypeName,
    Function,
    String,
    Number,
    Comment,
    Operator,
    Punctuation,
    Constant,
    Attribute,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxDefinition {
    pub meta: SyntaxMeta,
    #[serde(rename = "rule", default)]
    pub rules: Vec<SyntaxRuleDef>,
}

impl SyntaxDefinition {
    pub fn builtins() -> Vec<Self> {
        let files = [
            include_str!("../../../assets/syntax/css.toml"),
            include_str!("../../../assets/syntax/go.toml"),
            include_str!("../../../assets/syntax/html.toml"),
            include_str!("../../../assets/syntax/javascript.toml"),
            include_str!("../../../assets/syntax/json.toml"),
            include_str!("../../../assets/syntax/markdown.toml"),
            include_str!("../../../assets/syntax/plain-text.toml"),
            include_str!("../../../assets/syntax/python.toml"),
            include_str!("../../../assets/syntax/rust.toml"),
            include_str!("../../../assets/syntax/swift.toml"),
            include_str!("../../../assets/syntax/toml.toml"),
            include_str!("../../../assets/syntax/xml.toml"),
        ];

        files
            .iter()
            .map(|s| toml::from_str(s).expect("failed to parse builtin syntax definition"))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxMeta {
    pub name: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxRuleDef {
    pub token: TokenType,
    pub pattern: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    #[serde(default)]
    pub word_boundary: bool,
    pub escape: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineState {
    Normal,
    InsideRegion { rule_index: usize },
}

#[derive(Debug, Clone)]
pub struct TokenSpan {
    pub byte_range: Range<usize>,
    pub token: TokenType,
}

pub struct CompiledRule {
    pub token: TokenType,
    pub start_re: Regex,
    pub end_re: Option<Regex>,
    pub escape: Option<char>,
}

pub struct SyntaxHighlighter {
    pub def: SyntaxDefinition,
    pub rules: Vec<CompiledRule>,
}

impl SyntaxHighlighter {
    pub fn new(def: SyntaxDefinition) -> anyhow::Result<Self> {
        let mut rules = Vec::new();
        for rule in &def.rules {
            if let Some(ref pattern) = rule.pattern {
                let pattern = if rule.word_boundary {
                    format!(r"\b{}\b", pattern)
                } else {
                    pattern.clone()
                };
                rules.push(CompiledRule {
                    token: rule.token,
                    start_re: Regex::new(&pattern)?,
                    end_re: None,
                    escape: None,
                });
            } else if let (Some(start), Some(end)) = (&rule.start, &rule.end) {
                rules.push(CompiledRule {
                    token: rule.token,
                    start_re: Regex::new(start)?,
                    end_re: Some(Regex::new(end)?),
                    escape: rule.escape.as_ref().and_then(|s| s.chars().next()),
                });
            }
        }
        Ok(Self { def, rules })
    }

    /// Linear pre-pass to resolve multi-line region boundaries.
    /// Returns the state at the START of each line.
    pub fn compute_line_states(&self, lines: &[String], initial_state: LineState) -> Vec<LineState> {
        let mut states = Vec::with_capacity(lines.len());
        let mut current_state = initial_state;

        for line in lines {
            states.push(current_state);
            current_state = self.next_line_state(line, current_state);
        }

        states
    }

    pub fn next_line_state(&self, line: &str, mut state: LineState) -> LineState {
        let mut offset = 0;
        while offset < line.len() {
            match state {
                LineState::Normal => {
                    let mut best_match: Option<(usize, usize, usize)> = None; // (rule_index, start, end)
                    for (i, rule) in self.rules.iter().enumerate() {
                        if let Some(m) = rule.start_re.find(&line[offset..]) {
                            if best_match.is_none() || m.start() < best_match.unwrap().1 {
                                best_match = Some((i, m.start(), m.end()));
                            }
                        }
                    }

                    if let Some((i, _start, end)) = best_match {
                        let rule = &self.rules[i];
                        if rule.end_re.is_some() {
                            state = LineState::InsideRegion { rule_index: i };
                            offset += end;
                        } else {
                            offset += end;
                        }
                    } else {
                        break;
                    }
                }
                LineState::InsideRegion { rule_index } => {
                    let rule = &self.rules[rule_index];
                    let end_re = rule.end_re.as_ref().unwrap();
                    
                    let mut search_offset = offset;
                    let mut found_end = false;
                    while let Some(m) = end_re.find(&line[search_offset..]) {
                        let end_pos = search_offset + m.start();
                        if let Some(esc) = rule.escape {
                            if end_pos > 0 && line.as_bytes()[end_pos - 1] as char == esc {
                                // Check if the escape is escaped
                                let mut esc_count = 0;
                                for c in line[..end_pos].chars().rev() {
                                    if c == esc { esc_count += 1; } else { break; }
                                }
                                if esc_count % 2 != 0 {
                                    // Escaped
                                    search_offset += m.end();
                                    continue;
                                }
                            }
                        }
                        offset = search_offset + m.end();
                        state = LineState::Normal;
                        found_end = true;
                        break;
                    }
                    if !found_end {
                        return state;
                    }
                }
            }
        }
        state
    }

    pub fn highlight_line(&self, line: &str, mut state: LineState) -> Vec<TokenSpan> {
        let mut spans = Vec::new();
        let mut offset = 0;

        while offset < line.len() {
            match state {
                LineState::Normal => {
                    let mut best_match: Option<(usize, usize, usize)> = None;
                    for (i, rule) in self.rules.iter().enumerate() {
                        if let Some(m) = rule.start_re.find(&line[offset..]) {
                            if best_match.is_none() || m.start() < best_match.unwrap().1 {
                                best_match = Some((i, m.start(), m.end()));
                            }
                        }
                    }

                    if let Some((i, m_start, m_end)) = best_match {
                        if m_start > 0 {
                            // Text before match
                            // We don't push spans for text yet, we'll merge them at the end.
                        }
                        let rule = &self.rules[i];
                        let abs_start = offset + m_start;
                        let abs_end = offset + m_end;
                        
                        if rule.end_re.is_some() {
                            state = LineState::InsideRegion { rule_index: i };
                            spans.push(TokenSpan {
                                byte_range: abs_start..abs_end,
                                token: rule.token,
                            });
                            offset = abs_end;
                        } else {
                            spans.push(TokenSpan {
                                byte_range: abs_start..abs_end,
                                token: rule.token,
                            });
                            offset = abs_end;
                        }
                    } else {
                        break;
                    }
                }
                LineState::InsideRegion { rule_index } => {
                    let rule = &self.rules[rule_index];
                    let end_re = rule.end_re.as_ref().unwrap();
                    
                    let mut search_offset = offset;
                    let mut found_end = false;
                    while let Some(m) = end_re.find(&line[search_offset..]) {
                        let end_pos = search_offset + m.start();
                        if let Some(esc) = rule.escape {
                            if end_pos > 0 && line.as_bytes()[end_pos - 1] as char == esc {
                                let mut esc_count = 0;
                                for c in line[..end_pos].chars().rev() {
                                    if c == esc { esc_count += 1; } else { break; }
                                }
                                if esc_count % 2 != 0 {
                                    search_offset += m.end();
                                    continue;
                                }
                            }
                        }
                        let abs_end = search_offset + m.end();
                        spans.push(TokenSpan {
                            byte_range: offset..abs_end,
                            token: rule.token,
                        });
                        offset = abs_end;
                        state = LineState::Normal;
                        found_end = true;
                        break;
                    }
                    if !found_end {
                        spans.push(TokenSpan {
                            byte_range: offset..line.len(),
                            token: rule.token,
                        });
                        offset = line.len();
                    }
                }
            }
        }
        spans
    }

    pub fn highlight_lines_parallel(&self, lines: &[String], line_states: &[LineState]) -> Vec<Vec<TokenSpan>> {
        lines.par_iter().zip(line_states.par_iter())
            .map(|(line, state)| self.highlight_line(line, *state))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_builtin_syntaxes_compile() {
        let builtins = SyntaxDefinition::builtins();
        assert!(builtins.iter().any(|b| b.meta.name == "JSON"));
        for def in builtins {
            let name = def.meta.name.clone();
            if let Err(e) = SyntaxHighlighter::new(def) {
                panic!("Failed to compile syntax {}: {}", name, e);
            }
        }
    }

    #[test]
    fn test_json_syntax_highlighting() {
        let builtins = SyntaxDefinition::builtins();
        let json_def = builtins.into_iter().find(|b| b.meta.name == "JSON").unwrap();
        let highlighter = SyntaxHighlighter::new(json_def).unwrap();

        let line = r#"{"name": "zee", "count": 42, "valid": true}"#;
        let spans = highlighter.highlight_line(line, LineState::Normal);
        assert!(!spans.is_empty());
        // Verify key, string, number, constant, punctuation are captured
        let has_type = spans.iter().any(|s| s.token == TokenType::TypeName);
        let has_string = spans.iter().any(|s| s.token == TokenType::String);
        let has_number = spans.iter().any(|s| s.token == TokenType::Number);
        let has_const = spans.iter().any(|s| s.token == TokenType::Constant);
        let has_punct = spans.iter().any(|s| s.token == TokenType::Punctuation);
        assert!(has_type);
        assert!(has_string);
        assert!(has_number);
        assert!(has_const);
        assert!(has_punct);
    }
}

