#![warn(missing_docs)]
//! This crate provides an async client to the Save Page Now 2 API.
//!
//! The client can be used to
//! - request capture
//! - get capture status
//! - get user status
//! - get system status
//!
//! API reference:
//! <https://docs.google.com/document/d/1Nsv52MvSjbLb2PCpHlat0gkzw0EvtSgpKHu4mk0MnrA>

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, ClientBuilder, StatusCode,
};
use serde::{Deserialize, Serialize, Serializer};

/// Errors that may occur when constructing the client and sending requests
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

const API_CAPTURE_URL: &str = "https://web.archive.org/save";
const API_CAPTURE_STATUS_URL: &str = "https://web.archive.org/save/status";
const API_USER_STATUS_URL: &str = "https://web.archive.org/save/status/user";
const API_SYSTEM_STATUS_URL: &str = "https://web.archive.org/save/status/system";

/// Parameters for a capture request
///
/// Refer to the
/// [SNP2 docs](https://docs.google.com/document/d/1Nsv52MvSjbLb2PCpHlat0gkzw0EvtSgpKHu4mk0MnrA)
/// for an explanation of the parameters.
///
/// # Examples
///
/// Don't use any parameters:
/// ```
/// let params = spn::SPN2CaptureRequestOptParams::default();
/// ```
///
/// Use only some parameters
/// ```
/// let params = spn::SPN2CaptureRequestOptParams {
///     capture_all: true,
///     ..Default::default()
/// };
/// ```
#[allow(missing_docs)]
#[derive(Default, Serialize)]
pub struct SPN2CaptureRequestOptParams {
    #[serde(serialize_with = "serialize_bool_param")]
    pub capture_all: bool,
    #[serde(serialize_with = "serialize_bool_param")]
    pub capture_outlinks: bool,
    #[serde(serialize_with = "serialize_bool_param")]
    pub capture_screenshot: bool,
    #[serde(serialize_with = "serialize_bool_param")]
    pub delay_wb_availability: bool,
    #[serde(serialize_with = "serialize_bool_param")]
    pub force_get: bool,
    #[serde(serialize_with = "serialize_bool_param")]
    pub skip_first_archive: bool,
    #[serde(serialize_with = "serialize_bool_param")]
    pub outlinks_availability: bool,
    #[serde(serialize_with = "serialize_bool_param")]
    pub email_result: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(serialize_with = "serialize_duration_param")]
    pub if_not_archived_within: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(serialize_with = "serialize_duration_param")]
    pub js_behavior_timeout: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_cookie: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_password: Option<String>,
}

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
    /// Capture request was not successful, some error occured.
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
    /// Capture request was successfully processed.
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

impl SPN2Client {
    /// Issue a capture request for the given URL
    pub async fn request_capture(
        &self,
        url: &str,
        opt_params: &SPN2CaptureRequestOptParams,
    ) -> Result<SPN2CaptureResponse, Error> {
        let params = SPN2CaptureRequestParams { url, opt_params };
        let req = self
            .http_client
            .post(API_CAPTURE_URL)
            .timeout(self.timeout)
            .form(&params);
        eprintln!("{req:?}");
        let resp = req.send().await?;
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

fn serialize_bool_param<S>(b: &bool, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let b = if *b { 1 } else { 0 };
    s.serialize_u8(b)
}

fn serialize_duration_param<S>(d: &Option<Duration>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(d) = d {
        s.serialize_u64(d.as_secs())
    } else {
        s.serialize_none()
    }
}

#[derive(Serialize)]
struct SPN2CaptureRequestParams<'a> {
    url: &'a str,
    #[serde(flatten)]
    opt_params: &'a SPN2CaptureRequestOptParams,
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

    #[test]
    fn serialize_request_params() {
        let opt_params = SPN2CaptureRequestOptParams {
            capture_all: true,
            capture_outlinks: true,
            capture_screenshot: true,
            delay_wb_availability: true,
            force_get: true,
            skip_first_archive: true,
            outlinks_availability: true,
            email_result: false,
            if_not_archived_within: Some(Duration::from_secs(1)),
            js_behavior_timeout: None,
            capture_cookie: None,
            use_user_agent: Some("Dummy".to_string()),
            target_username: None,
            target_password: None,
        };
        let params = SPN2CaptureRequestParams {
            url: "example.com",
            opt_params: &opt_params,
        };
        let params_encoded =
            serde_urlencoded::to_string(params).expect("failed to serialize params");
        let expected = "url=example.com&capture_all=1&capture_outlinks=1&\
                        capture_screenshot=1&delay_wb_availability=1&force_get=1&\
                        skip_first_archive=1&outlinks_availability=1&email_result=0&\
                        if_not_archived_within=1&use_user_agent=Dummy";
        assert_eq!(expected, params_encoded);
    }
}
