//! Initial policy gateway for the fixed Relay protocol surface.
//!
//! This classifies typed Relay actions only. It does not authorize arbitrary processes,
//! shell commands, file writes, MCP calls, or privileged execution.

use crate::protocol::{Action, Request};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationClass {
    ReadOnly,
    Planning,
    RelayStateMutation,
    ManagedSystemChange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operation {
    pub id: &'static str,
    pub class: OperationClass,
    pub requires_confirmation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorizationError {
    ConfirmationRequired,
}

pub fn classify(request: &Request) -> Operation {
    let (class, requires_confirmation) = match request.action {
        Action::Status
        | Action::Health
        | Action::Units
        | Action::Generations
        | Action::Diagnose
        | Action::SearchOption
        | Action::SearchPackage
        | Action::Show
        | Action::UndoPreview
        | Action::RecoverPreview => (OperationClass::ReadOnly, false),
        Action::Plan => (OperationClass::Planning, false),
        Action::Discard => (OperationClass::RelayStateMutation, false),
        Action::Apply | Action::Undo | Action::Recover => {
            (OperationClass::ManagedSystemChange, true)
        }
    };
    Operation {
        id: request.action.as_str(),
        class,
        requires_confirmation,
    }
}

pub fn authorize(request: &Request) -> Result<Operation, AuthorizationError> {
    let operation = classify(request);
    if operation.requires_confirmation && !request.confirmed {
        return Err(AuthorizationError::ConfirmationRequired);
    }
    Ok(operation)
}

#[cfg(test)]
mod tests {
    use super::{AuthorizationError, OperationClass, authorize, classify};
    use crate::protocol::{Action, Request};

    fn request(action: Action, confirmed: bool) -> Request {
        Request {
            id: "gateway-test".into(),
            action,
            root: None,
            flake: None,
            host: None,
            state_dir: None,
            change_id: None,
            intent_json: None,
            confirmed,
            abort_pending: false,
            no_desktop_check: false,
            observe_seconds: None,
            expect_active: Vec::new(),
            expected_ids: Vec::new(),
            unit_filter: String::new(),
            unit_limit: 20,
            diagnostic_topic: None,
            diagnostic_unit: None,
            query: None,
        }
    }

    #[test]
    fn every_protocol_action_has_an_explicit_operation_class() {
        for action in [
            Action::Status,
            Action::Health,
            Action::Units,
            Action::Generations,
            Action::Diagnose,
            Action::SearchOption,
            Action::SearchPackage,
            Action::Show,
            Action::UndoPreview,
            Action::RecoverPreview,
        ] {
            assert_eq!(
                classify(&request(action, false)).class,
                OperationClass::ReadOnly
            );
        }
        assert_eq!(
            classify(&request(Action::Plan, false)).class,
            OperationClass::Planning
        );
        assert_eq!(
            classify(&request(Action::Discard, false)).class,
            OperationClass::RelayStateMutation
        );
        for action in [Action::Apply, Action::Undo, Action::Recover] {
            assert_eq!(
                classify(&request(action, false)).class,
                OperationClass::ManagedSystemChange
            );
            assert_eq!(
                authorize(&request(action, false)),
                Err(AuthorizationError::ConfirmationRequired)
            );
            assert!(authorize(&request(action, true)).is_ok());
        }
    }
}
