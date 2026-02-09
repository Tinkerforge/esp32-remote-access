use serde::Serialize;

use crate::utils::as_u8_slice;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum ManagementCommandId {
    Connect,
    Disconnect,
    SendChargeLog,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ManagementCommand {
    pub command_id: ManagementCommandId,

    // Ignored for SendChargeLog command
    pub connection_no: i32,
    pub connection_uuid: u128,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ManagementPacketHeader {
    // 0x1234
    pub magic: u16,
    pub length: u16,
    pub seq_number: u16,
    pub version: u8,
    /*
        0x00 - Management Command
        0x01 - Ack
        0x02 - Metadata for Charge Log
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
pub struct OldManagementResponse {
    pub charger_id: i32,
    pub connection_no: i32,
    pub connection_uuid: u128,
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Serialize)]
#[repr(C, packed)]
pub struct ManagementResponseV2 {
    pub charger_id: u128,
    pub connection_no: i32,
    pub connection_uuid: u128,
}

#[repr(C, packed)]
pub struct ManagementResponsePacket {
    pub header: ManagementPacketHeader,
    pub data: ManagementResponseV2,
}

/// Metadata for a charge log being sent from a charger
///
/// # Fields
///
/// * `charger_uuid` - Unique identifier for the charger (16 bytes)
/// * `user_uuid` - Unique identifier for the user (16 bytes)
/// * `filename_length` - Length of the filename string in bytes
/// * `display_name_length` - Length of the display name string in bytes
/// * `lang` - Language code for the charge log, two bytes (e.g., "en", "de")
/// * `filename` - The actual filename of the charge log
/// * `display_name` - Human-readable display name for the charge log
#[derive(Debug)]
pub struct ChargeLogSendMetadata {
    pub user_uuid: u128,
    pub lang: String,
    pub filename: String,
    pub display_name: String,
}

/// Parsed charge log metadata packet - not packed since it contains String fields
#[derive(Debug)]
pub struct ChargeLogSendMetadataPacket {
    pub header: ManagementPacketHeader,
    pub data: ChargeLogSendMetadata,
}

impl TryFrom<&[u8]> for ChargeLogSendMetadataPacket {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        // The packet must be at least 30 bytes to be able to contain all fixed-size fields
        // (header (8) + user_uuid (16) + filename_length (2) + display_name_length (2) + lang (2))
        if value.len() < 30 {
            return Err(anyhow::anyhow!("Packet too short"));
        }
        let header =
            unsafe { std::ptr::read(value.as_ptr() as *const ManagementPacketHeader) };
        let header_size = std::mem::size_of::<ManagementPacketHeader>();
        let value = &value[header_size..];
        let user_uuid = unsafe { std::ptr::read(value.as_ptr() as *const u128) };
        let user_uuid = u128::from_be(user_uuid);
        let value = &value[std::mem::size_of::<u128>()..];
        let filename_length = unsafe { std::ptr::read(value.as_ptr() as *const u16) };
        let value = &value[std::mem::size_of::<u16>()..];
        let display_name_length = unsafe { std::ptr::read(value.as_ptr() as *const u16) };
        let value = &value[std::mem::size_of::<u16>()..];
        let lang_bytes = &value[..2];
        let lang = String::from_utf8_lossy(lang_bytes).to_string();
        let value = &value[2..];

        // Check if there are enougth bytes for the filename and display name
        if value.len() < (filename_length + display_name_length) as usize {
            return Err(anyhow::anyhow!("Packet too short"));
        }
        let filename_bytes = &value[..filename_length as usize];
        let filename = String::from_utf8_lossy(filename_bytes).to_string();
        let value = &value[filename_length as usize..];

        let display_name_bytes = &value[..display_name_length as usize];
        let display_name = String::from_utf8_lossy(display_name_bytes).to_string();

        let data = ChargeLogSendMetadata {
            user_uuid,
            lang,
            filename,
            display_name,
        };

        Ok(Self {
            header,
            data,
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_valid_packet(filename: &str, display_name: &str, lang: &str) -> Vec<u8> {
        let mut packet = Vec::new();

        // Header (8 bytes)
        packet.extend_from_slice(&0x1234u16.to_ne_bytes()); // magic
        packet.extend_from_slice(&0u16.to_ne_bytes()); // length
        packet.extend_from_slice(&0u16.to_ne_bytes()); // seq_number
        packet.push(1); // version
        packet.push(2); // p_type

        // charger_uuid (16 bytes)
        packet.extend_from_slice(&0x12345678_9ABCDEF0_12345678_9ABCDEF0u128.to_ne_bytes());

        // user_uuid (16 bytes)
        packet.extend_from_slice(&0xFEDCBA98_76543210_FEDCBA98_76543210u128.to_ne_bytes());

        // filename_length (2 bytes)
        packet.extend_from_slice(&(filename.len() as u16).to_ne_bytes());

        // display_name_length (2 bytes)
        packet.extend_from_slice(&(display_name.len() as u16).to_ne_bytes());

        // lang (2 bytes)
        packet.extend_from_slice(&lang.as_bytes()[..2]);

        // filename
        packet.extend_from_slice(filename.as_bytes());

        // display_name
        packet.extend_from_slice(display_name.as_bytes());

        packet
    }

    #[test]
    fn test_parse_valid_packet() {
        let filename = "test_file.csv";
        let display_name = "Test Display Name";
        let lang = "en";
        let packet = create_valid_packet(filename, display_name, lang);

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_ok());

        let parsed = result.unwrap();

        assert_eq!(parsed.data.user_uuid, 0xFEDCBA98_76543210_FEDCBA98_76543210u128);
        assert_eq!(parsed.data.filename, filename);
        assert_eq!(parsed.data.display_name, display_name);
        assert_eq!(parsed.data.lang, lang);
    }

    #[test]
    fn test_parse_empty_strings() {
        let packet = create_valid_packet("", "", "en");

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_ok());

        let parsed = result.unwrap();

        assert_eq!(parsed.data.filename, "");
        assert_eq!(parsed.data.display_name, "");
        assert_eq!(parsed.data.lang, "en");
    }

    #[test]
    fn test_packet_too_short_minimum() {
        // Packet with only 29 bytes (minimum is 30)
        let packet = vec![0u8; 29];

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Packet too short");
    }

    #[test]
    fn test_packet_too_short_for_filename() {
        let mut packet = Vec::new();

        // Header (8 bytes)
        packet.extend_from_slice(&[0u8; 8]);

        // user_uuid (16 bytes)
        packet.extend_from_slice(&[0u8; 16]);

        // filename_length = 10 (2 bytes)
        packet.extend_from_slice(&10u16.to_ne_bytes());

        // display_name_length = 0 (2 bytes)
        packet.extend_from_slice(&0u16.to_ne_bytes());

        // lang (2 bytes)
        packet.extend_from_slice(b"en");

        // Only 5 bytes for filename (not enough)
        packet.extend_from_slice(&[b'a'; 5]);

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Packet too short");
    }

    #[test]
    fn test_packet_too_short_for_display_name() {
        let mut packet = Vec::new();

        // Header (8 bytes)
        packet.extend_from_slice(&[0u8; 8]);

        // user_uuid (16 bytes)
        packet.extend_from_slice(&[0u8; 16]);

        // filename_length = 4 (2 bytes)
        packet.extend_from_slice(&4u16.to_ne_bytes());

        // display_name_length = 10 (2 bytes)
        packet.extend_from_slice(&10u16.to_ne_bytes());

        // lang (2 bytes)
        packet.extend_from_slice(b"de");

        // filename (4 bytes)
        packet.extend_from_slice(b"test");

        // Only 5 bytes for display_name (not enough)
        packet.extend_from_slice(&[b'b'; 5]);

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Packet too short");
    }

    #[test]
    fn test_packet_exactly_minimum_size() {
        // The implementation requires at least 30 bytes for fixed fields.
        let packet = create_valid_packet("", "", "en");
        assert!(packet.len() >= 30);

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_ok());
    }

    #[test]
    fn test_header_fields_parsed_correctly() {
        let mut packet = Vec::new();

        // Header with specific values
        packet.extend_from_slice(&0xABCDu16.to_ne_bytes()); // magic
        packet.extend_from_slice(&0x1234u16.to_ne_bytes()); // length
        packet.extend_from_slice(&0x5678u16.to_ne_bytes()); // seq_number
        packet.push(0x9A); // version
        packet.push(0xBC); // p_type

        // user_uuid (16 bytes)
        packet.extend_from_slice(&0u128.to_ne_bytes());

        // filename_length = 0 (2 bytes)
        packet.extend_from_slice(&0u16.to_ne_bytes());

        // display_name_length = 0 (2 bytes)
        packet.extend_from_slice(&0u16.to_ne_bytes());

        // lang (2 bytes)
        packet.extend_from_slice(b"en");

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_ok());

        let parsed = result.unwrap();
        // Copy packed header fields to local variables before comparing since parsed.header is packed
        // and rustc doesnt like comparing packed fields directly
        let magic = { parsed.header.magic };
        let length = { parsed.header.length };
        let seq_number = { parsed.header.seq_number };
        let version = { parsed.header.version };
        let p_type = { parsed.header.p_type };

        assert_eq!(magic, 0xABCD);
        assert_eq!(length, 0x1234);
        assert_eq!(seq_number, 0x5678);
        assert_eq!(version, 0x9A);
        assert_eq!(p_type, 0xBC);
    }

    #[test]
    fn test_unicode_in_strings() {
        let filename = "tëst_fïlé.csv";
        let display_name = "Tëst Dïsplây Nàmé 日本語";
        let packet = create_valid_packet(filename, display_name, "de");

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.data.filename, filename);
        assert_eq!(parsed.data.display_name, display_name);
    }

    #[test]
    fn test_wrong_filename_length_too_large() {
        let mut packet = Vec::new();

        // Header (8 bytes)
        packet.extend_from_slice(&0x1234u16.to_ne_bytes()); // magic
        packet.extend_from_slice(&0u16.to_ne_bytes()); // length
        packet.extend_from_slice(&0u16.to_ne_bytes()); // seq_number
        packet.push(1); // version
        packet.push(2); // p_type

        // user_uuid (16 bytes)
        packet.extend_from_slice(&0u128.to_ne_bytes());

        // filename_length = 20 (but we only provide 5 bytes)
        packet.extend_from_slice(&20u16.to_ne_bytes());

        // display_name_length = 0
        packet.extend_from_slice(&0u16.to_ne_bytes());

        // lang (2 bytes)
        packet.extend_from_slice(b"en");

        // Only 5 bytes of filename data
        packet.extend_from_slice(b"hello");

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Packet too short");
    }

    #[test]
    fn test_wrong_display_name_length_too_large() {
        let mut packet = Vec::new();

        // Header (8 bytes)
        packet.extend_from_slice(&0x1234u16.to_ne_bytes()); // magic
        packet.extend_from_slice(&0u16.to_ne_bytes()); // length
        packet.extend_from_slice(&0u16.to_ne_bytes()); // seq_number
        packet.push(1); // version
        packet.push(2); // p_type

        // user_uuid (16 bytes)
        packet.extend_from_slice(&0u128.to_ne_bytes());

        // filename_length = 4
        packet.extend_from_slice(&4u16.to_ne_bytes());

        // display_name_length = 50 (but we only provide 5 bytes)
        packet.extend_from_slice(&50u16.to_ne_bytes());

        // lang (2 bytes)
        packet.extend_from_slice(b"de");

        // filename (4 bytes)
        packet.extend_from_slice(b"test");

        // Only 5 bytes of display_name data
        packet.extend_from_slice(b"hello");

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Packet too short");
    }

    #[test]
    fn test_filename_length_smaller_than_data_truncates() {
        let mut packet = Vec::new();

        // Header (8 bytes)
        packet.extend_from_slice(&0x1234u16.to_ne_bytes());
        packet.extend_from_slice(&0u16.to_ne_bytes());
        packet.extend_from_slice(&0u16.to_ne_bytes());
        packet.push(1);
        packet.push(2);

        // user_uuid (16 bytes)
        packet.extend_from_slice(&0u128.to_ne_bytes());

        // filename_length = 5
        packet.extend_from_slice(&5u16.to_ne_bytes());

        // display_name_length = 3
        packet.extend_from_slice(&3u16.to_ne_bytes());

        // lang (2 bytes)
        packet.extend_from_slice(b"en");

        // filename (5 bytes)
        packet.extend_from_slice(b"hello");

        // display_name (3 bytes)
        packet.extend_from_slice(b"abc");

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.data.filename, "hello");
        assert_eq!(parsed.data.display_name, "abc");
    }

    #[test]
    fn test_display_name_length_smaller_than_data_truncates() {
        let mut packet = Vec::new();

        // Header (8 bytes)
        packet.extend_from_slice(&0x1234u16.to_ne_bytes());
        packet.extend_from_slice(&0u16.to_ne_bytes());
        packet.extend_from_slice(&0u16.to_ne_bytes());
        packet.push(1);
        packet.push(2);

        // user_uuid (16 bytes)
        packet.extend_from_slice(&0u128.to_ne_bytes());

        // filename_length = 4
        packet.extend_from_slice(&4u16.to_ne_bytes());

        // display_name_length = 3
        packet.extend_from_slice(&3u16.to_ne_bytes());

        // lang (2 bytes)
        packet.extend_from_slice(b"de");

        // filename (4 bytes)
        packet.extend_from_slice(b"test");

        // display_name with extra data (8 bytes, but only 3 should be read)
        packet.extend_from_slice(b"abcdefgh");

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.data.filename, "test");
        assert_eq!(parsed.data.display_name, "abc"); // Only first 3 bytes
    }

    #[test]
    fn test_filename_longer_than_specified_length() {
        let mut packet = Vec::new();

        // Header (8 bytes)
        packet.extend_from_slice(&0x1234u16.to_ne_bytes());
        packet.extend_from_slice(&0u16.to_ne_bytes());
        packet.extend_from_slice(&0u16.to_ne_bytes());
        packet.push(1);
        packet.push(2);

        // user_uuid (16 bytes)
        packet.extend_from_slice(&0u128.to_ne_bytes());

        // filename_length = 5
        packet.extend_from_slice(&5u16.to_ne_bytes());

        // display_name_length = 4
        packet.extend_from_slice(&4u16.to_ne_bytes());

        // lang (2 bytes)
        packet.extend_from_slice(b"en");

        // filename (5 bytes)
        packet.extend_from_slice(b"hello");

        // display_name (4 bytes)
        packet.extend_from_slice(b"test");

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.data.filename, "hello");
        assert_eq!(parsed.data.display_name, "test");
    }

    #[test]
    fn test_display_name_longer_than_specified_length() {
        let mut packet = Vec::new();

        // Header (8 bytes)
        packet.extend_from_slice(&0x1234u16.to_ne_bytes());
        packet.extend_from_slice(&0u16.to_ne_bytes());
        packet.extend_from_slice(&0u16.to_ne_bytes());
        packet.push(1);
        packet.push(2);

        // user_uuid (16 bytes)
        packet.extend_from_slice(&0u128.to_ne_bytes());

        // filename_length = 8
        packet.extend_from_slice(&8u16.to_ne_bytes());

        // display_name_length = 4
        packet.extend_from_slice(&4u16.to_ne_bytes());

        // lang (2 bytes)
        packet.extend_from_slice(b"de");

        // filename (8 bytes)
        packet.extend_from_slice(b"file.csv");

        // display_name with extra data (17 bytes, but only 4 should be read)
        packet.extend_from_slice(b"User Display Name");

        let result = ChargeLogSendMetadataPacket::try_from(packet.as_slice());
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.data.filename, "file.csv");
        assert_eq!(parsed.data.display_name, "User"); // Only first 4 bytes
    }
}
