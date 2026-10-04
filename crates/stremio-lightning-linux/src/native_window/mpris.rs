//! Minimal MPRIS endpoint so desktop media keys (GNOME, KDE, ...) reach the
//! shell. Linux desktops route hardware media keys to MPRIS players, not to
//! window key events. Keys are forwarded as the same `media-key` transport
//! message the Windows shell emits.

use gtk::gio::{self, prelude::*};
use gtk::glib::{self, ToVariant};
use std::rc::Rc;
use stremio_lightning_identity::{APP_ID, APP_NAME};

const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT_IFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const INTROSPECTION: &str = r#"<node>
  <interface name="org.mpris.MediaPlayer2">
    <method name="Raise"/>
    <method name="Quit"/>
    <property name="CanQuit" type="b" access="read"/>
    <property name="CanRaise" type="b" access="read"/>
    <property name="HasTrackList" type="b" access="read"/>
    <property name="Identity" type="s" access="read"/>
    <property name="SupportedUriSchemes" type="as" access="read"/>
    <property name="SupportedMimeTypes" type="as" access="read"/>
  </interface>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="Next"/>
    <method name="Previous"/>
    <method name="Pause"/>
    <method name="PlayPause"/>
    <method name="Stop"/>
    <method name="Play"/>
    <property name="PlaybackStatus" type="s" access="read"/>
    <property name="CanGoNext" type="b" access="read"/>
    <property name="CanGoPrevious" type="b" access="read"/>
    <property name="CanPlay" type="b" access="read"/>
    <property name="CanPause" type="b" access="read"/>
    <property name="CanSeek" type="b" access="read"/>
    <property name="CanControl" type="b" access="read"/>
  </interface>
</node>"#;

/// Maps an MPRIS player method to the shared `media-key` action.
///
/// ponytail: the web side only knows a play/pause toggle, so `Play` and `Pause`
/// toggle too. Upgrade path: track real playback state and publish
/// `PlaybackStatus` changes via `PropertiesChanged`.
fn media_key_action(method: &str) -> Option<&'static str> {
    match method {
        "PlayPause" | "Play" | "Pause" => Some("play-pause"),
        "Next" => Some("next-track"),
        "Previous" => Some("previous-track"),
        _ => None,
    }
}

fn property(interface: &str, name: &str) -> glib::Variant {
    match (interface, name) {
        (ROOT_IFACE, "Identity") => APP_NAME.to_variant(),
        (ROOT_IFACE, "CanRaise") => true.to_variant(),
        (ROOT_IFACE, "SupportedUriSchemes") => ["stremio", "magnet"][..].to_variant(),
        (ROOT_IFACE, "SupportedMimeTypes") => ["application/x-bittorrent"][..].to_variant(),
        (PLAYER_IFACE, "PlaybackStatus") => "Playing".to_variant(),
        (PLAYER_IFACE, "CanSeek") => false.to_variant(),
        (PLAYER_IFACE, _) => true.to_variant(),
        _ => false.to_variant(),
    }
}

/// Owns the MPRIS bus name for the lifetime of the process. A missing session
/// bus or a lost name only means media keys are unavailable.
pub fn own(app: &gtk::Application, on_key: impl Fn(&str) + 'static) {
    let on_key: Rc<dyn Fn(&str)> = Rc::new(on_key);
    let app = app.clone();
    gio::bus_own_name(
        gio::BusType::Session,
        &format!("org.mpris.MediaPlayer2.{APP_ID}"),
        gio::BusNameOwnerFlags::NONE,
        move |connection, _| {
            let Ok(nodes) = gio::DBusNodeInfo::for_xml(INTROSPECTION) else {
                return;
            };
            for interface in [ROOT_IFACE, PLAYER_IFACE] {
                let Some(info) = nodes.lookup_interface(interface) else {
                    continue;
                };
                let on_key = on_key.clone();
                let app = app.clone();
                let result = connection
                    .register_object(OBJECT_PATH, &info)
                    .method_call(move |_, _, _, _, method, _, invocation| {
                        match (method, media_key_action(method)) {
                            (_, Some(action)) => on_key(action),
                            ("Raise", _) => {
                                if let Some(window) = app.active_window() {
                                    window.present();
                                }
                            }
                            _ => {}
                        }
                        invocation.return_value(None);
                    })
                    .property(|_, _, _, interface, name| property(interface, name))
                    .build();
                if let Err(error) = result {
                    stremio_lightning_core::logging::warn(
                        "native.window",
                        format!("[StremioLightning] Failed to register MPRIS {interface}: {error}"),
                    );
                }
            }
        },
        |_, _| {},
        |_, name| {
            stremio_lightning_core::logging::warn(
                "native.window",
                format!("[StremioLightning] Could not own D-Bus name {name}; media keys disabled"),
            );
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_mpris_methods_to_media_key_actions() {
        assert_eq!(media_key_action("PlayPause"), Some("play-pause"));
        assert_eq!(media_key_action("Next"), Some("next-track"));
        assert_eq!(media_key_action("Previous"), Some("previous-track"));
        assert_eq!(media_key_action("Stop"), None);
    }
}
