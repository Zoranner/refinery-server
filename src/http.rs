use std::time::Duration;

use reqwest::Client;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) fn new_client() -> Client {
    build_client(CONNECT_TIMEOUT, TOTAL_TIMEOUT)
}

fn build_client(connect_timeout: Duration, total_timeout: Duration) -> Client {
    Client::builder()
        .connect_timeout(connect_timeout)
        .timeout(total_timeout)
        .build()
        .expect("HTTP client configuration is valid")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn configured_client_enforces_total_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                .await;
        });

        let client = super::build_client(Duration::from_secs(1), Duration::from_millis(25));
        let error = client
            .get(format!("http://{address}"))
            .send()
            .await
            .expect_err("the total timeout must abort a stalled response");

        assert!(error.is_timeout());
    }
}
