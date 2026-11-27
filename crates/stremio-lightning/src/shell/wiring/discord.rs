//! Connects Player playback ticks and Settings toggles to `plumbing::discord::DiscordService`.

use stremio_core::models::common::Loadable;

use super::Wiring;
use crate::plumbing::discord::PresenceInput;
use crate::plumbing::player::Status;

impl Wiring {
    pub(super) fn sync_discord_enabled(&self) {
        let enabled = self.app.settings().view().discord_rpc_enabled;
        self.discord.set_enabled(enabled);
    }

    pub(super) fn update_discord_playback(&self, status: &Status) {
        let player_view = self.app.player().view();
        if !player_view.open || player_view.title.is_empty() {
            self.discord.clear();
            return;
        }
        let image = self.app.runtime().model().ok().and_then(|model| {
            let Some(Loadable::Ready(meta)) = &model.player.meta_item.as_ref()?.content else {
                return None;
            };
            meta.preview
                .poster
                .as_ref()
                .or(meta.preview.background.as_ref())
                .map(ToString::to_string)
        });
        self.discord.update(PresenceInput {
            title: player_view.title.clone(),
            image,
            paused: status.paused.unwrap_or(false),
            time_secs: status.time,
            duration_secs: status.duration,
        });
    }

    pub(super) fn clear_discord(&self) {
        self.discord.clear();
    }
}
