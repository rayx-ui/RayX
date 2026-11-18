//! RayX's operation service: typed targets, transactions, receipts and their events, its
//! registration, and a target's handler crossing as a handle.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::future::Future;

use futures::FutureExt as _;

/// What `call` answered: RayX's operation service answers at once.
fn now<F: Future>(call: F) -> F::Output {
    call.now_or_never().expect("answers at once")
}

mod service_contracts {
    use std::sync::{Arc, Mutex};

    use rayx_app::services::operations::{
        DomainOperation, OperationApplyResult, OperationApplyStatus, OperationEvent,
        OperationOrigin, OperationTargetHandler, OperationTargetId, OperationTransactionMode,
    };
    use rayx_services_operations::testing::operations;
    use serde::{Deserialize, Serialize};

    use super::now;

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
        let service = operations();
        let target = Arc::new(ValueTarget::default());
        let target_id = OperationTargetId::new("property-grid:released").unwrap();
        now(service.register_handler(target_id.clone(), target)).unwrap();

        let no_op = now(service.submit(&target_id, SetValue { value: 0 }, OperationOrigin::Import))
            .unwrap();
        assert_eq!(no_op.status, OperationApplyStatus::NoOp);
        assert!(no_op.inverse.is_none());

        assert!(now(service.release_target(target_id.clone())).unwrap());
        assert!(matches!(
            now(service.submit(&target_id, SetValue { value: 1 }, OperationOrigin::Import)),
            Err(rayx_app::services::operations::OperationError::TargetReleased(_))
        ));
    }

    #[test]
    fn transactions_commit_as_a_group_and_abort_with_semantic_inverses() {
        let service = operations();
        let target = Arc::new(ValueTarget::default());
        let target_id = OperationTargetId::new("property-grid:transactional").unwrap();
        now(service.register_handler(target_id.clone(), target.clone())).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let _subscription = service.subscribe({
            let events = events.clone();
            move |event| events.lock().unwrap().push(event.clone())
        });

        let transaction = now(service.begin_transaction(OperationTransactionMode::New)).unwrap();
        let first = transaction.enter(|| {
            let first = now(service.submit(
                &target_id,
                SetValue { value: 3 },
                OperationOrigin::LocalUser,
            ))
            .unwrap();
            let joined = now(service.begin_transaction(OperationTransactionMode::Join)).unwrap();
            assert_eq!(joined.id(), transaction.id());
            assert!(joined.is_joined());
            now(joined.commit()).unwrap();
            first
        });
        assert!(events.lock().unwrap().is_empty());
        now(transaction.commit()).unwrap();
        assert!(
            matches!(events.lock().unwrap().as_slice(), [OperationEvent::Committed(event)] if event == &first)
        );

        let transaction = now(service.begin_transaction(OperationTransactionMode::New)).unwrap();
        now(transaction.submit(
            &target_id,
            SetValue { value: 9 },
            OperationOrigin::LocalUser,
        ))
        .unwrap();
        assert_eq!(*target.value.lock().unwrap(), 9);
        now(transaction.abort()).unwrap();
        assert_eq!(*target.value.lock().unwrap(), 3);
        assert_eq!(events.lock().unwrap().len(), 1);

        let outer = now(service.begin_transaction(OperationTransactionMode::New)).unwrap();
        outer.enter(|| {
            assert!(now(service.begin_transaction(OperationTransactionMode::Reject)).is_err());
        });
        now(outer.abort()).unwrap();
    }

    #[test]
    fn validation_failure_preserves_target_and_observer_panics_do_not_falsify_commit() {
        let service = operations();
        let target = Arc::new(ValueTarget::default());
        let target_id = OperationTargetId::new("property-grid:failure").unwrap();
        now(service.register_handler(target_id.clone(), target.clone())).unwrap();
        let _panicking_subscriber = service.subscribe(|_| panic!("observer failure"));
        let events = Arc::new(Mutex::new(Vec::new()));
        let _subscription = service.subscribe({
            let events = events.clone();
            move |event| events.lock().unwrap().push(event.clone())
        });

        assert!(
            now(service.submit(&target_id, SetValue { value: -1 }, OperationOrigin::System))
                .is_err()
        );
        assert_eq!(*target.value.lock().unwrap(), 0);
        assert!(matches!(
            events.lock().unwrap().as_slice(),
            [OperationEvent::Failed(_)]
        ));

        now(service.submit(&target_id, SetValue { value: 8 }, OperationOrigin::System)).unwrap();
        assert_eq!(*target.value.lock().unwrap(), 8);
        assert!(matches!(
            events.lock().unwrap().last(),
            Some(OperationEvent::Committed(_))
        ));
    }

    #[test]
    fn submits_typed_operations_after_target_commit_with_redacted_diagnostics() {
        let service = operations();
        let target = Arc::new(ValueTarget::default());
        let target_id = OperationTargetId::new("property-grid:sample").unwrap();
        now(service.register_handler(target_id.clone(), target.clone())).unwrap();

        let events = Arc::new(Mutex::new(Vec::new()));
        let _subscription = service.subscribe({
            let events = events.clone();
            move |event| events.lock().unwrap().push(event.clone())
        });

        let receipt = now(service.submit(
            &target_id,
            SetValue { value: 42 },
            OperationOrigin::LocalUser,
        ))
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

mod registration {
    use std::sync::Arc;

    use rayx_app::services::operations::OperationService;
    use rayx_app::testing::FakeOperations;
    use rayx_app::{AppBuilder as _, RayXAppBuilder};
    use rayx_services_operations::OperationsAppBuilderExt as _;

    #[test]
    fn add_operations_registers_rayx_s_service_as_the_trait() {
        let services = RayXAppBuilder::new().add_operations().build().unwrap();
        let first = services.required_service::<dyn OperationService>().unwrap();
        let again = services.required_service::<dyn OperationService>().unwrap();
        assert!(Arc::ptr_eq(&first, &again), "a singleton");
        assert!(first.targets().is_empty());
    }

    #[test]
    fn the_later_registration_replaces_the_earlier() {
        let fake = FakeOperations::new();
        let fake_id = fake.instance_id();

        // RayX's, then the app's own: the app's.
        let mut builder = RayXAppBuilder::new().add_operations();
        builder
            .services_mut()
            .replace_service::<dyn OperationService>(
                Arc::clone(&fake) as Arc<dyn OperationService>
            );
        let services = builder.build().unwrap();
        let resolved = services.required_service::<dyn OperationService>().unwrap();
        assert_eq!(resolved.instance_id(), fake_id);

        // The app's own, then RayX's: RayX's.
        let mut builder = RayXAppBuilder::new();
        builder
            .services_mut()
            .add_singleton_as::<dyn OperationService>(fake as Arc<dyn OperationService>);
        let services = builder.add_operations().build().unwrap();
        let resolved = services.required_service::<dyn OperationService>().unwrap();
        assert_ne!(resolved.instance_id(), fake_id);
    }

    #[test]
    fn each_service_has_an_instance_of_its_own() {
        let first = rayx_services_operations::testing::operations();
        let second = rayx_services_operations::testing::operations();
        assert_ne!(first.instance_id(), second.instance_id());
    }
}

mod published_state {
    use std::sync::{Arc, Mutex};

    use rayx_app::services::operations::{
        DomainOperation, OperationApplyResult, OperationOrigin, OperationTargetHandler,
        OperationTargetId,
    };
    use rayx_services_operations::testing::operations;
    use serde::{Deserialize, Serialize};

    use super::now;

    #[derive(Clone, Deserialize, Serialize)]
    struct Touch;

    impl DomainOperation for Touch {
        const KIND: &'static str = "test.touch";
        const VERSION: u32 = 1;
    }

    struct Target;

    impl OperationTargetHandler<Touch> for Target {
        fn apply_atomically(
            &self,
            _operation: &Touch,
            _origin: &OperationOrigin,
        ) -> Result<OperationApplyResult<Touch>, String> {
            Ok(OperationApplyResult::no_op(0))
        }
    }

    #[test]
    fn the_state_is_the_registered_targets_and_changes_only_with_them() {
        let service = operations();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let _subscription = service.operations().subscribe({
            let heard = Arc::clone(&heard);
            move |state| heard.lock().unwrap().push(state.targets.len())
        });
        let first = OperationTargetId::new("state:b").unwrap();
        let second = OperationTargetId::new("state:a").unwrap();
        now(service.register_handler(first.clone(), Arc::new(Target))).unwrap();
        now(service.register_handler(second.clone(), Arc::new(Target))).unwrap();
        assert_eq!(service.targets(), [second.clone(), first.clone()], "sorted");
        assert!(service.has_target(&first));

        // A submission and a release of an unknown target change no target.
        now(service.submit(&first, Touch, OperationOrigin::System)).unwrap();
        let unknown = OperationTargetId::new("state:unknown").unwrap();
        assert!(!now(service.release_target(unknown)).unwrap());
        // (The diagnostics publish each submission too, with the same targets.)
        #[cfg(not(feature = "diagnostics"))]
        assert_eq!(*heard.lock().unwrap(), [1, 2]);

        assert!(now(service.release_target(first.clone())).unwrap());
        assert_eq!(service.targets(), [second]);
        assert!(!service.has_target(&first));
        let mut heard = heard.lock().unwrap().clone();
        #[cfg(not(feature = "diagnostics"))]
        assert_eq!(heard, [1, 2, 1]);
        heard.dedup();
        assert_eq!(heard, [1, 2, 1]);
    }
}

mod target_handles {
    use std::sync::{Arc, Mutex};

    use rayx_app::services::operations::{
        DomainOperation, OperationApplyResult, OperationApplyStatus, OperationOrigin,
        OperationRecord, OperationTargetHandle, OperationTargetHandler, OperationTargetId,
    };
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct Set(u32);

    impl DomainOperation for Set {
        const KIND: &'static str = "test.handle.set";
        const VERSION: u32 = 1;
    }

    #[derive(Default)]
    struct Target(Mutex<u32>);

    impl OperationTargetHandler<Set> for Target {
        fn apply_atomically(
            &self,
            operation: &Set,
            _origin: &OperationOrigin,
        ) -> Result<OperationApplyResult<Set>, String> {
            let mut value = self.0.lock().unwrap();
            let before = std::mem::replace(&mut *value, operation.0);
            Ok(OperationApplyResult::applied(
                Set(before),
                ["value"],
                u64::from(before),
                u64::from(operation.0),
            ))
        }
    }

    #[test]
    fn a_handle_crosses_as_a_number_and_is_taken_once() {
        let target = Arc::new(Target::default());
        let target_id = OperationTargetId::new("handle:target").unwrap();
        let handle = OperationTargetHandle::of::<Set, _>(target_id.clone(), Arc::clone(&target));
        let json = serde_json::to_value(&handle).unwrap();
        let number = json["handle"].as_u64().expect("a number");

        let taken: OperationTargetHandle = serde_json::from_value(json.clone()).unwrap();
        let record = OperationRecord::from_operation(target_id.clone(), &Set(4)).unwrap();
        let result = taken.apply(&record, &OperationOrigin::LocalUser).unwrap();
        assert_eq!(result.status, OperationApplyStatus::Applied);
        assert_eq!(*target.0.lock().unwrap(), 4);
        assert_eq!(
            result.inverse.unwrap().decode::<Set>().unwrap(),
            Set(0),
            "the inverse, as a record"
        );

        assert!(serde_json::from_value::<OperationTargetHandle>(json).is_err());
        assert!(OperationTargetHandle::take(number).is_none());
    }
}
