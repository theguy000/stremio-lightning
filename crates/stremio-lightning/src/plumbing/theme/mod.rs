//! Themes: `*.theme.css` files restyle the UI by stremio-web class name.
//!
//! Pure data and disk I/O only — zero Slint or screen imports. `shell::theme_apply` maps the
//! resulting `ResolvedTheme` onto the window's `ClassStyles` global.

use std::path::Path;

mod color;
pub mod parse;
pub mod store;

pub use parse::{Rgba, Rule, Style};

/// The 102 live UI elements a theme can reach, in the exact order `shell::theme_apply` binds to
/// `ui/class-styles.slint`. The first class of each entry is its id; a rule selecting any of its
/// classes applies to it.
pub const CLASSES: &[&[&str]] = &[
    &["board-row"],
    &["header-container"],
    &["title-container"],
    &["see-all-container"],
    &["meta-items-container"],
    &["meta-item-link"],
    &["poster-container"],
    &["poster-image"],
    &["title-bar-container"],
    &["title-label"],
    &["horizontal-nav-bar-container", "horizontal-nav-bar"],
    &["logo-container"],
    &["logo"],
    &["search-bar-container", "search-bar"],
    &["search-input"],
    &["placeholder-label"],
    &["submit-button-container", "button-container"],
    &["buttons-container"],
    &["button-container"],
    &[
        "menu-toggle",
        "label-container",
        "nav-menu-popup-label",
        "menu-button-container",
        "button-container",
    ],
    &["avatar"],
    &["vertical-nav-bar-container", "vertical-nav-bar"],
    &["nav-tab-button-container", "nav-tab-button"],
    &["icon"],
    &["select-input", "multiselect-menu", "select-input-container"],
    &["dropdown"],
    &["option"],
    &["placeholder"],
    &["title"],
    &["image-container"],
    &["image"],
    &["point"],
    &["text"],
    &["button", "button-container"],
    &["label"],
    &[
        "addon",
        "animation-fade-in",
        "addon-container",
        "button-container",
    ],
    &["selectable-inputs-container"],
    &["add-button-container", "button-container"],
    &["add-button-label"],
    &["info-container"],
    &["name-container"],
    &["version-container"],
    &["types-container"],
    &["description-container"],
    &["action-buttons-container"],
    &["uninstall-button-container", "button-container"],
    &["install-button-container", "button-container"],
    &["share-button-container", "button-container"],
    &["search", "text"],
    &["message-container"],
    &["menu"],
    &["version-info-label"],
    &["section"],
    &["avatar-container"],
    &["email-label"],
    &["link", "button-container"],
    &["videos-list-container"],
    &["video-container"],
    &["intro-container"],
    &["background-container"],
    &[
        "close-button-container",
        "close-button",
        "dismiss-button",
        "button-container",
    ],
    &["heading-container"],
    &["slogan-container", "slogan"],
    &["content-container"],
    &["form-container"],
    &["credentials-text-input", "text-input"],
    &["forgot-password-link-container"],
    &["forgot-password-link", "link", "button-container"],
    &["error-message", "message-container"],
    &["submit-button", "form-button", "button-container"],
    &["checkbox-container", "checkboxes-container"],
    &["checkbox"],
    &["options-container"],
    &["facebook-button", "form-button", "button-container"],
    &["apple-button", "form-button", "button-container"],
    &["login-form-button", "form-button", "button-container"],
    &["signup-form-button", "form-button", "button-container"],
    &["guest-login-button", "form-button", "button-container"],
    &["mods-menu-container"],
    &["mods-menu-button"],
    &["mods-content"],
    &["mods-page-title"],
    &["mods-page-note"],
    &["mod-card"],
    &["mod-title"],
    &["mod-byline"],
    &["mod-description"],
    &["mod-warning"],
    &["mod-notice"],
    &["mod-toggle-container"],
    &["mod-toggle"],
    &["mod-remove-button"],
    &["mod-remove-button-label"],
    &["mods-empty"],
    &["mods-log-time"],
    &["mods-log-source"],
    &["mods-log-level"],
    &["mods-log-text"],
    &["mods-log-detail"],
    &["mods-log-detail-text"],
    &["mods-about-label"],
    &["mods-about-value"],
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeWarning {
    pub filename: String,
    pub message: String,
    pub unreadable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedTheme {
    pub amoled: bool,
    pub styles: Vec<Style>,
    pub warnings: Vec<ThemeWarning>,
}

/// Unsupported declarations and unmatched selectors in one theme file.
pub fn warnings(css: &str) -> Vec<String> {
    inspect(css).warnings
}

/// Parses and resolves one theme's CSS text as plain data.
pub fn inspect(css: &str) -> InspectedTheme {
    let theme = parse::parse_theme(css);
    let (styles, unmatched) = parse::resolve(&theme.rules, CLASSES);
    let mut warnings = theme.warnings;
    warnings.extend(unmatched);
    InspectedTheme { styles, warnings }
}

/// Reads and resolves all enabled themes in order, preserving per-file warning attribution.
pub fn resolve_files(data_dir: &Path, enabled: &[String]) -> ResolvedTheme {
    let mut warnings = Vec::new();
    let mut amoled = false;
    let mut all_rules = Vec::new();

    for filename in enabled {
        if filename == "builtin:amoled" {
            amoled = true;
            continue;
        }
        if filename.starts_with("builtin:") {
            continue;
        }
        match store::read(data_dir, filename) {
            Ok(css) => {
                let parsed = parse::parse_theme(&css);
                let (_, unmatched) = parse::resolve(&parsed.rules, CLASSES);
                warnings.extend(parsed.warnings.into_iter().chain(unmatched).map(|message| {
                    ThemeWarning {
                        filename: filename.clone(),
                        message,
                        unreadable: false,
                    }
                }));
                all_rules.extend(parsed.rules);
            }
            Err(message) => warnings.push(ThemeWarning {
                filename: filename.clone(),
                message,
                unreadable: true,
            }),
        }
    }

    let (styles, _) = parse::resolve(&all_rules, CLASSES);
    ResolvedTheme {
        amoled,
        styles,
        warnings,
    }
}

/// What one theme file does, as plain data.
pub struct InspectedTheme {
    styles: Vec<Style>,
    /// Everything the file said that this app does not understand, or that reaches no component.
    pub warnings: Vec<String>,
}

impl InspectedTheme {
    /// The style the file gives the component with this id, or `None` if this app has no such
    /// component.
    pub fn style_of(&self, id: &str) -> Option<Style> {
        let index = CLASSES.iter().position(|classes| classes[0] == id)?;
        Some(self.styles[index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_colours_opacity_and_hiding() {
        let theme = inspect(
            "/** @name Demo */\n             .title-label { color: #fff; opacity: 0.5 !important }\n             [class*=\"see-all\"] { visibility: hidden; background: rgba(255, 0, 0, 0.5) }",
        );
        assert!(theme.warnings.is_empty(), "{:?}", theme.warnings);
        let title = theme.style_of("title-label").unwrap();
        assert_eq!(title.color, Some(Rgba(255, 255, 255, 255)));
        assert_eq!(title.opacity, Some(0.5));
        let see_all = theme.style_of("see-all-container").unwrap();
        assert_eq!(see_all.hidden, Some(true));
        assert_eq!(see_all.background, Some(Rgba(255, 0, 0, 128)));
        assert_eq!(theme.style_of("board-row"), Some(Style::default()));
    }

    #[test]
    fn substring_selectors_and_file_order_precedence() {
        let sub = inspect("[class*=\"title\"] { color: #f00 }");
        for class in ["title-container", "title-bar-container", "title-label"] {
            assert!(sub.style_of(class).unwrap().color.is_some(), "{class}");
        }
        assert!(sub.style_of("poster-image").unwrap().color.is_none());

        let cyan = Some(Rgba(0, 255, 255, 255));
        let purple = Some(Rgba(128, 0, 128, 255));
        let first = inspect(
            ".button-container { background: #800080 } .menu-toggle { background: #00ffff }",
        );
        assert_eq!(first.style_of("menu-toggle").unwrap().background, cyan);
        assert_eq!(
            first
                .style_of("submit-button-container")
                .unwrap()
                .background,
            purple
        );

        let second = inspect(
            ".menu-toggle { background: #00ffff } .button-container { background: #800080 }",
        );
        assert_eq!(second.style_of("menu-toggle").unwrap().background, purple);
    }

    #[test]
    fn a_rule_reaches_an_element_through_any_of_its_classes_and_intro_styles() {
        let theme = inspect(
            ".nav-tab-button { color: #f00 } .label-container { opacity: 0.5 }\n             .form-button { background: #303040 } .facebook-button { background: #1877f1 }\n             .slogan { opacity: 0.8 } .checkbox { visibility: hidden }",
        );
        assert!(theme.warnings.is_empty(), "{:?}", theme.warnings);
        assert!(theme
            .style_of("nav-tab-button-container")
            .unwrap()
            .color
            .is_some());
        assert_eq!(theme.style_of("menu-toggle").unwrap().opacity, Some(0.5));
        assert_eq!(
            theme.style_of("submit-button").unwrap().background,
            Some(Rgba(0x30, 0x30, 0x40, 255))
        );
        assert_eq!(
            theme.style_of("facebook-button").unwrap().background,
            Some(Rgba(0x18, 0x77, 0xf1, 255))
        );
        assert_eq!(
            theme.style_of("slogan-container").unwrap().opacity,
            Some(0.8)
        );
        assert_eq!(theme.style_of("checkbox").unwrap().hidden, Some(true));
    }
}
