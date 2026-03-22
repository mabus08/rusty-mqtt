use rusty_mqtt::MqttServer; 
use tokio::net::TcpStream;
use tokio::io::{AsyncWriteExt, AsyncReadExt};

#[tokio::test]
async fn test_external_connection() {
    let addr = "127.0.0.1:1885";
    
    tokio::spawn(async move {
        let server = MqttServer::new(addr);
        let _ = server.run().await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut stream = TcpStream::connect(addr).await.expect("Konnte nicht verbinden");
    stream.write_all(&[0x10, 0x02, 0x00, 0x00]).await.unwrap();

    let mut response = [0u8; 4];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(response[0], 0x20); // CONNACK Typ
}
