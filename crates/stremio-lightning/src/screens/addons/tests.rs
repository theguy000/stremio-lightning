use super::*;
use url::Url;

fn card(name: &str, description: &str) -> AddonCard {
    let manifest: Manifest = serde_json::from_value(serde_json::json!({
        "id": "example", "version": "1.0.0", "name": name,
        "description": description, "resources": ["catalog"], "types": ["movie"],
    }))
    .unwrap();
    card_of(
        &Descriptor {
            transport_url: Url::parse("https://example.com/manifest.json").unwrap(),
            manifest,
            flags: Default::default(),
        },
        false,
    )
}

#[test]
fn types_join_and_case_insensitive_search() {
    let t = |xs: &[&str]| xs.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(types_label(&t(&[])), "");
    assert_eq!(types_label(&t(&["movie"])), "Movie");
    assert_eq!(types_label(&t(&["movie", "series"])), "Movie & Series");
    assert_eq!(
        types_label(&t(&["movie", "series", "channel"])),
        "Movie, Series & Channel"
    );
    let cards = vec![
        card("Cinemeta", "The official addon"),
        card("YouTube", "Watch channels"),
    ];
    assert_eq!(visible(&cards, "").len(), 2);
    assert_eq!(visible(&cards, "CINE")[0].name, "Cinemeta");
    assert_eq!(visible(&cards, "channels")[0].name, "YouTube");
    assert!(visible(&cards, "zzz").is_empty());
}
