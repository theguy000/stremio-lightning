use gdk_pixbuf::Pixbuf;
use gtk::prelude::*;
use libc::{c_char, c_int, c_long, c_uchar, c_ulong};
use std::os::raw::c_void;

const X11_CLIENT_MESSAGE: c_int = 33;
const X11_PROP_MODE_REPLACE: c_int = 0;
const X11_PROP_MODE_REMOVE: c_long = 0;
const X11_PROP_MODE_ADD: c_long = 1;
const X11_SUBSTRUCTURE_NOTIFY_MASK: c_long = 1 << 19;
const X11_SUBSTRUCTURE_REDIRECT_MASK: c_long = 1 << 20;

#[repr(C)]
union XClientMessageData {
    _b: [c_char; 20],
    _s: [i16; 10],
    l: [c_long; 5],
}

#[repr(C)]
struct XClientMessageEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut c_void,
    window: c_ulong,
    message_type: c_ulong,
    format: c_int,
    data: XClientMessageData,
}

struct X11Surface {
    display: *mut c_void,
    window: c_ulong,
}

unsafe extern "C" {
    fn gdk_x11_display_get_xdisplay(display: *mut c_void) -> *mut c_void;
    fn gdk_x11_surface_get_xid(surface: *mut c_void) -> c_ulong;
}

#[link(name = "X11")]
unsafe extern "C" {
    fn XDefaultRootWindow(display: *mut c_void) -> c_ulong;
    fn XFlush(display: *mut c_void) -> c_int;
    fn XInternAtom(
        display: *mut c_void,
        atom_name: *const c_char,
        only_if_exists: c_int,
    ) -> c_ulong;
    fn XChangeProperty(
        display: *mut c_void,
        window: c_ulong,
        property: c_ulong,
        type_: c_ulong,
        format: c_int,
        mode: c_int,
        data: *const c_uchar,
        nelements: c_int,
    ) -> c_int;
    fn XSendEvent(
        display: *mut c_void,
        window: c_ulong,
        propagate: c_int,
        event_mask: c_long,
        event_send: *mut XClientMessageEvent,
    ) -> c_int;
}

pub(super) fn install_source_tree_window_icon(window: &gtk::ApplicationWindow) {
    let window = window.clone();
    window.connect_realize(move |window| {
        let Some(surface) = window.surface() else {
            return;
        };
        if !is_x11_surface(&surface) {
            return;
        }
        if let Err(error) = set_window_icon(&surface) {
            stremio_lightning_core::logging::error(
                "native.window",
                format!("[StremioLightning] Failed to set Linux taskbar icon: {error}"),
            );
        }
    });
}

pub(super) fn request_window_above(
    window: &gtk::ApplicationWindow,
    above: bool,
) -> Result<(), String> {
    let surface = window
        .surface()
        .ok_or_else(|| "Cannot update PiP window stacking before it has a surface".to_string())?;

    if !is_x11_surface(&surface) {
        if above {
            stremio_lightning_core::logging::warn(
                "native.pip",
                "[StremioLightning] PiP always-on-top is only available on Linux X11 sessions",
            );
        }
        return Ok(());
    }

    send_window_state_above(&surface, above)
}

fn is_x11_surface(surface: &gtk::gdk::Surface) -> bool {
    surface.type_().name().contains("X11")
}

fn set_window_icon(surface: &gtk::gdk::Surface) -> Result<(), String> {
    const NET_WM_ICON: &[u8] = b"_NET_WM_ICON\0";
    const CARDINAL: &[u8] = b"CARDINAL\0";

    let icon_data = embedded_window_icon_data()?;
    let surface = X11Surface::from_surface(surface, "taskbar icon")?;
    let icon_atom = intern_atom(surface.display, NET_WM_ICON, "taskbar icon property")?;
    let cardinal_atom = intern_atom(surface.display, CARDINAL, "taskbar icon cardinal type")?;

    // SAFETY: XChangeProperty writes a 32-bit ARGB icon payload to a valid X11 window. The
    // display/window handles and atoms were resolved from GDK/Xlib above, and icon_data is alive
    // for the duration of this synchronous call.
    unsafe {
        XChangeProperty(
            surface.display,
            surface.window,
            icon_atom,
            cardinal_atom,
            32,
            X11_PROP_MODE_REPLACE,
            icon_data.as_ptr().cast::<c_uchar>(),
            icon_data.len() as c_int,
        );
        XFlush(surface.display);
    }

    Ok(())
}

fn embedded_window_icon_data() -> Result<Vec<c_ulong>, String> {
    const ICON_BYTES: &[u8] = include_bytes!("../../../../assets/icons/128x128.png");

    let bytes = gtk::glib::Bytes::from(ICON_BYTES);
    let stream = gtk::gio::MemoryInputStream::from_bytes(&bytes);
    let pixbuf = Pixbuf::from_stream(&stream, None::<&gtk::gio::Cancellable>)
        .map_err(|error| format!("failed to load icon from bytes: {error}"))?;

    icon_data_from_pixbuf(&pixbuf)
}

fn icon_data_from_pixbuf(pixbuf: &Pixbuf) -> Result<Vec<c_ulong>, String> {
    let width = pixbuf.width();
    let height = pixbuf.height();
    let channels = pixbuf.n_channels();
    if width <= 0 || height <= 0 || channels < 3 {
        return Err("invalid icon image".to_string());
    }

    let pixels = pixbuf.read_pixel_bytes();
    let pixels = pixels.as_ref();
    let rowstride = pixbuf.rowstride() as usize;
    let width_usize = width as usize;
    let height_usize = height as usize;
    let channels_usize = channels as usize;

    let expected_min_len = (height_usize - 1) * rowstride + width_usize * channels_usize;
    if pixels.len() < expected_min_len {
        return Err("pixel buffer size is insufficient for icon dimensions".to_string());
    }

    let mut icon_data = Vec::with_capacity(2 + width_usize * height_usize);
    icon_data.push(width as c_ulong);
    icon_data.push(height as c_ulong);
    for y in 0..height_usize {
        let row_start = y * rowstride;
        for x in 0..width_usize {
            let offset = row_start + x * channels_usize;
            let r = pixels[offset] as c_ulong;
            let g = pixels[offset + 1] as c_ulong;
            let b = pixels[offset + 2] as c_ulong;
            let a = if channels_usize >= 4 {
                pixels[offset + 3] as c_ulong
            } else {
                0xff
            };
            icon_data.push((a << 24) | (r << 16) | (g << 8) | b);
        }
    }

    Ok(icon_data)
}

fn send_window_state_above(surface: &gtk::gdk::Surface, above: bool) -> Result<(), String> {
    const NET_WM_STATE: &[u8] = b"_NET_WM_STATE\0";
    const NET_WM_STATE_ABOVE: &[u8] = b"_NET_WM_STATE_ABOVE\0";

    let surface = X11Surface::from_surface(surface, "PiP window")?;
    let state_atom = intern_atom(surface.display, NET_WM_STATE, "PiP window state")?;
    let above_atom = intern_atom(surface.display, NET_WM_STATE_ABOVE, "PiP always-on-top")?;

    let action = if above {
        X11_PROP_MODE_ADD
    } else {
        X11_PROP_MODE_REMOVE
    };
    let mut event = XClientMessageEvent {
        type_: X11_CLIENT_MESSAGE,
        serial: 0,
        send_event: 1,
        display: surface.display,
        window: surface.window,
        message_type: state_atom,
        format: 32,
        data: XClientMessageData {
            l: [action, above_atom as c_long, 0, 1, 0],
        },
    };

    // SAFETY: The event targets a valid root window for the resolved display and carries the EWMH
    // _NET_WM_STATE client message expected by X11 window managers.
    unsafe {
        let root = XDefaultRootWindow(surface.display);
        let sent = XSendEvent(
            surface.display,
            root,
            0,
            X11_SUBSTRUCTURE_REDIRECT_MASK | X11_SUBSTRUCTURE_NOTIFY_MASK,
            &mut event,
        );
        if sent == 0 {
            return Err("Failed to send X11 PiP always-on-top request".to_string());
        }

        XFlush(surface.display);
    }

    Ok(())
}

fn intern_atom(display: *mut c_void, name: &[u8], context: &str) -> Result<c_ulong, String> {
    // SAFETY: XInternAtom expects a valid X11 display and a null-terminated atom name. All callers
    // pass static byte strings ending in NUL and a display resolved from GDK/Xlib.
    let atom = unsafe { XInternAtom(display, name.as_ptr().cast(), 0) };
    if atom == 0 {
        return Err(format!("Failed to resolve X11 {context} atom"));
    }
    Ok(atom)
}

impl X11Surface {
    fn from_surface(surface: &gtk::gdk::Surface, context: &str) -> Result<Self, String> {
        let display = surface.display();
        // SAFETY: GDK provides valid display/surface pointers for live GTK objects. The caller has
        // already checked that the surface backend is X11 before converting it to Xlib handles.
        let xdisplay = unsafe { gdk_x11_display_get_xdisplay(display.as_ptr() as *mut c_void) };
        if xdisplay.is_null() {
            return Err(format!("Failed to read X11 display for {context}"));
        }

        // SAFETY: The surface is a live X11 GDK surface, so gdk_x11_surface_get_xid can read its
        // backing X window id without taking ownership.
        let xid = unsafe { gdk_x11_surface_get_xid(surface.as_ptr() as *mut c_void) };
        if xid == 0 {
            return Err(format!("Failed to read X11 window id for {context}"));
        }

        Ok(Self {
            display: xdisplay,
            window: xid,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_window_icon_data_has_expected_header() {
        let icon_data = embedded_window_icon_data().unwrap();

        assert_eq!(icon_data.first().copied(), Some(128));
        assert_eq!(icon_data.get(1).copied(), Some(128));
        assert_eq!(icon_data.len(), 2 + 128 * 128);
    }
}
