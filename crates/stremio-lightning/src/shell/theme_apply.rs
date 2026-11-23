//! Slint adapter that projects `plumbing::theme::ResolvedTheme` onto `MainWindow`'s `Theme` and
//! `ClassStyles` globals and records any theme warnings into `Logs`.

use std::path::Path;

use slint::{Color, ComponentHandle};

use crate::plumbing::logs::{Level, Logs, Source};
use crate::plumbing::theme::{self, Rgba, Style, ThemeWarning};
use crate::screens::settings::LocalExtras;
use crate::{ClassStyle, ClassStyles, MainWindow, Theme as SlintTheme};

type Setter = fn(&ClassStyles<'_>, ClassStyle);

macro_rules! setters {
    ($($method:ident),+ $(,)?) => { &[$(|g: &ClassStyles<'_>, s: ClassStyle| g.$method(s)),+] };
}

/// Order MUST match `plumbing::theme::CLASSES` 1-for-1 (checked at compile time below).
const SETTERS: &[Setter] = setters![
    set_board_row,
    set_header_container,
    set_title_container,
    set_see_all_container,
    set_meta_items_container,
    set_meta_item_link,
    set_poster_container,
    set_poster_image,
    set_title_bar_container,
    set_title_label,
    set_horizontal_nav_bar_container,
    set_logo_container,
    set_logo,
    set_search_bar_container,
    set_search_input,
    set_placeholder_label,
    set_submit_button_container,
    set_buttons_container,
    set_button_container,
    set_menu_toggle,
    set_avatar,
    set_vertical_nav_bar_container,
    set_nav_tab_button_container,
    set_icon,
    set_select_input,
    set_dropdown,
    set_option,
    set_placeholder,
    set_title,
    set_image_container,
    set_image,
    set_point,
    set_text,
    set_button,
    set_label,
    set_addon,
    set_selectable_inputs_container,
    set_add_button_container,
    set_add_button_label,
    set_info_container,
    set_name_container,
    set_version_container,
    set_types_container,
    set_description_container,
    set_action_buttons_container,
    set_uninstall_button_container,
    set_install_button_container,
    set_share_button_container,
    set_search,
    set_message_container,
    set_menu,
    set_version_info_label,
    set_section,
    set_avatar_container,
    set_email_label,
    set_link,
    set_videos_list_container,
    set_video_container,
    set_intro_container,
    set_background_container,
    set_close_button_container,
    set_heading_container,
    set_slogan_container,
    set_content_container,
    set_form_container,
    set_credentials_text_input,
    set_forgot_password_link_container,
    set_forgot_password_link,
    set_error_message,
    set_submit_button,
    set_checkbox_container,
    set_checkbox,
    set_options_container,
    set_facebook_button,
    set_apple_button,
    set_login_form_button,
    set_signup_form_button,
    set_guest_login_button,
    set_mods_menu_container,
    set_mods_menu_button,
    set_mods_content,
    set_mods_page_title,
    set_mods_page_note,
    set_mod_card,
    set_mod_title,
    set_mod_byline,
    set_mod_description,
    set_mod_warning,
    set_mod_notice,
    set_mod_toggle_container,
    set_mod_toggle,
    set_mod_remove_button,
    set_mod_remove_button_label,
    set_mods_empty,
    set_mods_log_time,
    set_mods_log_source,
    set_mods_log_level,
    set_mods_log_text,
    set_mods_log_detail,
    set_mods_log_detail_text,
    set_mods_about_label,
    set_mods_about_value,
];

const _: () = assert!(SETTERS.len() == theme::CLASSES.len());

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

/// Resolves enabled themes from `settings.json`, applies them to `window`, and records any
/// unreadable-file errors or unsupported-CSS warnings into `logs`.
pub(crate) fn apply_and_record(
    window: &MainWindow,
    data_dir: &Path,
    logs: &Logs,
) -> Vec<ThemeWarning> {
    let enabled = LocalExtras::load(data_dir).enabled_themes;
    let resolved = theme::resolve_files(data_dir, &enabled);
    window.global::<SlintTheme>().set_amoled(resolved.amoled);
    let globals = window.global::<ClassStyles>();
    for (set_element, style) in SETTERS.iter().zip(resolved.styles) {
        set_element(&globals, to_slint(style));
    }
    for warning in &resolved.warnings {
        let level = if warning.unreadable {
            Level::Error
        } else {
            Level::Warn
        };
        logs.record(Source::Theme, &warning.filename, level, &warning.message);
    }
    resolved.warnings
}
