//! Title metadata preview panel (`MetaPreview`): logo, blurred poster backdrop, links, rating,
//! description with HTML entity decoding.

use stremio_core::constants::IMDB_LINK_CATEGORY;
use stremio_core::types::resource::MetaItemPreview;

/// What a preview panel shows for one card. `logo` and `background` are image-cache keys.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preview {
    /// The title's id: what the library actions are keyed by.
    pub id: String,
    pub title: String,
    pub runtime: String,
    pub release: String,
    pub rating: String,
    pub description: String,
    pub groups: Vec<(String, Vec<String>)>,
    pub has_trailer: bool,
    pub logo: Option<String>,
    pub background: Option<String>,
    /// Set by screens that draw the action group (the details page); Discover leaves both false.
    pub in_library: bool,
    pub watched: bool,
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    const ENTITIES: [(&str, &str); 6] = [
        ("&apos;", "'"),
        ("&quot;", "\""),
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&#39;", "'"),
    ];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match ENTITIES
            .iter()
            .find(|(entity, _)| after.starts_with(entity))
        {
            Some((entity, replacement)) => {
                out.push_str(replacement);
                rest = &after[entity.len()..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Builds the preview panel's content from a catalog item, like stremio-web's `MetaPreview`.
pub(crate) fn preview_of(item: &MetaItemPreview) -> Preview {
    let mut rating = String::new();
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for link in &item.links {
        let name = decode_entities(&link.name);
        match link.category.as_str() {
            IMDB_LINK_CATEGORY => rating = name,
            category
                if category.eq_ignore_ascii_case("share")
                    || category.eq_ignore_ascii_case("writers") => {}
            category => match groups.iter_mut().find(|(label, _)| label == category) {
                Some((_, names)) => names.push(name),
                None => groups.push((category.to_owned(), vec![name])),
            },
        }
    }
    for (label, _) in &mut groups {
        *label = label.to_uppercase();
    }
    Preview {
        id: item.id.clone(),
        title: decode_entities(&item.name),
        runtime: item.runtime.clone().unwrap_or_default(),
        release: item
            .release_info
            .clone()
            .or_else(|| item.released.map(|date| date.format("%Y").to_string()))
            .unwrap_or_default(),
        rating,
        description: item
            .description
            .as_deref()
            .map(decode_entities)
            .unwrap_or_default(),
        groups,
        has_trailer: !item.trailer_streams.is_empty(),
        logo: item.logo.as_ref().map(crate::plumbing::images::logo_key),
        background: item.poster.as_ref().map(crate::plumbing::images::blur_key),
        in_library: false,
        watched: false,
    }
}

#[cfg(test)]
mod tests {
    use super::decode_entities;

    #[test]
    fn entities_in_a_name_are_undone() {
        assert_eq!(decode_entities("Josh O&apos;Connor"), "Josh O'Connor");
        assert_eq!(
            decode_entities("Dungeons &amp; Dragons"),
            "Dungeons & Dragons"
        );
        assert_eq!(decode_entities("&quot;Weird&quot; Al"), "\"Weird\" Al");
        assert_eq!(decode_entities("&amp;apos;"), "&apos;");
        assert_eq!(decode_entities("Fish & Chips"), "Fish & Chips");
        assert_eq!(decode_entities("&notanentity;"), "&notanentity;");
        assert_eq!(decode_entities("No ampersand"), "No ampersand");
    }
}
