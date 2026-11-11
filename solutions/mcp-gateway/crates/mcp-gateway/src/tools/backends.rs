//! Adapters from the shared HTTP clients to the object-safe tool traits.

use std::collections::BTreeMap;
use std::time::Duration;

use jiff::Timestamp;
use time::OffsetDateTime;

use super::{
    AlertLister, BoxFuture, CheckRunner, KubeApi, LogLine, LogQuerier, PromQuerier, ReportSender,
    Sample,
};
use crate::domain::alert::Alert;
use crate::domain::status::CheckResult;

impl PromQuerier for home_clients::prometheus::Client {
    fn query<'a>(
        &'a self,
        promql: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<Sample>, String>> {
        Box::pin(async move {
            Self::query(self, promql, limit)
                .await
                .map(|samples| {
                    samples
                        .into_iter()
                        .map(|sample| Sample {
                            labels: sample.labels,
                            value: sample.value,
                            raw: sample.raw,
                        })
                        .collect()
                })
                .map_err(|error| error.to_string())
        })
    }
}

impl AlertLister for home_clients::alertmanager::Client {
    fn active<'a>(&'a self, limit: usize) -> BoxFuture<'a, Result<Vec<Alert>, String>> {
        Box::pin(async move {
            Self::active(self, limit)
                .await
                .map(|alerts| {
                    alerts
                        .into_iter()
                        .map(|alert| Alert {
                            fingerprint: alert.fingerprint,
                            labels: alert.labels,
                            annotations: alert.annotations,
                            starts_at: alert.starts_at.and_then(|starts| {
                                Timestamp::from_second(starts.unix_timestamp()).ok()
                            }),
                        })
                        .collect()
                })
                .map_err(|error| error.to_string())
        })
    }
}

impl LogQuerier for home_clients::loki::Client {
    fn query<'a>(
        &'a self,
        selector: &'a str,
        since: Duration,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<LogLine>, String>> {
        Box::pin(async move {
            Self::query(self, selector, since, limit)
                .await
                .map(|lines| {
                    lines
                        .into_iter()
                        .map(|line| LogLine { text: line.text })
                        .collect()
                })
                .map_err(|error| error.to_string())
        })
    }
}

impl KubeApi for crate::kubernetes::Client {
    fn get_workload<'a>(
        &'a self,
        namespace: &'a str,
        kind: &'a str,
        name: &'a str,
    ) -> BoxFuture<'a, Result<crate::kubernetes::Workload, String>> {
        Box::pin(async move {
            Self::get_workload(self, namespace, kind, name)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn list_pods<'a>(
        &'a self,
        namespace: &'a str,
        labels: &'a BTreeMap<String, String>,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::Pod>, String>> {
        Box::pin(async move {
            Self::list_pods(self, namespace, labels)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn list_events<'a>(
        &'a self,
        namespace: &'a str,
        object_name: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::Event>, String>> {
        Box::pin(async move {
            Self::list_events(self, namespace, object_name, limit)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        container: &'a str,
        tail_lines: usize,
        since: Duration,
    ) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            Self::pod_logs(self, namespace, pod, container, tail_lines, since)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn patch_restart<'a>(
        &'a self,
        namespace: &'a str,
        kind: &'a str,
        name: &'a str,
        at: Timestamp,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let at = OffsetDateTime::from_unix_timestamp(at.as_second())
                .unwrap_or(OffsetDateTime::UNIX_EPOCH);
            Self::patch_restart(self, namespace, kind, name, at)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn list_workloads<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::WorkloadRef>, String>> {
        Box::pin(async move {
            Self::list_workloads(self)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn list_flux<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::FluxResource>, String>> {
        Box::pin(async move {
            Self::list_flux(self)
                .await
                .map_err(|error| error.to_string())
        })
    }
}

impl CheckRunner for crate::checks::Runner {
    fn run_all<'a>(
        &'a self,
        services: &'a [crate::domain::catalog::Service],
    ) -> BoxFuture<'a, Vec<CheckResult>> {
        Box::pin(async move { Self::run_all(self, services).await })
    }
}

impl ReportSender for home_telegram::Sender {
    fn send<'a>(&'a self, title: &'a str, text: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            Self::send(self, title, text)
                .await
                .map_err(|error| error.to_string())
        })
    }
}
