//! The detail screen: the item's meta and the streams the addons answered with, plus the
//! action that opens an item and the one that plays a row.

use stremio_core::constants::{META_RESOURCE_NAME, STREAM_RESOURCE_NAME};
use stremio_core::models::common::{Loadable, ResourceError};
use stremio_core::models::meta_details::{MetaDetails, Selected as DetailsSelected};
use stremio_core::runtime::msg::{Action, ActionLoad};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::addon::ResourcePath;
use stremio_core::types::resource::{Stream, StreamUrls};
use url::Url;

use crate::{AppModel, AppRuntime};

const MAX_STREAMS: usize = 100;

/// `title` is `None` until the addons have answered the meta request, so the window keeps
/// showing the title of the card that was clicked.
#[derive(Clone, Default)]
pub(crate) struct Data {
    pub(crate) title: Option<String>,
    pub(crate) description: String,
    pub(crate) poster: Option<String>,
    pub(crate) status: String,
    pub(crate) streams: Vec<(String, String)>,
}

pub(crate) fn data(model: &AppModel) -> Data {
    let details = &model.meta_details;
    let item = details
        .meta_items
        .iter()
        .find_map(|page| match &page.content {
            Some(Loadable::Ready(item)) => Some(item),
            _ => None,
        });

    let (mut ready, mut loading, mut empty, mut failed) = (0, 0, 0, 0);
    for page in &details.streams {
        match &page.content {
            Some(Loadable::Ready(_)) => ready += 1,
            Some(Loadable::Err(ResourceError::EmptyContent)) => empty += 1,
            Some(Loadable::Err(_)) => failed += 1,
            _ => loading += 1,
        }
    }
    let found = streams_of(details).count();
    let streams = streams_of(details)
        .take(MAX_STREAMS)
        .map(|stream| {
            (
                stream
                    .name
                    .clone()
                    .unwrap_or_else(|| "Stream".to_owned())
                    .replace('\n', " "),
                stream
                    .description
                    .clone()
                    .unwrap_or_default()
                    .replace('\n', " | "),
            )
        })
        .collect();

    Data {
        title: item.map(|item| item.preview.name.clone()),
        description: item
            .and_then(|item| item.preview.description.clone())
            .unwrap_or_default(),
        poster: item.and_then(|item| item.preview.poster.as_ref().map(|url| url.to_string())),
        status: format!(
            "{found} streams from {ready} sources, {empty} had nothing, {failed} failed, {loading} loading"
        ),
        streams,
    }
}

/// Every stream the addons have answered with, in the order the details screen lists them, so
/// the index of a clicked row finds its stream again.
fn streams_of(details: &MetaDetails) -> impl Iterator<Item = &Stream> {
    details
        .streams
        .iter()
        .filter_map(|page| match &page.content {
            Some(Loadable::Ready(list)) => Some(list.iter()),
            _ => None,
        })
        .flatten()
}

/// The address the player opens: torrents and archives go through the stream server, plain
/// URLs are played as they are.
pub(crate) fn stream_url(stream: &Stream, server: &Url) -> Result<String, String> {
    let converted = stream
        .convert(Some(server))
        .map_err(|error| error.message())?;
    StreamUrls::new(converted, Some(server))
        .streaming_url
        .map(String::from)
        .ok_or_else(|| "it has no address the player can open".to_owned())
}

/// Series have no stream until an episode is picked, so the core guesses one. Everything else
/// asks the addons for streams of the item itself.
pub(crate) fn open(runtime: &AppRuntime, id: &str, kind: &str) {
    let (stream_path, guess_stream) = if kind == "series" {
        (None, true)
    } else {
        (
            Some(ResourcePath::without_extra(STREAM_RESOURCE_NAME, kind, id)),
            false,
        )
    };
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::MetaDetails(DetailsSelected {
            meta_path: ResourcePath::without_extra(META_RESOURCE_NAME, kind, id),
            stream_path,
            guess_stream,
        })),
    });
}

/// The address of the stream at `index`, as of right now.
pub(crate) fn stream_address(runtime: &AppRuntime, index: usize) -> Result<String, String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let stream = streams_of(&model.meta_details)
        .nth(index)
        .ok_or("the list changed, pick it again")?;
    stream_url(stream, &model.ctx.profile.settings.streaming_server_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stremio_core::types::resource::StreamSource;

    fn stream(source: StreamSource) -> Stream {
        Stream {
            source,
            name: None,
            description: None,
            thumbnail: None,
            subtitles: vec![],
            behavior_hints: Default::default(),
        }
    }

    #[test]
    fn torrents_play_through_the_stream_server_and_plain_urls_as_they_are() {
        let server = Url::parse("http://127.0.0.1:11470/").unwrap();

        let torrent = stream(StreamSource::Torrent {
            info_hash: [0xab; 20],
            file_idx: None,
            announce: vec![],
            file_must_include: vec![],
        });
        // The core leaves an empty query on the end of torrent addresses, so compare the parts.
        let url = Url::parse(&stream_url(&torrent, &server).unwrap()).unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_eq!(url.port(), Some(11470));
        assert_eq!(url.path(), format!("/{}/-1", "ab".repeat(20)));

        let plain = stream(StreamSource::Url {
            url: Url::parse("https://example.com/movie.mp4").unwrap(),
        });
        assert_eq!(
            stream_url(&plain, &server).unwrap(),
            "https://example.com/movie.mp4"
        );
    }
}
