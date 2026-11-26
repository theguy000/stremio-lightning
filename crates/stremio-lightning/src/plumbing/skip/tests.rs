use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

use super::parse::{arm_lookup, kitsu_mal, rates_of};
use super::*;

fn segments_of(value: &Value, duration: f64) -> Vec<Segment> {
    rates_of(value, duration)
        .iter()
        .map(Rate::segment)
        .collect()
}

#[test]
fn kitsu_and_arm_mappings() {
    let kitsu = json!({ "data": [
        { "attributes": { "externalSite": "trakt", "externalId": "1" } },
        { "attributes": { "externalSite": "myanimelist/anime", "externalId": "32281" } }
    ]});
    assert_eq!(kitsu_mal(&kitsu), Some(32281));

    let arm = json!([
        { "myanimelist": 16498, "thetvdb-season": 1 },
        { "myanimelist": 25777, "thetvdb-season": 3 }
    ]);
    assert_eq!(arm_lookup(&arm, 3), Some((25777, true)));
    assert_eq!(arm_lookup(&arm, 2), Some((16498, false)));
    assert_eq!(arm_lookup(&arm, 0), None);
    assert_eq!(
        arm_lookup(&json!({ "myanimelist": 4242 }), 1),
        Some((4242, false))
    );
    assert_eq!(arm_lookup(&json!([]), 1), None);
}

#[test]
fn skip_times_order_ending_pinning_and_release_clustering() {
    let basic = json!({ "found": true, "results": [
        { "skipType": "ed", "interval": { "startTime": 1342.795, "endTime": 1430.616 } },
        { "skipType": "op", "interval": { "startTime": 128.406, "endTime": 218.406 } },
        { "skipType": "mixed", "interval": { "startTime": 1.0, "endTime": 2.0 } }
    ]});
    let found = segments_of(&basic, 1440.0);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].kind, Kind::Opening);
    assert!(found[0].holds(150.0) && !found[0].holds(220.0));
    assert_eq!(segments_of(&json!({ "found": false }), 1440.0).len(), 0);

    let short = json!({ "found": true, "results": [
        { "skipType": "op", "interval": { "startTime": 194.0, "endTime": 284.0 }, "episodeLength": 1381.0 },
        { "skipType": "ed", "interval": { "startTime": 1291.0, "endTime": 1381.0 }, "episodeLength": 1381.0 }
    ]});
    let fitted = segments_of(&short, 1363.0);
    assert_eq!(
        fitted[0],
        Segment {
            kind: Kind::Opening,
            start: 194.0,
            end: 284.0
        }
    );
    assert_eq!(
        fitted[1],
        Segment {
            kind: Kind::Ending,
            start: 1273.0,
            end: 1363.0
        }
    );

    let close = json!({ "found": true, "results": [
        { "skipType": "op", "interval": { "startTime": 0.5, "endTime": 91.0 }, "episodeLength": 1421.962 },
        { "skipType": "ed", "interval": { "startTime": 1326.0, "endTime": 1419.947 }, "episodeLength": 1419.947 }
    ]});
    assert_eq!(
        segments_of(&close, 1420.0)
            .iter()
            .map(|s| s.kind)
            .collect::<Vec<_>>(),
        [Kind::Opening, Kind::Ending]
    );

    let split = json!({ "found": true, "results": [
        { "skipType": "op", "interval": { "startTime": 112.0, "endTime": 202.0 }, "episodeLength": 1440.065 },
        { "skipType": "ed", "interval": { "startTime": 1342.0, "endTime": 1431.0 }, "episodeLength": 1431.984 }
    ]});
    let nearest = segments_of(&split, 1363.0);
    assert_eq!(nearest.len(), 1);
    assert_eq!(nearest[0].kind, Kind::Ending);
    assert_eq!(nearest[0].end, 1363.0);
}

struct FakeTransport {
    gets: HashMap<String, Value>,
    post_reply: Value,
}

impl HttpTransport for FakeTransport {
    fn get_json<'a>(&'a self, url: &'a str) -> BoxFuture<'a, TransportReply> {
        let reply = match self.gets.get(url) {
            Some(v) => TransportReply::Ok(v.clone()),
            None => TransportReply::NotFound,
        };
        Box::pin(async move { reply })
    }

    fn post_json<'a>(&'a self, url: &'a str, _body: &'a Value) -> BoxFuture<'a, TransportReply> {
        let reply = if url.contains("rate-limit") {
            TransportReply::RateLimited
        } else {
            TransportReply::Ok(self.post_reply.clone())
        };
        Box::pin(async move { reply })
    }
}

#[test]
fn client_resolves_seasons_caches_submissions_and_maps_vote_errors() {
    let gets = HashMap::from([
            (
                "https://arm.haglund.dev/api/v2/imdb?id=tt1&include=myanimelist,thetvdb-season"
                    .to_owned(),
                json!([{ "myanimelist": 100, "thetvdb-season": 1 }]),
            ),
            (
                "https://api.jikan.moe/v4/anime/100/relations".to_owned(),
                json!({ "data": [{ "relation": "Sequel", "entry": [{ "type": "anime", "mal_id": 200 }] }] }),
            ),
            (
                "https://api.jikan.moe/v4/anime/200/relations".to_owned(),
                json!({ "data": [{ "relation": "Prequel", "entry": [{ "type": "anime", "mal_id": 100 }] }] }),
            ),
            (
                "https://api.aniskip.com/v2/skip-times/200/3?types[]=op&types[]=ed&types[]=recap&episodeLength=1420"
                    .to_owned(),
                json!({ "found": true, "results": [
                    { "skipId": "op-1", "skipType": "op", "interval": { "startTime": 30.0, "endTime": 120.0 }, "episodeLength": 1420.0 }
                ]}),
            ),
        ]);
    let client = SkipClient::with_transport(
        FakeTransport {
            gets,
            post_reply: json!({ "skipId": "ed-9" }),
        },
        Duration::ZERO,
    );
    let ep = EpisodeKey {
        meta_id: "tt1",
        season: 2,
        episode: 3,
        duration: 1420.0,
    };
    let initial = futures::executor::block_on(client.rates(&ep));
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].id, "op-1");

    let created = futures::executor::block_on(client.submit(
        &Submission {
            episode: ep,
            kind: Kind::Ending,
            start: 1300.0,
            end: 1390.0,
            provider: "lightning",
            submitter: "uuid",
        },
        None,
    ))
    .unwrap();
    assert_eq!(created.id, "ed-9");
    let after = futures::executor::block_on(client.rates(&ep));
    assert_eq!(after.len(), 2);
    assert_eq!(after[1].id, "ed-9");

    assert_eq!(
        futures::executor::block_on(client.vote("rate-limit", true)),
        Err(SkipError::RateLimited { voting: true })
    );
}
