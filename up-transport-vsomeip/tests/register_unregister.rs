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

pub mod test_lib;
use up_transport_vsomeip::UPTransportVsomeip;

#[cfg(test)]
mod tests {
    use crate::test_lib::PrintingListener;
    use crate::{test_lib, UPTransportVsomeip};
    use log::error;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;
    use up_rust::{UListener, UTransport, UUri};

    // #[test]
    // fn test_constructing_client() {
    //     test_lib::before_test();
    //
    //     let client = UPTransportVsomeip::new(&"foo".to_string(), &"me_authority".to_string(), 10);
    //
    //     thread::sleep(Duration::from_millis(100));
    //
    //     assert!(client.is_ok());
    // }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_registering_unregistering_publish() {
        test_lib::before_test();

        let client =
            UPTransportVsomeip::new(&"foo".to_string(), &"me_authority".to_string(), 10, None)
                .unwrap();

        let source_filter = UUri {
            authority_name: "foo".to_string(),
            ue_id: 0x01,
            ue_version_major: 1,
            resource_id: 10,
            ..Default::default()
        };
        let printing_helper: Arc<dyn UListener> = Arc::new(PrintingListener);

        tokio::time::sleep(Duration::from_millis(100)).await;

        let reg_res = client
            .register_listener(&source_filter, None, printing_helper.clone())
            .await;

        tokio::time::sleep(Duration::from_millis(200)).await;

        if let Err(ref err) = reg_res {
            error!("Issue registering listener for publishes: {:?}", err);
        }

        assert!(reg_res.is_ok());

        let unreg_res = client
            .unregister_listener(&source_filter, None, printing_helper)
            .await;

        if let Err(ref err) = unreg_res {
            error!("Issue unregistering listener for publishes: {:?}", err);
        }

        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(unreg_res.is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_registering_unregistering_request() {
        test_lib::before_test();

        let client =
            UPTransportVsomeip::new(&"foo".to_string(), &"me_authority".to_string(), 10, None)
                .unwrap();

        let source_filter = UUri {
            authority_name: "foo".to_string(),
            ue_id: 0x01,
            ue_version_major: 1,
            resource_id: 10,
            ..Default::default()
        };
        let sink_filter = UUri {
            authority_name: "bar".to_string(),
            ue_id: 0x02,
            ue_version_major: 1,
            resource_id: 20,
            ..Default::default()
        };
        let printing_helper: Arc<dyn UListener> = Arc::new(PrintingListener);

        tokio::time::sleep(Duration::from_millis(100)).await;

        let reg_res = client
            .register_listener(&source_filter, Some(&sink_filter), printing_helper.clone())
            .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        if let Err(ref err) = reg_res {
            error!("Issue registering listener for publishes: {:?}", err);
        }

        assert!(reg_res.is_ok());

        let unreg_res = client
            .unregister_listener(&source_filter, Some(&sink_filter), printing_helper)
            .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        if let Err(ref err) = unreg_res {
            error!("Issue unregistering listener for publishes: {:?}", err);
        }

        assert!(unreg_res.is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_registering_unregistering_response() {
        test_lib::before_test();

        let client =
            UPTransportVsomeip::new(&"foo".to_string(), &"me_authority".to_string(), 10, None)
                .unwrap();

        let source_filter = UUri {
            authority_name: "foo".to_string(),
            ue_id: 0x01,
            ue_version_major: 1,
            resource_id: 10,
            ..Default::default()
        };
        let sink_filter = UUri {
            authority_name: "bar".to_string(),
            ue_id: 0x02,
            ue_version_major: 1,
            resource_id: 0,
            ..Default::default()
        };
        let printing_helper: Arc<dyn UListener> = Arc::new(PrintingListener);

        tokio::time::sleep(Duration::from_millis(100)).await;

        let reg_res = client
            .register_listener(&source_filter, Some(&sink_filter), printing_helper.clone())
            .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        if let Err(ref err) = reg_res {
            error!("Issue registering listener for publishes: {:?}", err);
        }

        assert!(reg_res.is_ok());

        tokio::time::sleep(Duration::from_millis(100)).await;

        let unreg_res = client
            .unregister_listener(&source_filter, Some(&sink_filter), printing_helper)
            .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        if let Err(ref err) = unreg_res {
            error!("Issue unregistering listener for publishes: {:?}", err);
        }

        assert!(unreg_res.is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_registering_unregistering_all_point_to_point() {
        test_lib::before_test();

        let client = UPTransportVsomeip::new_with_config(
            &"foo".to_string(),
            &"me_authority".to_string(),
            10,
            Path::new("vsomeip_configs/example_ustreamer.json"),
            None,
        )
        .unwrap();

        let source_filter = UUri {
            authority_name: "*".to_string(),
            ue_id: 0x0000_FFFF,
            ue_version_major: 0xFF,
            resource_id: 0xFFFF,
            ..Default::default()
        };
        let sink_filter = UUri {
            authority_name: "me_authority".to_string(),
            ue_id: 0x0000_FFFF,
            ue_version_major: 0xFF,
            resource_id: 0xFFFF,
            ..Default::default()
        };
        let printing_helper: Arc<dyn UListener> = Arc::new(PrintingListener);

        tokio::time::sleep(Duration::from_millis(100)).await;

        let reg_res = client
            .register_listener(&source_filter, Some(&sink_filter), printing_helper.clone())
            .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        if let Err(ref err) = reg_res {
            error!("Issue registering listener for publishes: {:?}", err);
        }

        assert!(reg_res.is_ok());

        tokio::time::sleep(Duration::from_millis(100)).await;

        let unreg_res = client
            .unregister_listener(&source_filter, Some(&sink_filter), printing_helper)
            .await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        if let Err(ref err) = unreg_res {
            error!("Issue unregistering listener for publishes: {:?}", err);
        }

        assert!(unreg_res.is_ok());
    }
}
