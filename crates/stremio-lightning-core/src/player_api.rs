use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "method", content = "data")]
pub enum PlayerCommand {
    #[serde(rename = "mpv-observe-prop")]
    ObserveProperty(String),
    #[serde(rename = "mpv-set-prop")]
    SetProperty(String, Value),
    #[serde(rename = "mpv-command")]
    Command(Vec<Value>),
    #[serde(rename = "native-player-stop")]
    Stop,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PlayerPropertyChange {
    pub name: String,
    pub data: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PlayerEndedError {
    pub message: String,
    pub critical: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PlayerEnded {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<PlayerEndedError>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum PlayerEvent {
    #[serde(rename = "mpv-prop-change")]
    PropertyChange(PlayerPropertyChange),
    #[serde(rename = "mpv-event-ended")]
    Ended(PlayerEnded),
    #[serde(rename = "showPictureInPicture")]
    ShowPictureInPicture(Value),
    #[serde(rename = "hidePictureInPicture")]
    HidePictureInPicture(Value),
}

impl PlayerEvent {
    pub fn transport_args(&self) -> Value {
        match self {
            Self::PropertyChange(payload) => json!(["mpv-prop-change", payload]),
            Self::Ended(payload) => json!(["mpv-event-ended", payload]),
            Self::ShowPictureInPicture(payload) => json!(["showPictureInPicture", payload]),
            Self::HidePictureInPicture(payload) => json!(["hidePictureInPicture", payload]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_player_command_names() {
        assert_eq!(
            serde_json::to_value(PlayerCommand::ObserveProperty("pause".to_string())).unwrap(),
            json!({"method": "mpv-observe-prop", "data": "pause"})
        );
        assert_eq!(
            serde_json::to_value(PlayerCommand::SetProperty("pause".to_string(), json!(true)))
                .unwrap(),
            json!({"method": "mpv-set-prop", "data": ["pause", true]})
        );
    }

    #[test]
    fn player_events_use_shell_transport_args_shape() {
        assert_eq!(
            PlayerEvent::PropertyChange(PlayerPropertyChange {
                name: "pause".to_string(),
                data: json!(true),
            })
            .transport_args(),
            json!(["mpv-prop-change", {"name": "pause", "data": true}])
        );
        assert_eq!(
            PlayerEvent::Ended(PlayerEnded {
                reason: "eof".to_string(),
                error: None,
            })
            .transport_args(),
            json!(["mpv-event-ended", {"reason": "eof"}])
        );
        assert_eq!(
            PlayerEvent::ShowPictureInPicture(json!({})).transport_args(),
            json!(["showPictureInPicture", {}])
        );
        assert_eq!(
            PlayerEvent::HidePictureInPicture(json!({})).transport_args(),
            json!(["hidePictureInPicture", {}])
        );
    }
}
