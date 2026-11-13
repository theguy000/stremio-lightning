//! Field definitions for built-in plugin settings pages.

use serde_json::{json, Value as Json};

/// One control on the Plugin settings page.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Toggle {
        key: String,
        label: String,
        default: bool,
    },
    Text {
        key: String,
        label: String,
        default: String,
    },
    Select {
        key: String,
        label: String,
        options: Vec<String>,
        default: String,
    },
    Range {
        key: String,
        label: String,
        min: f64,
        max: f64,
        step: f64,
        default: f64,
    },
}

impl Field {
    pub fn key(&self) -> &str {
        match self {
            Self::Toggle { key, .. }
            | Self::Text { key, .. }
            | Self::Select { key, .. }
            | Self::Range { key, .. } => key,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Toggle { label, .. }
            | Self::Text { label, .. }
            | Self::Select { label, .. }
            | Self::Range { label, .. } => label,
        }
    }

    /// The value this field holds before the plugin or the user has saved one.
    pub fn default(&self) -> Json {
        match self {
            Self::Toggle { default, .. } => json!(default),
            Self::Text { default, .. } => json!(default),
            Self::Select { default, .. } => json!(default),
            // The slider holds an `f64`, but a whole-number slider reports whole numbers, and the
            // page writes what it draws. A default of `3` therefore stays `3` and does not become
            // `3.0`.
            Self::Range {
                min,
                max,
                step,
                default,
                ..
            } => whole(*min, *max, *step, *default),
        }
    }

    /// Whether a stored value is something this field can hold. A value of the wrong type is
    /// treated as the default rather than drawn in a control that would change it by being seen.
    pub fn accepts(&self, value: &Json) -> bool {
        match (self, value) {
            (Self::Toggle { .. }, Json::Bool(_)) | (Self::Range { .. }, Json::Number(_)) => true,
            (Self::Text { .. }, Json::String(_)) => true,
            (Self::Select { options, .. }, Json::String(text)) => options.contains(text),
            _ => false,
        }
    }

    /// The value a control reports, made into something this field can hold. A slider reports a
    /// number outside its range or between its steps; that number is brought inside the range and
    /// onto a step. A value that is not of the field's own kind at all becomes the default.
    pub fn conform(&self, value: &Json) -> Json {
        match (self, value) {
            (Self::Range { min, max, step, .. }, Json::Number(number)) => {
                let step = if *step > 0.0 { *step } else { 1.0 };
                let number = number.as_f64().unwrap_or(*min).clamp(*min, *max);
                let number = min + ((number - min) / step).round() * step;
                whole(*min, *max, step, number)
            }
            (Self::Toggle { .. }, Json::Number(number)) => Json::Bool(number.as_f64() != Some(0.0)),
            (_, other) if self.accepts(other) => other.clone(),
            (field, _) => field.default(),
        }
    }
}

/// A slider value as JSON. A slider set in whole numbers stays in whole numbers: the third
/// position of a 1-to-10 slider is `3`, not `3.0` or `3.0000000001`.
fn whole(min: f64, max: f64, step: f64, number: f64) -> Json {
    let number = if min.fract() == 0.0 && step.fract() == 0.0 {
        let rounded = number.round();
        if rounded.abs() <= i64::MAX as f64 {
            return Json::Number((rounded as i64).into());
        }
        rounded
    } else {
        number
    };
    match serde_json::Number::from_f64(number.clamp(min, max)) {
        Some(number) => Json::Number(number),
        None => Json::Null,
    }
}

/// What a field says it is, for the one line the page puts under a control.
pub fn hint(field: &Field) -> String {
    match field {
        Field::Toggle { .. } => "On or off.".to_owned(),
        Field::Text { .. } => "Any text.".to_owned(),
        Field::Select { options, .. } => {
            format!("One of {}.", options.join(", "))
        }
        Field::Range { min, max, step, .. } => {
            if *step == 1.0 {
                format!("A whole number from {} to {}.", shown(*min), shown(*max))
            } else {
                format!(
                    "A number from {} to {}, in steps of {}.",
                    shown(*min),
                    shown(*max),
                    shown(*step)
                )
            }
        }
    }
}

/// A range bound as the hint prints it. A whole number prints as one, so a slider from 1 to 10
/// does not read "1 to 10.0".
fn shown(number: f64) -> String {
    match serde_json::Number::from_f64(number) {
        Some(number) if number.is_i64() || number.is_u64() => number.to_string(),
        _ => format!("{number}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_number_range_prints_whole_numbers_in_its_hint() {
        let fields = [
            Field::Range {
                key: "rows".to_owned(),
                label: "Rows".to_owned(),
                min: 1.0,
                max: 10.0,
                step: 1.0,
                default: 1.0,
            },
            Field::Range {
                key: "volume".to_owned(),
                label: "Volume".to_owned(),
                min: 0.5,
                max: 1.5,
                step: 0.1,
                default: 0.5,
            },
        ];

        assert_eq!(hint(&fields[0]), "A whole number from 1 to 10.");
        assert_eq!(
            hint(&fields[1]),
            "A number from 0.5 to 1.5, in steps of 0.1."
        );
    }
}
