#![allow(non_upper_case_globals, non_snake_case)]

// Drag-n-Drop specification at
// https://freedesktop.org/wiki/Specifications/XDND/
// C++ implementation example
// https://www.codeproject.com/Tips/5387630/How-to-handle-X11-Drag-n-Drop-events

// The basic logic is similar to the mouse events
// mouse_down   => on_enter
// mouse_motion => on_pos
// mouse_up     => on_drop

use super::libx11::*;

#[derive(Default)]
pub struct X11DnD {
    source: Window,        // source window
    version: libc::c_long, // version of XDnD, should be 5
    format: Atom,          // we want UTF8_STRING for the paths of the dropped files
}

impl super::X11Display {
    pub(super) unsafe fn init_drag_n_drop(&mut self) {
        write_xdnd_aware(
            self.libx11.XChangeProperty,
            self.display,
            self.window,
            self.libx11.extensions.xdnd_aware,
        );
    }
}

type ChangeProperty = unsafe extern "C" fn(
    *mut Display,
    Window,
    Atom,
    Atom,
    libc::c_int,
    libc::c_int,
    *const libc::c_uchar,
    libc::c_int,
) -> libc::c_int;

unsafe fn write_xdnd_aware(
    change_property: ChangeProperty,
    display: *mut Display,
    window: Window,
    property: Atom,
) {
    // Xlib format32 reads native longs, including eight bytes per item on LP64.
    let version: [libc::c_long; 1] = [5];
    change_property(
        display,
        window,
        property,
        4, // XA_ATOM
        32,
        PropModeReplace,
        version.as_ptr().cast(),
        1,
    );
}

impl X11DnD {
    pub unsafe fn on_enter(
        &mut self,
        libx11: &mut LibX11,
        display: *mut Display,
        _window: Window,
        data: ClientMessageData,
    ) {
        // Store the relevant info
        self.source = data.l[0] as _;
        self.version = data.l[1] >> 24;

        // Then we need to determine the format (or type) to get the data
        // We want UTF8_STRING
        // The available ones depend on the source window and their number can vary
        self.format = 0;
        let formats: Vec<Atom> = if data.l[1] & 1 == 0 {
            // If there are fewer than 3, they are already in the client message we got
            vec![data.l[2] as _, data.l[3] as _, data.l[4] as _]
        } else {
            // Otherwise we need to read them from the window property
            let bytes = super::clipboard::get_property_bytes(
                libx11,
                display,
                self.source,
                libx11.extensions.xdnd_type_list,
            );
            // Realign the bytes into Atoms
            bytes.align_to::<Atom>().1.to_vec()
        };

        for format in formats {
            if format == libx11.extensions.utf8_string {
                self.format = format;
                break;
            }
        }
    }

    pub unsafe fn on_position(
        &mut self,
        libx11: &mut LibX11,
        display: *mut Display,
        window: Window,
        _data: ClientMessageData,
    ) {
        if self.version <= 5 {
            // We need to send back a client message of type XdndStatus
            let mut reply = XClientMessageEvent {
                type_0: ClientMessage,
                serial: 0,
                send_event: true as _,
                message_type: libx11.extensions.xdnd_status,
                window: self.source,
                display,
                format: 32,
                data: ClientMessageData {
                    l: [window as _, 0, 0, 0, 0],
                },
            };

            // The source window supports the desired format
            if self.format != 0 {
                // Notify that we can receive the drop
                reply.data.l[1] = 1;

                // Notify that we accept the XdndActionCopy action
                if self.version >= 2 {
                    reply.data.l[4] = libx11.extensions.xdnd_action_copy as _;
                }
            }
            (libx11.XSendEvent)(
                display,
                self.source,
                false as _,
                NoEventMask,
                &mut reply as *mut XClientMessageEvent as *mut _,
            );
            (libx11.XFlush)(display);
        }
    }

    pub unsafe fn on_drop(
        &mut self,
        libx11: &mut LibX11,
        display: *mut Display,
        window: Window,
        data: ClientMessageData,
    ) {
        if self.version <= 5 {
            if self.format != 0 {
                // Request to retrieve the data
                // The actual data will then be sent via a SelectionNotify event
                let mut time = CurrentTime;
                if self.version >= 1 {
                    time = data.l[3];
                }
                (libx11.XConvertSelection)(
                    display,
                    libx11.extensions.xdnd_selection,
                    self.format,
                    libx11.extensions.xdnd_selection,
                    window,
                    time as Time,
                );
            } else if self.version >= 2 {
                let mut reply = XClientMessageEvent {
                    type_0: ClientMessage,
                    serial: 0,
                    send_event: true as _,
                    message_type: libx11.extensions.xdnd_finished,
                    window: self.source,
                    display,
                    format: 32,
                    data: ClientMessageData {
                        l: [window as _, 0, 0, 0, 0],
                    },
                };
                (libx11.XSendEvent)(
                    display,
                    self.source,
                    false as _,
                    NoEventMask,
                    &mut reply as *mut XClientMessageEvent as *mut _,
                );
                (libx11.XFlush)(display);
            }
        }
    }
}

#[cfg(test)]
mod xdnd_property_tests {
    use super::*;

    #[test]
    fn xdnd_writer_supplies_one_native_long() {
        unsafe extern "C" fn checked_writer(
            display: *mut Display,
            window: Window,
            property: Atom,
            property_type: Atom,
            format: libc::c_int,
            mode: libc::c_int,
            data: *const libc::c_uchar,
            count: libc::c_int,
        ) -> libc::c_int {
            assert!(display.is_null());
            assert_eq!((window, property, property_type), (11, 12, 4));
            assert_eq!((format, mode, count), (32, PropModeReplace, 1));
            // Exercise Xlib's actual native-long read, not just the wire value.
            // Miri rejects the old four-byte allocation on an LP64 target.
            assert_eq!(*data.cast::<libc::c_long>(), 5);
            1
        }
        unsafe { write_xdnd_aware(checked_writer, std::ptr::null_mut(), 11, 12) };
    }

    #[test]
    #[ignore = "requires an isolated live X11 server; run explicitly under Xvfb"]
    fn live_x11_xdnd_awareness() {
        struct Connection {
            lib: LibX11,
            display: *mut Display,
            window: Window,
        }
        impl Drop for Connection {
            fn drop(&mut self) {
                unsafe {
                    if self.window != 0 {
                        (self.lib.XDestroyWindow)(self.display, self.window);
                    }
                    (self.lib.XCloseDisplay)(self.display);
                }
            }
        }
        unsafe {
            let lib = LibX11::try_load().unwrap();
            let display = (lib.XOpenDisplay)(std::ptr::null());
            assert!(!display.is_null(), "XOpenDisplay failed");
            let mut connection = Connection {
                lib,
                display,
                window: 0,
            };
            let screen = (*(display as _XPrivDisplay)).default_screen;
            let root = (*(*(display as _XPrivDisplay))
                .screens
                .offset(screen as isize))
            .root;
            connection.window = (connection.lib.XCreateWindow)(
                display,
                root,
                0,
                0,
                8,
                8,
                0,
                0,
                0,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
            );
            assert_ne!(connection.window, 0);
            let property = (connection.lib.XInternAtom)(display, b"XdndAware\0".as_ptr().cast(), 0);
            assert_ne!(property, 0);
            write_xdnd_aware(
                connection.lib.XChangeProperty,
                display,
                connection.window,
                property,
            );
            let mut property_type = 0;
            let mut format = 0;
            let mut count = 0;
            let mut remaining = 0;
            let mut data = std::ptr::null_mut();
            let status = (connection.lib.XGetWindowProperty)(
                display,
                connection.window,
                property,
                0,
                1,
                0,
                AnyPropertyType,
                &mut property_type,
                &mut format,
                &mut count,
                &mut remaining,
                &mut data,
            );
            struct Property {
                data: *mut libc::c_uchar,
                free: unsafe extern "C" fn(*mut libc::c_void) -> libc::c_int,
            }
            impl Drop for Property {
                fn drop(&mut self) {
                    if !self.data.is_null() {
                        unsafe { (self.free)(self.data.cast()) };
                    }
                }
            }
            let reply = Property {
                data,
                free: connection.lib.XFree,
            };
            assert_eq!(status, Success as libc::c_int);
            assert_eq!((property_type, format, count, remaining), (4, 32, 1, 0));
            assert!(!reply.data.is_null());
            assert_eq!(*reply.data.cast::<libc::c_long>(), 5);
        }
    }
}
