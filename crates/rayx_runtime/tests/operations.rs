//! The operation service: typed targets, transactions, receipts and their events.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod service_contracts {
    use std::sync::{Arc, Mutex};

    use rayx_runtime::services::operations::{
        DomainOperation, OperationApplyResult, OperationApplyStatus, OperationEvent,
        OperationOrigin, OperationService, OperationTargetHandler, OperationTargetId,
        OperationTransactionMode,
    };
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct SetValue {
        value: i64,
    }

    impl DomainOperation for SetValue {
        const KIND: &'static str = "test.set-value";
        const VERSION: u32 = 1;
    }

    #[derive(Default)]
    struct ValueTarget {
        value: Mutex<i64>,
    }

    impl OperationTargetHandler<SetValue> for ValueTarget {
        fn validate(&self, operation: &SetValue, _origin: &OperationOrigin) -> Result<(), String> {
            if operation.value < 0 {
                return Err("negative_value".to_string());
            }
            Ok(())
        }

        fn apply_atomically(
            &self,
            operation: &SetValue,
            _origin: &OperationOrigin,
        ) -> Result<OperationApplyResult<SetValue>, String> {
            let mut value = self.value.lock().unwrap();
            let previous = *value;
            if previous == operation.value {
                return Ok(OperationApplyResult::no_op(previous as u64));
            }
            *value = operation.value;
            Ok(OperationApplyResult::applied(
                SetValue { value: previous },
                ["property.value"],
                previous as u64,
                operation.value as u64,
            ))
        }
    }

    #[test]
    fn released_targets_and_redundant_operations_are_explicit() {
        let service = OperationService::new();
        let target = Arc::new(ValueTarget::default());
        let target_id = OperationTargetId::new("property-grid:released").unwrap();
        service.register_target(target_id.clone(), target).unwrap();

        let no_op = service
            .submit(&target_id, SetValue { value: 0 }, OperationOrigin::Import)
            .unwrap();
        assert_eq!(no_op.status, OperationApplyStatus::NoOp);
        assert!(no_op.inverse.is_none());

        assert!(service.release_target(&target_id).unwrap());
        assert!(matches!(
            service.submit(&target_id, SetValue { value: 1 }, OperationOrigin::Import),
            Err(rayx_runtime::services::operations::OperationError::TargetReleased(_))
        ));
    }

    #[test]
    fn transactions_commit_as_a_group_and_abort_with_semantic_inverses() {
        let service = OperationService::new();
        let target = Arc::new(ValueTarget::default());
        let target_id = OperationTargetId::new("property-grid:transactional").unwrap();
        service
            .register_target(target_id.clone(), target.clone())
            .unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let _subscription = service.subscribe({
            let events = events.clone();
            move |event| events.lock().unwrap().push(event.clone())
        });

        let transaction = service
            .begin_transaction(OperationTransactionMode::New)
            .unwrap();
        let first = service
            .submit(
                &target_id,
                SetValue { value: 3 },
                OperationOrigin::LocalUser,
            )
            .unwrap();
        let joined = service
            .begin_transaction(OperationTransactionMode::Join)
            .unwrap();
        assert_eq!(joined.id(), transaction.id());
        assert!(joined.is_joined());
        joined.commit().unwrap();
        assert!(events.lock().unwrap().is_empty());
        transaction.commit().unwrap();
        assert!(
            matches!(events.lock().unwrap().as_slice(), [OperationEvent::Committed(event)] if event == &first)
        );

        let transaction = service
            .begin_transaction(OperationTransactionMode::New)
            .unwrap();
        service
            .submit(
                &target_id,
                SetValue { value: 9 },
                OperationOrigin::LocalUser,
            )
            .unwrap();
        assert_eq!(*target.value.lock().unwrap(), 9);
        transaction.abort().unwrap();
        assert_eq!(*target.value.lock().unwrap(), 3);
        assert_eq!(events.lock().unwrap().len(), 1);

        let outer = service
            .begin_transaction(OperationTransactionMode::New)
            .unwrap();
        assert!(
            service
                .begin_transaction(OperationTransactionMode::Reject)
                .is_err()
        );
        outer.abort().unwrap();
    }

    #[test]
    fn validation_failure_preserves_target_and_observer_panics_do_not_falsify_commit() {
        let service = OperationService::new();
        let target = Arc::new(ValueTarget::default());
        let target_id = OperationTargetId::new("property-grid:failure").unwrap();
        service
            .register_target(target_id.clone(), target.clone())
            .unwrap();
        let _panicking_subscriber = service.subscribe(|_| panic!("observer failure")).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let _subscription = service.subscribe({
            let events = events.clone();
            move |event| events.lock().unwrap().push(event.clone())
        });

        assert!(
            service
                .submit(&target_id, SetValue { value: -1 }, OperationOrigin::System)
                .is_err()
        );
        assert_eq!(*target.value.lock().unwrap(), 0);
        assert!(matches!(
            events.lock().unwrap().as_slice(),
            [OperationEvent::Failed(_)]
        ));

        service
            .submit(&target_id, SetValue { value: 8 }, OperationOrigin::System)
            .unwrap();
        assert_eq!(*target.value.lock().unwrap(), 8);
        assert!(matches!(
            events.lock().unwrap().last(),
            Some(OperationEvent::Committed(_))
        ));
    }

    #[test]
    fn submits_typed_operations_after_target_commit_with_redacted_diagnostics() {
        let service = OperationService::new();
        let target = Arc::new(ValueTarget::default());
        let target_id = OperationTargetId::new("property-grid:sample").unwrap();
        service
            .register_target(target_id.clone(), target.clone())
            .unwrap();

        let events = Arc::new(Mutex::new(Vec::new()));
        let _subscription = service.subscribe({
            let events = events.clone();
            move |event| events.lock().unwrap().push(event.clone())
        });

        let receipt = service
            .submit(
                &target_id,
                SetValue { value: 42 },
                OperationOrigin::LocalUser,
            )
            .unwrap();

        assert_eq!(*target.value.lock().unwrap(), 42);
        assert_eq!(receipt.status, OperationApplyStatus::Applied);
        assert_eq!(receipt.before_revision, 0);
        assert_eq!(receipt.after_revision, 42);
        assert_eq!(
            receipt
                .inverse
                .as_ref()
                .unwrap()
                .decode::<SetValue>()
                .unwrap(),
            SetValue { value: 0 }
        );
        assert!(
            matches!(events.lock().unwrap().as_slice(), [OperationEvent::Committed(event)] if event == &receipt)
        );

        #[cfg(feature = "diagnostics")]
        {
            let diagnostics = service.diagnostics();
            assert_eq!(diagnostics.recent_transactions.len(), 1);
            assert_eq!(diagnostics.recent_transactions[0].kind, "test.set-value");
            assert!(!diagnostics.recent_transactions[0].contains_payload_values());
        }
    }
}
