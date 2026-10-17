#![allow(non_upper_case_globals, non_snake_case)]

//! Clipboard implementation for X11
//!
//! Clipboard API on X11 is pretty weird
//! <https://www.uninformativ.de/blog/postings/2017-04-02/0/POSTING-en.html>
//! so use this with caution.

use super::libx11::*;
use std::{cell::RefCell, convert::TryFrom, rc::Rc};

unsafe fn get_clipboard(
    libx11: &mut LibX11,
    display: *mut Display,
    window: Window,
    format: Atom,
) -> Option<Vec<u8>> {
    // We ask X server to ask current clipboard owner to convert selection to the desired format
    // Selection owner is other X11 application and if it will not satisfy our request for any reason -
    // we will wait for appropriate event forever
    // but OK lets believe that all other linux apps will gracefully send us nice utf-8 strings
    (libx11.XConvertSelection)(
        display,
        libx11.extensions.clipboard,
        format,
        libx11.extensions.xsel_data,
        window,
        CurrentTime as Time,
    );

    // And now X server will respond with clipboard data
    // But we want "get_clipboard" to be blocking function and get result asap
    // So we just start to wait for the event right here
    // In case that our app already is clipboard owner - we need to handle SelectionNotify for data response
    // and SelectionRequest - to handle this request we just did couple of lines above
    let mut event = XEvent { type_0: 0 };
    loop {
        (libx11.XNextEvent)(display, &mut event);
        if event.type_0 == SelectionNotify
            && event.xselection.selection == libx11.extensions.clipboard
        {
            // The property should be XSEL_DATA, otherwise we return None
            return (event.xselection.property == libx11.extensions.xsel_data)
                .then(|| get_property_bytes(libx11, display, window, event.xselection.property));
        }
        if event.type_0 == SelectionRequest {
            respond_to_clipboard_request(libx11, display, &mut event as *mut _);
        }
    }
}

/// Get the bytes from the window property
/// INCR protocol is implemented; will block until all data are received
/// Useful for getting data following a SelectionNotify event
//
//  X11 has the INCR protocol to transfer data in several batches
//  https://www.x.org/releases/X11R7.7/doc/xorg-docs/icccm/icccm.html#INCR_Properties
//
//  The data are sequentially written to the given property on the window
//  After each read, we delete the property to notify X that we are ready to receive more data
//  Used both for the clipboard and the drag-n-drop protocol
pub(crate) unsafe fn get_property_bytes(
    libx11: &mut LibX11,
    display: *mut Display,
    window: Window,
    property: Atom,
) -> Vec<u8> {
    let mut transfer = PropertyTransfer::default();
    loop {
        let mut reply = PropertyReply::new(libx11.XFree);
        reply.status = (libx11.XGetWindowProperty)(
            display,
            window,
            property,
            0,
            libc::c_long::MAX,
            false as _,
            AnyPropertyType,
            &mut reply.actual_type,
            &mut reply.format,
            &mut reply.count,
            &mut reply.remaining,
            &mut reply.buffer.ptr,
        );

        // accept owns and frees this reply, including INCR headers/terminators,
        // before we call Xlib again or wait for another property notification.
        let complete = match transfer.accept(reply, libx11.extensions.incr) {
            Some(complete) => complete,
            None => return Vec::new(),
        };
        if transfer.incremental {
            (libx11.XDeleteProperty)(display, window, property);
        }
        if complete {
            return transfer.bytes;
        }

        let mut event = XEvent { type_0: 0 };
        loop {
            (libx11.XNextEvent)(display, &mut event);
            if event.type_0 == PropertyNotify
                && event.xproperty.window == window
                && event.xproperty.atom == property
                && event.xproperty.state == PropertyNewValue
            {
                break;
            }
        }
    }
}

struct PropertyBuffer {
    ptr: *mut libc::c_uchar,
    free: unsafe extern "C" fn(*mut libc::c_void) -> libc::c_int,
}

impl Drop for PropertyBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { (self.free)(self.ptr.cast()) };
        }
    }
}

struct PropertyReply {
    status: libc::c_int,
    actual_type: Atom,
    format: libc::c_int,
    count: libc::c_ulong,
    remaining: libc::c_ulong,
    buffer: PropertyBuffer,
}

impl PropertyReply {
    fn new(free: unsafe extern "C" fn(*mut libc::c_void) -> libc::c_int) -> Self {
        Self {
            status: -1,
            actual_type: 0,
            format: 0,
            count: 0,
            remaining: 0,
            buffer: PropertyBuffer {
                ptr: std::ptr::null_mut(),
                free,
            },
        }
    }

    fn byte_len(&self) -> Option<usize> {
        if self.status != Success || self.actual_type == 0 || self.remaining != 0 {
            return None;
        }
        // XGetWindowProperty expands format 32 to native longs, not four-byte
        // wire values. Format 16 is native short, never native int.
        let element_size = match self.format {
            8 => std::mem::size_of::<libc::c_char>(),
            16 => std::mem::size_of::<libc::c_short>(),
            32 => std::mem::size_of::<libc::c_long>(),
            _ => return None,
        };
        let length = usize::try_from(self.count)
            .ok()?
            .checked_mul(element_size)?;
        if length > isize::MAX as usize || (length != 0 && self.buffer.ptr.is_null()) {
            return None;
        }
        Some(length)
    }

    /// The pointer must come from XGetWindowProperty with its matching metadata,
    /// or an equally sized initialized test allocation and matching deallocator.
    unsafe fn append_to(&self, output: &mut Vec<u8>) -> Option<()> {
        let length = self.byte_len()?;
        checked_property_total(output.len(), length)?;
        if length != 0 {
            output.try_reserve(length).ok()?;
            output.extend_from_slice(std::slice::from_raw_parts(self.buffer.ptr, length));
        }
        Some(())
    }
}

fn checked_property_total(current: usize, additional: usize) -> Option<usize> {
    current
        .checked_add(additional)
        .filter(|total| *total <= isize::MAX as usize)
}

#[derive(Default)]
struct PropertyTransfer {
    bytes: Vec<u8>,
    incremental: bool,
    data_type: Option<(Atom, libc::c_int)>,
}

impl PropertyTransfer {
    /// Consumes the Xlib allocation before returning to the event/FFI loop.
    /// The reply's allocation must satisfy PropertyReply::append_to's contract.
    unsafe fn accept(&mut self, reply: PropertyReply, incr: Atom) -> Option<bool> {
        reply.byte_len()?;
        if reply.actual_type == incr {
            if self.incremental || reply.format != 32 || reply.count != 1 {
                return None;
            }
            self.incremental = true;
            return Some(false);
        }
        let data_type = (reply.actual_type, reply.format);
        if self.data_type.is_some() && self.data_type != Some(data_type) {
            return None;
        }
        self.data_type = Some(data_type);
        reply.append_to(&mut self.bytes)?;
        Some(!self.incremental || reply.count == 0)
    }
}

thread_local! {
    static MESSAGE: RefCell<Option<Rc<str>>> = const { RefCell::new(None) };
}

fn store_message(message: String) {
    let message = Rc::<str>::from(message);
    // Construct and drop allocations outside the scoped TLS borrow.
    let previous = MESSAGE.with(|slot| slot.replace(Some(message)));
    drop(previous);
}

fn message_snapshot() -> Option<Rc<str>> {
    MESSAGE.with(|slot| slot.borrow().clone())
}

/// Claim that our app is X11 clipboard owner
/// Now when some other linux app will ask X11 for clipboard content - it will be redirected to our app
unsafe fn claim_clipboard_ownership(
    libx11: &mut LibX11,
    display: *mut Display,
    window: Window,
    message: String,
) {
    store_message(message);
    (libx11.XSetSelectionOwner)(
        display,
        libx11.extensions.clipboard,
        window,
        CurrentTime as Time,
    );
}

/// this function is supposed to be called from sapp's event loop
/// when XSelectionEvent received.
/// It will parse event and call XSendEvent with event response
pub(crate) unsafe fn respond_to_clipboard_request(
    libx11: &mut LibX11,
    display: *mut Display,
    event: *const XEvent,
) {
    assert!((*event).type_0 == SelectionRequest); // is it really SelectionRequest

    // The immutable snapshot survives reentrant Xlib callbacks without keeping
    // a TLS borrow or a reference into replaceable clipboard storage alive.
    let message = message_snapshot();
    let message = message.as_deref().unwrap_or("");

    let utf8_string = libx11.extensions.utf8_string;
    let xselectionrequest = (*event).xselectionrequest;
    let mut ev = XSelectionEvent {
        type_0: SelectionNotify,
        serial: 0,
        send_event: 0,
        display: xselectionrequest.display,
        requestor: xselectionrequest.requestor,
        selection: xselectionrequest.selection,
        target: xselectionrequest.target,
        property: xselectionrequest.property,
        time: xselectionrequest.time,
    };

    // only UTF8 requests are supported
    let length = libc::c_int::try_from(message.len()).ok();
    if let (true, Some(length)) = (xselectionrequest.target == utf8_string, length) {
        (libx11.XChangeProperty)(
            xselectionrequest.display,
            xselectionrequest.requestor,
            xselectionrequest.property,
            utf8_string,
            8 as libc::c_int,
            PropModeReplace,
            message.as_bytes().as_ptr(),
            length,
        );

        (libx11.XSendEvent)(
            display,
            ev.requestor,
            false as _,
            NoEventMask,
            &mut ev as *mut XSelectionEvent as *mut XEvent,
        );
    } else {
        // signal X that request is denied
        ev.property = 0 as Atom;

        (libx11.XSendEvent)(
            display,
            ev.requestor,
            false as _,
            NoEventMask,
            &mut ev as *mut XSelectionEvent as *mut XEvent,
        );
    }
}

pub struct X11Clipboard {
    libx11: LibX11,
    display: *mut Display,
    window: Window,
}
unsafe impl Send for X11Clipboard {}
unsafe impl Sync for X11Clipboard {}

impl X11Clipboard {
    pub fn new(libx11: LibX11, display: *mut Display, window: Window) -> X11Clipboard {
        X11Clipboard {
            libx11,
            display,
            window,
        }
    }
}
impl crate::native::Clipboard for X11Clipboard {
    fn get(&mut self) -> Option<String> {
        let utf8_string = self.libx11.extensions.utf8_string;
        let bytes =
            unsafe { get_clipboard(&mut self.libx11, self.display, self.window, utf8_string)? };
        Some(std::str::from_utf8(&bytes).ok()?.to_string())
    }

    fn set(&mut self, data: &str) {
        unsafe {
            claim_clipboard_ownership(&mut self.libx11, self.display, self.window, data.to_owned());
        };
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use std::cell::Cell;

    const INCR: Atom = 9;
    thread_local! {
        static FREES: Cell<usize> = const { Cell::new(0) };
    }

    unsafe extern "C" fn free_box<T>(pointer: *mut libc::c_void) -> libc::c_int {
        drop(Box::from_raw(pointer.cast::<T>()));
        FREES.with(|count| count.set(count.get() + 1));
        0
    }

    fn reply<T>(value: T, format: libc::c_int, count: libc::c_ulong) -> PropertyReply {
        PropertyReply {
            status: Success,
            actual_type: 1,
            format,
            count,
            remaining: 0,
            buffer: PropertyBuffer {
                ptr: Box::into_raw(Box::new(value)).cast(),
                free: free_box::<T>,
            },
        }
    }

    #[test]
    fn exact_char_short_and_native_long_allocations_are_copied() {
        let mut transfer = PropertyTransfer::default();
        assert_eq!(
            unsafe { transfer.accept(reply([1_u8, 2, 3], 8, 3), INCR) },
            Some(true)
        );
        assert_eq!(transfer.bytes, [1, 2, 3]);

        let value: libc::c_short = 0x1234;
        let mut transfer = PropertyTransfer::default();
        // This allocation is exactly one short. Counting it as c_int makes
        // Miri report an out-of-bounds read on both supported interpreter widths.
        assert_eq!(
            unsafe { transfer.accept(reply(value, 16, 1), INCR) },
            Some(true)
        );
        assert_eq!(transfer.bytes, value.to_ne_bytes());

        let values: [libc::c_long; 2] = [0x12345678, 0x654321];
        let mut transfer = PropertyTransfer::default();
        assert_eq!(
            unsafe { transfer.accept(reply(values, 32, 2), INCR) },
            Some(true)
        );
        let expected: Vec<u8> = values
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect();
        assert_eq!(
            transfer.bytes.len(),
            2 * std::mem::size_of::<libc::c_long>()
        );
        assert_eq!(transfer.bytes, expected);
    }

    #[test]
    fn invalid_replies_are_rejected_and_freed_before_reading() {
        FREES.with(|count| count.set(0));
        for case in 0..7 {
            let mut response = reply(0_u8, 8, 1);
            match case {
                0 => response.status = 1,
                1 => response.actual_type = 0,
                2 => response.format = 0,
                3 => response.format = 24,
                4 => response.remaining = 1,
                5 => response.count = libc::c_ulong::MAX,
                6 => {
                    response.format = 32;
                    response.count = libc::c_ulong::MAX;
                }
                _ => unreachable!(),
            }
            assert_eq!(
                unsafe { PropertyTransfer::default().accept(response, INCR) },
                None
            );
        }
        FREES.with(|count| assert_eq!(count.get(), 7));
        assert_eq!(checked_property_total(usize::MAX, 1), None);
        assert_eq!(checked_property_total(isize::MAX as usize, 1), None);
        assert_eq!(checked_property_total(4, 2), Some(6));
    }

    #[test]
    fn null_empty_property_never_constructs_a_raw_slice() {
        let mut empty = PropertyReply::new(free_box::<u8>);
        empty.status = Success;
        empty.actual_type = 1;
        empty.format = 8;
        assert_eq!(
            unsafe { PropertyTransfer::default().accept(empty, INCR) },
            Some(true)
        );
        let mut missing = PropertyReply::new(free_box::<u8>);
        missing.status = Success;
        missing.actual_type = 1;
        missing.format = 8;
        missing.count = 1;
        assert_eq!(
            unsafe { PropertyTransfer::default().accept(missing, INCR) },
            None
        );
    }

    #[test]
    fn incremental_header_chunks_and_terminator_each_release_their_allocation() {
        FREES.with(|count| count.set(0));
        let mut transfer = PropertyTransfer::default();
        let mut header = reply(3 as libc::c_long, 32, 1);
        header.actual_type = INCR;
        assert_eq!(unsafe { transfer.accept(header, INCR) }, Some(false));
        FREES.with(|count| assert_eq!(count.get(), 1));
        assert_eq!(
            unsafe { transfer.accept(reply([b'a', b'b'], 8, 2), INCR) },
            Some(false)
        );
        FREES.with(|count| assert_eq!(count.get(), 2));
        assert_eq!(
            unsafe { transfer.accept(reply(b'c', 8, 1), INCR) },
            Some(false)
        );
        FREES.with(|count| assert_eq!(count.get(), 3));
        assert_eq!(
            unsafe { transfer.accept(reply(0_u8, 8, 0), INCR) },
            Some(true)
        );
        FREES.with(|count| assert_eq!(count.get(), 4));
        assert_eq!(transfer.bytes, b"abc");
    }

    #[test]
    fn incremental_transfer_rejects_bad_headers_and_changed_chunk_types() {
        let mut transfer = PropertyTransfer::default();
        let mut header = reply(1_u8, 8, 1);
        header.actual_type = INCR;
        assert_eq!(unsafe { transfer.accept(header, INCR) }, None);

        let mut header = reply(1 as libc::c_long, 32, 1);
        header.actual_type = INCR;
        assert_eq!(unsafe { transfer.accept(header, INCR) }, Some(false));
        assert_eq!(
            unsafe { transfer.accept(reply(b'a', 8, 1), INCR) },
            Some(false)
        );
        let mut changed = reply(b'b', 8, 1);
        changed.actual_type = 2;
        assert_eq!(unsafe { transfer.accept(changed, INCR) }, None);
    }

    #[test]
    fn clipboard_snapshots_survive_replacement_and_are_thread_local() {
        store_message("first".to_owned());
        let snapshot = message_snapshot().unwrap();
        MESSAGE.with(|slot| assert!(slot.try_borrow_mut().is_ok()));
        store_message("second".to_owned());
        assert_eq!(&*snapshot, "first");
        assert_eq!(&*message_snapshot().unwrap(), "second");
        let child = std::thread::spawn(|| {
            assert!(message_snapshot().is_none());
            store_message("child".to_owned());
            message_snapshot().unwrap().to_string()
        });
        assert_eq!(child.join().unwrap(), "child");
        assert_eq!(&*message_snapshot().unwrap(), "second");
    }

    #[test]
    #[ignore = "requires an isolated live X11 server; run explicitly under Xvfb"]
    fn live_x11_properties_and_clipboard_roundtrip() {
        struct Connection {
            lib: LibX11,
            display: *mut Display,
            window: Window,
        }
        impl Drop for Connection {
            fn drop(&mut self) {
                unsafe {
                    (self.lib.XDestroyWindow)(self.display, self.window);
                    (self.lib.XCloseDisplay)(self.display);
                }
            }
        }
        unsafe {
            let mut lib = LibX11::try_load().unwrap();
            let display = (lib.XOpenDisplay)(std::ptr::null());
            assert!(!display.is_null(), "XOpenDisplay failed");
            lib.load_extensions(display);
            let screen = (*(display as _XPrivDisplay)).default_screen;
            let root = (*(*(display as _XPrivDisplay))
                .screens
                .offset(screen as isize))
            .root;
            let window = (lib.XCreateWindow)(
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
            assert_ne!(window, 0);
            let mut connection = Connection {
                lib,
                display,
                window,
            };
            let property = connection.lib.extensions.xsel_data;
            let atom = connection.lib.extensions.cardinal;
            let shorts: [libc::c_short; 2] = [0x1234, 0x4567];
            (connection.lib.XChangeProperty)(
                display,
                window,
                property,
                atom,
                16,
                PropModeReplace,
                shorts.as_ptr().cast(),
                2,
            );
            let expected: Vec<u8> = shorts
                .iter()
                .flat_map(|value| value.to_ne_bytes())
                .collect();
            assert_eq!(
                get_property_bytes(&mut connection.lib, display, window, property),
                expected
            );
            let longs: [libc::c_long; 2] = [0x12345678, 0x76543210];
            (connection.lib.XChangeProperty)(
                display,
                window,
                property,
                atom,
                32,
                PropModeReplace,
                longs.as_ptr().cast(),
                2,
            );
            let expected: Vec<u8> = longs.iter().flat_map(|value| value.to_ne_bytes()).collect();
            assert_eq!(
                get_property_bytes(&mut connection.lib, display, window, property),
                expected
            );
            let format = connection.lib.extensions.utf8_string;
            for text in ["X11 clipboard 🦀", ""] {
                claim_clipboard_ownership(&mut connection.lib, display, window, text.to_owned());
                let received = get_clipboard(&mut connection.lib, display, window, format).unwrap();
                assert_eq!(String::from_utf8(received).unwrap(), text);
            }
        }
    }
}
