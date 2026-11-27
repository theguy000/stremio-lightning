//! Screen dirty/stale bitmask, active viewport identity, and push notices from `App` to its
//! attached adapter (`shell` or a test observer). Zero Slint dependencies.

use super::AppModelField;

/// Bitmask of screen slices that changed or are stale.
///
/// Used both to coalesce cross-thread repaints (`SeqCst` `AtomicU16` swap) and to skip
/// rebuilding/projecting unaffected screens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScreenMask(u16);

impl ScreenMask {
    pub const EMPTY: Self = Self(0);
    pub const BOARD: Self = Self(1 << 0);
    pub const DISCOVER: Self = Self(1 << 1);
    pub const LIBRARY: Self = Self(1 << 2);
    pub const DETAILS: Self = Self(1 << 3);
    pub const PLAYER: Self = Self(1 << 4);
    pub const ADDONS: Self = Self(1 << 5);
    pub const CALENDAR: Self = Self(1 << 6);
    pub const SETTINGS: Self = Self(1 << 7);
    pub const MODS: Self = Self(1 << 8);
    /// Live log rows on the Mods -> Logs page only; never rebuilds plugins/themes.
    pub const MODS_LOGS: Self = Self(1 << 9);
    pub const IMAGE_SCREENS: Self = Self(
        Self::BOARD.0
            | Self::DISCOVER.0
            | Self::LIBRARY.0
            | Self::DETAILS.0
            | Self::PLAYER.0
            | Self::ADDONS.0
            | Self::CALENDAR.0,
    );
    pub const ALL: Self = Self((1 << 10) - 1);

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn from_bits(bits: u16) -> Self {
        Self(bits & Self::ALL.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    /// Maps a batch of changed `stremio-core` model fields to the screen slices they affect.
    pub fn from_fields(fields: &[AppModelField]) -> Self {
        let mut mask = Self::EMPTY;
        for field in fields {
            let bits = match field {
                AppModelField::Ctx => Self::BOARD
                    .union(Self::DISCOVER)
                    .union(Self::LIBRARY)
                    .union(Self::ADDONS)
                    .union(Self::CALENDAR)
                    .union(Self::DETAILS)
                    .union(Self::PLAYER)
                    .union(Self::SETTINGS),
                AppModelField::Board | AppModelField::ContinueWatchingPreview => Self::BOARD,
                AppModelField::Discover => Self::DISCOVER,
                AppModelField::Library => Self::LIBRARY,
                AppModelField::InstalledAddons
                | AppModelField::RemoteAddons
                | AppModelField::AddonDetails => Self::ADDONS,
                AppModelField::Calendar => Self::CALENDAR,
                AppModelField::MetaDetails => Self::DETAILS,
                AppModelField::Player => Self::PLAYER,
                AppModelField::StreamingServer | AppModelField::DataExport => Self::SETTINGS,
            };
            mask = mask.union(bits);
        }
        mask
    }
}

/// Which top-level screen or overlay currently owns the viewport.
///
/// Controls both active-screen image cache pinning and eager vs. lazy projection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActiveScreen {
    #[default]
    Board,
    Discover,
    Library,
    Calendar,
    Addons,
    Details,
    Player,
    Settings,
    Mods,
}

impl ActiveScreen {
    pub const fn mask(self) -> ScreenMask {
        match self {
            Self::Board => ScreenMask::BOARD,
            Self::Discover => ScreenMask::DISCOVER,
            Self::Library => ScreenMask::LIBRARY,
            Self::Calendar => ScreenMask::CALENDAR,
            Self::Addons => ScreenMask::ADDONS,
            Self::Details => ScreenMask::DETAILS,
            Self::Player => ScreenMask::PLAYER,
            Self::Settings => ScreenMask::SETTINGS,
            Self::Mods => ScreenMask::MODS,
        }
    }
}

/// Push notifications emitted by `App` to its attached adapter (`shell` or test observer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppNotice {
    /// One or more screen slices became dirty; drain via `App::take_dirty()`.
    Dirty,
    /// The player's resolved stream or open state changed; sync video playback immediately.
    PlayerStreamChanged,
    /// Account sign-in succeeded; dismiss the sign-in overlay and clear errors/spinners.
    AuthSucceeded,
    /// A core error occurred; always clear auth progress spinners and display `Some(msg)` if it
    /// was an authentication failure.
    AuthFailed(Option<String>),
}
