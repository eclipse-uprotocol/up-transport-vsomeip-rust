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

use log::trace;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use up_rust::{UListener, UMessage, UMessageBuilder, UStatus, UTransport, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const TEST_SLACK: usize = 1;

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
    }

    async fn on_error(&self, err: UStatus) {
        trace!("{:?}", err);
    }
}

#[tokio::test]
async fn publisher_subscriber() {
    env_logger::init();

    let authority_name = "foo";

    let ue_id = 10;
    let subscriber_ue_id = 20;
    let ue_version_major = 1;
    let resource_id = 0x8001;

    let publisher_topic = UUri {
        authority_name: authority_name.to_string(),
        ue_id: ue_id as u32,
        ue_version_major,
        resource_id,
        ..Default::default()
    };

    let subscriber_res = UPTransportVsomeip::new(&authority_name.to_string(), subscriber_ue_id);

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

    let publisher_res = UPTransportVsomeip::new(&authority_name.to_string(), ue_id);

    let Ok(publisher) = publisher_res else {
        panic!("Unable to establish publisher");
    };

    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Track the start time and set the duration for the loop
    let duration = Duration::from_millis(1000);
    let start_time = Instant::now();

    let mut iterations = 0;
    while Instant::now().duration_since(start_time) < duration {
        let publish_msg_res = UMessageBuilder::publish(publisher_topic.clone()).build();

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

    println!(
        "subscriber_listener_check.received_publish(): {}",
        subscriber_listener_check.received_publish()
    );

    assert!(iterations - subscriber_listener_check.received_publish() <= TEST_SLACK);
}
