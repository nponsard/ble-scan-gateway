#![no_std]

use heapless::Vec;

pub const MAX_SEEN: usize = 128;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct AddressesSeen {
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
