//! A target's handler for TSX: wraps either a Rust handler (via `OperationTarget.of(handle)` or
//! `OperationTarget::new(handle)`) or a JS callback handler (via `OperationTarget.fromHandler`).

use std::sync::{Arc, mpsc};

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use rayx_app::di::ThreadBound;
use rayx_app::services::operations::{
    OperationError, OperationMetadata, OperationOrigin, OperationRecord,
    OperationTarget as OperationTargetTrait, OperationTargetHandle, OperationTargetId,
    OperationTargetResult,
};
use rayx_app_nsh::EngineRunner;
use rayx_app_nsh::ts::json;
use serde_json::from_value;

type TargetCallback = Callback<(JsonValue, JsonValue), JsonValue>;

/// Normalizes a JS `OperationRecord` object, allowing either `payload: number[]` or
/// `payloadJson` / `payload_json` as a plain JSON value.
pub(crate) fn normalize_record_json(mut value: JsonValue) -> Result<JsonValue, String> {
    if let JsonValue::Object(ref mut map) = value {
        let missing_payload = matches!(map.get("payload"), None | Some(JsonValue::Null));
        if missing_payload
            && let Some(payload_json) = map
                .remove("payloadJson")
                .or_else(|| map.remove("payload_json"))
        {
            let bytes = serde_json::to_vec(&payload_json)
                .map_err(|error| format!("the payload does not encode: {error}"))?;
            let byte_array = bytes.into_iter().map(JsonValue::from).collect();
            map.insert("payload".to_owned(), JsonValue::Array(byte_array));
        }
    }
    Ok(value)
}

/// Converts a JS `OperationRecord` (accepting either `payload: number[]` or `payloadJson`) into
/// Rust's [`OperationRecord`].
pub(crate) fn read_record(value: JsonValue) -> Result<OperationRecord, String> {
    let normalized = normalize_record_json(value)?;
    from_value(normalized).map_err(|error| format!("the record does not convert: {error}"))
}

/// Decodes a JS `OperationRecord`'s payload into a JSON value.
pub(crate) fn decode_record_payload(value: JsonValue) -> Result<JsonValue, String> {
    let record = read_record(value)?;
    record.payload_json().map_err(|error| error.to_string())
}

/// Serializes `record` for a JS handler, attaching `payloadJson` alongside `payload` when the
/// payload bytes decode as JSON.
fn record_for_js_handler(record: &OperationRecord) -> JsonValue {
    let mut value = json(record);
    if let (JsonValue::Object(map), Ok(decoded)) = (&mut value, record.payload_json()) {
        map.insert("payloadJson".to_owned(), decoded);
    }
    value
}

/// Parses a JS handler's returned `OperationTargetResult`, normalizing `inverse` so TS handlers
/// may return either `payload: number[]` or `payloadJson`.
fn parse_target_result(mut value: JsonValue) -> Result<OperationTargetResult, OperationError> {
    if let JsonValue::Object(ref mut map) = value
        && let Some(inverse) = map.remove("inverse")
    {
        let normalized = if inverse.is_null() {
            JsonValue::Null
        } else {
            normalize_record_json(inverse).map_err(OperationError::Apply)?
        };
        map.insert("inverse".to_owned(), normalized);
    }
    from_value(value).map_err(|error| {
        OperationError::Apply(format!("the target result does not convert: {error}"))
    })
}

/// An [`OperationTargetTrait`] backed by a JS callback `(record, origin) => OperationTargetResult`.
///
/// The callback lives in [`ThreadBound`] on the engine thread. Calls made on the engine thread
/// invoke the callback synchronously when JS can run; calls from other threads dispatch onto the
/// engine thread through [`EngineRunner`] when available (never blocking the engine thread on
/// itself).
struct TsOperationTarget {
    handler: Arc<ThreadBound<TargetCallback>>,
    runner: Option<EngineRunner>,
}

impl TsOperationTarget {
    fn invoke(
        handler: &ThreadBound<TargetCallback>,
        record: &OperationRecord,
        origin: &OperationOrigin,
    ) -> Result<OperationTargetResult, OperationError> {
        let callback = handler
            .get()
            .map_err(|error| OperationError::Apply(error.to_string()))?;
        if !callback.can_run_now() {
            return Err(OperationError::Apply(
                "operation target handler cannot run now: JS is unavailable on the engine thread"
                    .into(),
            ));
        }
        let raw = callback
            .call((record_for_js_handler(record), json(origin)))
            .map_err(OperationError::Apply)?;
        parse_target_result(raw)
    }
}

impl OperationTargetTrait for TsOperationTarget {
    fn apply(
        &self,
        record: &OperationRecord,
        origin: &OperationOrigin,
    ) -> Result<OperationTargetResult, OperationError> {
        if self.handler.is_owner_thread() {
            return Self::invoke(&self.handler, record, origin);
        }
        let Some(runner) = &self.runner else {
            return Err(OperationError::Apply(
                "operation target handler must be invoked on the engine thread".into(),
            ));
        };
        if runner.is_engine_thread() {
            return Self::invoke(&self.handler, record, origin);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let handler = Arc::clone(&self.handler);
        let record = record.clone();
        let origin = origin.clone();
        runner
            .post(move || {
                let outcome = Self::invoke(&handler, &record, &origin);
                let _ = tx.send(outcome);
            })
            .map_err(|_| {
                OperationError::Apply(
                    "operation target handler could not run: the engine is gone".into(),
                )
            })?;
        rx.recv().map_err(|_| {
            OperationError::Apply(
                "operation target handler did not answer: the engine stopped".into(),
            )
        })?
    }
}

/// A target's handler, a Rust object: what an operation service's `registerTarget` gets. A TS
/// implementation of the service (`OperationServiceContract`) receives it as a handle
/// (`{ handle }`), takes it with `OperationTarget.of(handle)` (once) and applies each record
/// submitted to the target with `apply(record, origin)`. TS code can also construct a target
/// directly from a JS callback with `OperationTarget.fromHandler(handler)`.
#[class]
pub struct OperationTarget {
    handle: OperationTargetHandle,
}

#[class]
impl OperationTarget {
    /// The handler a handle stands for, taken: a handle is taken once.
    #[ts("(handle: OperationTargetHandle): OperationTarget")]
    pub fn of(handle: JsonValue) -> Result<Self, String> {
        from_value::<OperationTargetHandle>(handle)
            .map(OperationTarget::new)
            .map_err(|error| error.to_string())
    }

    /// Creates an operation target backed by a JS handler `(record, origin) => result`.
    ///
    /// `record` includes both `payload` (byte array) and `payloadJson` (decoded JSON), and the
    /// returned `inverse` record may provide either `payload` or `payloadJson`.
    #[ts(
        "(handler: (record: OperationRecord, origin: OperationOrigin) => OperationTargetResult): OperationTarget"
    )]
    pub fn from_handler(
        #[args(record, origin)] handler: Callback<(JsonValue, JsonValue), JsonValue>,
        cx: CallCx,
    ) -> Result<Self, String> {
        let kept = ThreadBound::new(handler, "a TS operation target handler")
            .map_err(|error| error.to_string())?;
        let runner = cx.with_owner(|| {
            rayx_app_nsh::app_services()
                .ok()
                .and_then(|services| services.service::<EngineRunner>().map(|r| (*r).clone()))
        });
        let target = Arc::new(TsOperationTarget {
            handler: Arc::new(kept),
            runner,
        });
        Ok(Self::new(OperationTargetHandle::new(target)))
    }

    /// Decodes an `OperationRecord`'s payload into a plain JSON value.
    #[ts("(record: OperationRecord): unknown")]
    pub fn payload_json(record: JsonValue) -> Result<JsonValue, String> {
        decode_record_payload(record)
    }

    /// Builds an `OperationRecord` from a plain JSON `payload` and optional `metadata`.
    #[ts(
        "(targetId: string, kind: string, version: number, payload: unknown, metadata?: OperationMetadata | null): OperationRecord"
    )]
    pub fn record_from_json(
        target_id: String,
        kind: String,
        version: u32,
        payload: JsonValue,
        metadata: Option<JsonValue>,
    ) -> Result<JsonValue, String> {
        let target_id = OperationTargetId::new(target_id).map_err(|error| error.to_string())?;
        let mut record = OperationRecord::from_json(target_id, kind, version, &payload)
            .map_err(|error| error.to_string())?;
        if let Some(meta_value) = metadata.filter(|value| !value.is_null()) {
            let parsed: OperationMetadata = from_value(meta_value)
                .map_err(|error| format!("the metadata does not convert: {error}"))?;
            record = record.with_metadata(parsed);
        }
        Ok(json(&record))
    }

    /// Applies `record` from `origin` (at once, synchronously); what it did: its status, the
    /// inverse record, the ids it changed and the target's revisions. Throws when the target
    /// refused or failed it, unchanged.
    #[ts("(record: OperationRecord, origin: OperationOrigin): OperationTargetResult")]
    pub fn apply(&self, record: JsonValue, origin: JsonValue) -> Result<JsonValue, String> {
        let record = read_record(record)?;
        let origin: OperationOrigin =
            from_value(origin).map_err(|error| format!("the origin does not convert: {error}"))?;
        self.handle
            .apply(&record, &origin)
            .map(|result: OperationTargetResult| json(&result))
            .map_err(|error| error.to_string())
    }
}

impl OperationTarget {
    /// The class over `handle`, for Rust code handing a target's handler to TS.
    pub fn new(handle: OperationTargetHandle) -> Self {
        Self { handle }
    }

    /// The handler's handle.
    pub fn handle(&self) -> OperationTargetHandle {
        self.handle.clone()
    }
}
