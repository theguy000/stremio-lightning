#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowConfig {
    pub title: &'static str,
    pub width: i32,
    pub height: i32,
    pub min_width: i32,
    pub min_height: i32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: crate::APP_NAME,
            width: 1280,
            height: 720,
            min_width: 640,
            min_height: 480,
        }
    }
}
