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

use std::sync::Arc;
use async_trait::async_trait;

use up_rust::{UTransport, UMessage, UStatus, UUri, UCode, UListener};

use crate::UPClientVsomeip;

#[async_trait]
impl UTransport for UPClientVsomeip {
    async fn send(&self, message: UMessage) -> Result<(), UStatus> {
        // implementation goes here
        println!("Sending message: {:?}", message);

        Ok(())
    }

    async fn register_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        _listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        // implementation goes here
        let sink_filter_str = {
            if let Some(sink_filter) = sink_filter {
                format!("{sink_filter:?}")
            } else {
                "".parse().unwrap()
            }
        };
        println!("Registering listener for source filter: {:?}{}", source_filter, sink_filter_str);

        Ok(())
    }

    async fn unregister_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        _listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        // implementation goes here
        let sink_filter_str = {
            if let Some(sink_filter) = sink_filter {
                format!("{sink_filter:?}")
            } else {
                "".parse().unwrap()
            }
        };
        println!("Unregistering listener for source filter: {:?}{}", source_filter, sink_filter_str);

        Ok(())
    }

    async fn receive(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<UMessage, UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "This method is not implemented for vsomeip. Use register_listener instead.",
        ))
    }
}
