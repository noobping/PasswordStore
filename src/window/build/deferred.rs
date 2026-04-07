use std::cell::OnceCell;
use std::rc::Rc;

#[derive(Clone)]
pub(super) struct DeferredState<T> {
    inner: Rc<DeferredStateInner<T>>,
}

struct DeferredStateInner<T> {
    state: OnceCell<T>,
    init: Box<dyn Fn() -> T>,
}

impl<T> DeferredState<T> {
    pub(super) fn new(init: impl Fn() -> T + 'static) -> Self {
        Self {
            inner: Rc::new(DeferredStateInner {
                state: OnceCell::new(),
                init: Box::new(init),
            }),
        }
    }

    pub(super) fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(self.inner.state.get_or_init(|| (self.inner.init)()))
    }

    pub(super) fn with_initialized<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.inner.state.get().map(f)
    }
}

#[cfg(test)]
mod tests {
    use super::DeferredState;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn deferred_state_initializes_once() {
        let init_count = Rc::new(Cell::new(0usize));
        let state = DeferredState::new({
            let init_count = init_count.clone();
            move || {
                init_count.set(init_count.get() + 1);
                7usize
            }
        });

        assert_eq!(state.with(|value| *value), 7);
        assert_eq!(state.with(|value| *value), 7);
        assert_eq!(state.with_initialized(|value| *value), Some(7));
        assert_eq!(init_count.get(), 1);
    }
}
