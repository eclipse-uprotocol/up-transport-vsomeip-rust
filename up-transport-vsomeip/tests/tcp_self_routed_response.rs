use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use up_rust::UCode;
use up_rust::{UListener, UMessage, UMessageBuilder, UPayloadFormat, UTransport, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const PORT: u16 = 30509;

enum Ev {
    Connected,
    ReqSent,
    RespReceived {
        client_id: u16,
        session_id: u16,
        return_code: u8,
        payload: Vec<u8>,
    },
    Failed(String),
}

struct RawClient {
    rx: Receiver<Ev>,
    _t: std::thread::JoinHandle<()>,
}

impl RawClient {
    fn start(client_id: u16, session_id: u16) -> Self {
        let (tx, rx) = mpsc::channel();

        let t = std::thread::spawn(move || {
            let mut s = loop {
                match TcpStream::connect(format!("127.0.0.1:{}", PORT)) {
                    Ok(s) => break s,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            };
            let local_addr = s.local_addr().unwrap();
            let peer_addr = s.peer_addr().unwrap();
            let is_ip = local_addr.is_ipv4() || local_addr.is_ipv6();

            println!(">>> [TCP CLIENT] System Socket Verification:");
            println!("    - Local OS socket : {}", local_addr);
            println!("    - Remote OS socket: {}", peer_addr);
            println!(
                "    - Protocol Stack  : {}",
                if is_ip {
                    "TCP/IP (Network Stack)"
                } else {
                    "IPC"
                }
            );

            // Assert mathematically from the OS that this is a TCP/IP socket, not an IPC socket
            assert!(
                is_ip,
                "The socket must be a TCP/IP socket, but an IPC was detected!"
            );

            tx.send(Ev::Connected).ok();

            // Send REQUEST
            let svc = 0x1234u16.to_be_bytes();
            let meth = 0x0421u16.to_be_bytes();
            let c_id = client_id.to_be_bytes();
            let s_id = session_id.to_be_bytes();

            let req = vec![
                svc[0], svc[1], meth[0], meth[1], 0, 0, 0,
                8, // length (8 bytes after offset 7)
                c_id[0], c_id[1], s_id[0], s_id[1], 1, 1, 0x00,
                0x00, // type=0x00 REQUEST, rc=0
            ];

            s.write_all(&req).expect("write");
            tx.send(Ev::ReqSent).ok();

            // Receive RESPONSE
            let mut hdr = [0u8; 16];
            if let Err(error) = s.read_exact(&mut hdr) {
                tx.send(Ev::Failed(format!("failed to read response header: {error}")))
                    .ok();
                return;
            }

            let message_length = u32::from_be_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
            let Some(payload_length) = message_length.checked_sub(8) else {
                tx.send(Ev::Failed(format!(
                    "invalid SOME/IP response length: {message_length}"
                )))
                .ok();
                return;
            };
            let mut payload = vec![0; payload_length];
            if let Err(error) = s.read_exact(&mut payload) {
                tx.send(Ev::Failed(format!("failed to read response payload: {error}")))
                    .ok();
                return;
            }

            tx.send(Ev::RespReceived {
                client_id: u16::from_be_bytes([hdr[8], hdr[9]]),
                session_id: u16::from_be_bytes([hdr[10], hdr[11]]),
                return_code: hdr[15],
                payload,
            })
            .ok();
        });

        RawClient { rx, _t: t }
    }

    fn wait(&self) -> Option<Ev> {
        self.rx.recv_timeout(Duration::from_secs(5)).ok()
    }
}

struct MyListener {
    transport: Arc<UPTransportVsomeip>,
    count: AtomicUsize,
    result_tx: Mutex<Option<tokio::sync::oneshot::Sender<Result<UUri, String>>>>,
}

#[async_trait::async_trait]
impl UListener for MyListener {
    async fn on_receive(&self, msg: UMessage) {
        self.count.fetch_add(1, Ordering::SeqCst);

        let result = async {
            let req_source = msg
                .attributes
                .source
                .as_ref()
                .ok_or("request has no source")?
                .clone();
            let req_sink = msg
                .attributes
                .sink
                .as_ref()
                .ok_or("request has no sink")?
                .clone();
            let reqid = msg
                .attributes
                .id
                .as_ref()
                .ok_or("request has no ID")?
                .clone();

            println!("\n--------------------------------------------------");
            println!(">>> [UPROTOCOL APP] 📥 UMessage REQUEST received from Transport:");
            println!("    - Request SOURCE (Sender): {:#x}", req_source.ue_id);
            println!("    - Request SINK (Dest)  : {:#x}", req_sink.ue_id);

            // Generate response by swapping source and sink
            let resp_sink = req_source.clone();
            let resp_source = req_sink;

            println!(
                ">>> [UPROTOCOL APP] 📤 Generating UMessage RESPONSE (swapping source/sink):"
            );
            println!("    - Response SOURCE (Sender): {:#x}", resp_source.ue_id);
            println!("    - Response SINK (Dest)  : {:#x}", resp_sink.ue_id);
            println!("--------------------------------------------------\n");

            let resp = UMessageBuilder::response(resp_sink, reqid, resp_source)
                .with_comm_status(UCode::OK)
                .build_with_payload(vec![1, 2, 3], UPayloadFormat::UPAYLOAD_FORMAT_RAW)
                .map_err(|error| format!("failed to build application response: {error}"))?;

            self.transport
                .send(resp)
                .await
                .map_err(|error| format!("failed to send application response: {error}"))?;

            Ok(req_source)
        }
        .await;

        if let Some(result_tx) = self.result_tx.lock().unwrap().take() {
            let _ = result_tx.send(result);
        }
    }
}

async fn build_service() -> (Arc<UPTransportVsomeip>, UUri, UUri) {
    let cfg = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vsomeip_configs/tcp_service.json");
    std::env::set_var("VSOMEIP_CONFIGURATION", cfg.to_str().unwrap());
    let t = Arc::new(
        UPTransportVsomeip::new_with_config(
            UUri::try_from_parts("foo", 0x1234u32, 1u8, 0u16).unwrap(),
            &"foo".to_string(),
            &cfg,
            None,
        )
        .expect("start transport"),
    );
    let client = UUri::any_with_resource_id(0);
    let service = UUri::try_from_parts("foo", 0x1234u32, 1u8, 0x0421u16).unwrap();
    (t, client, service)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_self_routed_response() {
    let _ = tracing_subscriber::fmt::try_init();

    let expected_client_id = 0x9999;
    let expected_session_id = 0x1111;
    let (service_transport, cu, su) = build_service().await;
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
    let listener = Arc::new(MyListener {
        transport: service_transport.clone(),
        count: AtomicUsize::new(0),
        result_tx: Mutex::new(Some(result_tx)),
    });

    // Register listener for REQUESTs (source = client, sink = service)
    service_transport
        .register_listener(&cu, Some(&su), listener.clone() as _)
        .await
        .expect("reg");

    // Wait for vsomeip to bind
    tokio::time::sleep(Duration::from_millis(800)).await;

    // Connect RawClient to vsomeip service
    let client = RawClient::start(expected_client_id, expected_session_id);

    assert!(matches!(client.wait(), Some(Ev::Connected)));
    assert!(matches!(client.wait(), Some(Ev::ReqSent)));

    let converted_source = tokio::time::timeout(Duration::from_secs(5), result_rx)
        .await
        .expect("listener did not process the converted request")
        .expect("listener dropped the request result")
        .expect("listener failed to send the application response");
    assert_eq!(
        converted_source.uentity_type_id(),
        expected_client_id,
        "request source must contain the original vSomeIP Client ID"
    );

    match client.wait() {
        Some(Ev::RespReceived {
            client_id,
            session_id,
            return_code,
            payload,
        }) => {
            assert_eq!(session_id, expected_session_id);
            assert_eq!(
                client_id, expected_client_id,
                "BUG: vsomeip overwrote the Client ID with its own local ID!"
            );
            assert_eq!(return_code, 0x00, "expected SOME/IP E_OK response");
            assert_eq!(payload, [1, 2, 3], "unexpected application payload");
        }
        Some(Ev::Failed(error)) => panic!("Failed to receive application response: {error}"),
        _ => panic!("Did not receive response"),
    }
}
