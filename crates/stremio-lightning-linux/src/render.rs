#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderLoopPlan {
    pub steps: Vec<&'static str>,
}

impl Default for RenderLoopPlan {
    fn default() -> Self {
        Self {
            steps: vec![
                "clear-frame",
                "render-mpv-gtk-glarea",
                "compose-transparent-webkitgtk6-overlay",
                "gtk-present-frame",
            ],
        }
    }
}

impl RenderLoopPlan {
    /// Render plan for the Servo engine path using unified wgpu compositing.
    ///
    /// Pipeline: `[clear] → [MPV video texture] → [Servo WebRender overlay] → [present]`
    ///
    /// Unlike the WebKit path which uses GTK's built-in overlay compositing,
    /// the Servo path composites both layers through a single wgpu device context,
    /// enabling true alpha-blended transparent web UI on top of the MPV video plane.
    pub fn servo() -> Self {
        Self {
            steps: vec![
                "clear-wgpu-frame",
                "render-mpv-texture-layer",
                "render-servo-webrender-overlay",
                "wgpu-present-frame",
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowInput {
    MouseMove { x: i32, y: i32 },
    MouseButton { button: u8, pressed: bool },
    Key { key: String, pressed: bool },
    Resized { width: u32, height: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_order_composites_webview_above_mpv() {
        let plan = RenderLoopPlan::default();
        assert_eq!(plan.steps[0], "clear-frame");
        assert!(
            plan.steps
                .iter()
                .position(|step| *step == "render-mpv-gtk-glarea")
                < plan
                    .steps
                    .iter()
                    .position(|step| *step == "compose-transparent-webkitgtk6-overlay")
        );
    }

    #[test]
    fn servo_render_order_composites_webrender_above_mpv() {
        let plan = RenderLoopPlan::servo();
        assert_eq!(plan.steps[0], "clear-wgpu-frame");
        let mpv_pos = plan
            .steps
            .iter()
            .position(|step| *step == "render-mpv-texture-layer")
            .expect("MPV texture layer must be present");
        let servo_pos = plan
            .steps
            .iter()
            .position(|step| *step == "render-servo-webrender-overlay")
            .expect("Servo WebRender overlay must be present");
        assert!(
            mpv_pos < servo_pos,
            "MPV texture must render before Servo overlay"
        );
        assert_eq!(
            plan.steps.last(),
            Some(&"wgpu-present-frame"),
            "Final step must present the composited frame"
        );
    }
}
