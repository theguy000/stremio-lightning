//! Minimal raw OpenGL loader used by `MpvSession` to clear letterbox bars and read the current
//! framebuffer binding inside the host's OpenGL context.

use std::ffi::{c_void, CStr, CString};

const GL_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const GL_SCISSOR_TEST: u32 = 0x0C11;

pub(crate) type GlLookup<'a> = &'a dyn Fn(&CStr) -> *const c_void;
type GlGetIntegerv = unsafe extern "system" fn(u32, *mut i32);
type GlClear = unsafe extern "system" fn(u32);
type GlClearColor = unsafe extern "system" fn(f32, f32, f32, f32);
type GlDisable = unsafe extern "system" fn(u32);

pub(super) struct Gl {
    clear: Option<GlClear>,
    clear_color: Option<GlClearColor>,
    disable: Option<GlDisable>,
    get_integerv: Option<GlGetIntegerv>,
}

fn loaded<F>(pointer: *const c_void) -> Option<F> {
    (!pointer.is_null()).then(|| {
        // SAFETY: a non-null result of a GL loader for a name has that function's own signature.
        unsafe { std::mem::transmute_copy::<*const c_void, F>(&pointer) }
    })
}

pub(super) fn gl_proc(lookup: &GlLookup<'_>, name: &str) -> *mut c_void {
    CString::new(name).map_or(std::ptr::null_mut(), |name| lookup(&name).cast_mut())
}

impl Gl {
    pub(super) fn new(lookup: GlLookup<'_>) -> Self {
        let load = |name: &str| gl_proc(&lookup, name);
        #[cfg(debug_assertions)]
        for name in ["glClear", "glClearColor", "glDisable", "glGetIntegerv"] {
            if load(name).is_null() {
                eprintln!("[player] the GL loader does not resolve {name}");
            }
        }
        Self {
            clear: loaded(load("glClear")),
            clear_color: loaded(load("glClearColor")),
            disable: loaded(load("glDisable")),
            get_integerv: loaded(load("glGetIntegerv")),
        }
    }

    pub(super) fn black(&self) {
        let (Some(disable), Some(clear_color), Some(clear)) =
            (self.disable, self.clear_color, self.clear)
        else {
            return;
        };
        // SAFETY: the host OpenGL context is current during frame draw.
        unsafe {
            disable(GL_SCISSOR_TEST);
            clear_color(0.0, 0.0, 0.0, 1.0);
            clear(GL_COLOR_BUFFER_BIT);
        }
    }

    pub(super) fn framebuffer(&self) -> i32 {
        let mut framebuffer = 0;
        if let Some(get_integerv) = self.get_integerv {
            // SAFETY: the host OpenGL context is current during frame draw.
            unsafe { get_integerv(GL_FRAMEBUFFER_BINDING, &mut framebuffer) };
        }
        framebuffer
    }
}
