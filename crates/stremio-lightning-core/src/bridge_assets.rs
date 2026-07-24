pub const BRIDGE_LOGGING_NAME: &str = "bridge/logging.js";
pub const BRIDGE_UTILS_NAME: &str = "bridge/utils.js";
pub const BRIDGE_SHELL_TRANSPORT_NAME: &str = "bridge/shell-transport.js";
pub const BRIDGE_EXTERNAL_LINKS_NAME: &str = "bridge/external-links.js";
pub const BRIDGE_SHELL_DETECTION_NAME: &str = "bridge/shell-detection.js";
pub const BRIDGE_BACK_BUTTON_NAME: &str = "bridge/back-button.js";
pub const BRIDGE_SHORTCUTS_NAME: &str = "bridge/shortcuts.js";
pub const BRIDGE_PIP_NAME: &str = "bridge/pip.js";
pub const BRIDGE_DISCORD_RPC_NAME: &str = "bridge/discord-rpc.js";
pub const BRIDGE_UPDATE_BANNER_NAME: &str = "bridge/update-banner.js";
pub const BRIDGE_NAME: &str = "bridge.js";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionScript {
    pub name: &'static str,
    pub source: String,
}

pub fn bridge_scripts() -> Vec<InjectionScript> {
    vec![
        InjectionScript {
            name: BRIDGE_LOGGING_NAME,
            source: include_str!("../../../web/bridge/src/logging.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_UTILS_NAME,
            source: include_str!("../../../web/bridge/src/utils.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_SHELL_TRANSPORT_NAME,
            source: include_str!("../../../web/bridge/src/shell-transport.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_EXTERNAL_LINKS_NAME,
            source: include_str!("../../../web/bridge/src/external-links.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_SHELL_DETECTION_NAME,
            source: include_str!("../../../web/bridge/src/shell-detection.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_BACK_BUTTON_NAME,
            source: include_str!("../../../web/bridge/src/back-button.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_SHORTCUTS_NAME,
            source: include_str!("../../../web/bridge/src/shortcuts.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_PIP_NAME,
            source: include_str!("../../../web/bridge/src/pip.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_DISCORD_RPC_NAME,
            source: include_str!("../../../web/bridge/src/discord-rpc.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_UPDATE_BANNER_NAME,
            source: include_str!("../../../web/bridge/src/update-banner.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_NAME,
            source: include_str!("../../../web/bridge/bridge.js").to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_bridge_scripts_have_stable_order_and_sources() {
        let scripts = bridge_scripts();

        assert_eq!(
            scripts.iter().map(|script| script.name).collect::<Vec<_>>(),
            vec![
                BRIDGE_LOGGING_NAME,
                BRIDGE_UTILS_NAME,
                BRIDGE_SHELL_TRANSPORT_NAME,
                BRIDGE_EXTERNAL_LINKS_NAME,
                BRIDGE_SHELL_DETECTION_NAME,
                BRIDGE_BACK_BUTTON_NAME,
                BRIDGE_SHORTCUTS_NAME,
                BRIDGE_PIP_NAME,
                BRIDGE_DISCORD_RPC_NAME,
                BRIDGE_UPDATE_BANNER_NAME,
                BRIDGE_NAME,
            ]
        );
        assert!(scripts
            .iter()
            .all(|script| !script.source.trim().is_empty()));
        assert!(!scripts
            .iter()
            .any(|script| script.source.contains("initCastFallback")));
    }
}
