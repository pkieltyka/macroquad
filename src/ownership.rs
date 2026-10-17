//! Private scoped ownership primitives. No reference or borrow guard escapes.
use std::cell::RefCell;

pub(crate) struct Scoped<T> {
    value: RefCell<Option<T>>,
}
impl<T> Scoped<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            value: RefCell::new(Some(value)),
        }
    }
    pub(crate) fn into_inner(self) -> T {
        self.value
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
            .as_mut()
            .expect("Recursive access to an active Macroquad component");
        f(value)
    }
}
struct Lease<'a, T> {
    owner: &'a Scoped<T>,
    value: Option<T>,
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
}
