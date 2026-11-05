//! Service URL parsing and machine configuration validation.

use crate::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceEndpoint {
    pub tls: bool,
    /// DNS name, IPv4 or IPv6 literal without brackets.
    pub host: String,
    pub port: u16,
    /// `""` or `"/path"` without a trailing slash.
    pub prefix: String,
}

fn is_lower_alnum_dash(c: u8) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'
}

fn has_control_or_space(text: &str) -> bool {
    text.bytes().any(|c| c < 0x20 || c == 0x7f || c == b' ')
}

/// Parses `http://host[:port][/path]` or `https://...`. Rejects userinfo,
/// queries, fragments, whitespace and control characters.
pub fn parse_service_url(url: &str) -> Result<ServiceEndpoint, Error> {
    if url.len() > 512 {
        return Err(Error::new("service url too long"));
    }
    if has_control_or_space(url) {
        return Err(Error::new("service url contains whitespace"));
    }

    let (tls, default_port, rest) = if let Some(rest) = url.strip_prefix("http://") {
        (false, 80, rest)
    } else if let Some(rest) = url.strip_prefix("https://") {
        (true, 443, rest)
    } else {
        return Err(Error::new(
            "service url must start with http:// or https://",
        ));
    };
    if rest.is_empty() {
        return Err(Error::new("service url without host"));
    }
    if rest.bytes().any(|c| matches!(c, b'?' | b'#' | b'@')) {
        return Err(Error::new(
            "service url must not carry credentials, query or fragment",
        ));
    }

    let mut out = ServiceEndpoint {
        tls,
        port: default_port,
        ..ServiceEndpoint::default()
    };
    let (authority, slash) = rest
        .find('/')
        .map_or((rest, None), |slash| (&rest[..slash], Some(slash)));
    if let Some(slash) = slash {
        rest[slash..]
            .trim_end_matches('/')
            .clone_into(&mut out.prefix);
        if out.prefix.contains("//") {
            return Err(Error::new("service url path has an empty segment"));
        }
    }
    if authority.is_empty() {
        return Err(Error::new("service url without host"));
    }

    let (host, port_text) = if authority.starts_with('[') {
        let close = authority
            .find(']')
            .ok_or(Error::new("service url has an unterminated IPv6 literal"))?;
        let host = &authority[1..close];
        let port_text = if close + 1 < authority.len() {
            if authority.as_bytes()[close + 1] != b':' {
                return Err(Error::new(
                    "service url has trailing data after IPv6 literal",
                ));
            }
            &authority[close + 2..]
        } else {
            ""
        };
        if host.is_empty() {
            return Err(Error::new("service url without host"));
        }
        if !host
            .bytes()
            .all(|c| c.is_ascii_hexdigit() || c == b':' || c == b'.')
        {
            return Err(Error::new("service url has an invalid IPv6 literal"));
        }
        (host, port_text)
    } else {
        let (host, port_text) = authority.rfind(':').map_or((authority, ""), |colon| {
            (&authority[..colon], &authority[colon + 1..])
        });
        if host.is_empty() {
            return Err(Error::new("service url without host"));
        }
        if host.len() > 253 {
            return Err(Error::new("service url host too long"));
        }
        if !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
        {
            return Err(Error::new("service url has an invalid host character"));
        }
        if host.starts_with('.') || host.ends_with('.') || host.contains("..") {
            return Err(Error::new("service url host has an empty label"));
        }
        (host, port_text)
    };

    if !port_text.is_empty() {
        if port_text.len() > 5 {
            return Err(Error::new("service url port is too long"));
        }
        let mut value: u32 = 0;
        for c in port_text.bytes() {
            if !c.is_ascii_digit() {
                return Err(Error::new("service url port is not numeric"));
            }
            value = value * 10 + u32::from(c - b'0');
        }
        if value == 0 || value > 65535 {
            return Err(Error::new("service url port out of range"));
        }
        out.port = value as u16;
    }

    host.clone_into(&mut out.host);
    Ok(out)
}

/// Validates the whole machine configuration and normalizes the URL. The
/// error is log-safe.
pub fn validate_config(
    service_url: &str,
    machine: &str,
    user: &str,
    domain: &str,
) -> Result<String, Error> {
    if machine.is_empty() || machine.len() > 32 {
        return Err(Error::new("machine must be 1..32 characters"));
    }
    if !machine.bytes().all(is_lower_alnum_dash) {
        return Err(Error::new("machine must match ^[a-z0-9-]{1,32}$"));
    }

    if user.is_empty() || user.len() > 256 {
        return Err(Error::new("UserName must be 1..256 characters"));
    }
    if user.bytes().any(|c| matches!(c, b'\\' | b'"' | b'@')) {
        return Err(Error::new("UserName must not contain '\\', '\"' or '@'"));
    }
    if has_control_or_space(user) {
        return Err(Error::new(
            "UserName contains whitespace or control characters",
        ));
    }

    if domain.is_empty() || domain.len() > 256 {
        return Err(Error::new("Domain must be 1..256 characters"));
    }
    if domain.bytes().any(|c| matches!(c, b'\\' | b'"')) {
        return Err(Error::new("Domain must not contain '\\' or '\"'"));
    }
    if has_control_or_space(domain) {
        return Err(Error::new(
            "Domain contains whitespace or control characters",
        ));
    }

    let endpoint = parse_service_url(service_url)?;
    let mut result = String::new();
    result.push_str(if endpoint.tls { "https://" } else { "http://" });
    push_host(&mut result, &endpoint.host);
    let default_port = if endpoint.tls { 443 } else { 80 };
    if endpoint.port != default_port {
        result.push(':');
        result.push_str(&endpoint.port.to_string());
    }
    result.push_str(&endpoint.prefix);
    Ok(result)
}

/// Log-safe service URL for log lines: scheme, host and port only, no path.
pub fn redact_service_url(url: &str) -> String {
    let Ok(endpoint) = parse_service_url(url) else {
        return "invalid-url".to_owned();
    };
    let mut result = String::new();
    result.push_str(if endpoint.tls { "https://" } else { "http://" });
    push_host(&mut result, &endpoint.host);
    result.push(':');
    result.push_str(&endpoint.port.to_string());
    result
}

fn push_host(out: &mut String, host: &str) {
    if host.contains(':') {
        out.push('[');
        out.push_str(host);
        out.push(']');
    } else {
        out.push_str(host);
    }
}

/// Request target path for an endpoint, e.g. `/v1/claim` or `/gw/v1/claim`.
pub fn endpoint_path(endpoint: &ServiceEndpoint, path: &str) -> String {
    let mut result = endpoint.prefix.clone();
    if !path.starts_with('/') {
        result.push('/');
    }
    result.push_str(path);
    result
}
