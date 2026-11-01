//! The local streaming server: where its files are, how it is started, and what state it is in
//! once it is. It reports a state, not a sentence; the Settings screen does the wording.

use std::path::{Path, PathBuf};

use stremio_lightning_core::streaming_server::{
    CommandSpec, RealProcessSpawner, StreamingServerSupervisor,
};

pub(crate) const ADDRESS: &str = "127.0.0.1:11470";

pub(crate) type Supervisor = StreamingServerSupervisor<RealProcessSpawner>;

/// What the server is doing. `PortTaken` is the normal case on a machine already running the
/// official app: our child exits at once because it cannot bind, and the app uses that one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum State {
    NotStarted(String),
    Ready,
    PortTaken,
    NotAnswering,
}

/// The folder holding `resources/`: the executable's folder unless `STREMIO_LIGHTNING_BUNDLE_DIR`
/// says otherwise.
pub(crate) fn bundle_dir() -> PathBuf {
    std::env::var_os("STREMIO_LIGHTNING_BUNDLE_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf))
        })
        .unwrap_or_default()
}

/// The stream server and its ffmpeg live in `<bundle>/resources`.
pub(crate) fn spec(bundle: &Path, data_dir: &Path) -> CommandSpec {
    let resources = bundle.join("resources");
    let program = resources.join(if cfg!(windows) {
        "stream-server.exe"
    } else {
        "stream-server"
    });

    // The server finds ffmpeg and ffprobe through PATH, so the bundled ones go first.
    let mut search = vec![resources];
    search.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(search)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();

    let logs = data_dir.join("logs");
    CommandSpec {
        program,
        // The Windows build of the server opens a tray icon unless told not to.
        args: if cfg!(windows) {
            vec![PathBuf::from("--no-tray")]
        } else {
            Vec::new()
        },
        env: [("PATH".to_owned(), path)].into(),
        stdout_log: logs.join("stremio-server.stdout.log"),
        stderr_log: logs.join("stremio-server.stderr.log"),
    }
}

/// Waits for something to answer on the server port and says what that is. The port is fixed,
/// so an already running server (the official app, say) answers instead of ours.
pub(crate) async fn wait_until_answering(
    server: &Supervisor,
    started: Result<(), String>,
) -> State {
    let Err(error) = started else {
        let mut answering = false;
        for _ in 0..60 {
            if tokio::net::TcpStream::connect(ADDRESS).await.is_ok() {
                answering = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        // Give a child that cannot bind the port time to exit before judging.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        return match (answering, server.is_running()) {
            (true, true) => State::Ready,
            (true, false) => State::PortTaken,
            (false, _) => State::NotAnswering,
        };
    };
    State::NotStarted(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_runs_from_the_bundle_with_its_ffmpeg_first_on_path() {
        let bundle = Path::new("bundle");
        let spec = spec(bundle, Path::new("data"));

        let resources = bundle.join("resources");
        assert_eq!(spec.program.parent(), Some(resources.as_path()));
        let path = spec.env.get("PATH").expect("PATH is set");
        assert_eq!(std::env::split_paths(path).next(), Some(resources));
        assert_eq!(spec.stdout_log.parent(), Some(Path::new("data/logs")));
    }
}
