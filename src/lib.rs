#![warn(missing_docs)]
//! This crate provides an async client to the Save Page Now 2 API
//! The client can be used to issue capture request and inspect captures statuses
//!
//! API reference: https://docs.google.com/document/d/1Nsv52MvSjbLb2PCpHlat0gkzw0EvtSgpKHu4mk0MnrA

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, ClientBuilder, StatusCode,
};
use serde::Deserialize;

/// Errors that may occur when constructing the client and sending requests
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

const API_CAPTURE_URL: &str = "https://web.archive.org/save";
const API_CAPTURE_STATUS_URL: &str = "https://web.archive.org/save/status";
const API_USER_STATUS_URL: &str = "https://web.archive.org/save/status/user";
const API_SYSTEM_STATUS_URL: &str = "https://web.archive.org/save/status/system";

/// The client for the SPN2 API
pub struct SPN2Client {
    http_client: Client,
    timeout: Duration,
}

impl SPN2Client {
    /// Create a new client that uses given credentials
    pub fn new(
        api_access_key: String,
        api_secret: String,
        timeout: Duration,
    ) -> Result<Self, Error> {
        let mut headers = HeaderMap::new();
        let mut auth_value = HeaderValue::from_str(&format!("LOW {api_access_key}:{api_secret}"))?;
        auth_value.set_sensitive(true);
        headers.insert("Authorization", auth_value);
        headers.insert("Accept", HeaderValue::from_static("application/json"));
        let http_client = ClientBuilder::new().default_headers(headers).build()?;
        Ok(Self {
            http_client,
            timeout,
        })
    }

    /// Set the timeout for requests to the SPN API
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }
}

/// The SPN2 API's response to a capture request
#[derive(Deserialize)]
pub struct SPN2CaptureResponse {
    /// The requested URL to capture
    pub url: String,
    /// The ID of the capture request
    /// Use this to issue status requests
    pub job_id: String,
}

/// The SPN2 API's response to a capture status request
#[derive(Deserialize, Debug)]
#[serde(tag = "status")]
pub enum SPN2CaptureStatus {
    /// Status: Pending
    /// Capture request has not been fully processed.
    #[serde(rename = "pending")]
    Pending {
        /// List of captured resources
        resources: Vec<String>,
    },
    /// Status: Error
    /// Capture request was not successful.
    #[serde(rename = "error")]
    Error {
        /// The type of exception
        exception: Option<String>,
        /// More specific error code
        status_ext: String,
        /// The error message
        message: String,
        /// List of captured resources
        resources: Vec<String>,
    },
    /// Status: Success
    /// Capture request not successful.
    #[serde(rename = "success")]
    Success {
        /// The requested URL after redirects
        original_url: String,
        /// Screenshot URL
        screenshot: Option<String>,
        /// Timestamp in YYYYMMDDHHMMSS format
        timestamp: String,
        /// Duration of capture processing
        duration_sec: f64,
        /// List of captured resources
        resources: Vec<String>,
        /// List of links to other sites
        outlinks: Vec<String>,
    },
}

/// The SPN2 API's response to a user status request
#[derive(Deserialize, Debug)]
pub struct SPN2UserStatus {
    /// The user's amount of available sessions
    pub available: usize,
    /// The user's amount of active sessions
    pub processing: usize,
}

/// The SPN2 API's response to a system status request
#[derive(Debug)]
pub enum SPN2SystemStatus {
    /// Everything is fine
    Ok,
    /// System is having issues, e.g being overloaded
    /// The system is still working, but delays are expected
    Issues {
        /// A description of the issues the system is having
        description: String,
    },
    /// System has critical problems, is not working
    Critical,
}

impl SPN2SystemStatus {
    fn from_json(json: serde_json::Value) -> Result<Self, Error> {
        let status = json
            .as_object()
            .and_then(|obj| obj.get("status"))
            .and_then(|status| status.as_str())
            .ok_or_else(|| format!("invalid response: {json}"))?;
        match status {
            "ok" => Ok(SPN2SystemStatus::Ok),
            msg => Ok(SPN2SystemStatus::Issues {
                description: msg.to_string(),
            }),
        }
    }
}

impl SPN2Client {
    /// Issue a capture request for the given URL
    pub async fn request_capture(&self, url: &str) -> Result<SPN2CaptureResponse, Error> {
        let params = [("url", url)];
        let resp = self
            .http_client
            .post(API_CAPTURE_URL)
            .timeout(self.timeout)
            .form(&params)
            .send()
            .await?;
        match resp.status() {
            StatusCode::OK => Ok(resp.json::<SPN2CaptureResponse>().await?),
            s => Err(format!("unexpected response status: {s}").into()),
        }
    }

    /// Get the current status of a capture job
    pub async fn get_capture_status(&self, job_id: &str) -> Result<SPN2CaptureStatus, Error> {
        let resp = self
            .http_client
            .get(format!("{API_CAPTURE_STATUS_URL}/{job_id}"))
            .timeout(self.timeout)
            .send()
            .await?;
        match resp.status() {
            StatusCode::OK => Ok(resp.json::<SPN2CaptureStatus>().await?),
            s => Err(format!("unexpected response status: {s}").into()),
        }
    }

    /// Get the current status of the user
    pub async fn get_user_status(&self) -> Result<SPN2UserStatus, Error> {
        let unix_secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let params = [("_t", unix_secs)];
        let resp = self
            .http_client
            .get(API_USER_STATUS_URL)
            .query(&params)
            .timeout(self.timeout)
            .send()
            .await?;
        match resp.status() {
            StatusCode::OK => Ok(resp.json::<SPN2UserStatus>().await?),
            s => Err(format!("unexpected response status: {s}").into()),
        }
    }

    /// Get the current status of the SPN system
    pub async fn get_system_status(&self) -> Result<SPN2SystemStatus, Error> {
        let resp = self
            .http_client
            .get(API_SYSTEM_STATUS_URL)
            .timeout(self.timeout)
            .send()
            .await?;
        match resp.status() {
            StatusCode::OK => SPN2SystemStatus::from_json(resp.json::<serde_json::Value>().await?),
            StatusCode::BAD_GATEWAY => Ok(SPN2SystemStatus::Critical),
            s => Err(format!("unexpected response status: {s}").into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_capture_status_pending() {
        let status = r#"
        {
          "status":"pending",
          "job_id":"e70f23c7-9eca-4c78-826d-26930564d7c8",
          "resources": [
            "https://ajax.googleapis.com/ajax/libs/jquery/1.7.2/jquery.min.js"
          ]
        }
        "#;
        let s: SPN2CaptureStatus = serde_json::from_str(status).unwrap();
        assert!(matches!(s, SPN2CaptureStatus::Pending { .. }));
    }

    #[test]
    fn deserialize_capture_status_error() {
        let status = r#"
        {
          "status":"error",
          "exception":"[Errno -2] Name or service not known",
          "status_ext":"error:invalid-host-resolution",
          "job_id":"2546c79b-ec70-4bec-b78b-1941c42a6374",
          "message":"Couldn't resolve host for http://example5123.com.",
          "resources": []
        }
        "#;
        let s: SPN2CaptureStatus = serde_json::from_str(status).unwrap();
        assert!(matches!(s, SPN2CaptureStatus::Error { .. }));
    }

    #[test]
    fn deserialize_capture_status_success() {
        let status = r#"
        {
          "http_status": 200,
          "counters": {
            "outlinks": 70,
            "embeds": 21
          },
          "original_url": "https://example.com",
          "timestamp": "20221002124400",
          "duration_sec": 6.214,
          "status": "success",
          "outlinks": [
            "https://example.com/"
          ],
          "job_id":"e70f23c7-9eca-4c78-826d-26930564d7c8",
          "resources": [
            "https://example.com/"
          ]
        }
        "#;
        let s: SPN2CaptureStatus = serde_json::from_str(status).unwrap();
        assert!(matches!(s, SPN2CaptureStatus::Success { .. }));
    }

    #[test]
    fn deserialize_system_status_success() {
        let status = serde_json::json!({
          "status": "Save Page Now servers are temporarily overloaded."
        });
        let s = SPN2SystemStatus::from_json(status);
        assert!(matches!(s, Ok(SPN2SystemStatus::Issues { .. })));
    }
}
