use connectrpc::interceptor::{
    NextStream, StreamRequest, StreamResponse, UnaryRequest, UnaryResponse,
};
use connectrpc::{ConnectError, Interceptor, Next, async_trait};
use home_app_auth::{AccessCache, Forbidden, Module, Principal, bearer};
use home_secret::Token;
use http::StatusCode;
use http::header::AUTHORIZATION;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub struct BearerAuth {
    expected: [u8; 32],
}

impl std::fmt::Debug for BearerAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("BearerAuth").finish_non_exhaustive()
    }
}

impl BearerAuth {
    pub fn new(token: &str) -> Self {
        let digest = Sha256::digest(token.as_bytes());
        let mut expected = [0_u8; 32];
        expected.copy_from_slice(&digest);
        Self { expected }
    }

    pub fn authorized(&self, header: Option<&http::HeaderValue>) -> bool {
        self.matches(header)
    }

    fn matches(&self, header: Option<&http::HeaderValue>) -> bool {
        let Some(header) = header.and_then(|value| value.to_str().ok()) else {
            return false;
        };
        let Some(provided) = bearer(header) else {
            return false;
        };
        let digest = Sha256::digest(provided.as_bytes());
        digest.as_slice().ct_eq(&self.expected).into()
    }
}

#[async_trait]
impl Interceptor for BearerAuth {
    async fn intercept_unary(
        &self,
        request: UnaryRequest,
        next: Next<'_>,
    ) -> Result<UnaryResponse, ConnectError> {
        if !self.matches(request.ctx.header(AUTHORIZATION)) {
            return Err(ConnectError::unauthenticated("unauthenticated"));
        }
        next.run(request).await
    }
}

#[derive(Clone)]
pub struct AccessAuth {
    cache: AccessCache,
    service: Token,
    previous_service: Option<Token>,
}

impl std::fmt::Debug for AccessAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("AccessAuth").finish_non_exhaustive()
    }
}

impl AccessAuth {
    pub fn new(cache: AccessCache, service: Token) -> Self {
        Self {
            cache,
            service,
            previous_service: None,
        }
    }

    #[must_use]
    pub fn with_previous(mut self, previous: Option<Token>) -> Self {
        self.previous_service = previous;
        self
    }

    #[cfg(test)]
    pub fn for_tests(token: &str) -> Self {
        let snapshot = home_app_auth::AccessSnapshot {
            revision: 1,
            tokens: vec![home_app_auth::TokenRecord {
                id: "test".to_owned(),
                name: "test".to_owned(),
                sha256: home_app_auth::sha256_hex(token),
                owner: true,
                modules: home_app_auth::Modules::owner()
                    .ids()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            }],
        };
        Self::new(AccessCache::new(snapshot), Token::new(token))
    }

    pub fn service_authorized(&self, header: Option<&http::HeaderValue>) -> bool {
        let Some(value) = header.and_then(|value| value.to_str().ok()) else {
            return false;
        };
        let Some(candidate) = bearer(value) else {
            return false;
        };
        self.service.ct_eq(candidate)
            || self
                .previous_service
                .as_ref()
                .is_some_and(|previous| previous.ct_eq(candidate))
    }

    pub fn authenticate(&self, header: Option<&http::HeaderValue>) -> Result<Principal, Forbidden> {
        self.cache
            .authenticate(header)
            .ok_or(Forbidden::Unauthenticated)
    }

    pub fn caller(&self, header: Option<&http::HeaderValue>) -> Result<Caller, Forbidden> {
        if self.service_authorized(header) {
            return Ok(Caller::Service);
        }
        self.authenticate(header).map(Caller::App)
    }

    pub fn app(
        &self,
        header: Option<&http::HeaderValue>,
        module: Module,
    ) -> Result<Principal, Forbidden> {
        let principal = self.authenticate(header)?;
        principal.require(module)?;
        Ok(principal)
    }

    pub fn any_app(&self, header: Option<&http::HeaderValue>) -> Result<Principal, Forbidden> {
        self.authenticate(header)
    }

    pub fn require(
        &self,
        header: Option<&http::HeaderValue>,
        module: Module,
    ) -> Result<Caller, Forbidden> {
        if self.service_authorized(header) {
            return Ok(Caller::Service);
        }
        self.app(header, module).map(Caller::App)
    }
}

#[derive(Debug, Clone)]
pub enum Caller {
    Service,
    App(Principal),
}

const RPC_MODULES: &[(&str, Option<Module>)] = &[
    ("home.v1.SchedulerService/GetStatus", Some(Module::Base)),
    ("home.v1.SchedulerService/ListDueEvents", None),
    ("home.v1.SchedulerService/AckEvent", None),
    ("home.v1.SchedulerService/EnqueueTask", Some(Module::Home)),
    ("home.v1.SchedulerService/CancelTask", Some(Module::Home)),
    (
        "home.v1.SchedulerService/CreateSchedule",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/GetSchedule",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/ListSchedules",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/UpdateSchedule",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/DeleteSchedule",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/SetScheduleEnabled",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/SnoozeSchedule",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/StopSchedule",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/SkipScheduleOccurrence",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/StartTimer",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/GetTimer",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/ListTimers",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/PauseTimer",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/ResumeTimer",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.SchedulerService/CancelTimer",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.AlarmService/CreateAlarm",
        Some(Module::AlarmsTimers),
    ),
    ("home.v1.AlarmService/GetAlarm", Some(Module::AlarmsTimers)),
    (
        "home.v1.AlarmService/ListAlarms",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.AlarmService/UpdateAlarm",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.AlarmService/DeleteAlarm",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.AlarmService/SetAlarmEnabled",
        Some(Module::AlarmsTimers),
    ),
    (
        "home.v1.AlarmService/SnoozeAlarm",
        Some(Module::AlarmsTimers),
    ),
    ("home.v1.AlarmService/StopAlarm", Some(Module::AlarmsTimers)),
    (
        "home.v1.ScenarioService/CreateScenario",
        Some(Module::Scenarios),
    ),
    (
        "home.v1.ScenarioService/GetScenario",
        Some(Module::Scenarios),
    ),
    (
        "home.v1.ScenarioService/ListScenarios",
        Some(Module::Scenarios),
    ),
    (
        "home.v1.ScenarioService/UpdateScenario",
        Some(Module::Scenarios),
    ),
    (
        "home.v1.ScenarioService/DeleteScenario",
        Some(Module::Scenarios),
    ),
    (
        "home.v1.ScenarioService/SetScenarioEnabled",
        Some(Module::Scenarios),
    ),
    (
        "home.v1.ScenarioService/RunScenario",
        Some(Module::Scenarios),
    ),
];

fn method_module(path: &str) -> Option<Option<Module>> {
    let path = path.trim_start_matches('/');
    RPC_MODULES
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, module)| *module)
}

fn connect_forbidden(forbidden: Forbidden) -> ConnectError {
    match forbidden {
        Forbidden::Unauthenticated => ConnectError::unauthenticated("unauthenticated"),
        Forbidden::ServiceOnly | Forbidden::Module(_) => {
            ConnectError::permission_denied(forbidden.body())
        }
    }
}

#[async_trait]
impl Interceptor for AccessAuth {
    async fn intercept_streaming(
        &self,
        mut request: StreamRequest,
        inbound: connectrpc::PayloadStream,
        next: NextStream<'_>,
    ) -> Result<StreamResponse, ConnectError> {
        let path = request.ctx.path().unwrap_or_default().to_owned();
        let module = method_module(&path)
            .ok_or_else(|| connect_forbidden(Forbidden::Module(Module::Admin)))?;
        if !self.service_authorized(request.ctx.header(AUTHORIZATION)) {
            let principal = self
                .authenticate(request.ctx.header(AUTHORIZATION))
                .map_err(connect_forbidden)?;
            match module {
                Some(module) => {
                    principal.require(module).map_err(connect_forbidden)?;
                    request.ctx.extensions_mut().insert(principal);
                }
                None => return Err(connect_forbidden(Forbidden::ServiceOnly)),
            }
        }
        next.run(request, inbound).await
    }

    async fn intercept_unary(
        &self,
        mut request: UnaryRequest,
        next: Next<'_>,
    ) -> Result<UnaryResponse, ConnectError> {
        let path = request.ctx.path().unwrap_or_default().to_owned();
        let module = method_module(&path);
        let caller =
            if module.is_none() && self.service_authorized(request.ctx.header(AUTHORIZATION)) {
                Caller::Service
            } else {
                self.caller(request.ctx.header(AUTHORIZATION))
                    .map_err(connect_forbidden)?
            };
        match (&caller, module) {
            (Caller::Service, _) => {}
            (Caller::App(principal), Some(Some(module))) => {
                principal.require(module).map_err(connect_forbidden)?;
                tracing::debug!(
                    token = principal.name.as_str(),
                    modules = ?principal.module_ids(),
                    method = path.as_str(),
                    "application request authorized"
                );
                request.ctx.extensions_mut().insert(principal.clone());
            }
            (Caller::App(principal), Some(None)) => {
                tracing::warn!(
                    token = principal.name.as_str(),
                    method = path.as_str(),
                    "service-only method called by an application token"
                );
                return Err(connect_forbidden(Forbidden::ServiceOnly));
            }
            (_, None) => {
                return Err(connect_forbidden(Forbidden::Module(Module::Admin)));
            }
        }
        next.run(request).await
    }
}

pub fn error_response(
    forbidden: Forbidden,
) -> (
    StatusCode,
    [(http::header::HeaderName, &'static str); 1],
    String,
) {
    let status = match forbidden {
        Forbidden::Unauthenticated => StatusCode::UNAUTHORIZED,
        Forbidden::ServiceOnly | Forbidden::Module(_) => StatusCode::FORBIDDEN,
    };
    (
        status,
        [(http::header::CONTENT_TYPE, "application/json")],
        forbidden.body(),
    )
}

#[cfg(test)]
mod tests {
    use home_app_auth::{AccessSnapshot, Module, Modules, TokenRecord, sha256_hex};
    use http::HeaderValue;

    use super::*;

    fn auth(modules: &Modules) -> AccessAuth {
        let snapshot = AccessSnapshot {
            revision: 1,
            tokens: vec![TokenRecord {
                id: "test".to_owned(),
                name: "test".to_owned(),
                sha256: sha256_hex("app-token"),
                owner: false,
                modules: modules.ids().into_iter().map(str::to_owned).collect(),
            }],
        };
        AccessAuth::new(AccessCache::new(snapshot), Token::new("service-token"))
    }

    fn header(value: &str) -> HeaderValue {
        HeaderValue::from_str(value).expect("header")
    }

    #[test]
    fn a_previous_service_token_is_accepted_only_when_configured() {
        let cache = AccessCache::default();
        let old = Some(&header("Bearer old-service"));
        let rolling = AccessAuth::new(cache.clone(), Token::new("new-service"))
            .with_previous(Some(Token::new("old-service")));
        assert!(rolling.service_authorized(old));
        let finalized = AccessAuth::new(cache, Token::new("new-service"));
        assert!(!finalized.service_authorized(old));
        assert!(!finalized.service_authorized(Some(&header("Bearer app-token"))));
    }

    #[test]
    fn the_table_covers_every_registered_rpc() {
        let expected: Vec<&str> = RPC_MODULES.iter().map(|(name, _)| *name).collect();
        assert_eq!(expected.len(), 35);
    }

    #[test]
    fn every_method_is_mapped_to_exactly_one_module() {
        for (path, _) in RPC_MODULES {
            assert!(method_module(&format!("/{path}")).is_some(), "{path}");
        }
        assert_eq!(
            method_module("/home.v1.SchedulerService/ListDueEvents"),
            Some(None)
        );
        assert_eq!(
            method_module("/home.v1.SchedulerService/AckEvent"),
            Some(None)
        );
        assert_eq!(
            method_module("/home.v1.SchedulerService/GetStatus"),
            Some(Some(home_app_auth::Module::Base))
        );
        assert_eq!(method_module("/home.v1.IntercomService/GetStatus"), None);
        assert_eq!(method_module("/x/NotARpc"), None);
    }

    #[test]
    fn a_member_is_denied_each_module_outside_its_grants() {
        let auth = auth(&Modules::member());
        let header = Some(&header("Bearer app-token"));
        for module in [
            home_app_auth::Module::Base,
            home_app_auth::Module::Home,
            home_app_auth::Module::Scenarios,
            home_app_auth::Module::AlarmsTimers,
            home_app_auth::Module::IntercomCall,
            home_app_auth::Module::Notifications,
        ] {
            auth.app(header, module).expect("member module");
        }
        for module in [
            home_app_auth::Module::Chat,
            home_app_auth::Module::Vpn,
            home_app_auth::Module::System,
            home_app_auth::Module::InfraReports,
            home_app_auth::Module::Alerts,
            home_app_auth::Module::Admin,
        ] {
            assert_eq!(
                auth.app(header, module),
                Err(Forbidden::Module(module)),
                "{module:?}"
            );
        }
    }

    #[test]
    fn the_service_token_bypasses_modules_but_not_app_only_routes() {
        let auth = auth(&Modules::empty());
        let header = Some(&header("Bearer service-token"));
        assert!(matches!(
            auth.require(header, Module::Notifications),
            Ok(Caller::Service)
        ));
        assert_eq!(
            auth.app(header, Module::Vpn),
            Err(Forbidden::Unauthenticated)
        );
    }
}
