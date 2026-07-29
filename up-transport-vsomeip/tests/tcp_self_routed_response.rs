use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;
use up_rust::UCode;
use up_rust::{UListener, UMessage, UMessageBuilder, UPayloadFormat, UTransport, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const PORT: u16 = 30509;

enum Ev {
    Connected,
    ReqSent,
    RespReceived { client_id: u16, session_id: u16 },
    Closed,
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
            println!("    - Protocol Stack  : {}", if is_ip { "TCP/IP (Network Stack)" } else { "IPC" });
            
            // Assert mathematically from the OS that this is a TCP/IP socket, not an IPC socket
            assert!(is_ip, "The socket must be a TCP/IP socket, but an IPC was detected!");
            
            tx.send(Ev::Connected).ok();

            // Send REQUEST
            let svc = 0x1234u16.to_be_bytes();
            let meth = 0x0421u16.to_be_bytes();
            let c_id = client_id.to_be_bytes();
            let s_id = session_id.to_be_bytes();
            
            let req = vec![
                svc[0], svc[1], meth[0], meth[1],
                0, 0, 0, 8, // length (8 bytes after offset 7)
                c_id[0], c_id[1], s_id[0], s_id[1],
                1, 1, 0x00, 0x00, // type=0x00 REQUEST, rc=0
            ];

          s.write_all(&req).expect("write");
            tx.send(Ev::ReqSent).ok();

            // Receive RESPONSE
            let mut hdr = [0u8; 16];
            if s.read_exact(&mut hdr).is_ok() {
                let r_c_id = u16::from_be_bytes([hdr[8], hdr[9]]);
                let r_s_id = u16::from_be_bytes([hdr[10], hdr[11]]);
                tx.send(Ev::RespReceived { client_id: r_c_id, session_id: r_s_id }).ok();
            } else {
                tx.send(Ev::Closed).ok();
            }
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
}

#[async_trait::async_trait]
impl UListener for MyListener {
    async fn on_receive(&self, msg: UMessage) {
        self.count.fetch_add(1, Ordering::SeqCst);
        
        let req_source = msg.attributes.source.as_ref().unwrap().clone();
        let req_sink = msg.attributes.sink.as_ref().unwrap().clone();
        let reqid = msg.attributes.id.as_ref().unwrap().clone();
        
        println!("\n--------------------------------------------------");
        println!(">>> [UPROTOCOL APP] 📥 UMessage REQUEST received from Transport:");
        println!("    - Request SOURCE (Sender): {:#x}", req_source.ue_id);
        println!("    - Request SINK (Dest)  : {:#x}", req_sink.ue_id);
        
        if req_source.ue_id == 0x1234 {
            println!("    ❌ BUG DETECTED! Request SOURCE is the Server's own ID (0x1234).");
            panic!("REGRESSION: The ue_id must be mapped to the original vSomeIP Client ID, not the Server ID.");
        } else {
            println!("    ✅ FIX SUCCESSFUL! Request SOURCE is correctly mapped to Client ID ({:#x}).", req_source.ue_id);
        }

        // Generate response by swapping source and sink
        let resp_sink = req_source;
        let resp_source = req_sink;

        println!(">>> [UPROTOCOL APP] 📤 Generating UMessage RESPONSE (swapping source/sink):");
        println!("    - Response SOURCE (Sender): {:#x}", resp_source.ue_id);
        println!("    - Response SINK (Dest)  : {:#x}", resp_sink.ue_id);
        println!("--------------------------------------------------\n");

        let resp = UMessageBuilder::response(resp_sink, reqid, resp_source)
            .with_comm_status(UCode::OK)
            .build_with_payload(vec![1, 2, 3], UPayloadFormat::UPAYLOAD_FORMAT_RAW)
            .unwrap();

        self.transport.send(resp).await.unwrap();
    }
}

async fn build_service() -> (Arc<UPTransportVsomeip>, UUri, UUri) {
    let cfg = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("vsomeip_configs/tcp_service.json");
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
    let client = UUri::try_from_parts("bar", 0x0345u32, 1u8, 0u16).unwrap();
    let service = UUri::try_from_parts("foo", 0x1234u32, 1u8, 0x0421u16).unwrap();
    (t, client, service)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_self_routed_response() {
    let _ = env_logger::builder().is_test(true).try_init();

    let (service_transport, cu, su) = build_service().await;
    let listener = Arc::new(MyListener { transport: service_transport.clone(), count: AtomicUsize::new(0) });
    
    // Register listener for REQUESTs (source = client, sink = service)
    service_transport
        .register_listener(&cu, Some(&su), listener.clone() as _)
        .await
        .expect("reg");

    // Wait for vsomeip to bind
    tokio::time::sleep(Duration::from_millis(800)).await;

    // Connect RawClient to vsomeip service
    let expected_client_id = 0x9999;
    let expected_session_id = 0x1111;
    let client = RawClient::start(expected_client_id, expected_session_id);

    assert!(matches!(client.wait(), Some(Ev::Connected)));
    assert!(matches!(client.wait(), Some(Ev::ReqSent)));

    match client.wait() {
        Some(Ev::RespReceived { client_id, session_id }) => {
            assert_eq!(session_id, expected_session_id);
            assert_eq!(client_id, expected_client_id, "BUG: vsomeip overwrote the Client ID with its own local ID!");
        }
        _ => panic!("Did not receive response"),
    }
}
