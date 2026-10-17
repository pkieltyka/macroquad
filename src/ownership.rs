//! Private scoped ownership primitives. No reference or borrow guard escapes.
use std::cell::RefCell;

pub(crate) struct Scoped<T> {
    // Boxed so that leasing moves a pointer rather than the whole component:
    // every engine API call takes a lease, and Context is kilobytes inline.
    value: RefCell<Option<Box<T>>>,
}
impl<T> Scoped<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            value: RefCell::new(Some(Box::new(value))),
        }
    }
    pub(crate) fn into_inner(self) -> T {
        *self
            .value
            .into_inner()
            .expect("Active scoped owner during shutdown")
    }
    pub(crate) fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        // Check outside the RefCell borrow: panic hooks may themselves use the
        // engine. User code/destructors never run with this cell borrowed.
        let value = self.value.borrow_mut().take();
        let mut lease = Lease { owner: self, value };
        let value = lease
            .value
            .as_deref_mut()
            .expect("Recursive access to an active Macroquad component");
        f(value)
    }
}
struct Lease<'a, T> {
    owner: &'a Scoped<T>,
    value: Option<Box<T>>,
}
impl<T> Drop for Lease<'_, T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            let old = self.owner.value.borrow_mut().replace(value);
            debug_assert!(old.is_none());
            drop(old);
        }
    }
}

pub(crate) struct OnceEntry(std::sync::atomic::AtomicBool);
impl OnceEntry {
    pub(crate) const fn new() -> Self {
        Self(std::sync::atomic::AtomicBool::new(false))
    }
    pub(crate) fn claim(&self) -> bool {
        self.0
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initialization_is_claimed_once_across_threads() {
        let entry = std::sync::Arc::new(OnceEntry::new());
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let entry = entry.clone();
                std::thread::spawn(move || entry.claim())
            })
            .collect();
        assert_eq!(
            threads
                .into_iter()
                .map(|thread| usize::from(thread.join().unwrap()))
                .sum::<usize>(),
            1
        );
        assert!(!entry.claim());
    }
    #[test]
    fn scoped_access_restores_after_nested_rejection_and_unwind() {
        let state = Scoped::new(vec![1]);
        state.with(|value| {
            value.push(2);
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.with(|_| ())))
                    .is_err()
            );
            value.push(3);
        });
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.with(|value| {
                value.push(4);
                panic!("unwind");
            })))
            .is_err()
        );
        assert_eq!(state.with(|value| value.clone()), [1, 2, 3, 4]);
    }
    #[test]
    fn leasing_moves_a_pointer_not_the_component() {
        // Inline storage moved the whole component on every lease. Keep those
        // component-sized moves out of the engine API paths.
        use std::mem::size_of;
        let pointers = 2 * size_of::<usize>();
        assert_eq!(size_of::<Scoped<[u8; 4096]>>(), pointers);
        assert_eq!(size_of::<Lease<'static, [u8; 4096]>>(), pointers);
        assert_eq!(size_of::<Lease<'static, crate::Context>>(), pointers);
        assert_eq!(
            size_of::<Lease<'static, crate::ui::ui_context::UiContext>>(),
            pointers
        );

        // The leased component keeps its address instead of moving to the stack.
        let state = Scoped::new([7u8; 4096]);
        let first = state.with(|value| value as *const [u8; 4096]);
        let nested = (|| state.with(|value| value as *const [u8; 4096]))();
        assert!(std::ptr::eq(first, nested));
        assert_eq!(state.into_inner()[4095], 7);
    }
    struct Counted(std::rc::Rc<std::cell::Cell<usize>>);
    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn leases_and_unwind_keep_the_component_alive_until_owner_drop() {
        let drops = std::rc::Rc::new(std::cell::Cell::new(0));
        let state = Scoped::new(Counted(drops.clone()));
        for _ in 0..3 {
            state.with(|_| assert_eq!(drops.get(), 0));
        }
        state.with(|_| {
            crate::test_backend::assert_panics_with(
                || state.with(|_| ()),
                "Recursive access to an active Macroquad component",
            );
            assert_eq!(drops.get(), 0);
        });
        crate::test_backend::assert_panics_with(
            || state.with(|_| panic!("outer callback unwind")),
            "outer callback unwind",
        );
        assert_eq!(drops.get(), 0);
        state.with(|_| assert_eq!(drops.get(), 0));
        drop(state);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn into_inner_transfers_exactly_one_live_component() {
        let drops = std::rc::Rc::new(std::cell::Cell::new(0));
        let state = Scoped::new(Counted(drops.clone()));
        state.with(|_| ());
        let component = state.into_inner();
        assert_eq!(drops.get(), 0);
        drop(component);
        assert_eq!(drops.get(), 1);
    }

    struct BorrowOnDrop<'a> {
        owner: &'a Scoped<usize>,
        drops: &'a std::cell::Cell<usize>,
    }
    impl Drop for BorrowOnDrop<'_> {
        fn drop(&mut self) {
            assert!(
                self.owner.value.try_borrow_mut().is_ok(),
                "callback capture destructor ran under the owner cell borrow"
            );
            self.drops.set(self.drops.get() + 1);
        }
    }

    #[test]
    fn rejected_and_unwinding_callback_destructors_run_outside_cell_borrows() {
        let state = Scoped::new(42usize);
        let drops = std::cell::Cell::new(0);
        state.with(|_| {
            let probe = BorrowOnDrop {
                owner: &state,
                drops: &drops,
            };
            crate::test_backend::assert_panics_with(
                || {
                    state.with(move |_| {
                        let _capture = &probe;
                        panic!("rejected callback must not run");
                    })
                },
                "Recursive access to an active Macroquad component",
            );
            assert_eq!(drops.get(), 1);
        });
        let probe = BorrowOnDrop {
            owner: &state,
            drops: &drops,
        };
        crate::test_backend::assert_panics_with(
            || {
                state.with(move |value| {
                    let _capture = &probe;
                    *value += 1;
                    panic!("callback unwind with capture");
                })
            },
            "callback unwind with capture",
        );
        assert_eq!(drops.get(), 2);
        assert_eq!(state.into_inner(), 43);
    }

    #[test]
    fn extracted_component_destructor_can_access_another_scoped_owner() {
        struct ShutdownComponent<'a>(&'a Scoped<usize>);
        impl Drop for ShutdownComponent<'_> {
            fn drop(&mut self) {
                self.0.with(|value| *value += 1);
            }
        }
        let other = Scoped::new(0);
        let state = Scoped::new(ShutdownComponent(&other));
        let component = state.into_inner();
        assert_eq!(other.with(|value| *value), 0);
        drop(component);
        assert_eq!(other.into_inner(), 1);
    }
    #[test]
    fn panicking_callback_destructor_restores_the_live_component() {
        struct PanicOnDrop<'a>(&'a Scoped<usize>);
        impl Drop for PanicOnDrop<'_> {
            fn drop(&mut self) {
                assert!(self.0.value.try_borrow_mut().is_ok());
                panic!("callback destructor unwind");
            }
        }
        let state = Scoped::new(7);
        let capture = PanicOnDrop(&state);
        crate::test_backend::assert_panics_with(
            || {
                state.with(move |value| {
                    let _capture = &capture;
                    *value += 1;
                })
            },
            "callback destructor unwind",
        );
        assert_eq!(state.into_inner(), 8);
    }
}
