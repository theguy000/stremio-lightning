use crate::host::Host;
use crate::settings::ShellSettings;
use crate::single_instance::LaunchIntent;
#[cfg(windows)]
use std::sync::Mutex;
use std::sync::{mpsc, Arc};

pub const WINDOWS_HOST_ADAPTER_NAME: &str = "windows-host-adapter";
pub const HOST_ADAPTER_NAME: &str = WINDOWS_HOST_ADAPTER_NAME;
pub const BRIDGE_LOGGING_NAME: &str = "bridge/logging.js";
pub const BRIDGE_UTILS_NAME: &str = "bridge/utils.js";
pub const BRIDGE_CAST_FALLBACK_NAME: &str = "bridge/cast-fallback.js";
pub const BRIDGE_SHELL_TRANSPORT_NAME: &str = "bridge/shell-transport.js";
pub const BRIDGE_EXTERNAL_LINKS_NAME: &str = "bridge/external-links.js";
pub const BRIDGE_SHELL_DETECTION_NAME: &str = "bridge/shell-detection.js";
pub const BRIDGE_BACK_BUTTON_NAME: &str = "bridge/back-button.js";
pub const BRIDGE_SHORTCUTS_NAME: &str = "bridge/shortcuts.js";
pub const BRIDGE_PIP_NAME: &str = "bridge/pip.js";
pub const BRIDGE_DISCORD_RPC_NAME: &str = "bridge/discord-rpc.js";
pub const BRIDGE_UPDATE_BANNER_NAME: &str = "bridge/update-banner.js";
pub const BRIDGE_NAME: &str = "bridge.js";
pub const MOD_UI_NAME: &str = "mod-ui-svelte.iife.js";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionScript {
    pub name: &'static str,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionBundle {
    scripts: Vec<InjectionScript>,
}

impl InjectionBundle {
    pub fn load() -> Self {
        let mut scripts = vec![InjectionScript {
            name: HOST_ADAPTER_NAME,
            source: host_adapter(),
        }];
        scripts.extend(bridge_module_scripts());
        scripts.extend([
            InjectionScript {
                name: BRIDGE_NAME,
                source: include_str!("../../../web/bridge/bridge.js").to_string(),
            },
            InjectionScript {
                name: MOD_UI_NAME,
                source: include_str!("../../../src/dist/mod-ui-svelte.iife.js").to_string(),
            },
        ]);

        Self { scripts }
    }

    pub fn scripts(&self) -> &[InjectionScript] {
        &self.scripts
    }
}

fn bridge_module_scripts() -> Vec<InjectionScript> {
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
            name: BRIDGE_CAST_FALLBACK_NAME,
            source: include_str!("../../../web/bridge/src/cast-fallback.js").to_string(),
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
    ]
}

pub struct WindowsWebView2Shell {
    url: String,
    devtools: bool,
    injection: InjectionBundle,
    #[allow(dead_code)]
    host: Arc<Host>,
    launch_intents: mpsc::Receiver<LaunchIntent>,
    #[cfg(windows)]
    ui_notifier: Arc<Mutex<Option<crate::window::UiThreadNotifier>>>,
}

impl WindowsWebView2Shell {
    #[cfg(windows)]
    pub fn new(
        settings: ShellSettings,
        launch_intents: mpsc::Receiver<LaunchIntent>,
        ui_notifier: Arc<Mutex<Option<crate::window::UiThreadNotifier>>>,
    ) -> Result<Self, String> {
        Self::build(settings, launch_intents, ui_notifier)
    }

    #[cfg(not(windows))]
    pub fn new(
        settings: ShellSettings,
        launch_intents: mpsc::Receiver<LaunchIntent>,
    ) -> Result<Self, String> {
        Self::build(settings, launch_intents)
    }

    #[cfg(windows)]
    fn build(
        settings: ShellSettings,
        launch_intents: mpsc::Receiver<LaunchIntent>,
        ui_notifier: Arc<Mutex<Option<crate::window::UiThreadNotifier>>>,
    ) -> Result<Self, String> {
        let url = settings.webui_url;
        let devtools = settings.devtools;
        if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1:")) {
            return Err(format!("Unsupported WebView2 load URL: {url}"));
        }

        Ok(Self {
            url,
            devtools,
            injection: InjectionBundle::load(),
            host: Arc::new(Host::with_streaming_server_disabled(
                env!("CARGO_PKG_VERSION"),
                settings.streaming_server_disabled,
            )),
            launch_intents,
            ui_notifier,
        })
    }

    #[cfg(not(windows))]
    fn build(
        settings: ShellSettings,
        launch_intents: mpsc::Receiver<LaunchIntent>,
    ) -> Result<Self, String> {
        let url = settings.webui_url;
        let devtools = settings.devtools;
        if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1:")) {
            return Err(format!("Unsupported WebView2 load URL: {url}"));
        }

        Ok(Self {
            url,
            devtools,
            injection: InjectionBundle::load(),
            host: Arc::new(Host::with_streaming_server_disabled(
                env!("CARGO_PKG_VERSION"),
                settings.streaming_server_disabled,
            )),
            launch_intents,
        })
    }

    pub fn document_start_script_names(&self) -> Vec<&'static str> {
        self.injection
            .scripts()
            .iter()
            .map(|script| script.name)
            .collect()
    }

    pub fn run(self) -> Result<(), String> {
        platform::run_webview2_shell(
            &self.url,
            self.devtools,
            &self.injection,
            self.host,
            self.launch_intents,
            #[cfg(windows)]
            self.ui_notifier,
        )
    }
}

#[cfg(any(windows, test))]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct CleanupReport {
    failures: Vec<String>,
}

#[cfg(any(windows, test))]
impl CleanupReport {
    fn record(&mut self, action: &'static str, result: Result<(), String>) {
        if let Err(error) = result {
            self.failures.push(format!("{action}: {error}"));
        }
    }

    #[cfg(test)]
    fn failures(&self) -> &[String] {
        &self.failures
    }

    #[cfg(windows)]
    fn log(self, context: &str) {
        for failure in self.failures {
            stremio_lightning_core::logging::error(
                "native.webview.windows",
                format!("{context}: {failure}"),
            );
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::{mpsc, Arc, CleanupReport, Host, InjectionBundle, LaunchIntent, Mutex};
    use crate::host::WindowsIpcOutbound;
    use crate::window::{
        focus_window, run_native_window_with_handler, MediaKeyAction, NativeWindowHandler,
        UiThreadNotifier, WindowConfig, WindowVisualState,
    };
    use std::{path::PathBuf, ptr};
    use webview2_com::{
        AddScriptToExecuteOnDocumentCreatedCompletedHandler, CoTaskMemPWSTR,
        CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
        CreateCoreWebView2EnvironmentCompletedHandler, Microsoft::Web::WebView2::Win32::*,
        NavigationCompletedEventHandler, NavigationStartingEventHandler,
        WebMessageReceivedEventHandler,
    };
    use windows::core::{Interface, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{E_POINTER, HWND, RECT};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

    impl CleanupReport {
        fn record_windows(&mut self, action: &'static str, result: windows::core::Result<()>) {
            self.record(action, result.map_err(|error| error.to_string()));
        }
    }

    pub fn run_webview2_shell(
        url: &str,
        devtools: bool,
        injection: &InjectionBundle,
        host: Arc<Host>,
        launch_intents: mpsc::Receiver<LaunchIntent>,
        ui_notifier: Arc<Mutex<Option<UiThreadNotifier>>>,
    ) -> Result<(), String> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|error| format!("Failed to initialize COM for WebView2: {error}"))?;
        }

        run_native_window_with_handler(
            WindowConfig::default(),
            WebView2WindowHost::new(
                url.to_string(),
                devtools,
                injection.clone(),
                host,
                launch_intents,
                ui_notifier,
            ),
        )
    }

    struct WebView2WindowHost {
        url: String,
        devtools: bool,
        injection: InjectionBundle,
        host: Arc<Host>,
        runtime: Option<WebView2Runtime>,
        launch_intents: mpsc::Receiver<LaunchIntent>,
        ui_notifier: Arc<Mutex<Option<UiThreadNotifier>>>,
    }

    #[derive(Default)]
    struct WebView2EventTokens {
        message_received: Option<i64>,
        navigation_starting: Option<i64>,
        navigation_completed: Option<i64>,
    }

    struct WebView2Runtime {
        controller: Option<ICoreWebView2Controller>,
        webview: Option<ICoreWebView2>,
        event_tokens: WebView2EventTokens,
    }

    impl WebView2Runtime {
        fn create(
            hwnd: HWND,
            devtools: bool,
            injection: &InjectionBundle,
            host: Arc<Host>,
            url: &str,
        ) -> Result<Self, String> {
            let environment = create_environment()?;
            let controller = create_controller(&environment, hwnd)?;
            let mut runtime = Self {
                controller: Some(controller),
                webview: None,
                event_tokens: WebView2EventTokens::default(),
            };

            runtime.configure_controller()?;
            runtime.resize_to_client_rect(hwnd)?;
            runtime.show()?;
            runtime.configure_webview(devtools, injection, host, url)?;

            Ok(runtime)
        }

        fn controller(&self) -> Result<&ICoreWebView2Controller, String> {
            self.controller
                .as_ref()
                .ok_or_else(|| "WebView2 controller is not available".to_string())
        }

        fn configure_controller(&self) -> Result<(), String> {
            configure_controller(self.controller()?)
        }

        fn resize_to_client_rect(&self, hwnd: HWND) -> Result<(), String> {
            let mut rect = RECT::default();
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect)
                    .map_err(|error| format!("Failed to read WebView2 host bounds: {error}"))?;
                self.controller()?
                    .SetBounds(rect)
                    .map_err(|error| format!("Failed to resize WebView2 controller: {error}"))?;
            }
            Ok(())
        }

        fn show(&self) -> Result<(), String> {
            unsafe {
                self.controller()?
                    .SetIsVisible(true)
                    .map_err(|error| format!("Failed to show WebView2 controller: {error}"))?;
            }
            Ok(())
        }

        fn configure_webview(
            &mut self,
            devtools: bool,
            injection: &InjectionBundle,
            host: Arc<Host>,
            url: &str,
        ) -> Result<(), String> {
            self.webview = Some(unsafe {
                self.controller()?
                    .CoreWebView2()
                    .map_err(|error| format!("Failed to get WebView2 instance: {error}"))?
            });
            let webview = self
                .webview
                .as_ref()
                .ok_or_else(|| "WebView2 instance is not available".to_string())?
                .clone();

            configure_webview(&webview, devtools)?;
            add_injection_scripts(&webview, injection)?;
            self.event_tokens.message_received = Some(add_message_handler(&webview, host.clone())?);
            self.event_tokens.navigation_starting = Some(add_navigation_starting_handler(
                &webview,
                host,
                url.to_string(),
            )?);
            self.event_tokens.navigation_completed =
                Some(add_navigation_completed_handler(&webview)?);
            navigate(&webview, url)
        }

        fn post_outbound_messages(&self, messages: Vec<WindowsIpcOutbound>) -> Result<(), String> {
            let Some(webview) = self.webview.as_ref() else {
                return Ok(());
            };
            post_outbound_messages(webview, messages)
        }

        fn cleanup(&mut self) {
            let mut report = CleanupReport::default();

            if let Some(webview) = self.webview.as_ref() {
                if let Some(token) = self.event_tokens.message_received.take() {
                    report.record_windows("remove WebView2 message handler", unsafe {
                        webview.remove_WebMessageReceived(token)
                    });
                }
                if let Some(token) = self.event_tokens.navigation_starting.take() {
                    report.record_windows("remove WebView2 navigation starting handler", unsafe {
                        webview.remove_NavigationStarting(token)
                    });
                }
                if let Some(token) = self.event_tokens.navigation_completed.take() {
                    report.record_windows("remove WebView2 navigation completed handler", unsafe {
                        webview.remove_NavigationCompleted(token)
                    });
                }
            }

            if let Some(controller) = self.controller.take() {
                report.record_windows("close WebView2 controller", unsafe { controller.Close() });
            }
            self.webview = None;
            report.log("Windows WebView2 cleanup failed");
        }
    }

    impl Drop for WebView2Runtime {
        fn drop(&mut self) {
            self.cleanup();
        }
    }

    impl WebView2WindowHost {
        fn new(
            url: String,
            devtools: bool,
            injection: InjectionBundle,
            host: Arc<Host>,
            launch_intents: mpsc::Receiver<LaunchIntent>,
            ui_notifier: Arc<Mutex<Option<UiThreadNotifier>>>,
        ) -> Self {
            Self {
                url,
                devtools,
                injection,
                host,
                runtime: None,
                launch_intents,
                ui_notifier,
            }
        }

        fn resize_to_client_rect(&self, hwnd: HWND) -> Result<(), String> {
            let Some(runtime) = self.runtime.as_ref() else {
                return Ok(());
            };
            runtime.resize_to_client_rect(hwnd)
        }

        fn post_host_events(&self) -> Result<(), String> {
            let Some(runtime) = self.runtime.as_ref() else {
                return Ok(());
            };
            runtime.post_outbound_messages(self.host.drain_ipc_events())
        }

        fn start_host_runtime(&self, hwnd: HWND, notifier: UiThreadNotifier) -> Result<(), String> {
            *self.ui_notifier.lock().map_err(|e| e.to_string())? = Some(notifier);
            self.host.bind_native_window(hwnd)?;
            self.host.initialize_native_player(hwnd, notifier)?;
            self.host.start_streaming_server()
        }
    }

    impl NativeWindowHandler for WebView2WindowHost {
        fn on_created(&mut self, hwnd: HWND) -> Result<(), String> {
            let notifier = UiThreadNotifier { hwnd };
            self.start_host_runtime(hwnd, notifier)?;
            self.runtime = Some(WebView2Runtime::create(
                hwnd,
                self.devtools,
                &self.injection,
                self.host.clone(),
                &self.url,
            )?);
            Ok(())
        }

        fn on_resized(&mut self, hwnd: HWND, _client_rect: RECT) -> Result<(), String> {
            self.resize_to_client_rect(hwnd)
        }

        fn on_window_state_changed(
            &mut self,
            _hwnd: HWND,
            state: WindowVisualState,
        ) -> Result<(), String> {
            match state {
                WindowVisualState::Minimized => self.host.update_window_visible(false)?,
                WindowVisualState::Maximized => {
                    self.host.update_window_visible(true)?;
                    self.host.update_window_maximized(true)?;
                }
                WindowVisualState::Restored => {
                    self.host.update_window_visible(true)?;
                    self.host.update_window_maximized(false)?;
                }
            }
            self.post_host_events()
        }

        fn on_focus_changed(&mut self, _hwnd: HWND, focused: bool) -> Result<(), String> {
            self.host.update_window_focus(focused)?;
            self.post_host_events()
        }

        fn on_media_key(&mut self, _hwnd: HWND, action: MediaKeyAction) -> Result<(), String> {
            let action = match action {
                MediaKeyAction::PlayPause => "play-pause",
                MediaKeyAction::NextTrack => "next-track",
                MediaKeyAction::PreviousTrack => "previous-track",
            };
            self.host.emit_media_key(action)?;
            self.post_host_events()
        }

        fn on_ui_thread_wake(&mut self, hwnd: HWND) -> Result<(), String> {
            while let Ok(intent) = self.launch_intents.try_recv() {
                focus_window(hwnd);
                self.host.emit_launch_intent(intent)?;
            }
            self.post_host_events()
        }

        fn on_destroying(&mut self, _hwnd: HWND) {
            if let Ok(mut notifier) = self.ui_notifier.lock() {
                *notifier = None;
            }
            if let Err(error) = self.host.shutdown() {
                stremio_lightning_core::logging::error(
                    "native.webview.windows",
                    format!("Failed to shut down Windows runtime: {error}"),
                );
            }
            if let Some(mut runtime) = self.runtime.take() {
                runtime.cleanup();
            }
        }
    }

    fn create_environment() -> Result<ICoreWebView2Environment, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let user_data_dir = webview2_user_data_dir()?;
        std::fs::create_dir_all(&user_data_dir).map_err(|error| {
            format!(
                "Failed to create WebView2 user data directory '{}': {error}",
                user_data_dir.display()
            )
        })?;
        let user_data_dir = user_data_dir
            .to_str()
            .ok_or_else(|| "WebView2 user data directory is not valid Unicode".to_string())?;
        let user_data_dir = windows::core::HSTRING::from(user_data_dir);
        let options = CoreWebView2EnvironmentOptions::default();
        unsafe {
            options.set_additional_browser_arguments(
                "--autoplay-policy=no-user-gesture-required --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection"
                    .to_string(),
            );
        }
        let options: ICoreWebView2EnvironmentOptions = options.into();
        CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| unsafe {
                CreateCoreWebView2EnvironmentWithOptions(
                    PCWSTR::null(),
                    PCWSTR(user_data_dir.as_ptr()),
                    &options,
                    &handler,
                )
                .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |error_code, environment| {
                error_code?;
                tx.send(environment.ok_or_else(|| windows::core::Error::from(E_POINTER)))
                    .map_err(|_| windows::core::Error::from(E_POINTER))?;
                Ok(())
            }),
        )
        .map_err(|error| format!("Failed to create WebView2 environment: {error:?}"))?;

        rx.recv()
            .map_err(|_| "WebView2 environment callback did not return".to_string())?
            .map_err(|error| format!("WebView2 environment creation failed: {error}"))
    }

    fn webview2_user_data_dir() -> Result<PathBuf, String> {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("stremio-lightning").join("WebView2"))
            .ok_or_else(|| "LOCALAPPDATA is not available for WebView2 user data".to_string())
    }

    fn create_controller(
        environment: &ICoreWebView2Environment,
        hwnd: HWND,
    ) -> Result<ICoreWebView2Controller, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let environment = environment.clone();
        CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| unsafe {
                environment
                    .CreateCoreWebView2Controller(hwnd, &handler)
                    .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |error_code, controller| {
                error_code?;
                tx.send(controller.ok_or_else(|| windows::core::Error::from(E_POINTER)))
                    .map_err(|_| windows::core::Error::from(E_POINTER))?;
                Ok(())
            }),
        )
        .map_err(|error| format!("Failed to create WebView2 controller: {error:?}"))?;

        rx.recv()
            .map_err(|_| "WebView2 controller callback did not return".to_string())?
            .map_err(|error| format!("WebView2 controller creation failed: {error}"))
    }

    fn configure_controller(controller: &ICoreWebView2Controller) -> Result<(), String> {
        // Stremio renders video through MPV using the native parent HWND. WebView2 must be
        // transparent so its HTML controls overlay MPV instead of painting an opaque white layer.
        let controller2 = controller
            .cast::<ICoreWebView2Controller2>()
            .map_err(|error| format!("Failed to get WebView2 controller2: {error}"))?;
        unsafe {
            controller2
                .SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                    A: 0,
                    R: 255,
                    G: 255,
                    B: 255,
                })
                .map_err(|error| {
                    format!("Failed to set transparent WebView2 background: {error}")
                })?;
        }
        Ok(())
    }

    fn configure_webview(webview: &ICoreWebView2, devtools: bool) -> Result<(), String> {
        let settings = unsafe {
            webview
                .Settings()
                .map_err(|error| format!("Failed to get WebView2 settings: {error}"))?
        };
        unsafe {
            apply_webview_setting("disable status bar", settings.SetIsStatusBarEnabled(false));
            apply_webview_setting(
                "set devtools availability",
                settings.SetAreDevToolsEnabled(devtools),
            );
            apply_webview_setting(
                "disable zoom controls",
                settings.SetIsZoomControlEnabled(false),
            );
            apply_webview_setting(
                "disable built-in error page",
                settings.SetIsBuiltInErrorPageEnabled(false),
            );
            apply_webview_setting(
                "disable host objects",
                settings.SetAreHostObjectsAllowed(false),
            );
            apply_webview_setting(
                "disable default script dialogs",
                settings.SetAreDefaultScriptDialogsEnabled(false),
            );
        }
        Ok(())
    }

    fn apply_webview_setting(action: &'static str, result: windows::core::Result<()>) {
        if let Err(error) = result {
            stremio_lightning_core::logging::error(
                "native.webview.windows",
                format!("Failed to {action}: {error}"),
            );
        }
    }

    fn add_injection_scripts(
        webview: &ICoreWebView2,
        injection: &InjectionBundle,
    ) -> Result<(), String> {
        for script in injection.scripts() {
            let source = script.source.clone();
            let webview = webview.clone();
            AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
                Box::new(move |handler| unsafe {
                    let source = CoTaskMemPWSTR::from(source.as_str());
                    webview
                        .AddScriptToExecuteOnDocumentCreated(*source.as_ref().as_pcwstr(), &handler)
                        .map_err(webview2_com::Error::WindowsError)
                }),
                Box::new(|error_code, _id| error_code),
            )
            .map_err(|error| {
                format!(
                    "Failed to inject WebView2 script '{}': {error:?}",
                    script.name
                )
            })?;
        }
        Ok(())
    }

    fn add_message_handler(webview: &ICoreWebView2, host: Arc<Host>) -> Result<i64, String> {
        let mut token = 0;
        unsafe {
            webview
                .add_WebMessageReceived(
                    &WebMessageReceivedEventHandler::create(Box::new(move |webview, args| {
                        if let (Some(webview), Some(args)) = (webview, args) {
                            let mut message = PWSTR(ptr::null_mut());
                            if args.WebMessageAsJson(&mut message).is_ok() {
                                let message = CoTaskMemPWSTR::from(message);
                                let message = message.to_string();
                                if is_toggle_devtools_message(&message) {
                                    if let Err(error) = webview.OpenDevToolsWindow() {
                                        stremio_lightning_core::logging::error(
                                            "native.webview.windows",
                                            format!("Failed to open WebView2 DevTools: {error}"),
                                        );
                                    }
                                }
                                if let Err(error) = post_outbound_messages(
                                    &webview,
                                    host.dispatch_ipc_message(&message),
                                ) {
                                    stremio_lightning_core::logging::error(
                                        "native.webview.windows",
                                        format!("Failed to post WebView2 IPC response: {error}"),
                                    );
                                }
                            }
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|error| format!("Failed to attach WebView2 message handler: {error}"))?;
        }
        Ok(token)
    }

    fn is_toggle_devtools_message(message: &str) -> bool {
        serde_json::from_str::<serde_json::Value>(message)
            .ok()
            .and_then(|value| {
                (value.get("kind").and_then(serde_json::Value::as_str) == Some("invoke"))
                    .then_some(value)
            })
            .and_then(|value| {
                value
                    .get("payload")?
                    .get("command")?
                    .as_str()
                    .map(str::to_string)
            })
            .as_deref()
            == Some("toggle_devtools")
    }

    fn add_navigation_starting_handler(
        webview: &ICoreWebView2,
        host: Arc<Host>,
        app_url: String,
    ) -> Result<i64, String> {
        let mut token = 0;
        unsafe {
            webview
                .add_NavigationStarting(
                    &NavigationStartingEventHandler::create(Box::new(move |_webview, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };

                        let mut uri = PWSTR(ptr::null_mut());
                        args.Uri(&mut uri)?;
                        let uri = CoTaskMemPWSTR::from(uri);
                        let uri = uri.to_string();
                        if !super::is_allowed_webview_navigation(&app_url, &uri) {
                            args.SetCancel(true)?;
                            if let Err(error) = host.invoke(
                                "open_external_url",
                                Some(serde_json::json!({ "url": uri })),
                            ) {
                                stremio_lightning_core::logging::error(
                                    "native.webview.windows",
                                    format!("Failed to open external navigation URL: {error}"),
                                );
                            }
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|error| {
                    format!("Failed to attach WebView2 navigation handler: {error}")
                })?;
        }
        Ok(token)
    }

    fn post_outbound_messages(
        webview: &ICoreWebView2,
        messages: Vec<WindowsIpcOutbound>,
    ) -> Result<(), String> {
        for outbound in messages {
            let serialized = serde_json::to_string(&outbound)
                .map_err(|error| format!("Failed to serialize Windows IPC response: {error}"))?;
            let serialized = CoTaskMemPWSTR::from(serialized.as_str());
            unsafe {
                webview
                    .PostWebMessageAsString(*serialized.as_ref().as_pcwstr())
                    .map_err(|error| format!("Failed to post WebView2 IPC response: {error}"))?;
            }
        }
        Ok(())
    }

    fn add_navigation_completed_handler(webview: &ICoreWebView2) -> Result<i64, String> {
        let mut token = 0;
        unsafe {
            webview
                .add_NavigationCompleted(
                    &NavigationCompletedEventHandler::create(Box::new(move |webview, _args| {
                        if let Some(webview) = webview {
                            let message = CoTaskMemPWSTR::from(
                                serde_json::json!({
                                    "kind": "native-ready",
                                    "payload": { "shell": "webview2" }
                                })
                                .to_string()
                                .as_str(),
                            );
                            webview.PostWebMessageAsString(*message.as_ref().as_pcwstr())?;
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|error| {
                    format!("Failed to attach WebView2 navigation completed handler: {error}")
                })?;
        }
        Ok(token)
    }

    fn navigate(webview: &ICoreWebView2, url: &str) -> Result<(), String> {
        let url = CoTaskMemPWSTR::from(url);
        unsafe {
            webview
                .Navigate(*url.as_ref().as_pcwstr())
                .map_err(|error| format!("Failed to navigate WebView2: {error}"))
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{mpsc, Arc, Host, InjectionBundle, LaunchIntent};

    pub fn run_webview2_shell(
        _url: &str,
        _devtools: bool,
        _injection: &InjectionBundle,
        _host: Arc<Host>,
        _launch_intents: mpsc::Receiver<LaunchIntent>,
    ) -> Result<(), String> {
        Err("WebView2 shell can only run on Windows".to_string())
    }
}

#[cfg(any(windows, test))]
fn is_allowed_webview_navigation(app_url: &str, target_url: &str) -> bool {
    let target = target_url.trim();
    if target.eq_ignore_ascii_case("about:blank") {
        return true;
    }

    match (url_origin(app_url), url_origin(target)) {
        (Some(app_origin), Some(target_origin)) => app_origin == target_origin,
        _ => false,
    }
}

#[cfg(any(windows, test))]
fn url_origin(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let scheme = url[..scheme_end].to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }

    let authority_start = scheme_end + 3;
    let authority = url[authority_start..]
        .split(['/', '?', '#'])
        .next()?
        .to_ascii_lowercase();
    if authority.is_empty() || authority.contains('@') {
        return None;
    }

    Some(format!("{scheme}://{authority}"))
}

pub fn windows_host_adapter() -> String {
    host_adapter()
}

pub fn host_adapter() -> String {
    r#"(function () {
  "use strict";

  if (window.StremioLightningHost) return;

  var nativeWebview = window.chrome && window.chrome.webview;
  var nativePostMessage = nativeWebview && typeof nativeWebview.postMessage === "function"
    ? nativeWebview.postMessage.bind(nativeWebview)
    : null;
  var nextRequestId = 1;
  var nextListenerId = 1;
  var pending = {};
  var listeners = {};
  function logError() {
    var logger = window.StremioLightningLogger;
    if (logger) {
      logger.error.apply(logger, ["bridge.host-adapter.windows"].concat(Array.prototype.slice.call(arguments)));
    } else {
      console.error.apply(console, arguments);
    }
  }

  function post(kind, payload) {
    if (!nativePostMessage) {
      return Promise.reject(new Error("WebView2 host bridge is not available"));
    }
    return new Promise(function (resolve, reject) {
      var id = nextRequestId++;
      pending[id] = { resolve: resolve, reject: reject };
      nativePostMessage({
        id: id,
        kind: kind,
        payload: payload || null
      });
    });
  }

  function resolveResponse(message) {
    var callbacks = pending[message.id];
    if (!callbacks) return;
    delete pending[message.id];
    if (message.ok) {
      callbacks.resolve(message.value);
    } else {
      var errorMessage = message.value && message.value.message ? message.value.message : String(message.value);
      callbacks.reject(new Error(errorMessage));
    }
  }

  function dispatchEventMessage(message) {
    Object.keys(listeners).forEach(function (id) {
      var listener = listeners[id];
      if (!listener || listener.event !== message.event) return;
      try {
        listener.callback({ event: message.event, payload: message.payload });
      } catch (error) {
        logError("[StremioLightning] Windows listener failed:", error);
      }
    });
  }

  window.chrome.webview.addEventListener("message", function (event) {
    var message = typeof event.data === "string" ? JSON.parse(event.data) : event.data;
    if (!message || !message.kind) return;
    if (message.kind === "response") resolveResponse(message);
    else if (message.kind === "event") dispatchEventMessage(message);
  });

  window.StremioLightningHost = {
    invoke: function (command, payload) {
      return post("invoke", { command: command, payload: payload });
    },
    listen: function (event, callback) {
      var id = nextListenerId++;
      listeners[id] = { event: event, callback: callback };
      return post("listen", { id: id, event: event }).then(function () {
        return function () {
          delete listeners[id];
          return post("unlisten", { id: id });
        };
      });
    },
    window: {
      minimize: function () { return post("window.minimize"); },
      toggleMaximize: function () { return post("window.toggleMaximize"); },
      close: function () { return post("window.close"); },
      isMaximized: function () { return post("window.isMaximized"); },
      isFullscreen: function () { return post("window.isFullscreen"); },
      setFullscreen: function (fullscreen) { return post("window.setFullscreen", { fullscreen: fullscreen }); },
      startDragging: function () { return post("window.startDragging"); }
    },
    webview: {
      setZoom: function (level) { return post("webview.setZoom", { level: level }); }
    }
  };
})();"#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_windows_adapter_before_shared_bridge() {
        let (_tx, rx) = mpsc::channel();
        #[cfg(windows)]
        let shell = WindowsWebView2Shell::new(
            ShellSettings::from_args([] as [&str; 0]),
            rx,
            Arc::new(Mutex::new(None)),
        )
        .unwrap();

        #[cfg(not(windows))]
        let shell =
            WindowsWebView2Shell::new(ShellSettings::from_args([] as [&str; 0]), rx).unwrap();

        assert_eq!(
            shell.document_start_script_names(),
            vec![
                WINDOWS_HOST_ADAPTER_NAME,
                BRIDGE_LOGGING_NAME,
                BRIDGE_UTILS_NAME,
                BRIDGE_CAST_FALLBACK_NAME,
                BRIDGE_SHELL_TRANSPORT_NAME,
                BRIDGE_EXTERNAL_LINKS_NAME,
                BRIDGE_SHELL_DETECTION_NAME,
                BRIDGE_BACK_BUTTON_NAME,
                BRIDGE_SHORTCUTS_NAME,
                BRIDGE_PIP_NAME,
                BRIDGE_DISCORD_RPC_NAME,
                BRIDGE_UPDATE_BANNER_NAME,
                BRIDGE_NAME,
                MOD_UI_NAME
            ]
        );
    }

    #[test]
    fn moved_shared_bridge_is_loaded_from_web_folder() {
        let bundle = InjectionBundle::load();
        let bridge = bundle
            .scripts()
            .iter()
            .find(|script| script.name == BRIDGE_NAME)
            .unwrap();

        assert!(bridge.source.contains("Native player mode enabled"));
    }

    #[test]
    fn windows_bundle_injects_svelte_mod_ui() {
        let bundle = InjectionBundle::load();
        let mod_ui = bundle
            .scripts()
            .iter()
            .find(|script| script.name == MOD_UI_NAME)
            .unwrap();

        assert!(mod_ui.source.contains("Mods UI initialized"));
    }

    #[test]
    fn windows_adapter_resolves_structured_logger_when_an_error_occurs() {
        let adapter = host_adapter();

        assert!(adapter.contains("function logError()"));
        assert!(adapter.contains("nativeWebview.postMessage.bind(nativeWebview)"));
        assert!(adapter.contains("nativePostMessage({"));
        assert!(adapter.contains("var logger = window.StremioLightningLogger"));
        assert!(adapter.contains("bridge.host-adapter.windows"));
    }

    #[test]
    fn webview_navigation_is_limited_to_configured_origin() {
        let app_url = "https://web.stremio.com/#/";

        assert!(is_allowed_webview_navigation(
            app_url,
            "https://web.stremio.com/#/player"
        ));
        assert!(is_allowed_webview_navigation(app_url, "about:blank"));
        assert!(!is_allowed_webview_navigation(
            app_url,
            "https://example.com/"
        ));
        assert!(!is_allowed_webview_navigation(
            app_url,
            "file:///C:/test.html"
        ));
        assert!(!is_allowed_webview_navigation(
            app_url,
            "javascript:alert(1)"
        ));
    }

    #[test]
    fn localhost_webview_origin_includes_port() {
        let app_url = "http://127.0.0.1:5173/";

        assert!(is_allowed_webview_navigation(
            app_url,
            "http://127.0.0.1:5173/player"
        ));
        assert!(!is_allowed_webview_navigation(
            app_url,
            "http://127.0.0.1:11470/"
        ));
    }

    #[test]
    fn cleanup_report_records_all_failures_without_short_circuiting() {
        let mut report = CleanupReport::default();

        report.record(
            "remove message handler",
            Err("message token failed".to_string()),
        );
        report.record("remove navigation handler", Ok(()));
        report.record("close controller", Err("close failed".to_string()));

        assert_eq!(
            report.failures(),
            [
                "remove message handler: message token failed",
                "close controller: close failed"
            ]
        );
    }
}
