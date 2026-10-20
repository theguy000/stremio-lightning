//! Themes: `*.theme.css` files restyle the native UI by stremio-web class name.
//!
//! stremio-web styles its components with CSS classes, and Stremio Lightning themes target
//! them. Slint has no CSS, so a theme is read here and its rules are copied into the
//! `ClassStyles` global (`ui/class-styles.slint`), which every component that reuses a web
//! class name reads.
//!
//! Understood:
//! - Selectors `.name`, `[class*="name"]`, `[class^="name"]` and `[class="name"]`, comma
//!   separated. `.name` is an exact class. The `[class...]` forms compare against the class
//!   name without the hash the web build appends, so `[class*="title"]` reaches every
//!   component whose class contains `title`.
//! - Properties `color`, `background` / `background-color` (a colour, not a gradient),
//!   `opacity`, `visibility: hidden | visible` and `display: none`. Hiding keeps the layout
//!   space, like `visibility: hidden` on the web.
//! - Colours as `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()` and `transparent`.
//!
//! Everything else is reported on stderr and skipped. Specificity is not computed: every
//! supported selector weighs the same, so a later declaration wins and files load in file name
//! order. An element that carries several web classes is reached through any of them (the menu
//! toggle carries five), and the file order decides between them, not the class list. Themes
//! live in the same folder the other shells use, `<data_local_dir>/stremio-lightning/themes`.

use std::fmt;

use slint::{Color, ComponentHandle};
use stremio_lightning_core::mods::{list_mods, read_mod_content, ModType};

use crate::{ClassStyle, ClassStyles, MainWindow};

type Setter = fn(&ClassStyles<'_>, ClassStyle);

/// The elements a theme can reach, each with the web classes it carries. The first class is the
/// element's id in `ui/*.slint`, and a rule that selects any of its classes applies to it. Each
/// element needs an `in property` in `ui/class-styles.slint` named after its id, which is the
/// setter named here.
macro_rules! elements {
    ($($setter:ident => [$($class:literal),+]),+ $(,)?) => {
        const ELEMENTS: &[(&[&str], Setter)] = &[
            $((&[$($class),+], |globals: &ClassStyles<'_>, style| globals.$setter(style))),+
        ];
    };
}

elements! {
    set_board_row => ["board-row"],
    set_header_container => ["header-container"],
    set_title_container => ["title-container"],
    set_see_all_container => ["see-all-container"],
    set_meta_items_container => ["meta-items-container"],
    set_meta_item_link => ["meta-item-link"],
    set_poster_container => ["poster-container"],
    set_poster_image => ["poster-image"],
    set_title_bar_container => ["title-bar-container"],
    set_title_label => ["title-label"],
    set_horizontal_nav_bar_container => ["horizontal-nav-bar-container", "horizontal-nav-bar"],
    set_logo_container => ["logo-container"],
    set_logo => ["logo"],
    set_search_bar_container => ["search-bar-container", "search-bar"],
    set_search_input => ["search-input"],
    set_placeholder_label => ["placeholder-label"],
    set_submit_button_container => ["submit-button-container", "button-container"],
    set_buttons_container => ["buttons-container"],
    set_button_container => ["button-container"],
    set_menu_toggle => ["menu-toggle", "label-container", "nav-menu-popup-label", "menu-button-container", "button-container"],
    set_avatar => ["avatar"],
    set_vertical_nav_bar_container => ["vertical-nav-bar-container", "vertical-nav-bar"],
    set_nav_tab_button_container => ["nav-tab-button-container", "nav-tab-button"],
    set_icon => ["icon"],
    set_select_input => ["select-input", "multiselect-menu", "select-input-container"],
    set_dropdown => ["dropdown"],
    set_option => ["option"],
    set_placeholder => ["placeholder"],
    set_title => ["title"],
    set_image_container => ["image-container"],
    set_image => ["image"],
    set_overview => ["overview"],
    set_point => ["point"],
    set_text => ["text"],
    set_button => ["button", "button-container"],
    set_label => ["label"],
    set_addon => ["addon", "animation-fade-in", "addon-container", "button-container"],
    set_selectable_inputs_container => ["selectable-inputs-container"],
    set_add_button_container => ["add-button-container", "button-container"],
    set_add_button_label => ["add-button-label"],
    set_info_container => ["info-container"],
    set_name_container => ["name-container"],
    set_version_container => ["version-container"],
    set_types_container => ["types-container"],
    set_description_container => ["description-container"],
    set_action_buttons_container => ["action-buttons-container"],
    set_uninstall_button_container => ["uninstall-button-container", "button-container"],
    set_share_button_container => ["share-button-container", "button-container"],
    set_search => ["search", "text"],
    set_message_container => ["message-container"],
    set_menu => ["menu"],
    set_version_info_label => ["version-info-label"],
    set_sections_container => ["sections-container"],
    set_section => ["section"],
    set_user => ["user"],
    set_avatar_container => ["avatar-container"],
    set_email_label => ["email-label"],
    set_link => ["link", "button-container"],
    // Intro & authentication overlay
    set_intro_container => ["intro-container"],
    set_background_container => ["background-container"],
    set_close_button_container => ["close-button-container", "close-button", "dismiss-button", "button-container"],
    set_heading_container => ["heading-container"],
    set_slogan_container => ["slogan-container", "slogan"],
    set_content_container => ["content-container"],
    set_form_container => ["form-container"],
    set_credentials_text_input => ["credentials-text-input", "text-input"],
    set_forgot_password_link_container => ["forgot-password-link-container"],
    set_forgot_password_link => ["forgot-password-link", "link", "button-container"],
    set_error_message => ["error-message", "message-container"],
    set_submit_button => ["submit-button", "form-button", "button-container"],
    set_checkbox_container => ["checkbox-container", "checkboxes-container"],
    set_checkbox => ["checkbox"],
    set_options_container => ["options-container"],
    set_facebook_button => ["facebook-button", "form-button", "button-container"],
    set_apple_button => ["apple-button", "form-button", "button-container"],
    set_login_form_button => ["login-form-button", "form-button", "button-container"],
    set_signup_form_button => ["signup-form-button", "form-button", "button-container"],
    set_guest_login_button => ["guest-login-button", "form-button", "button-container"],
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rgba(u8, u8, u8, u8);

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Style {
    color: Option<Rgba>,
    background: Option<Rgba>,
    opacity: Option<f32>,
    hidden: Option<bool>,
}

impl Style {
    fn merge(&mut self, later: Self) {
        self.color = later.color.or(self.color);
        self.background = later.background.or(self.background);
        self.opacity = later.opacity.or(self.opacity);
        self.hidden = later.hidden.or(self.hidden);
    }
}

#[derive(Debug, PartialEq)]
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

#[derive(Debug)]
struct Rule {
    selectors: Vec<Selector>,
    style: Style,
}

#[derive(Debug, Default)]
struct ParsedTheme {
    rules: Vec<Rule>,
    warnings: Vec<String>,
}

fn is_class_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|d| d * 17);
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        3 => Some(Rgba(digit(0)?, digit(1)?, digit(2)?, 255)),
        4 => Some(Rgba(digit(0)?, digit(1)?, digit(2)?, digit(3)?)),
        6 => Some(Rgba(pair(0)?, pair(2)?, pair(4)?, 255)),
        8 => Some(Rgba(pair(0)?, pair(2)?, pair(4)?, pair(6)?)),
        _ => None,
    }
}

fn fraction(text: &str) -> Option<f32> {
    let value = match text.strip_suffix('%') {
        Some(percent) => percent.parse::<f32>().ok()? / 100.0,
        None => text.parse::<f32>().ok()?,
    };
    (0.0..=1.0).contains(&value).then_some(value)
}

fn parse_color(value: &str) -> Option<Rgba> {
    if value == "transparent" {
        return Some(Rgba(0, 0, 0, 0));
    }
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex(hex);
    }
    let args = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = args
        .split([',', ' ', '/'])
        .filter(|part| !part.is_empty())
        .collect();
    let alpha = match parts.as_slice() {
        [_, _, _] => 255,
        [_, _, _, alpha] => (fraction(alpha)? * 255.0).round() as u8,
        _ => return None,
    };
    let channel = |part: &str| {
        part.parse::<f32>()
            .ok()
            .filter(|v| (0.0..=255.0).contains(v))
            .map(|v| v.round() as u8)
    };
    Some(Rgba(
        channel(parts[0])?,
        channel(parts[1])?,
        channel(parts[2])?,
        alpha,
    ))
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

fn parse_theme(css: &str) -> ParsedTheme {
    let css = strip_comments(css);
    let mut theme = ParsedTheme::default();
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
        if !statements.trim().is_empty() {
            theme
                .warnings
                .push(format!("skipped `{}`", statements.trim()));
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

/// One merged style per entry of `ELEMENTS`, plus a warning for each selector that reaches none.
/// Rules apply in file order. All supported selectors weigh the same, so the last rule that
/// reaches an element wins, whichever of the element's classes it names.
fn resolve(rules: &[Rule]) -> (Vec<Style>, Vec<String>) {
    let mut styles = vec![Style::default(); ELEMENTS.len()];
    let mut warnings = Vec::new();
    for rule in rules {
        for selector in &rule.selectors {
            let mut matched = false;
            for ((classes, _), style) in ELEMENTS.iter().zip(&mut styles) {
                if classes.iter().any(|class| selector.matches(class)) {
                    style.merge(rule.style);
                    matched = true;
                }
            }
            if !matched {
                warnings.push(format!("{selector} matches no component in the native UI"));
            }
        }
    }
    (styles, warnings)
}

fn to_slint(style: Style) -> ClassStyle {
    let color = |rgba: Rgba| Color::from_argb_u8(rgba.3, rgba.0, rgba.1, rgba.2);
    ClassStyle {
        has_color: style.color.is_some(),
        color: style.color.map_or_else(Color::default, color),
        has_background: style.background.is_some(),
        background: style.background.map_or_else(Color::default, color),
        has_opacity: style.opacity.is_some(),
        opacity: style.opacity.unwrap_or(1.0),
        hidden: style.hidden.unwrap_or(false),
    }
}

/// Reads every installed theme and writes the result into the window's `ClassStyles`.
pub fn apply(window: &MainWindow) {
    let Some(root) = dirs::data_local_dir() else {
        return;
    };
    let mut files = match list_mods(&root, ModType::Theme) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("themes: {error}");
            return;
        }
    };
    files.sort_by(|a, b| a.filename.cmp(&b.filename));

    let mut rules = Vec::new();
    for file in files {
        match read_mod_content(&root, &file.filename, ModType::Theme) {
            Ok(css) => {
                let theme = parse_theme(&css);
                for warning in &theme.warnings {
                    eprintln!("theme {}: {warning}", file.filename);
                }
                rules.extend(theme.rules);
            }
            Err(error) => eprintln!("theme {}: {error}", file.filename),
        }
    }
    if rules.is_empty() {
        return;
    }

    let (styles, unmatched) = resolve(&rules);
    for warning in unmatched {
        eprintln!("themes: {warning}");
    }
    let globals = window.global::<ClassStyles>();
    for ((_, set_element), style) in ELEMENTS.iter().zip(styles) {
        set_element(&globals, to_slint(style));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style_of(styles: &[Style], element: &str) -> Style {
        let index = ELEMENTS
            .iter()
            .position(|(classes, _)| classes[0] == element)
            .expect("a known element");
        styles[index]
    }

    fn resolved(css: &str) -> (Vec<Style>, Vec<String>) {
        let theme = parse_theme(css);
        let (styles, unmatched) = resolve(&theme.rules);
        (
            styles,
            theme.warnings.into_iter().chain(unmatched).collect(),
        )
    }

    #[test]
    fn reads_colours_opacity_and_hiding() {
        let (styles, warnings) = resolved(
            "/** @name Demo */\n\
             .title-label { color: #fff; opacity: 0.5 !important }\n\
             [class*=\"see-all\"] { visibility: hidden; background: rgba(255, 0, 0, 0.5) }",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let title = style_of(&styles, "title-label");
        assert_eq!(title.color, Some(Rgba(255, 255, 255, 255)));
        assert_eq!(title.opacity, Some(0.5));
        let see_all = style_of(&styles, "see-all-container");
        assert_eq!(see_all.hidden, Some(true));
        assert_eq!(see_all.background, Some(Rgba(255, 0, 0, 128)));
        assert_eq!(style_of(&styles, "board-row"), Style::default());
    }

    #[test]
    fn a_later_declaration_wins() {
        let (styles, _) = resolved(
            ".poster-image { visibility: hidden; opacity: 0.2 } .poster-image { visibility: visible }",
        );
        let poster = style_of(&styles, "poster-image");
        assert_eq!(poster.hidden, Some(false));
        assert_eq!(poster.opacity, Some(0.2));
    }

    #[test]
    fn substring_selectors_reach_every_class_containing_the_name() {
        let (styles, _) = resolved("[class*=\"title\"] { color: #f00 }");
        for class in ["title-container", "title-bar-container", "title-label"] {
            assert!(style_of(&styles, class).color.is_some(), "{class}");
        }
        assert!(style_of(&styles, "poster-image").color.is_none());
    }

    #[test]
    fn unsupported_input_is_reported_and_skipped() {
        let (styles, warnings) = resolved(
            "@media (max-width: 600px) { .title-label { color: #fff } }\n\
             .poster-image > img { opacity: 0 }\n\
             .nothing-like-this { color: #fff }\n\
             .title-label { opacity: 0.2; width: 10px }",
        );
        assert_eq!(warnings.len(), 4, "{warnings:?}");
        let title = style_of(&styles, "title-label");
        assert_eq!(title.color, None);
        assert_eq!(title.opacity, Some(0.2));
    }

    #[test]
    fn a_later_rule_wins_whichever_class_of_an_element_it_names() {
        // The menu toggle carries both classes, so the file decides and not the class list.
        let (styles, warnings) = resolved(
            ".button-container { background: #800080 } .menu-toggle { background: #00ffff }",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let cyan = Some(Rgba(0, 255, 255, 255));
        let purple = Some(Rgba(128, 0, 128, 255));
        assert_eq!(style_of(&styles, "menu-toggle").background, cyan);
        assert_eq!(style_of(&styles, "button-container").background, purple);
        assert_eq!(
            style_of(&styles, "submit-button-container").background,
            purple
        );

        let (styles, _) = resolved(
            ".menu-toggle { background: #00ffff } .button-container { background: #800080 }",
        );
        assert_eq!(style_of(&styles, "menu-toggle").background, purple);
    }

    #[test]
    fn a_rule_reaches_an_element_through_any_of_its_classes() {
        let (styles, warnings) =
            resolved(".nav-tab-button { color: #f00 } .label-container { opacity: 0.5 }");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(style_of(&styles, "nav-tab-button-container")
            .color
            .is_some());
        assert_eq!(style_of(&styles, "menu-toggle").opacity, Some(0.5));
        assert_eq!(
            style_of(&styles, "vertical-nav-bar-container"),
            Style::default()
        );
    }

    #[test]
    fn colour_forms() {
        assert_eq!(parse_color("#abc"), Some(Rgba(0xaa, 0xbb, 0xcc, 255)));
        assert_eq!(parse_color("#11223344"), Some(Rgba(0x11, 0x22, 0x33, 0x44)));
        assert_eq!(parse_color("rgb(1, 2, 3)"), Some(Rgba(1, 2, 3, 255)));
        assert_eq!(
            parse_color("rgb(255 0 0 / 50%)"),
            Some(Rgba(255, 0, 0, 128))
        );
        assert_eq!(parse_color("transparent"), Some(Rgba(0, 0, 0, 0)));
        assert_eq!(parse_color("rgb(300, 0, 0)"), None);
        assert_eq!(parse_color("red"), None);
    }

    #[test]
    fn intro_components_theming() {
        let (styles, warnings) = resolved(
            ".intro-container { background: #101020 }\n\
             .credentials-text-input { background: #202030; color: #ffffff }\n\
             .form-button { background: #303040 }\n\
             .facebook-button { background: #1877f1 }\n\
             .error-message { color: #ff5555 }\n\
             .slogan { opacity: 0.8 }\n\
             .checkbox { visibility: hidden }",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            style_of(&styles, "intro-container").background,
            Some(Rgba(0x10, 0x10, 0x20, 255))
        );
        assert_eq!(
            style_of(&styles, "credentials-text-input").background,
            Some(Rgba(0x20, 0x20, 0x30, 255))
        );
        assert_eq!(
            style_of(&styles, "credentials-text-input").color,
            Some(Rgba(255, 255, 255, 255))
        );
        // Shared .form-button matches submit, apple, and guest buttons
        assert_eq!(
            style_of(&styles, "submit-button").background,
            Some(Rgba(0x30, 0x30, 0x40, 255))
        );
        assert_eq!(
            style_of(&styles, "apple-button").background,
            Some(Rgba(0x30, 0x30, 0x40, 255))
        );
        assert_eq!(
            style_of(&styles, "guest-login-button").background,
            Some(Rgba(0x30, 0x30, 0x40, 255))
        );
        // Later .facebook-button rule overrides .form-button for facebook-button
        assert_eq!(
            style_of(&styles, "facebook-button").background,
            Some(Rgba(0x18, 0x77, 0xf1, 255))
        );
        assert_eq!(
            style_of(&styles, "error-message").color,
            Some(Rgba(0xff, 0x55, 0x55, 255))
        );
        assert_eq!(style_of(&styles, "slogan-container").opacity, Some(0.8));
        assert_eq!(style_of(&styles, "checkbox").hidden, Some(true));
    }
}
