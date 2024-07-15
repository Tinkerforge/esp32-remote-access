use serde::Serialize;

use crate::utils::as_u8_slice;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum ManagementCommandId {
    Connect,
    Disconnect,
    Ack,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ManagementCommand {
    pub command_id: ManagementCommandId,
    pub connection_no: i32,
    pub connection_uuid: u128,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ManagementPacketHeader {
    pub magic: u16,
    pub length: u16,
    pub seq_number: u16,
    pub version: u8,
    /*
        0x00 - Management Command
    */
    pub p_type: u8,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ManagementCommandPacket {
    pub header: ManagementPacketHeader,
    pub command: ManagementCommand,
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Serialize)]
#[repr(C, packed)]
pub struct ManagementResponse {
    pub charger_id: i32,
    pub connection_no: i32,
    pub connection_uuid: u128,
}

pub enum ManagementPacket {
    CommandPacket(ManagementCommandPacket),
}

impl ManagementPacket {
    pub fn as_bytes(&mut self) -> &[u8] {
        as_u8_slice(self)
    }

    fn get_header(&mut self) -> &mut ManagementPacketHeader {
        match self {
            Self::CommandPacket(p) => &mut p.header,
        }
    }

    pub fn set_seq_num(&mut self, num: u16) {
        let header = self.get_header();
        header.seq_number = num;
    }
}
