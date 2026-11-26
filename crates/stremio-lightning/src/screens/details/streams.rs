//! Pure stream-row projection, last-used matching, and resume-button summary for Details.

use stremio_core::models::common::Loadable;
use stremio_core::models::meta_details::MetaDetails;
use stremio_core::types::resource::{Stream, StreamSource, Video};
use stremio_core::types::streams::StreamsItemKey;

use super::Source;
use crate::app::AppModel;
use crate::screens::addon_name;

pub(super) fn stream_link(source: &StreamSource) -> (String, bool) {
    match source {
        StreamSource::Url { url } => (url.as_str().to_owned(), false),
        StreamSource::Torrent { info_hash, .. } => {
            let hex: String = info_hash.iter().map(|b| format!("{b:02x}")).collect();
            (format!("magnet:?xt=urn:btih:{hex}"), true)
        }
        StreamSource::External {
            external_url: Some(url),
            ..
        } => (url.as_str().to_owned(), false),
        _ => (String::new(), false),
    }
}

pub(super) fn source_rows(
    model: &AppModel,
    details: &MetaDetails,
    meta_id: Option<&str>,
    picked: Option<&str>,
) -> Vec<Source> {
    let last_stream: Option<&Stream> = meta_id.zip(picked).and_then(|(m, v)| {
        model
            .ctx
            .streams
            .items
            .get(&StreamsItemKey {
                meta_id: m.to_owned(),
                video_id: v.to_owned(),
            })
            .map(|item| &item.stream)
    });
    details
        .streams
        .iter()
        .filter_map(|page| match &page.content {
            Some(Loadable::Ready(list)) => Some((&page.request.base, list)),
            _ => None,
        })
        .flat_map(|(base, list)| {
            let addon = addon_name(model, base);
            list.iter().map(move |stream| {
                let (link, is_magnet) = stream_link(&stream.source);
                let raw_name = stream.name.as_deref().unwrap_or("").trim();
                let name = if raw_name.is_empty() {
                    if addon.is_empty() {
                        "Stream".to_owned()
                    } else {
                        addon.clone()
                    }
                } else {
                    raw_name.replace('\n', " ")
                };
                let last_used = last_stream.is_some_and(|last| last.source == stream.source);
                Source {
                    name,
                    description: stream.description.clone().unwrap_or_default().trim().to_owned(),
                    addon: addon.to_uppercase(),
                    link,
                    is_magnet,
                    last_used,
                }
            })
        })
        .collect()
}

pub(super) fn resume_summary(
    details: &MetaDetails,
    videos: &[Video],
    picked: Option<&str>,
) -> (String, String, Option<String>) {
    let Some(lib) = details.library_item.as_ref() else {
        return (String::new(), String::new(), None);
    };
    let Some(vid_id) = lib.state.video_id.as_deref() else {
        return (String::new(), String::new(), None);
    };
    if let Some(p) = picked {
        if p != vid_id {
            return (String::new(), String::new(), None);
        }
    }
    let offset = lib.state.time_offset;
    let duration = lib.state.duration;
    if offset == 0 || duration == 0 || offset >= duration {
        return (String::new(), String::new(), None);
    }
    let minutes_left = ((duration - offset) / 60_000).max(1);
    let sublabel = format!("{minutes_left} min left");
    let label = match videos
        .iter()
        .find(|v| v.id == vid_id)
        .and_then(|v| v.series_info.as_ref())
    {
        Some(info) => format!("Resume S{} E{}", info.season, info.episode),
        None => "Resume".to_owned(),
    };
    (label, sublabel, Some(vid_id.to_owned()))
}
