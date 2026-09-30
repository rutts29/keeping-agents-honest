use kah_run::fetch_policy::{FetchPolicy, public_address};
#[test]
fn research_fetch_is_exact_host_https_without_credentials_or_alternate_ports() {
    let policy = FetchPolicy::new("research", vec!["example.com".into()]).unwrap();
    assert!(policy.validate("https://example.com/docs?q=one").is_ok());
    for url in [
        "http://example.com",
        "https://example.com:444",
        "https://user:secret@example.com",
        "https://sub.example.com",
        "https://example.com.evil.test",
        "file:///etc/passwd",
    ] {
        assert!(policy.validate(url).is_err(), "{url}");
    }
    assert!(FetchPolicy::new("mine", vec!["example.com".into()]).is_err());
    assert!(FetchPolicy::new("research", vec!["*.example.com".into()]).is_err());
}
#[test]
fn private_loopback_link_local_mapped_and_special_addresses_are_rejected() {
    for ip in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "0.1.2.3",
        "198.18.0.1",
        "224.0.0.1",
        "::1",
        "fe80::1",
        "fc00::1",
        "::ffff:127.0.0.1",
        "2001:db8::1",
    ] {
        assert!(!public_address(ip.parse().unwrap()), "{ip}");
    }
    assert!(public_address("93.184.215.14".parse().unwrap()));
    assert!(public_address("2606:4700:4700::1111".parse().unwrap()));
}
