mod app_ext;
mod error;
mod origin;
mod receipt;
mod record;
mod service;
mod target;
mod transaction;

pub use error::OperationError;
pub use origin::OperationOrigin;
pub use receipt::{
    OperationApplyResult, OperationApplyStatus, OperationEvent, OperationFailure, OperationReceipt,
};
pub use record::{
    DomainOperation, MAX_OPERATION_PAYLOAD_BYTES, OperationRecord, OperationTargetId,
};
#[cfg(feature = "diagnostics")]
pub use service::{OperationDiagnostics, OperationTransactionDiagnostics};
pub use service::{OperationService, OperationSubscription};
pub use target::OperationTargetHandler;
pub use transaction::{OperationTransaction, OperationTransactionMode};

pub use app_ext::OperationsAppBuilderExt;
use target::{ErasedOperationTarget, TypedOperationTarget};
use transaction::current_transaction_id;
