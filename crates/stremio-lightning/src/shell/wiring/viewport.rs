//! Viewport visibility tracking and scroll-settle poster loading for Board and Discover.

use slint::ComponentHandle;

use super::Wiring;
use crate::app::GridReport;
use crate::screens::{board, discover};

const SETTLE_MS: u64 = 150;
const FLYING: f32 = 1800.0;

impl Wiring {
    pub(super) fn board_visible(
        &self,
        scroll: f32,
        height: f32,
        content_width: f32,
        viewport_width: f32,
    ) {
        let next = board::Viewport {
            scroll,
            height,
            content_width,
            viewport_width,
        };
        let prev = self.app.viewport().board_viewport();
        let board = self.app.board().view();
        let prev_urls = board::visible_images_with_spotlight(&board.rows, &board.spotlight, prev);
        let next_urls = board::visible_images_with_spotlight(&board.rows, &board.spotlight, next);
        self.app.viewport().set_board_viewport(next);
        if prev_urls != next_urls {
            self.app.viewport().sync_images();
            for url in next_urls {
                self.request_painting(url);
            }
        }
    }

    pub(super) fn visible_range(&self, scroll: f32, height: f32, pitch: f32, columns: i32) {
        let now = std::time::Instant::now();
        let previous = self.app.viewport().set_grid_report(GridReport {
            scroll,
            height,
            pitch,
            columns,
            at: now,
        });
        let flying = previous.is_some_and(|before| {
            let seconds = now.duration_since(before.at).as_secs_f32();
            seconds > 0.0 && seconds < 0.25 && (scroll - before.scroll).abs() / seconds > FLYING
        });
        let range = discover::visible_range(scroll, height, pitch, columns);
        let prev_drawn =
            previous.map(|b| discover::settled_range(b.scroll, b.height, b.pitch, b.columns));
        let next_drawn = discover::settled_range(scroll, height, pitch, columns);
        if prev_drawn != Some(next_drawn) {
            self.app.viewport().sync_images();
        }
        if self.app.viewport().discover_visible() != range {
            self.app.viewport().set_discover_visible(range);
            if crate::plumbing::images::tracing() {
                let note = if flying {
                    " (flying: posters wait)"
                } else {
                    ""
                };
                eprintln!(
                    "[grid] scrolled {scroll:.0}px of {height:.0}px: cards {}..{}{note}",
                    range.0,
                    range.0 + range.1
                );
            }
            if !flying {
                let discover = self.app.discover().view();
                for url in discover::card_images(&discover, range) {
                    self.request_painting(url);
                }
            }
        }
        let wiring = self.clone();
        self.settle.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(SETTLE_MS),
            move || wiring.fill_the_slack(),
        );
    }

    fn fill_the_slack(&self) {
        let Some(grid) = self.app.viewport().grid_report() else {
            return;
        };
        let range = discover::settled_range(grid.scroll, grid.height, grid.pitch, grid.columns);
        self.app.viewport().sync_images();
        let discover = self.app.discover().view();
        for url in discover::card_images(&discover, range) {
            self.request_painting(url);
        }
        if let Some(window) = self.window.upgrade() {
            let ui = window.global::<crate::DiscoverUi>();
            ui.set_settle_count(ui.get_settle_count() + 1);
        }
    }
}
