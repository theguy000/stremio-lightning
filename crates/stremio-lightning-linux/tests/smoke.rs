#[test]
#[ignore = "requires STREMIO_LIGHTNING_LINUX_SMOKE=1"]
fn linux_shell_smoke() {
    if std::env::var("STREMIO_LIGHTNING_LINUX_SMOKE")
        .ok()
        .as_deref()
        != Some("1")
    {
        eprintln!("Set STREMIO_LIGHTNING_LINUX_SMOKE=1 to run the Linux shell smoke test");
        return;
    }

    stremio_lightning_linux::smoke::run_local_smoke().unwrap();
}
