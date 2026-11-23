use std::process::ExitCode;
use std::time::Duration;

use crate::control_config::ControlConfig;

const TIMEOUT: Duration = Duration::from_secs(4);
const MAX_BODY: usize = 1 << 20;

pub async fn run_healthcheck() -> ExitCode {
    let config = match ControlConfig::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("healthcheck: configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    let address = match healthcheck_address(&config.listen) {
        Ok(address) => address,
        Err(error) => {
            eprintln!("healthcheck: {error}");
            return ExitCode::FAILURE;
        }
    };
    match check_catalog(&address, &config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("healthcheck: {error}");
            ExitCode::FAILURE
        }
    }
}

pub fn healthcheck_address(listen: &str) -> Result<String, String> {
    let Some((host, port)) = crate::control_config::split_host_port(listen.trim()) else {
        return Err(format!("invalid CONTROL_LISTEN: {listen}"));
    };
    let host = match host {
        "" | "0.0.0.0" | "::" => "127.0.0.1",
        other => other,
    };
    if host.contains(':') {
        Ok(format!("http://[{host}]:{port}"))
    } else {
        Ok(format!("http://{host}:{port}"))
    }
}

async fn check_catalog(address: &str, config: &ControlConfig) -> Result<(), String> {
    let _already_installed = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|_| "catalog request failed: build client".to_owned())?;
    let response = client
        .get(format!("{address}/v1/catalog"))
        .bearer_auth(config.control_token.expose())
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| "catalog request failed".to_owned())?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|_| "catalog response: read failed".to_owned())?;
    if body.len() > MAX_BODY {
        return Err("catalog response is too large".to_owned());
    }
    if !status.is_success() {
        return Err(format!("catalog returned HTTP {}", status.as_u16()));
    }
    let parsed: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| "catalog response is not JSON".to_owned())?;
    if !parsed
        .get("entries")
        .is_some_and(serde_json::Value::is_array)
    {
        return Err("catalog response has no entries array".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_wildcard_listeners_to_loopback() {
        assert_eq!(
            healthcheck_address("127.0.0.1:8099").expect("address"),
            "http://127.0.0.1:8099"
        );
        assert_eq!(
            healthcheck_address("0.0.0.0:8099").expect("address"),
            "http://127.0.0.1:8099"
        );
        assert_eq!(
            healthcheck_address("[::]:8099").expect("address"),
            "http://127.0.0.1:8099"
        );
        assert!(healthcheck_address("8099").is_err());
    }

    #[tokio::test]
    async fn probe_requires_a_json_entries_array() {
        use axum::{Router, http::StatusCode, routing::get};

        async fn serve(body: &'static str, status: StatusCode) -> String {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            let address = listener.local_addr().expect("address");
            let app =
                Router::new().route("/v1/catalog", get(move || async move { (status, body) }));
            tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
            format!("http://{address}")
        }

        let config = ControlConfig::load_from(
            &|key| match key {
                "HA_URL" => Some("http://ha.example:8123".to_owned()),
                "HA_TOKEN" => Some("ha".to_owned()),
                "SCHEDULER_ENDPOINT" => Some("http://scheduler.example:8098".to_owned()),
                "SCHEDULER_TOKEN" => Some("scheduler".to_owned()),
                "CONTROL_TOKEN" => Some("control".to_owned()),
                "CONTROL_CONSUMER_ID" => Some("ha-control-home".to_owned()),
                _ => None,
            },
            &|_| Ok(Vec::new()),
        )
        .expect("config");

        let address = serve(r#"{"entries":[],"statesOk":true}"#, StatusCode::OK).await;
        check_catalog(&address, &config).await.expect("healthy");

        let address = serve(r#"{"statesOk":true}"#, StatusCode::OK).await;
        assert!(check_catalog(&address, &config).await.is_err());

        let address = serve("nope", StatusCode::OK).await;
        assert!(check_catalog(&address, &config).await.is_err());

        let address = serve(r#"{"entries":[]}"#, StatusCode::UNAUTHORIZED).await;
        assert!(check_catalog(&address, &config).await.is_err());
    }
}
