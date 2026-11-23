//! Lookup and menu-building logic over `tables::SETTINGS`.

use serde_json::{json, Value};

use super::super::SelectMenu;
use super::tables::{DefaultVal, Options, Setting, SETTINGS};

fn setting(key: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|setting| setting.key == key)
}

fn current(setting: &Setting, value: &Value) -> String {
    let fallback = match setting.default {
        DefaultVal::Text(text) => text.to_owned(),
        DefaultVal::Number(number) => number.to_string(),
    };
    let unset = setting.unset.unwrap_or_default();
    match value.get(setting.key) {
        Some(Value::Null) => unset.to_owned(),
        Some(Value::String(text)) if text.is_empty() => {
            if unset.is_empty() {
                fallback
            } else {
                unset.to_owned()
            }
        }
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => fallback,
    }
}

pub(crate) fn input(key: &str, settings: &Value) -> SelectMenu {
    let Some(setting) = setting(key) else {
        return SelectMenu::default();
    };
    let current = current(setting, settings);
    let (labels, selected): (Vec<&str>, usize) = match setting.options {
        Options::Text(pairs) => {
            let selected = pairs
                .iter()
                .position(|(code, _)| code.eq_ignore_ascii_case(&current))
                .unwrap_or(0);
            (pairs.iter().map(|(_, label)| *label).collect(), selected)
        }
        Options::Number(pairs) => {
            let selected = current
                .parse::<u32>()
                .ok()
                .and_then(|value| pairs.iter().position(|(code, _)| *code == value))
                .unwrap_or(0);
            (pairs.iter().map(|(_, label)| *label).collect(), selected)
        }
    };
    SelectMenu {
        title: labels.get(selected).copied().unwrap_or_default().to_owned(),
        selected,
        options: labels
            .iter()
            .enumerate()
            .map(|(index, label)| ((*label).to_owned(), index == selected))
            .collect(),
    }
}

pub(crate) fn number_at(key: &str, settings: &Value) -> u32 {
    let Some(setting) = setting(key) else {
        return 0;
    };
    let fallback = match setting.default {
        DefaultVal::Number(number) => number,
        DefaultVal::Text(_) => 0,
    };
    settings
        .get(setting.key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(fallback)
}

pub(crate) fn code_at(key: &str, settings: &Value) -> String {
    setting(key).map_or_else(String::new, |s| current(s, settings))
}

pub(crate) fn value_at(key: &str, index: usize) -> Option<Value> {
    let setting = setting(key)?;
    let code = match setting.options {
        Options::Text(pairs) => {
            let (code, _) = *pairs.get(index)?;
            if Some(code) == setting.unset {
                return Some(Value::Null);
            }
            code
        }
        Options::Number(pairs) => return pairs.get(index).map(|(number, _)| json!(number)),
    };
    Some(json!(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_read_defaults_nulls_native_scripts_and_values() {
        let settings = json!({ "interfaceLanguage": "ben", "subtitlesSize": 150 });
        let lang = input("interfaceLanguage", &settings);
        assert_eq!((lang.title.as_str(), lang.selected), ("বাংলা", 12));
        let size = input("subtitlesSize", &settings);
        assert_eq!((size.title.as_str(), size.selected), ("150%", 3));

        let nulls = json!({ "subtitlesLanguage": null, "playerType": null });
        assert_eq!(input("subtitlesLanguage", &nulls).title, "None");
        assert_eq!(input("playerType", &nulls).title, "Disabled");
        assert_eq!(input("audioLanguage", &json!({})).title, "English");
        assert_eq!(input("seekTimeDuration", &json!({})).title, "10 seconds");

        let interface: Vec<String> = input("interfaceLanguage", &json!({}))
            .options
            .into_iter()
            .map(|(l, _)| l)
            .collect();
        for expected in [
            "Español",
            "Русский",
            "العربية",
            "বাংলা",
            "हिन्दी",
            "日本語",
            "한국어",
            "中文",
        ] {
            assert!(
                interface.iter().any(|l| l == expected),
                "{expected} missing"
            );
        }

        assert_eq!(value_at("subtitlesLanguage", 0), Some(Value::Null));
        assert_eq!(value_at("subtitlesLanguage", 1), Some(json!("eng")));
        assert_eq!(value_at("subtitlesSize", 3), Some(json!(150)));
        assert_eq!(value_at("playerType", 0), Some(Value::Null));
        assert_eq!(value_at("nonsense", 0), None);
    }
}
