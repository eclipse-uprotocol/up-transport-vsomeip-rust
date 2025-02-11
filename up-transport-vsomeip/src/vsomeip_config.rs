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

use crate::{ClientId, InstanceId, ServiceId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use up_rust::{UCode, UStatus};

#[derive(Debug, Deserialize, Serialize)]
pub struct ServiceConfig {
    #[serde(deserialize_with = "deserialize_hex_u16")]
    pub(crate) service: ServiceId,
    #[serde(deserialize_with = "deserialize_hex_u16")]
    pub(crate) instance: InstanceId,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VsomeipApplicationConfig {
    pub(crate) name: String,
    #[serde(deserialize_with = "deserialize_hex_u16")]
    pub(crate) id: ClientId,
}

impl VsomeipApplicationConfig {
    pub fn new(name: &str, id: ClientId) -> Self {
        // TODO: - PELE - Add validation that we have supplied a valid application_name
        // and application_id according to vsomeip spec

        Self {
            name: name.to_string(),
            id,
        }
    }
}

fn deserialize_hex_u16<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let hex_str = String::deserialize(deserializer)?;
    u16::from_str_radix(hex_str.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)
}

fn read_json_file(file_path: &Path) -> Result<Value, serde_json::Error> {
    let mut file = File::open(file_path).map_err(|e| {
        println!(" Failed to open the file path: {:?}", e);
        serde_json::Error::io(e)
    })?;

    let mut content = String::new();
    file.read_to_string(&mut content).map_err(|e| {
        println!(" Failed to read the file: {:?}", e);
        serde_json::Error::io(e)
    })?;

    let parsed = serde_json::from_str(&content).map_err(|e| {
        println!(" Failed to parse JSON file  : {:?}", e);
        e
    })?;

    Ok(parsed)
}

pub(crate) fn extract_application(file_path: &Path) -> Result<VsomeipApplicationConfig, UStatus> {
    let json_data = read_json_file(file_path).map_err(|e| {
        UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!("Failed to read JSON File: {e}"),
        )
    })?;

    let applications = json_data
        .get("applications")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("'applications' array is not Found: {:?}", file_path),
            )
        })?;

    if applications.is_empty() {
        return Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            "applications array is Empty",
        ));
    }

    let app_config: VsomeipApplicationConfig = serde_json::from_value(applications[0].clone())
        .map_err(|e| UStatus::fail_with_code(UCode::INVALID_ARGUMENT, format!(": {e}")))?;

    Ok(app_config)
}

pub(crate) fn extract_services(file_path: &Path) -> Result<Vec<ServiceConfig>, UStatus> {
    let json_data = read_json_file(file_path).map_err(|e| {
        UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!("Failed to read JSON File: {e}"),
        )
    })?;

    let services = json_data
        .get("services")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("'services' Array is not Found : {:?}", file_path),
            )
        })?;

    if services.is_empty() {
        return Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            "services array is Empty",
        ));
    }

    let service_configs: Vec<ServiceConfig> =
        serde_json::from_value(Value::Array(services.clone())).map_err(|e| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("Failed to change service: {e}"),
            )
        })?;

    Ok(service_configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_temp_file_with_content(content: &str) -> std::path::PathBuf {
        let temp_path = std::env::temp_dir().join("test_config.json");

        let mut file = File::create(&temp_path).expect("Failed to create template File");
        write!(file, "{}", content).expect("Failed to Write tempfile");
        file.flush().expect("Failed to file ");

        temp_path
    }

    #[test]
    fn test_deserialize_hex_u16() {
        let data = "\"0x1A3F\"";
        let deserialized: Result<u16, _> = serde_json::from_str(data);
        assert!(
            deserialized.is_err(),
            "Expected an error due to string type"
        );
    }

    #[test]
    fn test_read_json_file() {
        let content = r#"{ "key": "value" }"#;
        let file_path = create_temp_file_with_content(content);
        let result = read_json_file(&file_path);
        assert!(result.is_ok(), "Failed to read JSON file");
        assert_eq!(result.unwrap(), serde_json::json!({"key": "value"}));
    }

    #[test]
    fn test_extract_application() {
        let content = r#"{
             "applications": [
                 { 
                     "name": "TestApp", 
                     "id": "0x1234"
                 }
             ]
         }"#;
        let file_path = create_temp_file_with_content(content);
        let result = extract_application(&file_path);
        assert!(result.is_ok(), "Failed to extract application");
        let config = result.unwrap();
        assert_eq!(config.name, "TestApp");
        assert_eq!(config.id, 0x1234);
    }

    #[test]
    fn test_extract_services() {
        let content = r#"{
             "services": [
                 { 
                     "service": "0xABCD",
                     "instance": "0x1234"
                 }
             ]
         }"#;
        let file_path = create_temp_file_with_content(content);
        let result = extract_services(&file_path);
        assert!(result.is_ok(), "Failed to extract services");

        let services = result.unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].service, 0xABCD);
        assert_eq!(services[0].instance, 0x1234);
    }
}
