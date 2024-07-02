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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use up_rust::{UCode, UStatus};

#[derive(Debug, Deserialize, Serialize)]
pub struct ApplicationConfig {
    pub(crate) name: String,
    #[serde(deserialize_with = "deserialize_hex_u16")]
    pub(crate) id: u16,
}

fn deserialize_hex_u16<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let hex_str = String::deserialize(deserializer)?;
    u16::from_str_radix(hex_str.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)
}

fn read_json_file(file_path: &PathBuf) -> Result<Value, serde_json::Error> {
    let mut file = match File::open(file_path) {
        Ok(file) => file,
        Err(e) => {
            return Err(serde_json::Error::io(e));
        }
    };

    let mut content = String::new();
    if let Err(e) = file.read_to_string(&mut content) {
        return Err(serde_json::Error::io(e));
    }

    serde_json::from_str(&content)
}

pub(crate) fn extract_applications(file_path: &PathBuf) -> Result<Vec<ApplicationConfig>, UStatus> {
    let file_content = read_json_file(file_path);

    return match file_content {
        Ok(json_data) => {
            if let Some(applications_value) =
                json_data.get("applications").and_then(|v| v.as_array())
            {
                match serde_json::from_value::<Vec<ApplicationConfig>>(Value::from(
                    applications_value.clone(),
                )) {
                    Ok(applications) => Ok(applications),
                    Err(e) => {
                        let err_msg = format!("Error deserializing 'applications': {:?}", e);
                        Err(UStatus::fail_with_code(UCode::INVALID_ARGUMENT, err_msg))
                    }
                }
            } else {
                let err_msg = format!("The 'applications' array is not found in the vsomeip configuration file: {file_path:?}");
                Err(UStatus::fail_with_code(UCode::INVALID_ARGUMENT, err_msg))
            }
        }
        Err(e) => {
            let err_msg = format!("Error reading JSON file: {:?}", e);
            Err(UStatus::fail_with_code(UCode::INVALID_ARGUMENT, err_msg))
        }
    };
}

// TODO: Add unit tests
