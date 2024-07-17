use async_trait::async_trait;
use hello_world_protos::hello_world_service::{HelloRequest, HelloResponse};
use log::trace;
use std::fs::canonicalize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use up_rust::UPayloadFormat::UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY;
use up_rust::{UListener, UMessage, UMessageBuilder, UStatus, UTransport, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const HELLO_SERVICE_ID: u16 = 0x6000;
const HELLO_INSTANCE_ID: u16 = 0x0001;
const HELLO_METHOD_ID: u16 = 0x7FFF;

/// IMPORTANT: should match vsomeip::DEFAULT_MAJOR in (interface/vsomeip/constants.hpp):
// but Autosar works better if vsomeip::DEFAULT_MAJOR is 1 (thus Autosar needs custom vsomeip build)
const HELLO_SERVICE_MAJOR: u8 = 1;
const _HELLO_SERVICE_MINOR: u32 = 0;

const HELLO_SERVICE_AUTHORITY: &str = "linux";
const HELLO_SERVICE_UE_ID: u32 = (HELLO_INSTANCE_ID as u32) << 16 | HELLO_SERVICE_ID as u32;
const HELLO_SERVICE_UE_VERSION_MAJOR: u8 = HELLO_SERVICE_MAJOR;
const HELLO_SERVICE_RESOURCE_ID: u16 = HELLO_METHOD_ID;

const CLIENT_AUTHORITY: &str = "me_authority";
const CLIENT_UE_ID: u16 = 0x5678;
const CLIENT_UE_VERSION_MAJOR: u8 = 1;
const CLIENT_RESOURCE_ID: u16 = 0;

const REQUEST_TTL: u32 = 1000;

struct ServiceResponseListener;

#[async_trait]
impl UListener for ServiceResponseListener {
    async fn on_receive(&self, msg: UMessage) {
        println!("ServiceResponseListener: Received a message: {msg:?}");

        let mut msg = msg.clone();

        if let Some(ref mut attributes) = msg.attributes.as_mut() {
            attributes.payload_format =
                ::protobuf::EnumOrUnknown::new(UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY);
        }

        let Ok(hello_response) = msg.extract_protobuf_payload::<HelloResponse>() else {
            panic!("Unable to parse into HelloResponse");
        };

        println!("Here we received response: {hello_response:?}");
    }

    async fn on_error(&self, err: UStatus) {
        println!("ServiceResponseListener: Encountered an error: {err:?}");
    }
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    println!("mE_client");

    let crate_dir = env!("CARGO_MANIFEST_DIR");
    // TODO: Make configurable to pass the path to the vsomeip config as a command line argument
    let vsomeip_config = PathBuf::from(crate_dir).join("vsomeip_configs/hello_service.json");
    let vsomeip_config = canonicalize(vsomeip_config).ok();
    trace!("vsomeip_config: {vsomeip_config:?}");

    // There will be a single vsomeip_transport, as there is a connection into device and a streamer
    // TODO: Add error handling if we fail to create a UPTransportVsomeip
    let client: Arc<dyn UTransport> = Arc::new(
        UPTransportVsomeip::new_with_config(
            &CLIENT_AUTHORITY.to_string(),
            &HELLO_SERVICE_AUTHORITY.to_string(),
            CLIENT_UE_ID,
            &vsomeip_config.unwrap(),
            None,
        )
        .unwrap(),
    );

    let source = UUri {
        authority_name: CLIENT_AUTHORITY.to_string(),
        ue_id: CLIENT_UE_ID as u32,
        ue_version_major: CLIENT_UE_VERSION_MAJOR as u32,
        resource_id: CLIENT_RESOURCE_ID as u32,
        ..Default::default()
    };
    let sink = UUri {
        authority_name: HELLO_SERVICE_AUTHORITY.to_string(),
        ue_id: HELLO_SERVICE_UE_ID,
        ue_version_major: HELLO_SERVICE_UE_VERSION_MAJOR as u32,
        resource_id: HELLO_SERVICE_RESOURCE_ID as u32,
        ..Default::default()
    };

    let service_response_listener: Arc<dyn UListener> = Arc::new(ServiceResponseListener);
    client
        .register_listener(&sink, Some(&source), service_response_listener)
        .await?;

    let mut i = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let hello_request = HelloRequest {
            name: format!("me_client@i={}", i).to_string(),
            ..Default::default()
        };
        i += 1;

        let request_msg = UMessageBuilder::request(sink.clone(), source.clone(), REQUEST_TTL)
            .build_with_protobuf_payload(&hello_request)
            .unwrap();
        println!("Sending Request message:\n{request_msg:?}");

        client.send(request_msg).await?;
    }
}
