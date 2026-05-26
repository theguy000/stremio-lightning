use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use stremio_lightning_linux::streaming_server::{
    CommandSpec, ProcessChild, ProcessSpawner, RealProcessSpawner,
};

#[test]
fn test_real_process_spawner_execution() {
    // If this is the child spawned by the test, run child logic and exit immediately
    if std::env::var("STREMIO_TEST_SPAWN_CHILD").is_ok() {
        println!("hello world from spawned child");
        std::process::exit(0);
    }

    let temp_dir = std::env::temp_dir().join(format!(
        "stremio-lightning-test-spawner-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let stdout_log = temp_dir.join("stdout.log");
    let stderr_log = temp_dir.join("stderr.log");

    // Parent retrieves the path to the currently running test binary
    let current_exe = std::env::current_exe().expect("Failed to find current exe path");

    let mut env = BTreeMap::new();
    env.insert("STREMIO_TEST_SPAWN_CHILD".to_string(), "1".to_string());

    let spec = CommandSpec {
        program: current_exe,
        // Pass the name of this test and --nocapture so child stdout is written to the log
        args: vec![
            PathBuf::from("test_real_process_spawner_execution"),
            PathBuf::from("--nocapture"),
        ],
        env,
        stdout_log: stdout_log.clone(),
        stderr_log: stderr_log.clone(),
    };

    let spawner = RealProcessSpawner;
    let mut child = spawner.spawn(spec).expect("Failed to spawn self");

    // Monitor child exit state
    let mut count = 0;
    let mut exited = false;
    while count < 40 {
        if child.has_exited().unwrap() {
            exited = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        count += 1;
    }

    assert!(exited, "Child process did not exit in time");
    assert!(child.has_exited().unwrap());

    let stdout_content = fs::read_to_string(&stdout_log).unwrap();
    assert!(
        stdout_content.contains("hello world from spawned child"),
        "Stdout log was: {stdout_content}"
    );

    let _ = fs::remove_dir_all(temp_dir);
}
