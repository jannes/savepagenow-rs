use std::{env, fs, time::Duration};

use spn::{SPN2CaptureStatus, SPN2Client};
use tokio::time;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let api_access_key = args
        .get(0)
        .expect("first arg: <api_access_key>")
        .to_string();
    let api_secret_file = args
        .get(1)
        .expect("second arg: <path to file containing api secret>");
    let url = args.get(2).expect("third arg: <url>");
    let api_secret = fs::read_to_string(api_secret_file)
        .expect("failed to read api secret file")
        .trim()
        .to_string();
    let client = SPN2Client::new(api_access_key, api_secret, Duration::from_secs(5))
        .expect("failed to create spn2 client");
    let user_status = client.get_user_status().await.unwrap();
    println!("user status: {user_status:?}");
    let capture_resp = client
        .request_capture(url)
        .await
        .expect("failed to get capture response");
    println!("job_id: {}", capture_resp.job_id);
    let user_status = client.get_user_status().await.unwrap();
    println!("user status: {user_status:?}");
    loop {
        let status = client
            .get_capture_status(&capture_resp.job_id)
            .await
            .expect("failed to get capture status");
        match status {
            SPN2CaptureStatus::Pending { resources } => {
                println!("PENDING");
                println!("resources: {resources:#?}");
                time::sleep(Duration::from_secs(2)).await;
            }
            e @ SPN2CaptureStatus::Error { .. } => {
                println!("ERROR: {e:?}");
                break;
            }
            s @ SPN2CaptureStatus::Success { .. } => {
                println!("SUCCESS: {s:?}");
                break;
            }
        }
    }
    let user_status = client.get_user_status().await.unwrap();
    println!("user status: {user_status:?}");
    let system_status = client.get_system_status().await.unwrap();
    println!("system status: {system_status:?}");
}
