use super::*;
use serde_json::json;

#[test]
fn maps_transport_commands_to_shared_player_commands() {
    let mut player = WindowsPlayer::default();
    player
        .handle_transport("mpv-observe-prop", Some(json!("pause")))
        .unwrap();
    player
        .handle_transport("mpv-set-prop", Some(json!(["pause", true])))
        .unwrap();
    player
        .handle_transport(
            "mpv-command",
            Some(json!(["loadfile", "file:///video.mp4"])),
        )
        .unwrap();

    assert_eq!(
        player.commands(),
        &[
            PlayerCommand::ObserveProperty("pause".to_string()),
            PlayerCommand::SetProperty("pause".to_string(), json!(true)),
            PlayerCommand::Command(vec![json!("loadfile"), json!("file:///video.mp4")]),
        ]
    );
}

#[test]
fn clears_secondary_subtitle_before_setting_sid() {
    let mut player = WindowsPlayer::default();
    player
        .handle_transport("mpv-set-prop", Some(json!(["sid", 3])))
        .unwrap();

    assert_eq!(
        player.commands(),
        &[
            PlayerCommand::SetProperty("secondary-sid".to_string(), json!("no")),
            PlayerCommand::SetProperty("sid".to_string(), json!(3)),
        ]
    );
}

#[test]
fn clears_secondary_subtitle_after_sub_add() {
    let mut player = WindowsPlayer::default();
    player
        .handle_transport(
            "mpv-command",
            Some(json!(["sub-add", "file:///tmp/sub.srt", "select"])),
        )
        .unwrap();

    assert_eq!(
        player.commands(),
        &[
            PlayerCommand::Command(vec![
                json!("sub-add"),
                json!("file:///tmp/sub.srt"),
                json!("select"),
            ]),
            PlayerCommand::SetProperty("secondary-sid".to_string(), json!("no")),
        ]
    );
}

#[test]
fn extracts_mpv_command_name_and_string_args() {
    let values = [json!("loadfile"), json!("file:///video.mp4"), json!(true)];
    let (name, args) = command_name_and_args(&values).unwrap();
    assert_eq!(name, "loadfile");
    assert_eq!(args, ["file:///video.mp4", "true"]);
}

#[test]
fn preserves_urls_redacts_secrets_and_bounds_mpv_diagnostics() {
    let sanitized = sanitize_mpv_log_message(
        "Failed HTTPS://media.example/video?token=secret and magnet:?xt=secret",
    );
    assert_eq!(
        sanitized,
        "Failed HTTPS://media.example/video?token=[redacted] and magnet:?xt=secret"
    );

    let long = sanitize_mpv_log_message(&"x".repeat(MAX_MPV_LOG_MESSAGE_LENGTH + 1));
    assert!(long.starts_with(&"x".repeat(MAX_MPV_LOG_MESSAGE_LENGTH)));
    assert!(long.ends_with("... [truncated]"));
}

#[cfg(windows)]
#[test]
fn maps_end_file_reasons_to_shared_causes() {
    use super::backend::end_file_cause;
    use stremio_lightning_core::player_api::EndFileCause;

    assert_eq!(
        end_file_cause(libmpv2::mpv_end_file_reason::Eof),
        EndFileCause::Eof
    );
    assert_eq!(
        end_file_cause(libmpv2::mpv_end_file_reason::Stop),
        EndFileCause::Stop
    );
    assert_eq!(
        end_file_cause(libmpv2::mpv_end_file_reason::Redirect),
        EndFileCause::Redirect
    );
    assert_eq!(
        end_file_cause(libmpv2::mpv_end_file_reason::Error),
        EndFileCause::Error
    );
    assert_eq!(
        end_file_cause(libmpv2::mpv_end_file_reason::Quit),
        EndFileCause::Quit
    );
}
