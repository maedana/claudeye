#[cfg(target_os = "macos")]
pub fn get_cursor_screen_position() -> Option<(f64, f64)> {
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
    let event = CGEvent::new(source).ok()?;
    let point = event.location();
    Some((point.x, point.y))
}

#[cfg(target_os = "linux")]
pub fn get_cursor_screen_position() -> Option<(f64, f64)> {
    use std::mem::MaybeUninit;
    use std::ptr;
    use x11_dl::xlib;

    unsafe {
        let xlib = xlib::Xlib::open().ok()?;
        let display = (xlib.XOpenDisplay)(ptr::null());
        if display.is_null() {
            return None;
        }

        let screen = (xlib.XDefaultScreen)(display);
        let root = (xlib.XRootWindow)(display, screen);

        let mut root_return = MaybeUninit::uninit();
        let mut child_return = MaybeUninit::uninit();
        let mut root_x = MaybeUninit::uninit();
        let mut root_y = MaybeUninit::uninit();
        let mut win_x = MaybeUninit::uninit();
        let mut win_y = MaybeUninit::uninit();
        let mut mask = MaybeUninit::uninit();

        let ok = (xlib.XQueryPointer)(
            display,
            root,
            root_return.as_mut_ptr(),
            child_return.as_mut_ptr(),
            root_x.as_mut_ptr(),
            root_y.as_mut_ptr(),
            win_x.as_mut_ptr(),
            win_y.as_mut_ptr(),
            mask.as_mut_ptr(),
        );

        let result = if ok != 0 {
            Some((root_x.assume_init() as f64, root_y.assume_init() as f64))
        } else {
            None
        };

        (xlib.XCloseDisplay)(display);
        result
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn get_cursor_screen_position() -> Option<(f64, f64)> {
    None
}

pub fn is_cursor_in_rect(
    cursor_x: f64,
    cursor_y: f64,
    rect_x: f32,
    rect_y: f32,
    rect_w: f32,
    rect_h: f32,
) -> bool {
    let cx = cursor_x as f32;
    let cy = cursor_y as f32;
    cx >= rect_x && cx <= rect_x + rect_w && cy >= rect_y && cy <= rect_y + rect_h
}

/// Snap threshold for opacity lerp (values closer than this snap to target).
pub const OPACITY_SNAP_THRESHOLD: f32 = 0.01;

pub fn lerp_opacity(current: f32, target: f32, factor: f32) -> f32 {
    let result = current + (target - current) * factor.clamp(0.0, 1.0);
    if (result - target).abs() < OPACITY_SNAP_THRESHOLD {
        target
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_inside_rect() {
        assert!(is_cursor_in_rect(50.0, 50.0, 0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn cursor_outside_rect_right() {
        assert!(!is_cursor_in_rect(150.0, 50.0, 0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn cursor_outside_rect_below() {
        assert!(!is_cursor_in_rect(50.0, 150.0, 0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn cursor_outside_rect_left() {
        assert!(!is_cursor_in_rect(-1.0, 50.0, 0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn cursor_outside_rect_above() {
        assert!(!is_cursor_in_rect(50.0, -1.0, 0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn cursor_on_edge_top_left() {
        assert!(is_cursor_in_rect(0.0, 0.0, 0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn cursor_on_edge_bottom_right() {
        assert!(is_cursor_in_rect(100.0, 100.0, 0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn cursor_in_offset_rect() {
        assert!(is_cursor_in_rect(150.0, 250.0, 100.0, 200.0, 100.0, 100.0));
    }

    #[test]
    fn cursor_outside_offset_rect() {
        assert!(!is_cursor_in_rect(50.0, 250.0, 100.0, 200.0, 100.0, 100.0));
    }

    #[test]
    fn lerp_moves_toward_target() {
        let result = lerp_opacity(1.0, 0.0, 0.5);
        assert!((result - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn lerp_snaps_when_close() {
        let result = lerp_opacity(0.005, 0.0, 0.5);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn lerp_no_change_at_target() {
        let result = lerp_opacity(1.0, 1.0, 0.5);
        assert_eq!(result, 1.0);
    }

    #[test]
    fn lerp_factor_clamped_above_one() {
        let result = lerp_opacity(1.0, 0.0, 2.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn lerp_factor_clamped_below_zero() {
        let result = lerp_opacity(1.0, 0.0, -1.0);
        assert_eq!(result, 1.0);
    }

    #[test]
    fn lerp_gradual_convergence() {
        let mut val = 1.0;
        for _ in 0..20 {
            val = lerp_opacity(val, 0.15, 0.25);
        }
        assert_eq!(val, 0.15);
    }
}
