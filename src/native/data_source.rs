// Native backend DataSource implementation.
//
// Wraps a NativeTestCase behind interior mutability so it can implement
// the DataSource trait (which takes &self).  The Rc<RefCell<...>> handle
// is shared with the runner so it can extract nodes/spans after the test.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ciborium::Value;

use crate::backend::{DataSource, DataSourceError};
use crate::native::core::{ChoiceNode, NativeTestCase, Span};
use crate::native::schema;

/// Shared handle to the underlying `NativeTestCase`.
///
/// Both `NativeDataSource` and the caller hold a clone of this handle.
/// After the test case completes, the caller reads nodes/spans from it.
pub type NativeTestCaseHandle = Rc<RefCell<NativeTestCase>>;

pub struct NativeDataSource {
    inner: NativeTestCaseHandle,
    aborted: Cell<bool>,
}

impl NativeDataSource {
    /// Create a new `NativeDataSource` and return a shared handle.
    ///
    /// The handle can be used to extract `nodes` and `spans` after the
    /// test case has finished running.
    pub fn new(ntc: NativeTestCase) -> (Self, NativeTestCaseHandle) {
        let inner = Rc::new(RefCell::new(ntc));
        let handle = Rc::clone(&inner);
        (
            NativeDataSource {
                inner,
                aborted: Cell::new(false),
            },
            handle,
        )
    }

    /// Convenience: extract choice nodes from a handle after a test case.
    pub fn take_nodes(handle: &NativeTestCaseHandle) -> Vec<ChoiceNode> {
        handle.borrow().nodes.clone()
    }

    /// Convenience: extract spans from a handle after a test case.
    pub fn take_spans(handle: &NativeTestCaseHandle) -> Vec<Span> {
        handle.borrow().spans.clone()
    }

    fn dispatch(&self, command: &str, payload: &Value) -> Result<Value, DataSourceError> {
        if self.aborted.get() {
            return Err(DataSourceError::StopTest);
        }
        let mut ntc = self.inner.borrow_mut();
        schema::dispatch_request(&mut ntc, command, payload).map_err(|_stop| {
            self.aborted.set(true);
            DataSourceError::StopTest
        })
    }
}

impl DataSource for NativeDataSource {
    fn generate(&self, schema: &Value) -> Result<Value, DataSourceError> {
        use crate::cbor_utils::cbor_map;
        self.dispatch("generate", &cbor_map! {"schema" => schema.clone()})
    }

    fn start_span(&self, label: u64) -> Result<(), DataSourceError> {
        use crate::cbor_utils::cbor_map;
        self.dispatch("start_span", &cbor_map! {"label" => label})?;
        Ok(())
    }

    fn stop_span(&self, discard: bool) -> Result<(), DataSourceError> {
        use crate::cbor_utils::cbor_map;
        self.dispatch("stop_span", &cbor_map! {"discard" => discard})?;
        Ok(())
    }

    fn new_collection(
        &self,
        min_size: u64,
        max_size: Option<u64>,
    ) -> Result<String, DataSourceError> {
        use crate::cbor_utils::{cbor_map, map_insert};
        let mut payload = cbor_map! { "min_size" => min_size };
        if let Some(max) = max_size {
            map_insert(&mut payload, "max_size", max);
        }
        let Value::Integer(i) = self.dispatch("new_collection", &payload)? else {
            unreachable!("new_collection always returns Value::Integer")
        };
        let n: i128 = i.into();
        Ok(n.to_string())
    }

    fn collection_more(&self, collection: &str) -> Result<bool, DataSourceError> {
        use crate::cbor_utils::cbor_map;
        let collection_id: i64 = collection.parse().unwrap();
        let response = self.dispatch(
            "collection_more",
            &cbor_map! { "collection_id" => collection_id },
        )?;
        let Value::Bool(b) = response else {
            unreachable!("collection_more always returns Value::Bool")
        };
        Ok(b)
    }

    fn collection_reject(
        &self,
        collection: &str,
        why: Option<&str>,
    ) -> Result<(), DataSourceError> {
        use crate::cbor_utils::{cbor_map, map_insert};
        let collection_id: i64 = collection.parse().unwrap();
        let mut payload = cbor_map! { "collection_id" => collection_id };
        if let Some(reason) = why {
            map_insert(&mut payload, "why", reason.to_string());
        }
        self.dispatch("collection_reject", &payload)?;
        Ok(())
    }

    fn new_pool(&self) -> Result<i128, DataSourceError> {
        use crate::cbor_utils::cbor_map;
        let Value::Integer(i) = self.dispatch("new_pool", &cbor_map! {})? else {
            unreachable!("new_pool always returns Value::Integer")
        };
        Ok(i.into())
    }

    fn pool_add(&self, pool_id: i128) -> Result<i128, DataSourceError> {
        use crate::cbor_utils::cbor_map;
        let Value::Integer(i) = self.dispatch("pool_add", &cbor_map! {"pool_id" => pool_id})?
        else {
            unreachable!("pool_add always returns Value::Integer")
        };
        Ok(i.into())
    }

    fn pool_generate(&self, pool_id: i128, consume: bool) -> Result<i128, DataSourceError> {
        use crate::cbor_utils::cbor_map;
        let Value::Integer(i) = self.dispatch(
            "pool_generate",
            &cbor_map! {
                "pool_id" => pool_id,
                "consume" => consume,
            },
        )?
        else {
            unreachable!("pool_generate always returns Value::Integer")
        };
        Ok(i.into())
    }

    fn mark_complete(&self, _status: &str, _origin: Option<&str>) {
        // No-op for native backend: there is no server to notify.
    }

    fn test_aborted(&self) -> bool {
        self.aborted.get()
    }
}

#[cfg(test)]
#[path = "../../tests/embedded/native/data_source_tests.rs"]
mod tests;
