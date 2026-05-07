use crate::host::WindowsHost;
use std::sync::Arc;

pub const WINDOWS_HOST_ADAPTER_NAME: &str = "windows-host-adapter";
pub const NATIVE_FLAGS_NAME: &str = "native-flags";
pub const BRIDGE_NAME: &str = "bridge.js";

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
        Self {
            scripts: vec![
                InjectionScript {
                    name: WINDOWS_HOST_ADAPTER_NAME,
                    source: windows_host_adapter(),
                },
                InjectionScript {
                    name: NATIVE_FLAGS_NAME,
                    source: "window.__STREMIO_LIGHTNING_ENABLE_NATIVE_PLAYER__ = true;".to_string(),
                },
                InjectionScript {
                    name: BRIDGE_NAME,
                    source: include_str!("../../../web/bridge/bridge.js").to_string(),
                },
            ],
        }
    }

    pub fn scripts(&self) -> &[InjectionScript] {
        &self.scripts
    }
}

pub struct WindowsWebView2Shell {
    url: String,
    injection: InjectionBundle,
    #[allow(dead_code)]
    host: Arc<WindowsHost>,
}

impl WindowsWebView2Shell {
    pub fn new(url: impl Into<String>) -> Result<Self, String> {
        let url = url.into();
        if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1:")) {
            return Err(format!("Unsupported WebView2 load URL: {url}"));
        }

        Ok(Self {
            url,
            injection: InjectionBundle::load(),
            host: Arc::new(WindowsHost::default()),
        })
    }

    pub fn document_start_script_names(&self) -> Vec<&'static str> {
        self.injection
            .scripts()
            .iter()
            .map(|script| script.name)
            .collect()
    }

    pub fn run(&self) -> Result<(), String> {
        platform::run_webview2_shell(&self.url, &self.injection, self.host.clone())
    }
}

#[cfg(windows)]
mod platform {
    use super::{Arc, InjectionBundle, WindowsHost};
    use crate::window::{run_native_window_with_handler, NativeWindowHandler, WindowConfig};
    use std::ptr;
    use webview2_com::{
        AddScriptToExecuteOnDocumentCreatedCompletedHandler, CoTaskMemPWSTR,
        CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
        CreateCoreWebView2EnvironmentCompletedHandler, Microsoft::Web::WebView2::Win32::*,
        NavigationCompletedEventHandler, WebMessageReceivedEventHandler,
    };
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{E_POINTER, HWND, RECT};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

    pub fn run_webview2_shell(
        url: &str,
        injection: &InjectionBundle,
        host: Arc<WindowsHost>,
    ) -> Result<(), String> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|error| format!("Failed to initialize COM for WebView2: {error}"))?;
        }

        run_native_window_with_handler(
            WindowConfig::default(),
            WebView2WindowHost::new(url.to_string(), injection.clone(), host),
        )
    }

    struct WebView2WindowHost {
        url: String,
        injection: InjectionBundle,
        host: Arc<WindowsHost>,
        controller: Option<ICoreWebView2Controller>,
        webview: Option<ICoreWebView2>,
        navigation_completed_token: Option<i64>,
    }

    impl WebView2WindowHost {
        fn new(url: String, injection: InjectionBundle, host: Arc<WindowsHost>) -> Self {
            Self {
                url,
                injection,
                host,
                controller: None,
                webview: None,
                navigation_completed_token: None,
            }
        }

        fn resize_to_client_rect(&self, hwnd: HWND) -> Result<(), String> {
            let Some(controller) = self.controller.as_ref() else {
                return Ok(());
            };

            let mut rect = RECT::default();
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect)
                    .map_err(|error| format!("Failed to read WebView2 host bounds: {error}"))?;
                controller
                    .SetBounds(rect)
                    .map_err(|error| format!("Failed to resize WebView2 controller: {error}"))?;
            }
            Ok(())
        }
    }

    impl NativeWindowHandler for WebView2WindowHost {
        fn on_created(&mut self, hwnd: HWND) -> Result<(), String> {
            let environment = create_environment()?;
            let controller = create_controller(&environment, hwnd)?;

            self.controller = Some(controller.clone());
            self.resize_to_client_rect(hwnd)?;

            unsafe {
                controller
                    .SetIsVisible(true)
                    .map_err(|error| format!("Failed to show WebView2 controller: {error}"))?;
            }

            let webview = unsafe {
                controller
                    .CoreWebView2()
                    .map_err(|error| format!("Failed to get WebView2 instance: {error}"))?
            };

            configure_webview(&webview)?;
            add_injection_scripts(&webview, &self.injection)?;
            add_message_handler(&webview, self.host.clone())?;
            self.navigation_completed_token = Some(add_navigation_completed_handler(&webview)?);
            navigate(&webview, &self.url)?;

            self.webview = Some(webview);
            Ok(())
        }

        fn on_resized(&mut self, hwnd: HWND, _client_rect: RECT) -> Result<(), String> {
            self.resize_to_client_rect(hwnd)
        }

        fn on_destroying(&mut self, _hwnd: HWND) {
            if let (Some(webview), Some(token)) = (
                self.webview.as_ref(),
                self.navigation_completed_token.take(),
            ) {
                let _ = unsafe { webview.remove_NavigationCompleted(token) };
            }
            if let Some(controller) = self.controller.take() {
                let _ = unsafe { controller.Close() };
            }
            self.webview = None;
        }
    }

    fn create_environment() -> Result<ICoreWebView2Environment, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let options = CoreWebView2EnvironmentOptions::default();
        unsafe {
            options.set_additional_browser_arguments(
                "--autoplay-policy=no-user-gesture-required --disable-features=msWebOOUI,msPdfOOUI"
                    .to_string(),
            );
        }
        let options: ICoreWebView2EnvironmentOptions = options.into();
        CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| unsafe {
                CreateCoreWebView2EnvironmentWithOptions(
                    PCWSTR::null(),
                    PCWSTR::null(),
                    &options,
                    &handler,
                )
                .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |error_code, environment| {
                error_code?;
                tx.send(environment.ok_or_else(|| windows::core::Error::from(E_POINTER)))
                    .expect("send WebView2 environment");
                Ok(())
            }),
        )
        .map_err(|error| format!("Failed to create WebView2 environment: {error:?}"))?;

        rx.recv()
            .map_err(|_| "WebView2 environment callback did not return".to_string())?
            .map_err(|error| format!("WebView2 environment creation failed: {error}"))
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
                    .expect("send WebView2 controller");
                Ok(())
            }),
        )
        .map_err(|error| format!("Failed to create WebView2 controller: {error:?}"))?;

        rx.recv()
            .map_err(|_| "WebView2 controller callback did not return".to_string())?
            .map_err(|error| format!("WebView2 controller creation failed: {error}"))
    }

    fn configure_webview(webview: &ICoreWebView2) -> Result<(), String> {
        let settings = unsafe {
            webview
                .Settings()
                .map_err(|error| format!("Failed to get WebView2 settings: {error}"))?
        };
        unsafe {
            settings.SetIsStatusBarEnabled(false).ok();
            settings.SetAreDevToolsEnabled(cfg!(debug_assertions)).ok();
            settings.SetIsZoomControlEnabled(false).ok();
            settings.SetAreHostObjectsAllowed(false).ok();
            settings.SetAreDefaultScriptDialogsEnabled(false).ok();
        }
        Ok(())
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

    fn add_message_handler(webview: &ICoreWebView2, host: Arc<WindowsHost>) -> Result<(), String> {
        let mut token = 0;
        unsafe {
            webview
                .add_WebMessageReceived(
                    &WebMessageReceivedEventHandler::create(Box::new(move |webview, args| {
                        if let (Some(webview), Some(args)) = (webview, args) {
                            let mut message = PWSTR(ptr::null_mut());
                            if args.WebMessageAsJson(&mut message).is_ok() {
                                let message = CoTaskMemPWSTR::from(message);
                                for outbound in host.dispatch_ipc_message(&message.to_string()) {
                                    match serde_json::to_string(&outbound) {
                                        Ok(serialized) => {
                                            let serialized = CoTaskMemPWSTR::from(serialized.as_str());
                                            if let Err(error) = webview.PostWebMessageAsJson(
                                                *serialized.as_ref().as_pcwstr(),
                                            ) {
                                                eprintln!(
                                                    "[StremioLightning] Failed to post WebView2 IPC response: {error}"
                                                );
                                            }
                                        }
                                        Err(error) => eprintln!(
                                            "[StremioLightning] Failed to serialize Windows IPC response: {error}"
                                        ),
                                    }
                                }
                            }
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|error| format!("Failed to attach WebView2 message handler: {error}"))?;
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
    use super::{Arc, InjectionBundle, WindowsHost};

    pub fn run_webview2_shell(
        _url: &str,
        _injection: &InjectionBundle,
        _host: Arc<WindowsHost>,
    ) -> Result<(), String> {
        Err("WebView2 shell can only run on Windows".to_string())
    }
}

fn windows_host_adapter() -> String {
    r#"(function () {
  "use strict";

  if (window.StremioLightningHost) return;

  var nextRequestId = 1;
  var nextListenerId = 1;
  var pending = {};
  var listeners = {};

  function post(kind, payload) {
    if (!window.chrome || !window.chrome.webview) {
      return Promise.reject(new Error("WebView2 host bridge is not available"));
    }
    return new Promise(function (resolve, reject) {
      var id = nextRequestId++;
      pending[id] = { resolve: resolve, reject: reject };
      window.chrome.webview.postMessage({
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
        console.error("[StremioLightning] Windows listener failed:", error);
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
        let shell = WindowsWebView2Shell::new("https://web.stremio.com/").unwrap();

        assert_eq!(
            shell.document_start_script_names(),
            vec![WINDOWS_HOST_ADAPTER_NAME, NATIVE_FLAGS_NAME, BRIDGE_NAME]
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

        assert!(bridge
            .source
            .contains("Stremio Lightning - Frontend Bridge"));
    }
}
