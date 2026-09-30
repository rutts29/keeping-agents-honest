//! Opt-in read-only HTTPS, with exact hosts and the actual connection addresses
//! checked by the resolver. No redirects, proxies, cookies or credentials.
use std::{net::IpAddr, time::Duration};
use ureq::{
    Agent,
    config::Config,
    http::Uri,
    unversioned::{
        resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver},
        transport::{DefaultConnector, NextTimeout},
    },
};

#[derive(Clone, Debug)]
pub struct FetchPolicy {
    hosts: Vec<String>,
}
impl FetchPolicy {
    pub fn new(domain: &str, hosts: Vec<String>) -> Result<Self, String> {
        if !hosts.is_empty() && domain != "research" {
            return Err(
                "FETCH_DOMAIN stage=fetch_policy: only Research accepts an allowlist".into(),
            );
        }
        if hosts.len() > 16 {
            return Err("FETCH_HOST_BOUND stage=fetch_policy".into());
        }
        let mut normalized = vec![];
        for host in hosts {
            let host = host.to_ascii_lowercase();
            if host.len() > 253
                || !host.contains('.')
                || host.split('.').any(|label| {
                    label.is_empty()
                        || label.len() > 63
                        || label.starts_with('-')
                        || label.ends_with('-')
                        || !label
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
            {
                return Err("FETCH_HOST stage=fetch_policy: use exact DNS names, without wildcards or ports".into());
            }
            if !normalized.contains(&host) {
                normalized.push(host);
            }
        }
        Ok(Self { hosts: normalized })
    }
    pub fn validate(&self, url: &str) -> Result<Uri, String> {
        if url.len() > 4096 {
            return Err("FETCH_URL_BOUND stage=fetch_policy".into());
        }
        let uri: Uri = url.parse().map_err(|_| "FETCH_URL stage=fetch_policy")?;
        let host = uri.host().unwrap_or("").to_ascii_lowercase();
        let authority = uri
            .authority()
            .map(|a| a.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        if uri.scheme_str() != Some("https")
            || !self.hosts.contains(&host)
            || !(authority == host || authority == format!("{host}:443"))
        {
            return Err("FETCH_NOT_ALLOWED stage=fetch_policy: HTTPS port 443 and an exact allowed host are required".into());
        }
        Ok(uri)
    }
    pub fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        let uri = self.validate(url)?;
        let config = Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(10)))
            .build();
        let agent = Agent::with_parts(
            config,
            DefaultConnector::default(),
            PublicResolver(self.clone()),
        );
        let mut response = agent
            .get(uri)
            .call()
            .map_err(|_| "FETCH_HTTP stage=fetch_request")?;
        if !response.status().is_success() {
            return Err(format!(
                "FETCH_STATUS stage=fetch_request status={}",
                response.status().as_u16()
            ));
        }
        response
            .body_mut()
            .with_config()
            .limit(2 * 1024 * 1024)
            .read_to_vec()
            .map_err(|_| "FETCH_BODY_BOUND_OR_IO stage=fetch_response".into())
    }
}
#[derive(Debug)]
struct PublicResolver(FetchPolicy);
impl Resolver for PublicResolver {
    fn resolve(
        &self,
        uri: &Uri,
        config: &Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        self.0
            .validate(&uri.to_string())
            .map_err(|_| ureq::Error::HostNotFound)?;
        let addresses = DefaultResolver::default().resolve(uri, config, timeout)?;
        if addresses
            .iter()
            .any(|address| !public_address(address.ip()))
        {
            return Err(ureq::Error::HostNotFound);
        }
        Ok(addresses)
    }
}
pub fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let b = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_documentation()
                || b[0] == 0
                || b[0] >= 224
                || b[0] == 100 && (64..=127).contains(&b[1])
                || b[0] == 198 && matches!(b[1], 18 | 19)
                || b[0] == 192 && b[1] == 0 && b[2] == 0)
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_address(v4.into());
            }
            let s = ip.segments();
            // Conservative global-unicast allow range, excluding special-use,
            // documentation and transition prefixes (including embedded IPv4).
            s[0] & 0xe000 == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
                && !(s[0] == 0x3fff && s[1] & 0xf000 == 0)
        }
    }
}
