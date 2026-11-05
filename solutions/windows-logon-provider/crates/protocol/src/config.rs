//! Service URL parsing and machine configuration validation.

use http::Uri;

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

    let (tls, default_port) = if url.starts_with("http://") {
        (false, 80)
    } else if url.starts_with("https://") {
        (true, 443)
    } else {
        return Err(Error::new(
            "service url must start with http:// or https://",
        ));
    };
    if url.bytes().any(|c| matches!(c, b'?' | b'#' | b'@')) {
        return Err(Error::new(
            "service url must not carry credentials, query or fragment",
        ));
    }

    let uri: Uri = url
        .parse()
        .map_err(|_| Error::new("service url is not a valid URI"))?;
    let authority = uri
        .authority()
        .ok_or(Error::new("service url without host"))?;
    let port = parse_port(authority.as_str(), default_port)?;

    let host = authority.host();
    let host = if let Some(rest) = host.strip_prefix('[') {
        let literal = rest
            .strip_suffix(']')
            .ok_or(Error::new("service url has an unterminated IPv6 literal"))?;
        if literal.is_empty() {
            return Err(Error::new("service url without host"));
        }
        if !literal
            .bytes()
            .all(|c| c.is_ascii_hexdigit() || c == b':' || c == b'.')
        {
            return Err(Error::new("service url has an invalid IPv6 literal"));
        }
        literal
    } else {
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
        host
    };

    let prefix = if uri.path() == "/" {
        ""
    } else {
        let prefix = uri.path().trim_end_matches('/');
        if prefix.contains("//") {
            return Err(Error::new("service url path has an empty segment"));
        }
        prefix
    };

    Ok(ServiceEndpoint {
        tls,
        host: host.to_owned(),
        port,
        prefix: prefix.to_owned(),
    })
}

/// Port after the host of an authority. Rejects non-numeric and out-of-range
/// values instead of letting `http::Uri` silently truncate them.
fn parse_port(authority: &str, default_port: u16) -> Result<u16, Error> {
    let port_text = if let Some(rest) = authority.strip_prefix('[') {
        let close = rest
            .find(']')
            .ok_or(Error::new("service url has an unterminated IPv6 literal"))?;
        &rest[close + 1..]
    } else {
        authority.rfind(':').map_or("", |colon| &authority[colon..])
    };
    let Some(text) = port_text.strip_prefix(':') else {
        return Ok(default_port);
    };
    if text.is_empty() {
        return Ok(default_port);
    }
    if text.len() > 5 || !text.bytes().all(|c| c.is_ascii_digit()) {
        return Err(Error::new("service url port is not numeric"));
    }
    let port: u16 = text
        .parse()
        .map_err(|_| Error::new("service url port out of range"))?;
    if port == 0 {
        return Err(Error::new("service url port out of range"));
    }
    Ok(port)
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

    Ok(render(&parse_service_url(service_url)?, false))
}

/// Log-safe service URL for log lines: scheme, host and port only, no path.
pub fn redact_service_url(url: &str) -> String {
    let Ok(endpoint) = parse_service_url(url) else {
        return "invalid-url".to_owned();
    };
    render(
        &ServiceEndpoint {
            prefix: String::new(),
            ..endpoint
        },
        true,
    )
}

/// `scheme://host[:port][prefix]`; `explicit_port` keeps the port even when it
/// is the scheme default (the log always shows it).
fn render(endpoint: &ServiceEndpoint, explicit_port: bool) -> String {
    let mut result = String::new();
    result.push_str(if endpoint.tls { "https://" } else { "http://" });
    push_host(&mut result, &endpoint.host);
    let default_port = if endpoint.tls { 443 } else { 80 };
    if explicit_port || endpoint.port != default_port {
        result.push(':');
        result.push_str(&endpoint.port.to_string());
    }
    result.push_str(&endpoint.prefix);
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
