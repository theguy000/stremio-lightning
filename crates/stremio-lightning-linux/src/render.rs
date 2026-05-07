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
}
