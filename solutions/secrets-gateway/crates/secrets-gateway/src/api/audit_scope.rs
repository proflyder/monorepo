use axum::response::Response;
use home_app_auth::Principal;

use super::GatewayState;
use super::reply::{Rejection, openbao_error};
use crate::audit::{Action, Event};
use crate::openbao;

#[derive(Clone, Copy)]
pub(super) struct AuditScope<'a> {
    state: &'a GatewayState,
    principal: &'a str,
    action: Action,
    path: Option<&'a str>,
    key: Option<&'a str>,
}

impl<'a> AuditScope<'a> {
    pub(super) fn new(state: &'a GatewayState, principal: &'a Principal, action: Action) -> Self {
        Self {
            state,
            principal: &principal.id,
            action,
            path: None,
            key: None,
        }
    }

    pub(super) const fn principal(&self) -> &'a str {
        self.principal
    }

    pub(super) const fn at(self, path: &'a str) -> Self {
        Self {
            path: Some(path),
            ..self
        }
    }

    pub(super) const fn with_key(self, key: Option<&'a str>) -> Self {
        Self { key, ..self }
    }

    pub(super) fn record(&self, version: Option<u64>, outcome: &str) {
        self.record_detail(version, outcome, None);
    }

    pub(super) fn record_detail(&self, version: Option<u64>, outcome: &str, detail: Option<&str>) {
        self.state.audit.record(&Event {
            action: self.action,
            principal: self.principal,
            path: self.path,
            key: self.key,
            version,
            outcome,
            detail,
        });
    }

    pub(super) fn reject(
        &self,
        version: Option<u64>,
        outcome: &str,
        response: Response,
    ) -> Rejection {
        self.record(version, outcome);
        response.into()
    }

    pub(super) fn openbao(&self, version: Option<u64>, failure: openbao::Error) -> Rejection {
        self.reject(version, "error", openbao_error(failure))
    }
}
