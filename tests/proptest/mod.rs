// Test code is allowed to panic on failure
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::string_slice
)]

//! Property-based tests for my-operator.
//!
//! Uses proptest to generate random inputs and verify invariants.

use proptest::prelude::*;

use my_operator::controller::state_machine::{ResourceEvent, ResourceStateMachine};
use my_operator::crd::Phase;

/// Strategy for generating valid replica counts.
fn valid_replicas() -> impl Strategy<Value = i32> {
    1..=10i32
}

/// Strategy for generating valid messages.
fn valid_message() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 ]{1,100}".prop_map(|s| s.to_string())
}

/// Strategy for generating random phases.
fn any_phase() -> impl Strategy<Value = Phase> {
    prop_oneof![
        Just(Phase::Pending),
        Just(Phase::Creating),
        Just(Phase::Running),
        Just(Phase::Updating),
        Just(Phase::Degraded),
        Just(Phase::Failed),
        Just(Phase::Deleting),
    ]
}

/// Strategy for generating random events.
fn any_event() -> impl Strategy<Value = ResourceEvent> {
    prop_oneof![
        Just(ResourceEvent::ResourcesApplied),
        Just(ResourceEvent::AllReplicasReady),
        Just(ResourceEvent::ReplicasDegraded),
        Just(ResourceEvent::SpecChanged),
        Just(ResourceEvent::ReconcileError),
        Just(ResourceEvent::DeletionRequested),
        Just(ResourceEvent::RecoveryInitiated),
        Just(ResourceEvent::FullyRecovered),
    ]
}

proptest! {
    /// Property: Replicas must be between 1 and 10.
    #[test]
    fn test_replica_bounds(replicas in valid_replicas()) {
        prop_assert!(replicas >= 1);
        prop_assert!(replicas <= 10);
    }

    /// Property: Messages are non-empty.
    #[test]
    fn test_message_non_empty(message in valid_message()) {
        prop_assert!(!message.is_empty());
    }

    /// Property: State machine transition checks are deterministic.
    /// Same (phase, event) pair always yields the same result.
    #[test]
    fn test_state_transitions_deterministic(
        phase in any_phase(),
        event in any_event()
    ) {
        let sm = ResourceStateMachine::new();
        let result1 = sm.can_transition(&phase, &event);
        let result2 = sm.can_transition(&phase, &event);
        prop_assert_eq!(result1, result2);
    }

    /// Property: Deleting phase cannot transition to anything.
    /// Once in Deleting, no events trigger a transition.
    #[test]
    fn test_deleting_is_terminal(event in any_event()) {
        let sm = ResourceStateMachine::new();
        let can_transition = sm.can_transition(&Phase::Deleting, &event);
        prop_assert!(!can_transition, "Deleting should not transition on {:?}", event);
    }

    /// Property: All phases can transition to Deleting via DeletionRequested.
    #[test]
    fn test_all_can_delete(phase in any_phase()) {
        let sm = ResourceStateMachine::new();
        let can_delete = sm.can_transition(&phase, &ResourceEvent::DeletionRequested);
        if phase == Phase::Deleting {
            // Deleting is terminal, no transitions out
            prop_assert!(!can_delete, "Deleting should not be able to transition");
        } else {
            prop_assert!(can_delete, "Phase {:?} should be able to transition to Deleting", phase);
        }
    }
}

#[cfg(test)]
mod crd_property_tests {
    use super::*;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
    use my_operator::crd::{MyResource, MyResourceSpec};

    /// Strategy for generating valid MyResourceSpec.
    fn valid_spec() -> impl Strategy<Value = MyResourceSpec> {
        (valid_replicas(), valid_message()).prop_map(|(replicas, message)| MyResourceSpec {
            replicas,
            message,
            labels: std::collections::BTreeMap::new(),
        })
    }

    proptest! {
        /// Property: Valid specs can be serialized and deserialized.
        #[test]
        fn test_spec_roundtrip(spec in valid_spec()) {
            let json = serde_json::to_string(&spec).expect("Serialization should succeed");
            let parsed: MyResourceSpec = serde_json::from_str(&json).expect("Deserialization should succeed");
            prop_assert_eq!(spec.replicas, parsed.replicas);
            prop_assert_eq!(spec.message, parsed.message);
        }

        /// Property: MyResource with valid spec is valid.
        #[test]
        fn test_resource_with_valid_spec(spec in valid_spec()) {
            let resource = MyResource {
                metadata: ObjectMeta {
                    name: Some("test".to_string()),
                    namespace: Some("default".to_string()),
                    ..Default::default()
                },
                spec,
                status: None,
            };

            prop_assert!(resource.metadata.name.is_some());
            prop_assert!(resource.spec.replicas >= 1);
            prop_assert!(resource.spec.replicas <= 10);
        }
    }
}
