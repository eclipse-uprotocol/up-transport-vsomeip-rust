/********************************************************************************
 * Copyright (c) 2023 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use log::{info, trace};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use up_rust::{UListener, UMessage, UMessageBuilder, UPayloadFormat, UTransport, UUri};
use up_transport_vsomeip::{UPTransportVsomeip, VsomeipApplicationConfig};

const TEST_DURATION: u64 = 1000;
const MAX_ITERATIONS: usize = 100;

pub struct SubscriberListener {
    received_publish: AtomicUsize,
}
impl SubscriberListener {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            received_publish: AtomicUsize::new(0),
        }
    }

    pub fn received_publish(&self) -> usize {
        self.received_publish.load(Ordering::SeqCst)
    }
}
#[async_trait::async_trait]
impl UListener for SubscriberListener {
    async fn on_receive(&self, msg: UMessage) {
        trace!("{:?}", msg);
        self.received_publish.fetch_add(1, Ordering::SeqCst);

        let Some(payload_bytes) = msg.payload else {
            panic!("No bytes included in payload");
        };

        let Ok(payload_string) = std::str::from_utf8(&payload_bytes) else {
            panic!("Unable to convert back to payload_string");
        };

        info!("We received payload_string: {payload_string}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn publisher_subscriber() {
    env_logger::init();

    let authority_name = "foo";

    let ue_id = 10;
    let subscriber_ue_id = 20;
    let ue_version_major = 1;
    let resource_id = 0x8001;

    let publisher_topic =
        UUri::try_from_parts(authority_name, ue_id, ue_version_major, resource_id).unwrap();

    let vsomeip_application_config_subscriber =
        VsomeipApplicationConfig::new("subscriber_app", 0x344);
    let subscriber_uri =
        UUri::try_from_parts(authority_name, subscriber_ue_id, ue_version_major, 0).unwrap();

    let subscriber_res = UPTransportVsomeip::new(
        vsomeip_application_config_subscriber,
        subscriber_uri,
        &"me_authority".to_string(),
        None,
    );

    let Ok(subscriber) = subscriber_res else {
        panic!("Unable to establish subscriber");
    };

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let subscriber_listener_check = Arc::new(SubscriberListener::new());
    let subscriber_listener: Arc<dyn UListener> = subscriber_listener_check.clone();

    let reg_res = subscriber
        .register_listener(&publisher_topic, None, subscriber_listener)
        .await;

    if let Err(err) = reg_res {
        panic!("Unable to register: {:?}", err);
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let vsomeip_application_config_publisher =
        VsomeipApplicationConfig::new("publisher_app", 0x343);
    let publisher_uri = UUri::try_from_parts(authority_name, ue_id, 1, 0).unwrap();
    let publisher_res = UPTransportVsomeip::new(
        vsomeip_application_config_publisher,
        publisher_uri,
        &"me_authority".to_string(),
        None,
    );

    let Ok(publisher) = publisher_res else {
        panic!("Unable to establish publisher");
    };

    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Track the start time and set the duration for the loop
    let duration = Duration::from_millis(TEST_DURATION);
    let start_time = Instant::now();

    let mut iterations = 0;
    while (Instant::now().duration_since(start_time) < duration) && (iterations < MAX_ITERATIONS) {
        let publish_payload_string = format!("publish_message@i={iterations}");
        let publish_payload = publish_payload_string.into_bytes();

        let publish_msg_res = UMessageBuilder::publish(publisher_topic.clone())
            .build_with_payload(publish_payload, UPayloadFormat::UPAYLOAD_FORMAT_TEXT);

        let Ok(publish_msg) = publish_msg_res else {
            panic!(
                "Unable to create Publish UMessage: {:?}",
                publish_msg_res.err().unwrap()
            );
        };

        trace!("Publish message we're about to send:\n{publish_msg:?}");

        let send_res = publisher.send(publish_msg).await;

        if let Err(err) = send_res {
            panic!("Unable to send Publish UMessage: {:?}", err);
        }

        iterations += 1;
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;

    println!("iterations: {}", iterations);

    println!(
        "subscriber_listener_check.received_publish(): {}",
        subscriber_listener_check.received_publish()
    );

    assert_eq!(iterations, subscriber_listener_check.received_publish());
}
