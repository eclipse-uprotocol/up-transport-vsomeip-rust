use async_trait::async_trait;
use hello_world_protos::hello_world_service::{HelloRequest, HelloResponse};
use log::{error, trace};
use std::fs::canonicalize;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use up_rust::UPayloadFormat::UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY;
use up_rust::{UListener, UMessage, UMessageBuilder, UStatus, UTransport, UUri};
use up_transport_vsomeip::UPTransportVsomeip;
use up_transport_vsomeip::UeId;

const HELLO_SERVICE_ID: u16 = 0x6000;
const HELLO_INSTANCE_ID: u32 = 0x0001;
const HELLO_METHOD_ID: u16 = 0x7FFF;

/// IMPORTANT: should match vsomeip::DEFAULT_MAJOR in (interface/vsomeip/constants.hpp):
// but Autosar works better if vsomeip::DEFAULT_MAJOR is 1 (thus Autosar needs custom vsomeip build)
const HELLO_SERVICE_MAJOR: u8 = 1;
const _HELLO_SERVICE_MINOR: u32 = 0;

const HELLO_SERVICE_AUTHORITY: &str = "linux";
const HELLO_SERVICE_UE_ID: u32 = (HELLO_INSTANCE_ID << 16u32) | HELLO_SERVICE_ID as u32;
const _HELLO_SERVICE_UE_VERSION_MAJOR: u8 = HELLO_SERVICE_MAJOR;
const HELLO_SERVICE_RESOURCE_ID: u16 = HELLO_METHOD_ID;

const CLIENT_AUTHORITY: &str = "me_authority";
const _CLIENT_UE_ID: u16 = 0x5678;
const _CLIENT_UE_VERSION_MAJOR: u8 = 1;
const _CLIENT_RESOURCE_ID: u16 = 0;

const _REQUEST_TTL: u32 = 1000;

struct ServiceRequestResponder {
    client: Arc<dyn UTransport>,
}
impl ServiceRequestResponder {
    pub fn new(client: Arc<dyn UTransport>) -> Self {
        Self { client }
    }
}
#[async_trait]
impl UListener for ServiceRequestResponder {
    async fn on_receive(&self, msg: UMessage) {
        println!("ServiceRequestResponder: Received a message: {msg:?}");

        let mut msg = msg.clone();

        if let Some(ref mut attributes) = msg.attributes.as_mut() {
            attributes.payload_format =
                ::protobuf::EnumOrUnknown::new(UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY);
        }

        let hello_request = msg.extract_protobuf_payload::<HelloRequest>();

        let hello_request = match hello_request {
            Ok(hello_request) => {
                println!("hello_request: {hello_request:?}");
                hello_request
            }
            Err(err) => {
                error!("Unable to parse HelloRequest: {err:?}");
                return;
            }
        };

        let hello_response = HelloResponse {
            message: format!("The response to the request: {}", hello_request.name),
            ..Default::default()
        };

        let response_msg = UMessageBuilder::response_for_request(msg.attributes.as_ref().unwrap())
            .build_with_wrapped_protobuf_payload(&hello_response)
            .unwrap();
        self.client.send(response_msg).await.unwrap();
    }

    async fn on_error(&self, err: UStatus) {
        println!("ServiceRequestResponder: Encountered an error: {err:?}");
    }
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    println!("mE_service");

    let crate_dir = env!("CARGO_MANIFEST_DIR");
    // TODO: Make configurable to pass the path to the vsomeip config as a command line argument
    let vsomeip_config = PathBuf::from(crate_dir).join("vsomeip_configs/hello_service.json");
    let vsomeip_config = canonicalize(vsomeip_config).ok();
    trace!("vsomeip_config: {vsomeip_config:?}");

    // There will be a single vsomeip_transport, as there is a connection into device and a streamer
    // TODO: Add error handling if we fail to create a UPTransportVsomeip
    let service: Arc<dyn UTransport> = Arc::new(
        UPTransportVsomeip::new_with_config(
            &HELLO_SERVICE_AUTHORITY.to_string(),
            &CLIENT_AUTHORITY.to_string(),
            HELLO_SERVICE_UE_ID as UeId,
            &vsomeip_config.unwrap(),
            None,
        )
        .unwrap(),
    );

    let source_filter = UUri {
        authority_name: "*".to_string(),
        ue_id: 0x0000_FFFF,
        ue_version_major: 0xFF,
        resource_id: 0xFFFF,
        ..Default::default()
    };
    let sink_filter = UUri {
        authority_name: HELLO_SERVICE_AUTHORITY.to_string(),
        ue_id: HELLO_SERVICE_UE_ID,
        ue_version_major: HELLO_SERVICE_MAJOR as u32,
        resource_id: HELLO_SERVICE_RESOURCE_ID as u32,
        ..Default::default()
    };
    let service_request_responder: Arc<dyn UListener> =
        Arc::new(ServiceRequestResponder::new(service.clone()));
    // TODO: Need to revisit how the vsomeip config file is used in non point-to-point cases
    service
        .register_listener(
            &source_filter,
            Some(&sink_filter),
            service_request_responder.clone(),
        )
        .await?;

    thread::park();
    Ok(())
}
