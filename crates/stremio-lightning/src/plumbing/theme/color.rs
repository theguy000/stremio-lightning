//! CSS color parser for theme files (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`,
//! `transparent`).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

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

pub(super) fn parse_color(value: &str) -> Option<Rgba> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
