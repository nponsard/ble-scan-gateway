#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
use heapless::Vec;
use serde::{Deserialize, Serialize};
#[cfg(feature = "std")]
use std::vec::Vec;

pub const MAX_SEEN: usize = 128;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddressesSeen {
    #[cfg(feature = "std")]
    pub addrs: Vec<[u8; 6]>,
    #[cfg(not(feature = "std"))]
    pub addrs: Vec<[u8; 6], MAX_SEEN>,
}

#[cfg(feature = "from-impl")]
impl From<Vec<bt_hci::param::BdAddr, MAX_SEEN>> for AddressesSeen {
    fn from(input: Vec<bt_hci::param::BdAddr, MAX_SEEN>) -> Self {
        Self {
            addrs: input.iter().map(|a| a.into_inner()).collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f32,
    pub longitude: f32,
    pub altitude: f32,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GatewayUpdate {
    pub location: Option<Location>,
    #[cfg(feature = "std")]
    pub seen: Vec<[u8; 6]>,
    #[cfg(not(feature = "std"))]
    pub seen: Vec<[u8; 6], MAX_SEEN>,
}
