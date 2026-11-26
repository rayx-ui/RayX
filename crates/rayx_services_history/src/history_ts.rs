//! The history for TSX.

use std::sync::Arc;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::ChangeSubscription;
use rayx_app::services::history::{HistoryError, HistoryScopeId, HistoryService as History};
use rayx_app::services::operations::OperationTargetId;
use rayx_app_nsh::ts::{Listeners, ServiceClass, json};

/// The app's history, for TSX: `services.getRequiredService(HistoryService)`, then
/// `await history.undo("document")`, `redo`, `canUndo(scope)` and `canRedo(scope)` (at once),
/// `bindTargetScope(target, scope)` to record a target's operations in a scope, and `onChange`
/// to hear every step.
///
/// It wraps whichever `dyn HistoryService` is registered (RayX's, or a TS implementation of the
/// contract `HistoryServiceContract` registered under this class); its commands return promises
/// that resolve once the history answered and reject with its error.
#[class]
pub struct HistoryService {
    service: Arc<dyn History>,
    listeners: Listeners,
}

impl ServiceClass for HistoryService {
    type Service = dyn History;

    fn wrap(service: Arc<dyn History>) -> Self {
        Self {
            service,
            listeners: Listeners::default(),
        }
    }
}

/// The history `this` wraps.
fn service(this: &Instance<HistoryService>) -> Arc<dyn History> {
    this.with(|this| Arc::clone(&this.service))
}

/// The history's error, for TS.
fn message(error: HistoryError) -> String {
    error.to_string()
}

/// `id` as a scope's id.
fn scope(id: String) -> Result<HistoryScopeId, String> {
    HistoryScopeId::new(id).map_err(message)
}

#[class]
impl HistoryService {
    /// The contract this class is the token of (TS types only: `null` at run time).
    #[getter]
    #[ts("(): HistoryServiceContract | null")]
    pub fn contract(&self) -> Option<bool> {
        None
    }

    /// Whether the scope `scopeId` has an entry to undo.
    pub fn can_undo(&self, scope_id: String) -> Result<bool, String> {
        Ok(self.service.can_undo(&scope(scope_id)?))
    }

    /// Whether the scope `scopeId` has an entry to redo.
    pub fn can_redo(&self, scope_id: String) -> Result<bool, String> {
        Ok(self.service.can_redo(&scope(scope_id)?))
    }

    /// The scope's undo and redo depths: `[undo, redo]`.
    pub fn depths(&self, scope_id: String) -> Result<Vec<u32>, String> {
        let (undo, redo) = self.service.stack_depths(&scope(scope_id)?);
        Ok(vec![
            u32::try_from(undo).unwrap_or(u32::MAX),
            u32::try_from(redo).unwrap_or(u32::MAX),
        ])
    }

    /// Label key of the next entry to undo in `scopeId`, or `null` when empty.
    pub fn undo_label(&self, scope_id: String) -> Result<Option<String>, String> {
        Ok(self.service.undo_label(&scope(scope_id)?))
    }

    /// Label key of the next entry to redo in `scopeId`, or `null` when empty.
    pub fn redo_label(&self, scope_id: String) -> Result<Option<String>, String> {
        Ok(self.service.redo_label(&scope(scope_id)?))
    }

    /// Records the operations of the target `targetId` in the scope `scopeId` from now on, and
    /// loads the scope's history.
    pub async fn bind_target_scope(
        this: Instance<Self>,
        target_id: String,
        scope_id: String,
    ) -> Result<(), String> {
        let target_id = OperationTargetId::new(target_id).map_err(|error| error.to_string())?;
        service(&this)
            .bind_target_scope(target_id, scope(scope_id)?)
            .await
            .map_err(message)
    }

    /// Undoes the scope's latest entry; whether there was one.
    pub async fn undo(this: Instance<Self>, scope_id: String) -> Result<bool, String> {
        service(&this).undo(scope(scope_id)?).await.map_err(message)
    }

    /// Redoes the scope's latest undone entry; whether there was one.
    pub async fn redo(this: Instance<Self>, scope_id: String) -> Result<bool, String> {
        service(&this).redo(scope(scope_id)?).await.map_err(message)
    }

    /// Clears both of the scope's stacks.
    pub async fn clear_scope(this: Instance<Self>, scope_id: String) -> Result<(), String> {
        service(&this)
            .clear_scope(scope(scope_id)?)
            .await
            .map_err(message)
    }

    /// Bounds the scope's entries to `limit` (at least 1), the oldest going first.
    pub async fn set_scope_entry_limit(
        this: Instance<Self>,
        scope_id: String,
        limit: u32,
    ) -> Result<(), String> {
        service(&this)
            .set_scope_entry_limit(scope(scope_id)?, limit as usize)
            .await
            .map_err(message)
    }

    /// The records the scope keeps: newest undo entries first, then newest redo entries.
    #[ts("(scopeId: string): OperationRecord[]")]
    pub async fn retained_records(
        this: Instance<Self>,
        scope_id: String,
    ) -> Result<JsonValue, String> {
        service(&this)
            .retained_records(scope(scope_id)?)
            .await
            .map(|records| json(&records))
            .map_err(message)
    }

    /// Calls `listener` with every history event (`{ Recorded: entry }`, `{ UndoCompleted:
    /// scope }`, …), until the function it returns is called (or the reactive owner of the
    /// call goes), or this object is disposed.
    #[ts("(listener: (event: HistoryEvent) => void): () => void")]
    pub fn on_change(
        &mut self,
        #[args(event)] listener: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        let service = Arc::clone(&self.service);
        self.listeners.listen(listener, move |post| {
            Ok::<ChangeSubscription, String>(
                service.events().subscribe(move |event| post(json(event))),
            )
        })
    }
}

impl HistoryService {
    /// The history, for Rust bridges that take an `Instance<HistoryService>`.
    pub fn service(&self) -> &Arc<dyn History> {
        &self.service
    }
}
