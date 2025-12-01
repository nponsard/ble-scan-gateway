#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
use heapless::Vec;
use serde::{Deserialize, Serialize};
#[cfg(feature = "std")]
use std::vec::Vec;

pub const MAX_SEEN: usize = 32;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddressesSeen {
    #[cfg(feature = "std")]
    pub addrs: Vec<DetectedTag>,
    #[cfg(not(feature = "std"))]
    pub addrs: Vec<DetectedTag, MAX_SEEN>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f32,
    pub longitude: f32,
    pub altitude: f32,
    pub heading: f32,
    #[serde(rename = "horizontalSpeed")]
    pub horizontal_speed: f32,
    #[serde(rename = "verticalSpeed")]
    pub vertical_spedd: f32,
    #[serde(rename = "timeOfFix")]
    pub time_of_fix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectedTag {
    pub id: heapless::String<64>,
    pub age: u16,
    pub rssi: i8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GatewayUpdate {
    #[cfg(feature = "std")]
    #[serde(rename = "gatewayId")]
    pub gateway_id: String,
    #[cfg(not(feature = "std"))]
    #[serde(rename = "gatewayId")]
    pub gateway_id: heapless::String<64>,
    pub timestamp: i64,

    #[cfg(feature = "std")]
    #[serde(rename = "detectedTags")]
    pub detected_tags: Vec<DetectedTag>,
    #[cfg(not(feature = "std"))]
    #[serde(rename = "detectedTags")]
    pub detected_tags: Vec<DetectedTag, MAX_SEEN>,

    #[serde(rename = "batteryLevel")]
    pub battery_level: Option<u8>,
    pub location: Option<Location>,
}
