use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use http::Request;
use stremio_core::runtime::Env;
use stremio_lightning_native_env::AppEnv;
use tokio::runtime::Runtime;

// `AppEnv` is process-global, so every test shares one runtime and one init.
fn rt() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        let rt = Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("sl-native-env-test-{}", std::process::id()));
        AppEnv::init(dir, rt.handle().clone()).unwrap();
        rt
    })
}

#[test]
fn storage_round_trips_and_deletes() {
    rt().block_on(async {
        let key = "profile/with:odd chars";
        assert_eq!(AppEnv::get_storage::<Vec<u32>>(key).await.unwrap(), None);

        AppEnv::set_storage(key, Some(&vec![1u32, 2, 3]))
            .await
            .unwrap();
        assert_eq!(
            AppEnv::get_storage::<Vec<u32>>(key).await.unwrap(),
            Some(vec![1, 2, 3])
        );

        AppEnv::set_storage::<Vec<u32>>(key, None).await.unwrap();
        assert_eq!(AppEnv::get_storage::<Vec<u32>>(key).await.unwrap(), None);
        // Deleting a missing key is not an error.
        AppEnv::set_storage::<Vec<u32>>(key, None).await.unwrap();
    });
}

#[test]
fn sequential_tasks_run_in_submission_order() {
    rt().block_on(async {
        let seen = Arc::new(Mutex::new(Vec::new()));
        for i in 0..20u32 {
            let seen = seen.clone();
            AppEnv::exec_sequential(async move {
                // Earlier jobs sleep longer, so only a real queue keeps the order.
                tokio::time::sleep(Duration::from_millis(u64::from(20 - i))).await;
                seen.lock().unwrap().push(i);
            });
        }
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(*seen.lock().unwrap(), (0..20).collect::<Vec<_>>());
    });
}

#[test]
#[ignore = "needs network"]
fn fetch_reads_a_public_addon_manifest() {
    rt().block_on(async {
        let request = Request::get("https://v3-cinemeta.strem.io/manifest.json")
            .body(())
            .unwrap();
        let manifest: serde_json::Value = AppEnv::fetch(request).await.unwrap();
        assert_eq!(manifest["id"], "com.linvo.cinemeta");
    });
}
