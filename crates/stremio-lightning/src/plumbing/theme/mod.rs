//! Themes: `*.theme.css` files restyle the UI by stremio-web class name.
//!
//! stremio-web styles its components with CSS classes, and Stremio Lightning themes target
//! them. Slint has no CSS, so a theme is read (`parse`) and its rules are copied into the
//! `ClassStyles` global (`ui/class-styles.slint`), which every component that reuses a web
//! class name reads. This file holds the element table and the copying; `parse` holds the CSS.
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
//! Everything else is reported, to the theme's card and to the Logs page, and skipped. Specificity is not computed: every
//! supported selector weighs the same, so a later declaration wins. An element that carries
//! several web classes is reached through any of them (the menu toggle carries five), and the file
//! order decides between them, not the class list.
//!
//! A theme is on only when the Mods screen turned it on: `apply_enabled` reads `enabled_themes`
//! from the shell's own `settings.json` and applies those files, in the order the list names
//! them. Themes live in `<data_local_dir>/stremio-lightning/themes`, the folder the app's own
//! mod handling names.

use std::path::Path;

use slint::{Color, ComponentHandle};
use stremio_lightning_core::mods::{read_mod_content, ModType};

use crate::{ClassStyle, ClassStyles, MainWindow};

pub mod parse;

pub use parse::{Rgba, Rule, Style};

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
        const CLASSES: &[&[&str]] = &[$(&[$($class),+]),+];
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
    set_videos_list_container => ["videos-list-container"],
    set_video_container => ["video-container"],
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
    // Mods section: the frame and the two offline pages.
    set_mods_menu_container => ["mods-menu-container"],
    set_mods_menu_button => ["mods-menu-button"],
    set_mods_content => ["mods-content"],
    set_mods_page_title => ["mods-page-title"],
    set_mods_page_note => ["mods-page-note"],
    set_mod_card => ["mod-card"],
    set_mod_title => ["mod-title"],
    set_mod_byline => ["mod-byline"],
    set_mod_description => ["mod-description"],
    set_mod_warning => ["mod-warning"],
    set_mod_notice => ["mod-notice"],
    set_mod_toggle_container => ["mod-toggle-container"],
    set_mod_toggle => ["mod-toggle"],
    set_mod_remove_button => ["mod-remove-button"],
    set_mod_remove_button_label => ["mod-remove-button-label"],
    set_mods_empty => ["mods-empty"],
    // The Marketplace page, and the buttons that update the installed mods.
    set_market_card => ["market-card"],
    set_market_title => ["market-title"],
    set_market_byline => ["market-byline"],
    set_market_description => ["market-description"],
    set_market_heading => ["market-heading"],
    set_mod_update => ["mod-update"],
    set_market_button => ["market-button"],
    set_market_button_label => ["market-button-label"],
    // The Logs page and the About page.
    set_mods_log_filter => ["mods-log-filter"],
    set_mods_log_time => ["mods-log-time"],
    set_mods_log_source => ["mods-log-source"],
    set_mods_log_level => ["mods-log-level"],
    set_mods_log_text => ["mods-log-text"],
    set_mods_log_detail => ["mods-log-detail"],
    set_mods_log_detail_text => ["mods-log-detail-text"],
    set_mods_about_label => ["mods-about-label"],
    set_mods_about_value => ["mods-about-value"],
}

/// What this app does not understand in a theme file, one line each: the declarations it skips
/// and the selectors that reach no component. A card shows these before the theme is turned on.
pub fn warnings(css: &str) -> Vec<String> {
    inspect(css).warnings
}

/// Reads a theme file's text and answers what it does to this app's components, so a theme can
/// be checked without a window.
pub fn inspect(css: &str) -> InspectedTheme {
    let theme = parse::parse_theme(css);
    let (styles, unmatched) = parse::resolve(&theme.rules, CLASSES);
    let mut warnings = theme.warnings;
    warnings.extend(unmatched);
    InspectedTheme {
        ids: ELEMENTS.iter().map(|(classes, _)| classes[0]).collect(),
        styles,
        warnings,
    }
}

/// What one theme file does, as plain data.
pub struct InspectedTheme {
    ids: Vec<&'static str>,
    styles: Vec<parse::Style>,
    /// Everything the file said that this app does not understand, or that reaches no component.
    pub warnings: Vec<String>,
}

impl InspectedTheme {
    /// The style the file gives the component with this id, or `None` if this app has no such
    /// component.
    pub fn style_of(&self, id: &str) -> Option<parse::Style> {
        let index = self.ids.iter().position(|known| *known == id)?;
        Some(self.styles[index])
    }
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

/// Reads the themes `settings.json` lists as enabled -- in the order it lists them -- and writes
/// the result into the window's `ClassStyles`. An empty list leaves the window's own styling, and
/// a file named there that no longer exists is skipped and reported in the list this returns.
///
/// The list is one line per thing the app could not use, each naming the file it came from: the
/// caller records them, which is what puts a theme's trouble on the Logs page as well as on its
/// card. Nothing is printed to stderr.
pub fn apply_enabled(window: &MainWindow, data_dir: &Path) -> Vec<(String, String)> {
    let enabled = crate::screens::settings::LocalExtras::load(data_dir).enabled_themes;
    let mut troubles = Vec::new();

    let mut rules = Vec::new();
    for filename in &enabled {
        match read_mod_content(data_dir, filename, ModType::Theme) {
            Ok(css) => {
                let theme = parse::parse_theme(&css);
                for warning in &theme.warnings {
                    troubles.push((filename.clone(), warning.clone()));
                }
                rules.extend(theme.rules);
            }
            Err(error) => troubles.push((filename.clone(), error)),
        }
    }

    let (styles, unmatched) = parse::resolve(&rules, CLASSES);
    for warning in unmatched {
        troubles.push((enabled.join(", "), warning));
    }
    let globals = window.global::<ClassStyles>();
    for ((_, set_element), style) in ELEMENTS.iter().zip(styles) {
        set_element(&globals, to_slint(style));
    }
    troubles
}

/// The rules the enabled themes give, read the way `apply_enabled` reads them, with every line the
/// app could not use recorded as it goes. This is what a plugin's own rules are drawn over, so the
/// theme stays on the window.
pub fn enabled_rules(data_dir: &Path, logs: &crate::plumbing::logs::Logs) -> Vec<parse::Rule> {
    let enabled = crate::screens::settings::LocalExtras::load(data_dir).enabled_themes;
    let mut rules = Vec::new();
    for filename in &enabled {
        match read_mod_content(data_dir, filename, ModType::Theme) {
            Ok(css) => {
                let theme = parse::parse_theme(&css);
                logs.theme(filename, &theme.warnings);
                rules.extend(theme.rules);
            }
            Err(error) => logs.theme_unreadable(filename, error),
        }
    }
    rules
}

/// `apply_enabled` for the window's own thread, with the lines recorded as it goes: this is what
/// every press that changes the theme list calls. The recorder is the app's one, so a theme's
/// warnings land on the Logs page beside the plugin lines.
pub fn apply_and_record(
    window: &MainWindow,
    data_dir: &Path,
    logs: &crate::plumbing::logs::Logs,
) -> Vec<(String, String)> {
    let troubles = apply_enabled(window, data_dir);
    for (filename, warning) in &troubles {
        logs.theme(filename, std::slice::from_ref(warning));
    }
    troubles
}

/// The web classes a plugin's `applyTheme` rules may select, in the order the element table lists
/// them. A plugin that names anything else is told so when its file loads.
pub fn classes() -> Vec<&'static str> {
    CLASSES
        .iter()
        .flat_map(|classes| classes.iter().copied())
        .collect()
}

/// Draws the window with the rules a plugin asked for: the same understood selectors and
/// properties a theme file has, read from JSON rather than CSS. A rule this app does not
/// understand is left out and said in the list this returns, one line each.
pub fn install(window: &MainWindow, rules: &[serde_json::Value]) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut parsed = Vec::new();
    for rule in rules {
        let (Some(selector), Some(property), Some(value)) = (
            rule.get("selector").and_then(serde_json::Value::as_str),
            rule.get("property").and_then(serde_json::Value::as_str),
            rule.get("value").and_then(serde_json::Value::as_str),
        ) else {
            warnings.push(
                "a rule needs `selector`, `property` and `value` strings, so it was left out."
                    .to_owned(),
            );
            continue;
        };
        let mut said = Vec::new();
        let style = parse::parse_declarations(&format!("{property}: {value}"), selector, &mut said);
        warnings.extend(said);
        parsed.extend(parse::rule(selector, style));
    }
    let (styles, unmatched) = parse::resolve(&parsed, CLASSES);
    warnings.extend(unmatched);
    let globals = window.global::<ClassStyles>();
    for ((_, set_element), style) in ELEMENTS.iter().zip(styles) {
        set_element(&globals, to_slint(style));
    }
    warnings
}

/// Draws the window with a plugin's rules over the enabled theme's, which is what the window
/// showed before the plugin asked for anything. A plugin that sets one property of an element
/// leaves the theme's other properties on it; a plugin that names nothing leaves the theme as it
/// was. The warnings are the plugin's alone, one line each.
pub fn install_over(
    window: &MainWindow,
    theme_rules: &[parse::Rule],
    plugin_rules: &[serde_json::Value],
) -> Vec<String> {
    let mut parsed = theme_rules.to_vec();
    let mut warnings = parse_plugin(&mut parsed, plugin_rules);
    let (styles, unmatched) = parse::resolve(&parsed, CLASSES);
    let globals = window.global::<ClassStyles>();
    for ((_, set_element), style) in ELEMENTS.iter().zip(styles) {
        set_element(&globals, to_slint(style));
    }
    warnings.extend(unmatched);
    warnings
}

/// Reads a plugin's JSON rules into `parsed`, after whatever is already there.
fn parse_plugin(parsed: &mut Vec<parse::Rule>, rules: &[serde_json::Value]) -> Vec<String> {
    let mut warnings = Vec::new();
    for rule in rules {
        let (Some(selector), Some(property), Some(value)) = (
            rule.get("selector").and_then(serde_json::Value::as_str),
            rule.get("property").and_then(serde_json::Value::as_str),
            rule.get("value").and_then(serde_json::Value::as_str),
        ) else {
            warnings.push(
                "a rule needs `selector`, `property` and `value` strings, so it was left out."
                    .to_owned(),
            );
            continue;
        };
        let mut said = Vec::new();
        let style = parse::parse_declarations(&format!("{property}: {value}"), selector, &mut said);
        warnings.extend(said);
        parsed.extend(parse::rule(selector, style));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use parse::parse_theme;

    fn style_of(styles: &[Style], element: &str) -> Style {
        let index = ELEMENTS
            .iter()
            .position(|(classes, _)| classes[0] == element)
            .expect("a known element");
        styles[index]
    }

    fn resolved(css: &str) -> (Vec<Style>, Vec<String>) {
        let theme = parse_theme(css);
        let (styles, unmatched) = parse::resolve(&theme.rules, CLASSES);
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
    fn substring_selectors_reach_every_class_containing_the_name() {
        let (styles, _) = resolved("[class*=\"title\"] { color: #f00 }");
        for class in ["title-container", "title-bar-container", "title-label"] {
            assert!(style_of(&styles, class).color.is_some(), "{class}");
        }
        assert!(style_of(&styles, "poster-image").color.is_none());
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
