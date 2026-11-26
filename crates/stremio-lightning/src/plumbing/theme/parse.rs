//! Reading a theme file: CSS text in, one merged style per element out. No Slint here, so it
//! can be tested against a made-up element table.

use std::fmt;

use super::color::parse_color;
pub use super::color::Rgba;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Style {
    pub color: Option<Rgba>,
    pub background: Option<Rgba>,
    pub opacity: Option<f32>,
    pub hidden: Option<bool>,
}

impl Style {
    fn merge(&mut self, later: Self) {
        self.color = later.color.or(self.color);
        self.background = later.background.or(self.background);
        self.opacity = later.opacity.or(self.opacity);
        self.hidden = later.hidden.or(self.hidden);
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Selector {
    Exact(String),
    Contains(String),
    Prefix(String),
}

impl Selector {
    fn matches(&self, class: &str) -> bool {
        match self {
            Self::Exact(name) => class == name,
            Self::Contains(part) => class.contains(part.as_str()),
            Self::Prefix(prefix) => class.starts_with(prefix.as_str()),
        }
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(name) => write!(f, ".{name}"),
            Self::Contains(part) => write!(f, "[class*=\"{part}\"]"),
            Self::Prefix(prefix) => write!(f, "[class^=\"{prefix}\"]"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Rule {
    selectors: Vec<Selector>,
    style: Style,
}

#[derive(Debug, Default)]
pub struct Theme {
    pub rules: Vec<Rule>,
    /// Everything the file said that this app does not understand.
    pub warnings: Vec<String>,
}

fn is_class_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn parse_selector(text: &str) -> Option<Selector> {
    let text = text.trim();
    if let Some(name) = text.strip_prefix('.') {
        return is_class_name(name).then(|| Selector::Exact(name.to_string()));
    }
    let inner = text.strip_prefix("[class")?.strip_suffix(']')?;
    let (value, make): (&str, fn(String) -> Selector) = if let Some(v) = inner.strip_prefix("*=") {
        (v, Selector::Contains)
    } else if let Some(v) = inner.strip_prefix("^=") {
        (v, Selector::Prefix)
    } else {
        (inner.strip_prefix('=')?, Selector::Exact)
    };
    let value = value.trim().trim_matches(['"', '\'']);
    is_class_name(value).then(|| make(value.to_string()))
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Length of the block body that starts at `after_open`, up to its matching `}`.
fn block_len(after_open: &str) -> Option<usize> {
    let mut depth = 1;
    for (index, c) in after_open.char_indices() {
        if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn set<T>(slot: &mut Option<T>, value: Option<T>) -> bool {
    let understood = value.is_some();
    if understood {
        *slot = value;
    }
    understood
}

fn parse_declarations(body: &str, rule: &str, warnings: &mut Vec<String>) -> Style {
    let mut style = Style::default();
    for declaration in body.split(';').map(str::trim).filter(|d| !d.is_empty()) {
        let Some((property, value)) = declaration.split_once(':') else {
            warnings.push(format!("{rule}: cannot read `{declaration}`"));
            continue;
        };
        let property = property.trim().to_ascii_lowercase();
        let value = value.trim().to_ascii_lowercase();
        let value = value.trim_end_matches("!important").trim();
        let understood = match property.as_str() {
            "color" => set(&mut style.color, parse_color(value)),
            "background" | "background-color" => set(&mut style.background, parse_color(value)),
            "opacity" => set(
                &mut style.opacity,
                value
                    .parse::<f32>()
                    .ok()
                    .filter(|o| (0.0..=1.0).contains(o)),
            ),
            "visibility" => set(
                &mut style.hidden,
                match value {
                    "hidden" => Some(true),
                    "visible" => Some(false),
                    _ => None,
                },
            ),
            "display" => set(&mut style.hidden, (value == "none").then_some(true)),
            _ => false,
        };
        if !understood {
            warnings.push(format!("{rule}: `{declaration}` is not supported"));
        }
    }
    style
}

pub fn parse_theme(css: &str) -> Theme {
    let css = strip_comments(css);
    let mut theme = Theme::default();
    let mut rest = css.as_str();
    while let Some(open) = rest.find('{') {
        let Some(len) = block_len(&rest[open + 1..]) else {
            theme
                .warnings
                .push("a `{` is never closed, so the rest of the file is ignored".to_string());
            return theme;
        };
        let body = &rest[open + 1..open + 1 + len];
        let prelude = &rest[..open];
        rest = &rest[open + len + 2..];

        // Statements such as `@import ...;` can sit in front of the next rule.
        let (statements, prelude) = prelude.rsplit_once(';').unwrap_or(("", prelude));
        let stmt = statements.trim();
        if !stmt.is_empty() {
            theme.warnings.push(format!("skipped `{stmt}`"));
        }
        let prelude = prelude.trim();
        if prelude.starts_with('@') {
            theme
                .warnings
                .push(format!("`{prelude}` blocks are not supported"));
            continue;
        }

        let mut selectors = Vec::new();
        for text in prelude.split(',') {
            match parse_selector(text) {
                Some(selector) => selectors.push(selector),
                None => theme
                    .warnings
                    .push(format!("selector `{}` is not supported", text.trim())),
            }
        }
        if selectors.is_empty() {
            continue;
        }
        let style = parse_declarations(body, prelude, &mut theme.warnings);
        theme.rules.push(Rule { selectors, style });
    }
    theme
}

/// One merged style per entry of `elements`, each entry being the web classes one element
/// carries, plus a warning for each selector that reaches none. Rules apply in file order.
pub fn resolve(rules: &[Rule], elements: &[&[&str]]) -> (Vec<Style>, Vec<String>) {
    let mut styles = vec![Style::default(); elements.len()];
    let mut warnings = Vec::new();
    for rule in rules {
        for selector in &rule.selectors {
            let mut matched = false;
            for (classes, style) in elements.iter().zip(&mut styles) {
                if classes.iter().any(|class| selector.matches(class)) {
                    style.merge(rule.style);
                    matched = true;
                }
            }
            if !matched {
                warnings.push(format!("{selector} matches no component in the UI"));
            }
        }
    }
    (styles, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELEMENTS: &[&[&str]] = &[
        &["board-row"],
        &["title-label"],
        &["poster-image"],
        &["search-bar-container", "search-bar"],
    ];

    fn resolved(css: &str) -> (Vec<Style>, Vec<String>) {
        let theme = parse_theme(css);
        let (styles, unmatched) = resolve(&theme.rules, ELEMENTS);
        (
            styles,
            theme.warnings.into_iter().chain(unmatched).collect(),
        )
    }

    #[test]
    fn declarations_land_on_the_element_they_name() {
        let (styles, warnings) = resolved(".title-label { color: #fff; opacity: 0.5 !important }");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(styles[1].color, Some(Rgba(255, 255, 255, 255)));
        assert_eq!(styles[1].opacity, Some(0.5));
        assert_eq!(styles[0].color, None);
    }

    #[test]
    fn a_later_declaration_wins() {
        let (styles, _) = resolved(
            ".poster-image { visibility: hidden; opacity: 0.2 } .poster-image { visibility: visible }",
        );
        assert_eq!(styles[2].hidden, Some(false));
        assert_eq!(styles[2].opacity, Some(0.2));
    }

    #[test]
    fn a_rule_reaches_an_element_through_any_of_its_classes() {
        let (styles, warnings) = resolved(".search-bar { color: #f00 }");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(styles[3].color.is_some());
    }

    #[test]
    fn unsupported_input_is_reported_and_skipped() {
        let (styles, warnings) = resolved(
            "@media (max-width: 600px) { .title-label { color: #fff } }\n             .poster-image > img { opacity: 0 }\n             .nothing-like-this { color: #fff }\n             .title-label { opacity: 0.2; width: 10px }",
        );
        assert_eq!(warnings.len(), 4, "{warnings:?}");
        assert_eq!(styles[1].color, None);
        assert_eq!(styles[1].opacity, Some(0.2));
    }

    #[test]
    fn comments_and_leading_statements_are_skipped_not_misread() {
        let (styles, warnings) =
            resolved("/** @name Demo */\n@import \"other.css\";\n.title-label { color: #000 }");
        assert_eq!(
            warnings,
            vec!["skipped `@import \"other.css\"`".to_string()]
        );
        assert_eq!(styles[1].color, Some(Rgba(0, 0, 0, 255)));
    }
}
