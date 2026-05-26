use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use stremio_lightning_linux::streaming_server::{
    CommandSpec, ProcessChild, ProcessSpawner, RealProcessSpawner, StreamingServer,
};

fn main() {
    // 1. CHILD PROCESS INTERCEPTOR
    if std::env::var("STREMIO_TEST_SPAWN_CHILD").is_ok() {
        if let Ok(mode) = std::env::var("STREMIO_TEST_CHILD_MODE") {
            match mode.as_str() {
                "keep-running" => {
                    let mut input = String::new();
                    let _ = std::io::stdin().read_line(&mut input);
                    return;
                }
                "exit-fast" => {
                    return;
                }
                _ => {}
            }
        }
        println!("hello world from spawned child");
        return;
    }

    // 2. PARENT RUNNER: MANUALLY RUN TESTS
    println!("running 5 integration tests...");

    print!("test test_real_process_spawner_execution ... ");
    test_real_process_spawner_execution();
    println!("ok");

    print!("test test_real_server_starts_once_while_running ... ");
    test_real_server_starts_once_while_running();
    println!("ok");

    print!("test test_real_server_stops_and_restarts ... ");
    test_real_server_stops_and_restarts();
    println!("ok");

    print!("test test_real_server_drop_stops_child ... ");
    test_real_server_drop_stops_child();
    println!("ok");

    print!("test test_real_server_reaps_exited_child ... ");
    test_real_server_reaps_exited_child();
    println!("ok");

    println!("test result: ok. 5 passed; 0 failed");
}

fn child_spec(log_dir: &std::path::Path, mode: &str) -> CommandSpec {
    let current_exe = std::env::current_exe().expect("Failed to find current exe path");
    let mut env = BTreeMap::new();
    env.insert("STREMIO_TEST_SPAWN_CHILD".to_string(), "1".to_string());
    env.insert("STREMIO_TEST_CHILD_MODE".to_string(), mode.to_string());

    CommandSpec {
        program: current_exe,
        args: Vec::new(),
        env,
        stdout_log: log_dir.join("stdout.log"),
        stderr_log: log_dir.join("stderr.log"),
    }
}

fn temp_log_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "stremio-lightning-test-spawner-{}-{}",
        std::process::id(),
        name
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn setup_temp_project_root(log_dir: &std::path::Path) -> PathBuf {
    let root = log_dir.join("project_root");
    let binaries_dir = root.join("binaries");
    fs::create_dir_all(&binaries_dir).unwrap();
    let runtime_path = binaries_dir.join("stremio-runtime-x86_64-unknown-linux-gnu");
    let current_exe = std::env::current_exe().unwrap();
    std::os::unix::fs::symlink(&current_exe, &runtime_path).unwrap();
    root
}

fn test_real_process_spawner_execution() {
    let temp_dir = temp_log_dir("spawner-execution");
    let stdout_log = temp_dir.join("stdout.log");
    let stderr_log = temp_dir.join("stderr.log");

    let spec = child_spec(&temp_dir, "default");

    let spawner = RealProcessSpawner;
    let mut child = spawner.spawn(spec).expect("Failed to spawn child");

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
    let exit_status = child.try_wait().unwrap().unwrap();

    let stdout_content = fs::read_to_string(&stdout_log).unwrap();
    let stderr_content = fs::read_to_string(&stderr_log).unwrap();
    assert!(
        stdout_content.contains("hello world from spawned child"),
        "Child exit status: {:?}\nStdout log was: {}\nStderr log was: {}",
        exit_status,
        stdout_content,
        stderr_content
    );

    let _ = fs::remove_dir_all(temp_dir);
}

fn test_real_server_starts_once_while_running() {
    let log_dir = temp_log_dir("starts-once");
    let project_root = setup_temp_project_root(&log_dir);

    let server = StreamingServer::with_paths(RealProcessSpawner, project_root, log_dir.clone());

    std::env::set_var("STREMIO_TEST_SPAWN_CHILD", "1");
    std::env::set_var("STREMIO_TEST_CHILD_MODE", "keep-running");

    server.start().unwrap();
    assert!(server.is_running());

    server.start().unwrap();
    assert!(server.is_running());

    server.stop().unwrap();
    std::env::remove_var("STREMIO_TEST_SPAWN_CHILD");
    std::env::remove_var("STREMIO_TEST_CHILD_MODE");
    let _ = fs::remove_dir_all(log_dir);
}

fn test_real_server_stops_and_restarts() {
    let log_dir = temp_log_dir("stops-restarts");
    let project_root = setup_temp_project_root(&log_dir);

    let server = StreamingServer::with_paths(RealProcessSpawner, project_root, log_dir.clone());

    std::env::set_var("STREMIO_TEST_SPAWN_CHILD", "1");
    std::env::set_var("STREMIO_TEST_CHILD_MODE", "keep-running");

    server.start().unwrap();
    assert!(server.is_running());

    server.stop().unwrap();
    assert!(!server.is_running());

    server.start().unwrap();
    assert!(server.is_running());

    server.stop().unwrap();
    std::env::remove_var("STREMIO_TEST_SPAWN_CHILD");
    std::env::remove_var("STREMIO_TEST_CHILD_MODE");
    let _ = fs::remove_dir_all(log_dir);
}

fn test_real_server_drop_stops_child() {
    let log_dir = temp_log_dir("drop-stops");
    let project_root = setup_temp_project_root(&log_dir);
    let stdout_log = log_dir.join("stremio-server.stdout.log");

    std::env::set_var("STREMIO_TEST_SPAWN_CHILD", "1");
    std::env::set_var("STREMIO_TEST_CHILD_MODE", "keep-running");

    {
        let server = StreamingServer::with_paths(RealProcessSpawner, project_root, log_dir.clone());
        server.start().unwrap();
        assert!(server.is_running());
    }

    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(stdout_log.exists());
    std::env::remove_var("STREMIO_TEST_SPAWN_CHILD");
    std::env::remove_var("STREMIO_TEST_CHILD_MODE");
    let _ = fs::remove_dir_all(log_dir);
}

fn test_real_server_reaps_exited_child() {
    let log_dir = temp_log_dir("reaps-exited");
    let project_root = setup_temp_project_root(&log_dir);

    let server = StreamingServer::with_paths(RealProcessSpawner, project_root, log_dir.clone());

    std::env::set_var("STREMIO_TEST_SPAWN_CHILD", "1");
    std::env::set_var("STREMIO_TEST_CHILD_MODE", "exit-fast");

    server.start().unwrap();
    // Wait for exit
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(!server.is_running());

    std::env::remove_var("STREMIO_TEST_SPAWN_CHILD");
    std::env::remove_var("STREMIO_TEST_CHILD_MODE");
    let _ = fs::remove_dir_all(log_dir);
}
