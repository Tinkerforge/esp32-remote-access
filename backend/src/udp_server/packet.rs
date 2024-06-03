

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

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ManagementResponse {
    pub charger_id: i32,
    pub connection_no: i32,
    pub connection_uuid: u128,
}
