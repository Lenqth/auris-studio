//! A bounded, manual check against the latest published GitHub release.

use std::io::Read;
use std::time::Duration;

use semver::Version;
use serde::Deserialize;

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/uthree/auris-studio/releases/latest";
pub(crate) const RELEASE_PAGE: &str = "https://github.com/uthree/auris-studio/releases/latest";
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

#[derive(Debug, PartialEq)]
pub(crate) enum UpdateStatus {
    Current(String),
    Available { current: String, latest: String },
    NoRelease,
}

#[derive(Debug)]
pub(crate) enum CheckError {
    Request(String),
    InvalidResponse(String),
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(message) | Self::InvalidResponse(message) => formatter.write_str(message),
        }
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

pub(crate) fn check_latest_release() -> Result<UpdateStatus, CheckError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let response = agent
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .header(
            "User-Agent",
            concat!("auris-studio/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|error| CheckError::Request(error.to_string()))?;
    if response.status().as_u16() == 404 {
        return Ok(UpdateStatus::NoRelease);
    }
    if !response.status().is_success() {
        return Err(CheckError::Request(format!("HTTP {}", response.status())));
    }
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| CheckError::Request(error.to_string()))?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(CheckError::InvalidResponse("response is too large".into()));
    }
    compare_release(env!("CARGO_PKG_VERSION"), &bytes)
}

fn compare_release(current: &str, response: &[u8]) -> Result<UpdateStatus, CheckError> {
    let release: Release = serde_json::from_slice(response)
        .map_err(|error| CheckError::InvalidResponse(error.to_string()))?;
    let installed =
        Version::parse(current).map_err(|error| CheckError::InvalidResponse(error.to_string()))?;
    let latest = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|error| CheckError::InvalidResponse(error.to_string()))?;
    if latest > installed {
        Ok(UpdateStatus::Available {
            current: current.to_owned(),
            latest: latest.to_string(),
        })
    } else {
        Ok(UpdateStatus::Current(current.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_published_versions_without_lexical_ordering() {
        assert_eq!(
            compare_release("0.9.0", br#"{"tag_name":"v0.10.0"}"#).unwrap(),
            UpdateStatus::Available {
                current: "0.9.0".into(),
                latest: "0.10.0".into(),
            }
        );
        assert_eq!(
            compare_release("0.10.0", br#"{"tag_name":"v0.9.0"}"#).unwrap(),
            UpdateStatus::Current("0.10.0".into())
        );
        assert_eq!(
            compare_release("0.9.0", br#"{"tag_name":"v0.9.0"}"#).unwrap(),
            UpdateStatus::Current("0.9.0".into())
        );
    }

    #[test]
    fn rejects_unusable_release_metadata() {
        assert!(compare_release("0.9.0", br#"{"tag_name":"latest"}"#).is_err());
        assert!(compare_release("0.9.0", br#"{}"#).is_err());
    }
}
