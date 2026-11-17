use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use super::{
    DomainOperation, OperationApplyResult, OperationApplyStatus, OperationError, OperationOrigin,
    OperationRecord, OperationTargetId,
};

/// What a target's handler answers for a record: [`OperationApplyResult`] with the inverse as a
/// record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationTargetResult {
    pub status: OperationApplyStatus,
    pub inverse: Option<OperationRecord>,
    pub affected_ids: Vec<String>,
    pub before_revision: u64,
    pub after_revision: u64,
}

/// A target's handler as an operation service calls it: a record and its origin in, what it did
/// out. It leaves the target unchanged when it fails. Usually made from an
/// [`OperationTargetHandler`] of a [`DomainOperation`] ([`OperationTargetHandle::of`]).
pub trait OperationTarget: Send + Sync + 'static {
    /// Applies `record` from `origin`.
    fn apply(
        &self,
        record: &OperationRecord,
        origin: &OperationOrigin,
    ) -> Result<OperationTargetResult, OperationError>;
}

/// A target's handler of the operations of `T`.
pub trait OperationTargetHandler<T: DomainOperation>: Send + Sync + 'static {
    /// Refuses `operation` (with a message) before anything changes.
    fn validate(&self, _operation: &T, _origin: &OperationOrigin) -> Result<(), String> {
        Ok(())
    }

    /// The handler must leave the target unchanged when it returns an error.
    fn apply_atomically(
        &self,
        operation: &T,
        origin: &OperationOrigin,
    ) -> Result<OperationApplyResult<T>, String>;
}

/// An [`OperationTargetHandler`] of `T` as an [`OperationTarget`]: it decodes each record as `T`,
/// validates and applies it and encodes the inverse.
struct TypedOperationTarget<T, H> {
    target_id: OperationTargetId,
    handler: Arc<H>,
    _operation: PhantomData<fn() -> T>,
}

impl<T, H> OperationTarget for TypedOperationTarget<T, H>
where
    T: DomainOperation,
    H: OperationTargetHandler<T>,
{
    fn apply(
        &self,
        record: &OperationRecord,
        origin: &OperationOrigin,
    ) -> Result<OperationTargetResult, OperationError> {
        let operation = record.decode::<T>()?;
        self.handler
            .validate(&operation, origin)
            .map_err(OperationError::Validation)?;
        let result = self
            .handler
            .apply_atomically(&operation, origin)
            .map_err(OperationError::Apply)?;
        let inverse = result
            .inverse
            .as_ref()
            .map(|inverse| OperationRecord::from_operation(self.target_id.clone(), inverse))
            .transpose()?;
        Ok(OperationTargetResult {
            status: result.status,
            inverse,
            affected_ids: result.affected_ids,
            before_revision: result.before_revision,
            after_revision: result.after_revision,
        })
    }
}

/// The handles that crossed as JSON and wait to be taken, by number.
static HANDLES: Mutex<BTreeMap<u64, Arc<dyn OperationTarget>>> = Mutex::new(BTreeMap::new());

/// The next handle's number.
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// A target's handler as it reaches an operation service: a Rust object, which a TS
/// implementation of the service calls through its class handle (`OperationTarget` in
/// `rayx_services_operations`), never a closure.
///
/// It crosses to TS as `{ "handle": n }`: serializing it keeps the handler under `n` until the
/// handle is taken back (deserialized, or `OperationTarget.of(handle)` in TS), once, or until
/// this handle and its clones are dropped, whichever comes first. A contract's TS adapter keeps
/// its arguments until the call answers, so a handler TS never took (its `registerTarget`
/// threw, the engine is gone) is released with the call.
#[derive(Clone)]
pub struct OperationTargetHandle {
    target: Arc<dyn OperationTarget>,
    crossings: Arc<Crossings>,
}

/// The numbers a handle (and its clones) crossed as, released from [`HANDLES`] when the last of
/// them is dropped (a number taken already is gone from there).
#[derive(Default)]
struct Crossings(Mutex<Vec<u64>>);

impl Drop for Crossings {
    fn drop(&mut self) {
        let numbers = std::mem::take(self.0.get_mut().unwrap_or_else(PoisonError::into_inner));
        if numbers.is_empty() {
            return;
        }
        let mut handles = HANDLES.lock().unwrap_or_else(PoisonError::into_inner);
        for number in numbers {
            handles.remove(&number);
        }
    }
}

impl OperationTargetHandle {
    /// The handle of `target`.
    pub fn new(target: Arc<dyn OperationTarget>) -> Self {
        Self {
            target,
            crossings: Arc::default(),
        }
    }

    /// The handle of `handler`, a handler of `T`'s operations for `target_id`.
    pub fn of<T, H>(target_id: OperationTargetId, handler: Arc<H>) -> Self
    where
        T: DomainOperation,
        H: OperationTargetHandler<T>,
    {
        Self::new(Arc::new(TypedOperationTarget::<T, H> {
            target_id,
            handler,
            _operation: PhantomData,
        }))
    }

    /// The handler.
    pub fn target(&self) -> &Arc<dyn OperationTarget> {
        &self.target
    }

    /// Applies `record` from `origin` through the handler.
    pub fn apply(
        &self,
        record: &OperationRecord,
        origin: &OperationOrigin,
    ) -> Result<OperationTargetResult, OperationError> {
        self.target.apply(record, origin)
    }

    /// The handle that crossed as `number`, taken (a handle is taken once, while the handle
    /// that crossed lives).
    pub fn take(number: u64) -> Option<Self> {
        HANDLES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&number)
            .map(Self::new)
    }
}

impl fmt::Debug for OperationTargetHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationTargetHandle")
            .finish_non_exhaustive()
    }
}

/// A handle as JSON.
#[derive(Serialize, Deserialize)]
struct HandleJson {
    handle: u64,
}

impl Serialize for OperationTargetHandle {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let number = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
        HANDLES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(number, Arc::clone(&self.target));
        self.crossings
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(number);
        HandleJson { handle: number }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OperationTargetHandle {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let HandleJson { handle } = HandleJson::deserialize(deserializer)?;
        Self::take(handle).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "the operation target handle {handle} was taken already, or never crossed"
            ))
        })
    }
}
