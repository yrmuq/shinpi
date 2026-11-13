use serde::Deserialize;
use std::time::Duration;

#[derive(Default)]
pub enum Status {
    #[default]
    Idle,
    Checking,
    Current,
    Available(String),
    Failed,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

fn version(value: &str) -> Option<[u64; 3]> {
    let parts = value.strip_prefix('v').unwrap_or(value).split('.');
    let parts: Vec<_> = parts
        .map(str::parse::<u64>)
        .collect::<Result<_, _>>()
        .ok()?;
    parts.try_into().ok()
}

fn available(release: Release, current: &str) -> Result<Option<String>, ()> {
    if release.draft || release.prerelease {
        return Err(());
    }
    let latest = version(&release.tag_name).ok_or(())?;
    let current = version(current).ok_or(())?;
    Ok((latest > current).then_some(release.tag_name))
}

pub fn check() -> Result<Option<String>, ()> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();
    let body = agent
        .get("https://api.github.com/repos/yrmuq/shinpi/releases/latest")
        .header("User-Agent", concat!("shinpi/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|_| ())?
        .body_mut()
        .read_to_string()
        .map_err(|_| ())?;
    available(
        serde_json::from_str(&body).map_err(|_| ())?,
        env!("CARGO_PKG_VERSION"),
    )
}

fn allowed_link(url: &str) -> bool {
    [
        "https://github.com/yrmuq/shinpi",
        "https://github.com/yrmuq/shinpi/releases/latest",
        "https://github.com/yrmuq/shinpi/blob/main/LICENSE",
    ]
    .contains(&url)
        || crate::about::parts().iter().any(|part| part.url == url)
}

pub async fn open_link(url: &str) -> Result<(), zbus::Error> {
    if !allowed_link(url) {
        return Ok(());
    }
    // Keep the client connected while the portal handles its asynchronous request.
    static CONNECTION: std::sync::OnceLock<zbus::Connection> = std::sync::OnceLock::new();
    let connection = if let Some(connection) = CONNECTION.get() {
        connection.clone()
    } else {
        let connection = zbus::Connection::session().await?;
        CONNECTION.get_or_init(|| connection).clone()
    };
    let portal = zbus::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.OpenURI",
    )
    .await?;
    let options = std::collections::HashMap::<&str, zbus::Variant<'_>>::new();
    let _: zbus::OwnedObjectPath = portal.call("OpenURI", &("", url, options)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_limited_to_the_application_and_readme_upstreams() {
        for part in crate::about::parts() {
            assert!(allowed_link(&part.url));
        }
        assert!(allowed_link(
            "https://github.com/yrmuq/shinpi/releases/latest"
        ));
        for url in [
            "file:///etc/passwd",
            "https://github.com/yrmuq/shinpi.evil",
            "https://example.com",
            "javascript:alert(1)",
        ] {
            assert!(!allowed_link(url));
        }
    }

    #[test]
    fn only_newer_stable_versions_are_offered() {
        for (tag, prerelease, draft, expected) in [
            ("v0.10.0", false, false, Ok(Some("v0.10.0".into()))),
            ("v0.2.0", false, false, Ok(None)),
            ("v0.1.0", false, false, Ok(None)),
            ("v0.3.0", true, false, Err(())),
            ("v0.3.0", false, true, Err(())),
            ("v0.3.0-beta.1", false, false, Err(())),
            ("invalid", false, false, Err(())),
        ] {
            let release = Release {
                tag_name: tag.into(),
                prerelease,
                draft,
            };
            assert_eq!(available(release, "0.2.0"), expected, "{tag}");
        }
    }
}
