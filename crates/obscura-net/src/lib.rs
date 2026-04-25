pub mod client;
pub mod cookies;
pub mod interceptor;
pub mod robots;
pub mod blocklist;
#[cfg(feature = "stealth")]
pub mod wreq_client;

pub use client::{ObscuraHttpClient, ObscuraNetError, RequestInfo, ResourceType, Response};
pub use cookies::{CookieInfo, CookieJar};
pub use robots::RobotsCache;
pub use blocklist::is_blocked as is_tracker_blocked;
#[cfg(feature = "stealth")]
pub use wreq_client::{StealthHttpClient, STEALTH_USER_AGENT};

/// Default user-agent string used across all HTTP clients.
pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36";

/// Validates a URL to prevent SSRF attacks.
/// Rejects private/internal IPs, localhost, and non-HTTP schemes.
pub fn validate_url(url: &url::Url) -> Result<(), ObscuraNetError> {
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(ObscuraNetError::Network(format!(
            "Forbidden URL scheme '{}' - only http and https are allowed",
            scheme
        )));
    }

    if let Some(host) = url.host() {
        match host {
            url::Host::Ipv4(ip) => {
                if ip.is_loopback()
                    || ip.is_private()
                    || ip.is_link_local()
                    || ip.is_broadcast()
                    || ip.is_documentation()
                {
                    return Err(ObscuraNetError::Network(format!(
                        "Access to private/internal IP address {} is not allowed",
                        ip
                    )));
                }
            }
            url::Host::Ipv6(ip) => {
                if ip.is_loopback() || ip.is_unicast_link_local() {
                    return Err(ObscuraNetError::Network(format!(
                        "Access to private/internal IPv6 address {} is not allowed",
                        ip
                    )));
                }
            }
            url::Host::Domain(domain) => {
                let lower_domain = domain.to_lowercase();
                if lower_domain == "localhost"
                    || lower_domain.ends_with(".localhost")
                    || lower_domain == "127.0.0.1"
                    || lower_domain == "::1"
                {
                    return Err(ObscuraNetError::Network(format!(
                        "Access to localhost domain '{}' is not allowed",
                        domain
                    )));
                }
            }
        }
    }

    Ok(())
}
