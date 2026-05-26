#[test]
#[ignore = "requires STREMIO_LIGHTNING_LINUX_E2E=1"]
fn linux_shell_e2e() {
    let enabled = std::env::var("STREMIO_LIGHTNING_LINUX_E2E")
        .or_else(|_| std::env::var("STREMIO_LIGHTNING_LINUX_SMOKE"))
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);

    if !enabled {
        eprintln!("Set STREMIO_LIGHTNING_LINUX_E2E=1 or STREMIO_LIGHTNING_LINUX_SMOKE=1 to run the Linux shell E2E integration test");
        return;
    }

    stremio_lightning_linux::e2e_host::run_local_e2e().unwrap();
}
