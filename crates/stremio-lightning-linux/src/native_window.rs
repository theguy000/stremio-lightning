use crate::app::AppConfig;
use crate::render::RenderLoopPlan;
use glow::HasContext;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::prelude::*;
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub fn run_native_window(config: AppConfig) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let window_attributes = Window::default_attributes()
        .with_title(format!("Stremio Lightning Linux - {}", config.url))
        .with_inner_size(LogicalSize::new(1500.0, 850.0))
        .with_resizable(true)
        .with_visible(true);

    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_transparency(false);
    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attributes));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |configs| {
            configs
                .max_by_key(|config| {
                    let transparency = config.supports_transparency().unwrap_or(false) as i32;
                    (transparency, -(config.num_samples() as i32))
                })
                .expect("glutin did not return any GL configs")
        })
        .map_err(|e| format!("Failed to build GL display/window: {e}"))?;

    let window = Arc::new(window.ok_or_else(|| "GL display did not create a window".to_string())?);
    let raw_window_handle = window
        .window_handle()
        .map_err(|e| format!("Failed to get raw window handle: {e}"))?
        .as_raw();

    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));
    let fallback_context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(None))
        .build(Some(raw_window_handle));

    let not_current_context = unsafe {
        gl_config
            .display()
            .create_context(&gl_config, &context_attributes)
            .or_else(|_| {
                gl_config
                    .display()
                    .create_context(&gl_config, &fallback_context_attributes)
            })
            .map_err(|e| format!("Failed to create GL context: {e}"))?
    };

    let size = window.inner_size();
    let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        non_zero(size.width),
        non_zero(size.height),
    );
    let surface = unsafe {
        gl_config
            .display()
            .create_window_surface(&gl_config, &surface_attributes)
            .map_err(|e| format!("Failed to create GL window surface: {e}"))?
    };
    let context = not_current_context
        .make_current(&surface)
        .map_err(|e| format!("Failed to make GL context current: {e}"))?;

    let _ = surface.set_swap_interval(&context, SwapInterval::Wait(non_zero(1)));

    let gl = unsafe {
        glow::Context::from_loader_function(|symbol| {
            gl_config
                .display()
                .get_proc_address(CString::new(symbol).unwrap().as_c_str())
        })
    };

    let runtime = GlRuntime {
        window,
        surface,
        context,
        gl,
        render_plan: RenderLoopPlan::default(),
        started_at: Instant::now(),
        frame: 0,
    };
    let mut app = NativeWindowApp {
        runtime: Some(runtime),
    };

    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("Linux native event loop failed: {e}"))
}

fn non_zero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value.max(1)).expect("value was clamped to non-zero")
}

struct NativeWindowApp {
    runtime: Option<GlRuntime>,
}

impl ApplicationHandler for NativeWindowApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        if runtime.window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && matches!(event.logical_key, Key::Named(NamedKey::Escape)) =>
            {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => runtime.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                if let Err(error) = runtime.render() {
                    eprintln!("{error}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(runtime) = self.runtime.as_ref() {
            runtime.window.request_redraw();
        }
    }
}

struct GlRuntime {
    window: Arc<Window>,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    gl: glow::Context,
    render_plan: RenderLoopPlan,
    started_at: Instant,
    frame: u64,
}

impl GlRuntime {
    fn resize(&self, width: u32, height: u32) {
        self.surface
            .resize(&self.context, non_zero(width), non_zero(height));
    }

    fn render(&mut self) -> Result<(), String> {
        let size = self.window.inner_size();
        let t = self.started_at.elapsed().as_secs_f32();
        self.frame = self.frame.saturating_add(1);

        unsafe {
            self.gl
                .viewport(0, 0, size.width as i32, size.height as i32);

            self.gl.clear_color(0.015, 0.012, 0.02, 1.0);
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

            let mpv_luma = 0.05 + 0.02 * (t * 0.7).sin().abs();
            self.gl.clear_color(mpv_luma, 0.018, 0.026, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);

            self.gl.enable(glow::SCISSOR_TEST);
            let overlay_height = (size.height / 7).max(96);
            self.gl.scissor(
                0,
                size.height.saturating_sub(overlay_height) as i32,
                size.width as i32,
                overlay_height as i32,
            );
            self.gl.clear_color(0.07, 0.075, 0.085, 0.92);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
            self.gl.disable(glow::SCISSOR_TEST);
        }

        self.surface
            .swap_buffers(&self.context)
            .map_err(|e| format!("Failed to swap GL buffers: {e}"))?;

        if self.frame == 1 {
            println!(
                "[StremioLightning] Native Linux window rendering: {}",
                self.render_plan.steps.join(" -> ")
            );
        }

        Ok(())
    }
}
