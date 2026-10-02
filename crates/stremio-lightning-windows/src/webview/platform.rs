#![cfg_attr(windows, allow(unsafe_code))]

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(not(windows))]
pub use fallback_impl::*;

#[cfg(windows)]
mod windows_impl {
    use super::super::navigation::is_allowed_webview_navigation;
    use super::super::types::{CleanupReport, InjectionBundle, WebViewError};
    use crate::host::{Host, WindowsIpcOutbound};
    use crate::single_instance::LaunchIntent;
    use crate::window::{
        focus_window, run_native_window_with_handler, set_window_title, MediaKeyAction,
        NativeWindowHandler, UiThreadNotifier, WindowConfig, WindowVisualState,
    };
    use std::path::PathBuf;
    use std::ptr;
    use std::sync::atomic::Ordering;
    use std::sync::{mpsc, Arc, Mutex};
    #[allow(clippy::wildcard_imports)]
    use webview2_com::{
        AcceleratorKeyPressedEventHandler, AddScriptToExecuteOnDocumentCreatedCompletedHandler,
        CoTaskMemPWSTR, CoreWebView2EnvironmentOptions,
        CreateCoreWebView2ControllerCompletedHandler,
        CreateCoreWebView2EnvironmentCompletedHandler, DocumentTitleChangedEventHandler,
        Microsoft::Web::WebView2::Win32::*, NavigationCompletedEventHandler,
        NavigationStartingEventHandler, NewWindowRequestedEventHandler, ProcessFailedEventHandler,
        WebMessageReceivedEventHandler, WebResourceResponseReceivedEventHandler,
    };
    use windows::core::{Interface, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{E_POINTER, HWND, RECT};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_F5, VK_P, VK_R};

    /// # Errors
    /// Returns an error when `COM` or `WebView2` initialization fails, or the message loop exits with an error.
    pub fn run_webview2_shell(
        url: &str,
        devtools: bool,
        injection: &InjectionBundle,
        host: Arc<Host>,
        launch_intents: mpsc::Receiver<LaunchIntent>,
        ui_notifier: Arc<Mutex<Option<UiThreadNotifier>>>,
    ) -> Result<(), WebViewError> {
        // SAFETY: CoInitializeEx initializes the COM library for the current thread
        // with single-threaded apartment model, required for WebView2 and Win32 UI.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|error| WebViewError::ComInitialization(error.to_string()))?;
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
        .map_err(WebViewError::Window)
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
        accelerator_key_pressed: Option<i64>,
        message_received: Option<i64>,
        navigation_starting: Option<i64>,
        new_window_requested: Option<i64>,
        navigation_completed: Option<i64>,
        document_title_changed: Option<i64>,
        process_failed: Option<i64>,
        web_resource_response_received: Option<i64>,
    }

    #[derive(Default)]
    struct PostScratch {
        utf16: Vec<u16>,
        utf8: Vec<u8>,
    }

    struct WebView2Runtime {
        controller: Option<ICoreWebView2Controller>,
        webview: Option<ICoreWebView2>,
        scratch: PostScratch,
        event_tokens: WebView2EventTokens,
    }

    impl WebView2Runtime {
        fn create(
            hwnd: HWND,
            devtools: bool,
            injection: &InjectionBundle,
            host: Arc<Host>,
            url: &str,
        ) -> Result<Self, WebViewError> {
            let environment = create_environment()?;
            log_webview2_runtime_version(&environment);
            let controller = create_controller(&environment, hwnd)?;
            let mut runtime = Self {
                controller: Some(controller),
                webview: None,
                scratch: PostScratch::default(),
                event_tokens: WebView2EventTokens::default(),
            };

            runtime.configure_controller()?;
            runtime.configure_webview(hwnd, devtools, injection, host, url)?;
            runtime.resize_to_client_rect(hwnd)?;
            runtime.show()?;
            runtime.focus()?;

            Ok(runtime)
        }

        fn controller(&self) -> Result<&ICoreWebView2Controller, WebViewError> {
            self.controller
                .as_ref()
                .ok_or(WebViewError::ControllerUnavailable)
        }

        fn configure_controller(&mut self) -> Result<(), WebViewError> {
            let controller = self.controller()?.clone();
            configure_controller(&controller)?;
            self.event_tokens.accelerator_key_pressed =
                Some(add_accelerator_key_pressed_handler(&controller)?);
            Ok(())
        }

        fn resize_to_client_rect(&self, hwnd: HWND) -> Result<(), WebViewError> {
            let mut rect = RECT::default();
            // SAFETY: hwnd is a valid window handle and rect is a local mutable buffer.
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &raw mut rect)
                    .map_err(|error| WebViewError::HostBounds(error.to_string()))?;
            }
            self.apply_bounds(rect)
        }

        fn apply_bounds(&self, rect: RECT) -> Result<(), WebViewError> {
            // SAFETY: self.controller() returns an active valid COM interface.
            unsafe {
                self.controller()?
                    .SetBounds(rect)
                    .map_err(|error| WebViewError::ResizeController(error.to_string()))?;
            }
            Ok(())
        }

        fn show(&self) -> Result<(), WebViewError> {
            // SAFETY: self.controller() returns an active valid COM interface.
            unsafe {
                self.controller()?
                    .SetIsVisible(true)
                    .map_err(|error| WebViewError::ShowController(error.to_string()))?;
            }
            Ok(())
        }

        fn focus(&self) -> Result<(), WebViewError> {
            // SAFETY: self.controller() returns an active valid COM interface.
            unsafe {
                self.controller()?
                    .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)
                    .map_err(|error| WebViewError::FocusController(error.to_string()))?;
            }
            Ok(())
        }

        fn configure_webview(
            &mut self,
            hwnd: HWND,
            devtools: bool,
            injection: &InjectionBundle,
            host: Arc<Host>,
            url: &str,
        ) -> Result<(), WebViewError> {
            // SAFETY: self.controller() returns an active valid COM interface.
            self.webview = Some(unsafe {
                self.controller()?
                    .CoreWebView2()
                    .map_err(|error| WebViewError::GetInstance(error.to_string()))?
            });
            let webview = self
                .webview
                .as_ref()
                .ok_or(WebViewError::InstanceUnavailable)?
                .clone();

            configure_webview(&webview, devtools)?;
            add_injection_scripts(&webview, injection)?;
            self.event_tokens.message_received = Some(add_message_handler(&webview, host.clone())?);
            self.event_tokens.navigation_starting = Some(add_navigation_starting_handler(
                &webview,
                host.clone(),
                url.to_string(),
            )?);
            self.event_tokens.new_window_requested =
                Some(add_new_window_requested_handler(&webview, host)?);
            self.event_tokens.navigation_completed =
                Some(add_navigation_completed_handler(&webview)?);
            self.event_tokens.document_title_changed =
                Some(add_document_title_changed_handler(&webview, hwnd)?);
            self.event_tokens.process_failed = Some(add_process_failed_handler(&webview)?);
            self.event_tokens.web_resource_response_received =
                add_web_resource_response_received_handler(&webview);
            navigate(&webview, url)
        }

        fn post_outbound_messages(
            &mut self,
            messages: &[WindowsIpcOutbound],
        ) -> Result<(), WebViewError> {
            let Some(webview) = self.webview.as_ref() else {
                return Ok(());
            };
            post_outbound_messages(webview, messages, &mut self.scratch)
        }

        fn cleanup(&mut self) {
            let mut report = CleanupReport::default();

            if let Some(webview) = self.webview.as_ref() {
                if let Some(token) = self.event_tokens.message_received.take() {
                    // SAFETY: webview is a valid COM interface and token was returned by add.
                    report.record_windows("remove WebView2 message handler", unsafe {
                        webview.remove_WebMessageReceived(token)
                    });
                }
                if let Some(token) = self.event_tokens.navigation_starting.take() {
                    // SAFETY: webview is a valid COM interface and token was returned by add.
                    report.record_windows("remove WebView2 navigation starting handler", unsafe {
                        webview.remove_NavigationStarting(token)
                    });
                }
                if let Some(token) = self.event_tokens.new_window_requested.take() {
                    // SAFETY: webview is a valid COM interface and token was returned by add.
                    report.record_windows("remove WebView2 new window handler", unsafe {
                        webview.remove_NewWindowRequested(token)
                    });
                }
                if let Some(token) = self.event_tokens.navigation_completed.take() {
                    // SAFETY: webview is a valid COM interface and token was returned by add.
                    report.record_windows("remove WebView2 navigation completed handler", unsafe {
                        webview.remove_NavigationCompleted(token)
                    });
                }
                if let Some(token) = self.event_tokens.document_title_changed.take() {
                    // SAFETY: webview is a valid COM interface and token was returned by add.
                    report
                        .record_windows("remove WebView2 document title changed handler", unsafe {
                            webview.remove_DocumentTitleChanged(token)
                        });
                }
                if let Some(token) = self.event_tokens.process_failed.take() {
                    // SAFETY: webview is a valid COM interface and token was returned by add.
                    report.record_windows("remove WebView2 process failed handler", unsafe {
                        webview.remove_ProcessFailed(token)
                    });
                }
                if let Some(token) = self.event_tokens.web_resource_response_received.take() {
                    match webview.cast::<ICoreWebView2_2>() {
                        Ok(webview2) => report.record_windows(
                            "remove WebView2 resource response handler",
                            // SAFETY: webview2 is valid and token was returned by add.
                            unsafe { webview2.remove_WebResourceResponseReceived(token) },
                        ),
                        Err(error) => report.record(
                            "remove WebView2 resource response handler",
                            Err(error.to_string()),
                        ),
                    }
                }
            }

            if let (Some(controller), Some(token)) = (
                self.controller.as_ref(),
                self.event_tokens.accelerator_key_pressed.take(),
            ) {
                // SAFETY: controller is a valid COM interface and token was returned by add.
                report.record_windows("remove WebView2 accelerator key handler", unsafe {
                    controller.remove_AcceleratorKeyPressed(token)
                });
            }

            if let Some(controller) = self.controller.take() {
                // SAFETY: controller is a valid COM interface being closed on teardown.
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

        fn post_host_events(&mut self) -> Result<(), String> {
            let Some(runtime) = self.runtime.as_mut() else {
                return Ok(());
            };
            let mut pending = self.host.drain_pending_responses();
            self.host.drain_ipc_events_into(&mut pending);
            runtime
                .post_outbound_messages(&pending)
                .map_err(|error| error.to_string())
        }

        fn start_host_runtime(&self, hwnd: HWND, notifier: UiThreadNotifier) -> Result<(), String> {
            *self.ui_notifier.lock().map_err(|e| e.to_string())? = Some(notifier.clone());
            self.host.bind_ui_notifier(notifier.clone())?;
            self.host.bind_native_window(hwnd)?;
            self.host.initialize_native_player(hwnd, notifier)?;
            self.host.start_streaming_server()
        }
    }

    impl NativeWindowHandler for WebView2WindowHost {
        fn on_created(&mut self, hwnd: HWND) -> Result<(), String> {
            let notifier = UiThreadNotifier::new(hwnd);
            self.start_host_runtime(hwnd, notifier)?;
            self.runtime = Some(
                WebView2Runtime::create(
                    hwnd,
                    self.devtools,
                    &self.injection,
                    self.host.clone(),
                    &self.url,
                )
                .map_err(|error| error.to_string())?,
            );
            Ok(())
        }

        fn on_resized(&mut self, _hwnd: HWND, client_rect: RECT) -> Result<(), String> {
            let Some(runtime) = self.runtime.as_ref() else {
                return Ok(());
            };
            runtime
                .apply_bounds(client_rect)
                .map_err(|error| error.to_string())
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
            self.post_host_events()?;

            if focused {
                if let Some(runtime) = self.runtime.as_ref() {
                    runtime.focus().map_err(|error| error.to_string())?;
                }
            }
            Ok(())
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
            if let Ok(notifier) = self.ui_notifier.lock() {
                if let Some(notifier) = notifier.as_ref() {
                    notifier.clear_pending();
                }
            }
            while let Ok(intent) = self.launch_intents.try_recv() {
                focus_window(hwnd);
                self.host.emit_launch_intent(&intent)?;
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

    fn create_environment() -> Result<ICoreWebView2Environment, WebViewError> {
        let (tx, rx) = std::sync::mpsc::channel();
        let user_data_dir = webview2_user_data_dir()?;
        std::fs::create_dir_all(&user_data_dir).map_err(|error| {
            WebViewError::UserDataDirectory(user_data_dir.display().to_string(), error.to_string())
        })?;
        let user_data_str = user_data_dir
            .to_str()
            .ok_or(WebViewError::InvalidUserDataPath)?;
        let user_data_hstring = windows::core::HSTRING::from(user_data_str);
        let options = CoreWebView2EnvironmentOptions::default();
        // SAFETY: options is a valid COM wrapper object for CoreWebView2EnvironmentOptions.
        unsafe {
            options.set_additional_browser_arguments(
                "--autoplay-policy=no-user-gesture-required \
                 --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection"
                    .to_string(),
            );
        }
        let options: ICoreWebView2EnvironmentOptions = options.into();
        CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                // SAFETY: user_data_hstring is null-terminated and options/handler are valid.
                unsafe {
                    CreateCoreWebView2EnvironmentWithOptions(
                        PCWSTR::null(),
                        PCWSTR(user_data_hstring.as_ptr()),
                        &options,
                        &handler,
                    )
                    .map_err(webview2_com::Error::WindowsError)
                }
            }),
            Box::new(move |error_code, environment| {
                error_code?;
                tx.send(environment.ok_or_else(|| windows::core::Error::from(E_POINTER)))
                    .map_err(|_| windows::core::Error::from(E_POINTER))?;
                Ok(())
            }),
        )
        .map_err(|error| WebViewError::EnvironmentCreation(format!("{error:?}")))?;

        rx.recv()
            .map_err(|_| {
                WebViewError::EnvironmentCreation(
                    "WebView2 environment callback did not return".to_string(),
                )
            })?
            .map_err(|error| WebViewError::EnvironmentCreation(error.to_string()))
    }

    fn webview2_user_data_dir() -> Result<PathBuf, WebViewError> {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("stremio-lightning").join("WebView2"))
            .ok_or(WebViewError::LocalAppDataUnavailable)
    }

    fn create_controller(
        environment: &ICoreWebView2Environment,
        hwnd: HWND,
    ) -> Result<ICoreWebView2Controller, WebViewError> {
        let (tx, rx) = std::sync::mpsc::channel();
        let environment = environment.clone();
        CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                // SAFETY: environment is valid COM interface, hwnd is a valid window handle.
                unsafe {
                    environment
                        .CreateCoreWebView2Controller(hwnd, &handler)
                        .map_err(webview2_com::Error::WindowsError)
                }
            }),
            Box::new(move |error_code, controller| {
                error_code?;
                tx.send(controller.ok_or_else(|| windows::core::Error::from(E_POINTER)))
                    .map_err(|_| windows::core::Error::from(E_POINTER))?;
                Ok(())
            }),
        )
        .map_err(|error| WebViewError::ControllerCreation(format!("{error:?}")))?;

        rx.recv()
            .map_err(|_| {
                WebViewError::ControllerCreation(
                    "WebView2 controller callback did not return".to_string(),
                )
            })?
            .map_err(|error| WebViewError::ControllerCreation(error.to_string()))
    }

    fn log_webview2_runtime_version(environment: &ICoreWebView2Environment) {
        let mut version = PWSTR(ptr::null_mut());
        // SAFETY: environment is a valid COM interface; version receives an allocated PWSTR.
        let Ok(()) = (unsafe { environment.BrowserVersionString(&raw mut version) }) else {
            stremio_lightning_core::logging::warn(
                "native.webview.windows",
                "WebView2 runtime version is unavailable",
            );
            return;
        };

        let version = CoTaskMemPWSTR::from(version).to_string();
        if version.is_empty() {
            stremio_lightning_core::logging::warn(
                "native.webview.windows",
                "WebView2 runtime version is unavailable",
            );
        } else {
            stremio_lightning_core::logging::update_webview_metadata("WebView2", Some(&version));
            stremio_lightning_core::logging::info(
                "native.webview.windows",
                format!("WebView2 runtime version: {version}"),
            );
        }
    }

    fn configure_controller(controller: &ICoreWebView2Controller) -> Result<(), WebViewError> {
        let controller2 = controller
            .cast::<ICoreWebView2Controller2>()
            .map_err(|error| WebViewError::GetController2(error.to_string()))?;
        // SAFETY: controller2 is a valid COM interface; setting background color to transparent.
        unsafe {
            controller2
                .SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                    A: 0,
                    R: 255,
                    G: 255,
                    B: 255,
                })
                .map_err(|error| WebViewError::SetBackgroundColor(error.to_string()))?;
        }
        Ok(())
    }

    fn add_accelerator_key_pressed_handler(
        controller: &ICoreWebView2Controller,
    ) -> Result<i64, WebViewError> {
        let mut token = 0;
        // SAFETY: controller is a valid COM interface; callback is boxed and retained.
        unsafe {
            controller
                .add_AcceleratorKeyPressed(
                    &AcceleratorKeyPressedEventHandler::create(Box::new(
                        move |_controller, args| {
                            let Some(args) = args else {
                                return Ok(());
                            };

                            let mut virtual_key = 0u32;
                            args.VirtualKey(&raw mut virtual_key)?;
                            let control_down = GetKeyState(i32::from(VK_CONTROL.0)) < 0;
                            if should_block_browser_accelerator(virtual_key, control_down) {
                                args.SetHandled(true)?;
                            }
                            Ok(())
                        },
                    )),
                    &raw mut token,
                )
                .map_err(|error| WebViewError::AttachAcceleratorHandler(error.to_string()))?;
        }
        Ok(token)
    }

    pub(crate) fn should_block_browser_accelerator(virtual_key: u32, control_down: bool) -> bool {
        if virtual_key == u32::from(VK_F5.0) {
            return true;
        }
        control_down && (virtual_key == u32::from(VK_R.0) || virtual_key == u32::from(VK_P.0))
    }

    fn configure_webview(webview: &ICoreWebView2, devtools: bool) -> Result<(), WebViewError> {
        // SAFETY: webview is a valid COM interface.
        let settings = unsafe {
            webview
                .Settings()
                .map_err(|error| WebViewError::GetSettings(error.to_string()))?
        };
        // SAFETY: settings is a valid COM interface; configuring built-in browser UI controls.
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
    ) -> Result<(), WebViewError> {
        let source = injection.combined_source();
        let webview = webview.clone();
        AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                let source = CoTaskMemPWSTR::from(source.as_str());
                // SAFETY: webview is valid COM interface; source is valid CoTaskMemPWSTR.
                unsafe {
                    webview
                        .AddScriptToExecuteOnDocumentCreated(*source.as_ref().as_pcwstr(), &handler)
                        .map_err(webview2_com::Error::WindowsError)
                }
            }),
            Box::new(|error_code, _id| error_code),
        )
        .map_err(|error| {
            WebViewError::ScriptInjection("injection-bundle".to_string(), format!("{error:?}"))
        })?;
        Ok(())
    }

    fn add_message_handler(webview: &ICoreWebView2, host: Arc<Host>) -> Result<i64, WebViewError> {
        let mut token = 0;
        let mut scratch = PostScratch::default();
        // Reused for every inbound message: the page sends one for the app's
        // lifetime, so the reply list never needs to be reallocated.
        let mut outbound = Vec::new();
        // SAFETY: webview is a valid COM interface; handler closure is boxed and retained.
        unsafe {
            webview
                .add_WebMessageReceived(
                    &WebMessageReceivedEventHandler::create(Box::new(move |webview, args| {
                        if let (Some(webview), Some(args)) = (webview, args) {
                            let mut message = PWSTR(ptr::null_mut());
                            if args.WebMessageAsJson(&raw mut message).is_ok() {
                                let message = CoTaskMemPWSTR::from(message);
                                let message = message.to_string();
                                if is_toggle_devtools_message(&message) {
                                    if let Err(error) = webview.OpenDevToolsWindow() {
                                        stremio_lightning_core::logging::error(
                                            "native.webview.windows",
                                            format!("Failed to open DevTools: {error}"),
                                        );
                                    }
                                }
                                outbound.clear();
                                host.dispatch_ipc_message_async_into(&message, &mut outbound);
                                if let Err(error) =
                                    post_outbound_messages(&webview, &outbound, &mut scratch)
                                {
                                    stremio_lightning_core::logging::error(
                                        "native.webview.windows",
                                        format!("Failed to post IPC response: {error}"),
                                    );
                                }
                            }
                        }
                        Ok(())
                    })),
                    &raw mut token,
                )
                .map_err(|error| WebViewError::AttachMessageHandler(error.to_string()))?;
        }
        Ok(token)
    }

    fn is_toggle_devtools_message(message: &str) -> bool {
        message.contains("toggle_devtools")
            && serde_json::from_str::<serde_json::Value>(message).is_ok_and(|value| {
                value["kind"] == "invoke" && value["payload"]["command"] == "toggle_devtools"
            })
    }

    fn add_navigation_starting_handler(
        webview: &ICoreWebView2,
        host: Arc<Host>,
        app_url: String,
    ) -> Result<i64, WebViewError> {
        let mut token = 0;
        let mut scratch = PostScratch::default();
        // SAFETY: webview is a valid COM interface; handler closure is boxed and retained.
        unsafe {
            webview
                .add_NavigationStarting(
                    &NavigationStartingEventHandler::create(Box::new(move |webview, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };

                        let mut uri = PWSTR(ptr::null_mut());
                        args.Uri(&raw mut uri)?;
                        let uri = CoTaskMemPWSTR::from(uri);
                        let uri = uri.to_string();
                        if !is_allowed_webview_navigation(&app_url, &uri) {
                            args.SetCancel(true)?;
                            let result = webview.map_or(Ok(()), |webview| {
                                handle_external_navigation(&webview, &host, uri, &mut scratch)
                            });
                            if let Err(error) = result {
                                stremio_lightning_core::logging::error(
                                    "native.webview.windows",
                                    format!("Failed to handle navigation URL: {error}"),
                                );
                            }
                        }
                        Ok(())
                    })),
                    &raw mut token,
                )
                .map_err(|error| WebViewError::AttachNavigationHandler(error.to_string()))?;
        }
        Ok(token)
    }

    fn add_new_window_requested_handler(
        webview: &ICoreWebView2,
        host: Arc<Host>,
    ) -> Result<i64, WebViewError> {
        let mut token = 0;
        let mut scratch = PostScratch::default();
        // SAFETY: webview is a valid COM interface; handler closure is boxed and retained.
        unsafe {
            webview
                .add_NewWindowRequested(
                    &NewWindowRequestedEventHandler::create(Box::new(move |webview, args| {
                        let (Some(webview), Some(args)) = (webview, args) else {
                            return Ok(());
                        };
                        let mut uri = PWSTR(ptr::null_mut());
                        args.Uri(&raw mut uri)?;
                        args.SetHandled(true)?;
                        let uri = CoTaskMemPWSTR::from(uri).to_string();
                        if let Err(error) =
                            handle_external_navigation(&webview, &host, uri, &mut scratch)
                        {
                            stremio_lightning_core::logging::error(
                                "native.webview.windows",
                                format!("Failed to handle new window URL: {error}"),
                            );
                        }
                        Ok(())
                    })),
                    &raw mut token,
                )
                .map_err(|error| WebViewError::AttachNewWindowHandler(error.to_string()))?;
        }
        Ok(token)
    }

    fn handle_external_navigation(
        webview: &ICoreWebView2,
        host: &Host,
        uri: String,
        scratch: &mut PostScratch,
    ) -> Result<(), String> {
        if uri
            .get(.."stremio://".len())
            .is_some_and(|value| value.eq_ignore_ascii_case("stremio://"))
        {
            host.emit_launch_intent(&LaunchIntent::StremioDeepLink(uri))?;
            let outbound = host.drain_ipc_events();
            post_outbound_messages(webview, &outbound, scratch)
                .map_err(|error| error.to_string())
        } else {
            host.invoke("open_external_url", Some(serde_json::json!({ "url": uri })))?;
            Ok(())
        }
    }

fn post_outbound_messages(
    webview: &ICoreWebView2,
    messages: &[WindowsIpcOutbound],
    scratch: &mut PostScratch,
) -> Result<(), WebViewError> {
    for outbound in messages {
        scratch.utf8.clear();
        serde_json::to_writer(&mut scratch.utf8, outbound)
            .map_err(|error| WebViewError::SerializeIpcResponse(error.to_string()))?;
            // SAFETY: serde_json writes valid UTF-8 into the scratch buffer.
            let serialized = unsafe { std::str::from_utf8_unchecked(&scratch.utf8) };
            fill_utf16_scratch(&mut scratch.utf16, serialized);
            // SAFETY: webview is a valid COM interface; scratch.utf16 is a null-terminated UTF-16 string.
            unsafe {
                webview
                    .PostWebMessageAsJson(PCWSTR(scratch.utf16.as_ptr()))
                    .map_err(|error| WebViewError::PostIpcResponse(error.to_string()))?;
            }
        }
        Ok(())
    }

    pub(crate) fn fill_utf16_scratch(scratch: &mut Vec<u16>, text: &str) {
        scratch.clear();
        scratch.extend(text.encode_utf16());
        scratch.push(0);
    }

    fn add_navigation_completed_handler(webview: &ICoreWebView2) -> Result<i64, WebViewError> {
        let mut token = 0;
        // SAFETY: webview is a valid COM interface; handler closure is boxed and retained.
        unsafe {
            webview
                .add_NavigationCompleted(
                    &NavigationCompletedEventHandler::create(Box::new(move |webview, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };
                        let mut success = windows::core::BOOL::default();
                        args.IsSuccess(&raw mut success)?;
                        if !success.as_bool() {
                            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN;
                            args.WebErrorStatus(&raw mut status)?;
                            if status.0 != 14 {
                                stremio_lightning_core::logging::error(
                                    "native.webview.windows",
                                    format!(
                                        "WebView2 navigation failed: status={}",
                                        web_error_status_name(status.0)
                                    ),
                                );
                            }
                        }

                        if let Some(webview) = webview {
                            let message = CoTaskMemPWSTR::from(
                                serde_json::json!({
                                    "kind": "native-ready",
                                    "payload": { "shell": "webview2" }
                                })
                                .to_string()
                                .as_str(),
                            );
                            webview.PostWebMessageAsJson(*message.as_ref().as_pcwstr())?;
                        }
                        Ok(())
                    })),
                    &raw mut token,
                )
                .map_err(|error| {
                    WebViewError::AttachNavigationCompletedHandler(error.to_string())
                })?;
        }
        Ok(token)
    }

    fn add_document_title_changed_handler(
        webview: &ICoreWebView2,
        hwnd: HWND,
    ) -> Result<i64, WebViewError> {
        let mut token = 0;
        // SAFETY: webview is a valid COM interface; handler closure is boxed and retained.
        unsafe {
            webview
                .add_DocumentTitleChanged(
                    &DocumentTitleChangedEventHandler::create(Box::new(move |_webview, _args| {
                        set_window_title(hwnd);
                        Ok(())
                    })),
                    &raw mut token,
                )
                .map_err(|error| {
                    WebViewError::AttachDocumentTitleChangedHandler(error.to_string())
                })?;
        }
        Ok(token)
    }

    fn add_process_failed_handler(webview: &ICoreWebView2) -> Result<i64, WebViewError> {
        let mut token = 0;
        // SAFETY: webview is a valid COM interface; handler closure is boxed and retained.
        unsafe {
            webview
                .add_ProcessFailed(
                    &ProcessFailedEventHandler::create(Box::new(move |_webview, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };
                        let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND_UNKNOWN_PROCESS_EXITED;
                        args.ProcessFailedKind(&raw mut kind)?;
                        stremio_lightning_core::logging::error(
                            "native.webview.windows",
                            format!(
                                "WebView2 process failed: kind={}",
                                process_failed_kind_name(kind.0)
                            ),
                        );
                        Ok(())
                    })),
                    &raw mut token,
                )
                .map_err(|error| WebViewError::AttachProcessFailedHandler(error.to_string()))?;
        }
        Ok(token)
    }

    fn add_web_resource_response_received_handler(webview: &ICoreWebView2) -> Option<i64> {
        let webview2 = match webview.cast::<ICoreWebView2_2>() {
            Ok(webview2) => webview2,
            Err(error) => {
                super::super::NATIVE_HTTP_CAPTURE_AVAILABLE.store(false, Ordering::Relaxed);
                stremio_lightning_core::logging::warn(
                    "native.webview.windows",
                    format!("WebView2 response diagnostics unavailable: {error}"),
                );
                return None;
            }
        };
        let mut token = 0;
        // SAFETY: webview2 is a valid COM interface; callback closure is boxed and retained.
        let registration = unsafe {
            webview2.add_WebResourceResponseReceived(
                &WebResourceResponseReceivedEventHandler::create(Box::new(
                    move |_webview, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };
                        let Some(status) = web_resource_response_status(&args) else {
                            return Ok(());
                        };
                        if status < 400 {
                            return Ok(());
                        }

                        let (method, resource) = web_resource_request_descriptor(&args);
                        stremio_lightning_core::logging::error(
                            "native.webview.windows",
                            format!(
                                "WebView2 HTTP resource failed: status={status} \
                                 method={method} resource={resource}"
                            ),
                        );
                        Ok(())
                    },
                )),
                &raw mut token,
            )
        };
        if let Err(error) = registration {
            super::super::NATIVE_HTTP_CAPTURE_AVAILABLE.store(false, Ordering::Relaxed);
            stremio_lightning_core::logging::warn(
                "native.webview.windows",
                format!("WebView2 response diagnostics could not start: {error}"),
            );
            return None;
        }
        super::super::NATIVE_HTTP_CAPTURE_AVAILABLE.store(true, Ordering::Relaxed);
        Some(token)
    }

    fn web_resource_response_status(
        args: &ICoreWebView2WebResourceResponseReceivedEventArgs,
    ) -> Option<i32> {
        // SAFETY: args is a valid COM interface; querying response and status code.
        unsafe {
            let response = args.Response().ok()?;
            let mut status = 0;
            response.StatusCode(&raw mut status).ok()?;
            Some(status)
        }
    }

    fn web_resource_request_descriptor(
        args: &ICoreWebView2WebResourceResponseReceivedEventArgs,
    ) -> (&'static str, &'static str) {
        // SAFETY: args is a valid COM interface; querying request method and URI.
        unsafe {
            let Ok(request) = args.Request() else {
                return ("unknown", "unknown resource");
            };

            let mut method = PWSTR(ptr::null_mut());
            let method = request
                .Method(&raw mut method)
                .ok()
                .map(|()| CoTaskMemPWSTR::from(method).to_string())
                .unwrap_or_default();
            let mut uri = PWSTR(ptr::null_mut());
            let resource = request
                .Uri(&raw mut uri)
                .ok()
                .map(|()| CoTaskMemPWSTR::from(uri).to_string())
                .unwrap_or_default();
            (
                safe_http_method_name(&method),
                safe_webview_resource_descriptor(&resource),
            )
        }
    }

    fn safe_http_method_name(method: &str) -> &'static str {
        match method {
            "GET" => "GET",
            "POST" => "POST",
            "PUT" => "PUT",
            "PATCH" => "PATCH",
            "DELETE" => "DELETE",
            "HEAD" => "HEAD",
            "OPTIONS" => "OPTIONS",
            _ => "unknown",
        }
    }

    pub(crate) fn safe_webview_resource_descriptor(uri: &str) -> &'static str {
        let scheme = uri
            .trim()
            .split_once(':')
            .map(|(scheme, _)| scheme)
            .unwrap_or_default();
        if scheme.eq_ignore_ascii_case("http") {
            "http resource"
        } else if scheme.eq_ignore_ascii_case("https") {
            "https resource"
        } else if scheme.eq_ignore_ascii_case("data") {
            "data resource"
        } else if scheme.eq_ignore_ascii_case("blob") {
            "blob resource"
        } else if scheme.eq_ignore_ascii_case("about") {
            "about resource"
        } else if scheme.eq_ignore_ascii_case("file") {
            "file resource"
        } else {
            "other resource"
        }
    }

    pub(crate) fn web_error_status_name(status: i32) -> &'static str {
        match status {
            1 => "certificate-common-name-incorrect",
            2 => "certificate-expired",
            3 => "client-certificate-errors",
            4 => "certificate-revoked",
            5 => "certificate-invalid",
            6 => "server-unreachable",
            7 => "timeout",
            8 => "http-invalid-server-response",
            9 => "connection-aborted",
            10 => "connection-reset",
            11 => "disconnected",
            12 => "cannot-connect",
            13 => "host-not-resolved",
            14 => "operation-canceled",
            15 => "redirect-failed",
            16 => "unexpected-error",
            17 => "authentication-required",
            18 => "proxy-authentication-required",
            _ => "unknown",
        }
    }

    pub(crate) fn process_failed_kind_name(kind: i32) -> &'static str {
        match kind {
            0 => "browser-process-exited",
            1 => "render-process-exited",
            2 => "render-process-unresponsive",
            3 => "frame-render-process-exited",
            4 => "utility-process-exited",
            5 => "sandbox-helper-process-exited",
            6 => "gpu-process-exited",
            7 => "ppapi-plugin-process-exited",
            8 => "ppapi-broker-process-exited",
            _ => "unknown",
        }
    }

    fn navigate(webview: &ICoreWebView2, url: &str) -> Result<(), WebViewError> {
        let url = CoTaskMemPWSTR::from(url);
        // SAFETY: webview is a valid COM interface; url is a valid null-terminated CoTaskMemPWSTR.
        unsafe {
            webview
                .Navigate(*url.as_ref().as_pcwstr())
                .map_err(|error| WebViewError::Navigate(error.to_string()))
        }
    }
}

#[cfg(not(windows))]
mod fallback_impl {
    use super::super::types::{InjectionBundle, WebViewError};
    use crate::host::Host;
    use crate::single_instance::LaunchIntent;
    use std::sync::{mpsc, Arc};

    pub fn run_webview2_shell(
        _url: &str,
        _devtools: bool,
        _injection: &InjectionBundle,
        _host: Arc<Host>,
        _launch_intents: mpsc::Receiver<LaunchIntent>,
    ) -> Result<(), WebViewError> {
        Err(WebViewError::WindowsOnly)
    }
}
