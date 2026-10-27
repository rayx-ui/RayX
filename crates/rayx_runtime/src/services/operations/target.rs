use std::sync::Arc;

use super::{
    DomainOperation, OperationApplyResult, OperationError, OperationOrigin, OperationRecord,
    OperationTargetId,
};

pub trait OperationTargetHandler<T: DomainOperation>: Send + Sync + 'static {
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

pub(crate) trait ErasedOperationTarget: Send + Sync {
    fn apply(
        &self,
        record: &OperationRecord,
        origin: &OperationOrigin,
    ) -> Result<ErasedApplyResult, OperationError>;
}

pub(crate) struct ErasedApplyResult {
    pub status: super::OperationApplyStatus,
    pub inverse: Option<OperationRecord>,
    pub affected_ids: Vec<String>,
    pub before_revision: u64,
    pub after_revision: u64,
}

pub(crate) struct TypedOperationTarget<T, H> {
    target_id: OperationTargetId,
    handler: Arc<H>,
    _operation: std::marker::PhantomData<fn() -> T>,
}

impl<T, H> TypedOperationTarget<T, H>
where
    T: DomainOperation,
    H: OperationTargetHandler<T>,
{
    pub fn new(target_id: OperationTargetId, handler: Arc<H>) -> Self {
        Self {
            target_id,
            handler,
            _operation: std::marker::PhantomData,
        }
    }
}

impl<T, H> ErasedOperationTarget for TypedOperationTarget<T, H>
where
    T: DomainOperation,
    H: OperationTargetHandler<T>,
{
    fn apply(
        &self,
        record: &OperationRecord,
        origin: &OperationOrigin,
    ) -> Result<ErasedApplyResult, OperationError> {
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
        Ok(ErasedApplyResult {
            status: result.status,
            inverse,
            affected_ids: result.affected_ids,
            before_revision: result.before_revision,
            after_revision: result.after_revision,
        })
    }
}
