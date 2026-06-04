//! Acceptance tests for the MQTT broker MVP (PRD 0001).
//!
//! These tests fly real TCP connections against a freshly spawned `MqttServer`
//! instance and verify observable wire-level behaviour. They do not poke
//! internals.
//!
//! Frame layout helpers below are intentionally hand-rolled so the tests stay
//! independent from any future codec implementation.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use rusty_mqtt::MqttServer;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Spawn a broker on an ephemeral port and return its `host:port` address.
async fn spawn_broker() -> String {
    // bind to :0 so the kernel hands us a free port; we read it back via the
    // test-only accessor `local_addr`.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr").to_string();
    drop(listener); // free the port; MqttServer::run will bind again.

    let bind = addr.clone();
    tokio::spawn(async move {
        let mut server = MqttServer::new(&bind);
        let _ = server.run().await;
    });

    // give the server a moment to bind.
    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

/// Build a CONNECT packet for MQTT 3.1.1 with clean session, keep_alive=0.
fn connect_packet(client_id: &str) -> Vec<u8> {
    let mut variable_header = Vec::new();
    variable_header.extend_from_slice(&[0x00, 0x04]); // protocol name length
    variable_header.extend_from_slice(b"MQTT");
    variable_header.push(0x04); // protocol level 4 (3.1.1)
    variable_header.push(0x02); // connect flags: clean session
    variable_header.extend_from_slice(&[0x00, 0x00]); // keep alive = 0

    let mut payload = Vec::new();
    let cid_bytes = client_id.as_bytes();
    payload.extend_from_slice(&(cid_bytes.len() as u16).to_be_bytes());
    payload.extend_from_slice(cid_bytes);

    let mut packet = vec![0x10];
    let remaining_len = variable_header.len() + payload.len();
    encode_varint(remaining_len, &mut packet);
    packet.extend_from_slice(&variable_header);
    packet.extend_from_slice(&payload);
    packet
}

/// Build a SUBSCRIBE packet with a single topic filter at the given requested QoS.
fn subscribe_packet(packet_id: u16, topic_filter: &str, qos: u8) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&packet_id.to_be_bytes());
    let tf = topic_filter.as_bytes();
    body.extend_from_slice(&(tf.len() as u16).to_be_bytes());
    body.extend_from_slice(tf);
    body.push(qos);

    let mut packet = vec![0x82]; // SUBSCRIBE | reserved bits 0010
    encode_varint(body.len(), &mut packet);
    packet.extend_from_slice(&body);
    packet
}

/// Build a PUBLISH packet with QoS 0, no DUP, no RETAIN.
fn publish_packet(topic: &str, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    let t = topic.as_bytes();
    body.extend_from_slice(&(t.len() as u16).to_be_bytes());
    body.extend_from_slice(t);
    body.extend_from_slice(payload);

    let mut packet = vec![0x30]; // PUBLISH QoS0
    encode_varint(body.len(), &mut packet);
    packet.extend_from_slice(&body);
    packet
}

fn encode_varint(mut value: usize, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Read exactly one MQTT control packet from the stream. Returns the full
/// raw bytes including the fixed header.
async fn read_packet(stream: &mut TcpStream) -> Vec<u8> {
    let mut fixed = [0u8; 1];
    stream
        .read_exact(&mut fixed)
        .await
        .expect("read fixed header");
    let mut raw = vec![fixed[0]];

    // varint remaining length
    let mut multiplier: usize = 1;
    let mut remaining: usize = 0;
    for _ in 0..4 {
        let mut b = [0u8; 1];
        stream.read_exact(&mut b).await.expect("read varint byte");
        raw.push(b[0]);
        remaining += (b[0] & 0x7F) as usize * multiplier;
        if b[0] & 0x80 == 0 {
            break;
        }
        multiplier *= 128;
    }

    let mut body = vec![0u8; remaining];
    if remaining > 0 {
        stream.read_exact(&mut body).await.expect("read body");
    }
    raw.extend_from_slice(&body);
    raw
}

async fn expect_connack_ok(stream: &mut TcpStream) {
    let pkt = timeout(Duration::from_secs(2), read_packet(stream))
        .await
        .expect("CONNACK timeout");
    assert_eq!(
        pkt[0], 0x20,
        "expected CONNACK packet type, got 0x{:02X}",
        pkt[0]
    );
    assert_eq!(pkt[1], 0x02, "expected CONNACK remaining length 2");
    assert_eq!(pkt[3], 0x00, "expected CONNACK return code 0x00");
}

async fn expect_suback(stream: &mut TcpStream, expected_packet_id: u16, expected_granted: &[u8]) {
    let pkt = timeout(Duration::from_secs(2), read_packet(stream))
        .await
        .expect("SUBACK timeout");
    assert_eq!(
        pkt[0], 0x90,
        "expected SUBACK packet type, got 0x{:02X}",
        pkt[0]
    );
    // remaining length is varint; for short SUBACKs it's a single byte.
    let body = &pkt[2..];
    let pid = u16::from_be_bytes([body[0], body[1]]);
    assert_eq!(pid, expected_packet_id, "SUBACK packet id");
    assert_eq!(&body[2..], expected_granted, "SUBACK granted QoS list");
}

// ---------------------------------------------------------------------------
// AT1 — Tracer Bullet: end-to-end pub/sub roundtrip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at1_publish_is_delivered_to_matching_wildcard_subscriber() {
    let addr = spawn_broker().await;

    // --- subscriber connects, subscribes to home/+/temp ---
    let mut sub = TcpStream::connect(&addr).await.expect("sub connect");
    sub.write_all(&connect_packet("sub-1")).await.unwrap();
    expect_connack_ok(&mut sub).await;

    sub.write_all(&subscribe_packet(1, "home/+/temp", 0))
        .await
        .unwrap();
    expect_suback(&mut sub, 1, &[0x00]).await;

    // --- publisher connects ---
    let mut pubc = TcpStream::connect(&addr).await.expect("pub connect");
    pubc.write_all(&connect_packet("pub-1")).await.unwrap();
    expect_connack_ok(&mut pubc).await;

    // give the broker a tick to finish registering the subscription before
    // the PUBLISH races in.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // --- publish ---
    let payload = b"21.5";
    pubc.write_all(&publish_packet("home/kitchen/temp", payload))
        .await
        .unwrap();

    // --- subscriber must receive a PUBLISH with the same topic + payload ---
    let received = timeout(Duration::from_secs(2), read_packet(&mut sub))
        .await
        .expect("subscriber did not receive a PUBLISH within 2s");

    assert_eq!(
        received[0] & 0xF0,
        0x30,
        "expected PUBLISH packet type, got 0x{:02X}",
        received[0]
    );

    // skip fixed header (1) + varint remaining length (assume single byte for
    // these small payloads).
    let body = &received[2..];
    let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
    let topic = std::str::from_utf8(&body[2..2 + topic_len]).expect("utf8 topic");
    assert_eq!(topic, "home/kitchen/temp", "delivered topic name");

    // QoS 0 has no packet identifier, so payload starts right after the topic.
    let delivered_payload = &body[2 + topic_len..];
    assert_eq!(delivered_payload, payload, "payload byte-exact");
}

// ---------------------------------------------------------------------------
// AT2 — Hash-Wildcard matches arbitrary topic depth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at2_hash_wildcard_matches_any_depth() {
    let addr = spawn_broker().await;

    let mut sub = TcpStream::connect(&addr).await.unwrap();
    sub.write_all(&connect_packet("sub-2")).await.unwrap();
    expect_connack_ok(&mut sub).await;
    sub.write_all(&subscribe_packet(7, "sensors/#", 0))
        .await
        .unwrap();
    expect_suback(&mut sub, 7, &[0x00]).await;

    let mut pubc = TcpStream::connect(&addr).await.unwrap();
    pubc.write_all(&connect_packet("pub-2")).await.unwrap();
    expect_connack_ok(&mut pubc).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Three publishes at different depths.
    pubc.write_all(&publish_packet("sensors/a", b"1"))
        .await
        .unwrap();
    pubc.write_all(&publish_packet("sensors/a/b", b"2"))
        .await
        .unwrap();
    pubc.write_all(&publish_packet("sensors/a/b/c", b"3"))
        .await
        .unwrap();

    let mut received_payloads = Vec::new();
    for _ in 0..3 {
        let pkt = timeout(Duration::from_secs(2), read_packet(&mut sub))
            .await
            .expect("expected three PUBLISHes");
        let body = &pkt[2..];
        let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
        received_payloads.push(body[2 + topic_len..].to_vec());
    }
    received_payloads.sort();
    assert_eq!(
        received_payloads,
        vec![b"1".to_vec(), b"2".to_vec(), b"3".to_vec()]
    );
}

// ---------------------------------------------------------------------------
// AT3 — Fan-out to multiple subscribers on the same topic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at3_multiple_subscribers_all_receive() {
    let addr = spawn_broker().await;

    let mut subs = Vec::new();
    for i in 0..3 {
        let mut s = TcpStream::connect(&addr).await.unwrap();
        s.write_all(&connect_packet(&format!("sub-{}", i)))
            .await
            .unwrap();
        expect_connack_ok(&mut s).await;
        s.write_all(&subscribe_packet(1, "broadcast", 0))
            .await
            .unwrap();
        expect_suback(&mut s, 1, &[0x00]).await;
        subs.push(s);
    }

    let mut pubc = TcpStream::connect(&addr).await.unwrap();
    pubc.write_all(&connect_packet("publisher")).await.unwrap();
    expect_connack_ok(&mut pubc).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    pubc.write_all(&publish_packet("broadcast", b"hello"))
        .await
        .unwrap();

    for s in &mut subs {
        let pkt = timeout(Duration::from_secs(2), read_packet(s))
            .await
            .expect("each subscriber must receive the publish");
        assert_eq!(pkt[0] & 0xF0, 0x30);
        let body = &pkt[2..];
        let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
        assert_eq!(&body[2 + topic_len..], b"hello");
    }
}

// ---------------------------------------------------------------------------
// AT4 — Large binary payload survives byte-exact (forces multi-byte varint)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at4_large_binary_payload_byte_exact() {
    let addr = spawn_broker().await;

    let mut sub = TcpStream::connect(&addr).await.unwrap();
    sub.write_all(&connect_packet("bin-sub")).await.unwrap();
    expect_connack_ok(&mut sub).await;
    sub.write_all(&subscribe_packet(42, "bin/data", 0))
        .await
        .unwrap();
    expect_suback(&mut sub, 42, &[0x00]).await;

    let mut pubc = TcpStream::connect(&addr).await.unwrap();
    pubc.write_all(&connect_packet("bin-pub")).await.unwrap();
    expect_connack_ok(&mut pubc).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 500-byte payload with every byte value spanning 0x00..0xFF — also forces
    // a 2-byte remaining-length varint (>= 128 bytes).
    let payload: Vec<u8> = (0..500).map(|i| (i % 256) as u8).collect();
    pubc.write_all(&publish_packet("bin/data", &payload))
        .await
        .unwrap();

    let pkt = timeout(Duration::from_secs(2), read_packet(&mut sub))
        .await
        .expect("subscriber must receive the publish");
    assert_eq!(pkt[0] & 0xF0, 0x30);

    // parse varint remaining length from received packet
    let mut idx = 1usize;
    let mut multiplier: usize = 1;
    let mut remaining: usize = 0;
    loop {
        let b = pkt[idx];
        idx += 1;
        remaining += (b & 0x7F) as usize * multiplier;
        if b & 0x80 == 0 {
            break;
        }
        multiplier *= 128;
    }
    let body = &pkt[idx..idx + remaining];
    let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
    assert_eq!(&body[2..2 + topic_len], b"bin/data");
    assert_eq!(&body[2 + topic_len..], &payload[..], "payload byte-exact");
}

// ---------------------------------------------------------------------------
// AT12 — Client-ID takeover: second CONNECT with same client id kicks the first
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at12_second_connect_with_same_client_id_kicks_first() {
    let addr = spawn_broker().await;

    // First session establishes itself.
    let mut first = TcpStream::connect(&addr).await.unwrap();
    first.write_all(&connect_packet("dup-id")).await.unwrap();
    expect_connack_ok(&mut first).await;

    // Second session with the same client id.
    let mut second = TcpStream::connect(&addr).await.unwrap();
    second.write_all(&connect_packet("dup-id")).await.unwrap();
    expect_connack_ok(&mut second).await;

    // The first socket must observe a close (EOF) from the broker.
    let mut probe = [0u8; 16];
    let n = timeout(Duration::from_secs(2), first.read(&mut probe))
        .await
        .expect("first socket should be closed by broker after takeover")
        .expect("read on closed socket");
    assert_eq!(n, 0, "first socket must read 0 bytes (EOF) after takeover");

    // The second session must still work: subscribe + receive from a third pub.
    second
        .write_all(&subscribe_packet(1, "takeover/check", 0))
        .await
        .unwrap();
    expect_suback(&mut second, 1, &[0x00]).await;

    let mut pubc = TcpStream::connect(&addr).await.unwrap();
    pubc.write_all(&connect_packet("pub-takeover"))
        .await
        .unwrap();
    expect_connack_ok(&mut pubc).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    pubc.write_all(&publish_packet("takeover/check", b"ok"))
        .await
        .unwrap();

    let pkt = timeout(Duration::from_secs(2), read_packet(&mut second))
        .await
        .expect("second (new) session must receive the publish");
    let body = &pkt[2..];
    let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
    assert_eq!(&body[2 + topic_len..], b"ok");
}
