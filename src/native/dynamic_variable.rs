// Port of `hypothesis.utils.dynamicvariables.DynamicVariable`.
//
// A value with a default that can be temporarily overridden within a scope.
// Nested `with_value` calls stack LIFO: the innermost override wins until
// its scope exits, at which point the previous value is restored.

use std::sync::Mutex;

pub struct DynamicVariable<T: Clone> {
    default: T,
    stack: Mutex<Vec<T>>,
}

impl<T: Clone> DynamicVariable<T> {
    pub fn new(default: T) -> Self {
        Self {
            default,
            stack: Mutex::new(Vec::new()),
        }
    }

    pub fn value(&self) -> T {
        self.stack
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_else(|| self.default.clone())
    }

    pub fn with_value<R>(&self, value: T, f: impl FnOnce() -> R) -> R {
        struct Guard<'a, T: Clone>(&'a DynamicVariable<T>);
        impl<T: Clone> Drop for Guard<'_, T> {
            fn drop(&mut self) {
                self.0.stack.lock().unwrap().pop();
            }
        }
        self.stack.lock().unwrap().push(value);
        let _guard = Guard(self);
        f()
    }
}
