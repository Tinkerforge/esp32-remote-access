/* esp32-remote-access
 * Copyright (C) 2026 Frederic Henrichs <frederic@tinkerforge.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 59 Temple Place - Suite 330,
 * Boston, MA 02111-1307, USA.
 */

#[cfg(feature = "pcap-logging")]
mod enabled {
    use std::fs::{File, OpenOptions};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock;
    use pcap_file::pcapng::blocks::interface_description::InterfaceDescriptionBlock;
    use pcap_file::pcapng::PcapNgWriter;
    use pcap_file::DataLink;

    /// A thread-safe wrapper around PcapNgWriter for logging network packets directly to a file.
    #[derive(Clone)]
    pub struct PcapLogger {
        writer: Arc<Mutex<Option<PcapNgWriter<File>>>>,
        file_path: Arc<Mutex<Option<PathBuf>>>,
    }

    impl PcapLogger {
        /// Creates a new PcapLogger (disabled by default).
        pub fn new() -> Self {
            Self {
                writer: Arc::new(Mutex::new(None)),
                file_path: Arc::new(Mutex::new(None)),
            }
        }

        /// Enables pcap logging and writes to the specified file path.
        /// If the file already exists, it will be overwritten.
        pub fn enable(&self, path: PathBuf) -> Result<(), String> {
            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path)
                .map_err(|e| format!("Failed to open pcap file: {}", e))?;

            let pcap_writer = PcapNgWriter::new(file)
                .map_err(|e| format!("Failed to create PcapNgWriter: {}", e))?;

            let mut writer_guard = self.writer.lock().unwrap();
            *writer_guard = Some(pcap_writer);

            let mut path_guard = self.file_path.lock().unwrap();
            *path_guard = Some(path);

            Ok(())
        }

        /// Disables pcap logging and closes the file.
        pub fn disable(&self) {
            let mut writer_guard = self.writer.lock().unwrap();
            *writer_guard = None;

            let mut path_guard = self.file_path.lock().unwrap();
            *path_guard = None;
        }

        /// Returns whether pcap logging is enabled.
        pub fn is_enabled(&self) -> bool {
            let guard = self.writer.lock().unwrap();
            guard.is_some()
        }

        /// Returns the current file path if logging is enabled.
        pub fn get_file_path(&self) -> Option<PathBuf> {
            let guard = self.file_path.lock().unwrap();
            guard.clone()
        }

        /// Logs a packet to the pcap file.
        /// The packet should be raw IP data (IPv4).
        pub fn log_packet(&self, data: &[u8]) {
            let mut writer_guard = self.writer.lock().unwrap();
            let writer = match writer_guard.as_mut() {
                Some(w) => w,
                None => return,
            };

            let interface = InterfaceDescriptionBlock {
                linktype: DataLink::IPV4,
                snaplen: 0,
                options: vec![],
            };

            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO);

            let packet = EnhancedPacketBlock {
                interface_id: 0,
                timestamp,
                original_len: data.len() as u32,
                data: std::borrow::Cow::Borrowed(data),
                options: vec![],
            };

            if let Err(e) = writer.write_pcapng_block(interface) {
                log::warn!("Failed to write pcap interface block: {}", e);
                return;
            }
            if let Err(e) = writer.write_pcapng_block(packet) {
                log::warn!("Failed to write pcap packet block: {}", e);
            }
        }
    }

    impl Default for PcapLogger {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(not(feature = "pcap-logging"))]
mod disabled {
    use std::path::PathBuf;

    /// A no-op stub for PcapLogger when the pcap-logging feature is disabled.
    #[derive(Clone, Default)]
    pub struct PcapLogger;

    impl PcapLogger {
        /// Creates a new PcapLogger (no-op).
        pub fn new() -> Self {
            Self
        }

        /// No-op: pcap-logging feature is disabled.
        #[allow(unused_variables)]
        pub fn enable(&self, path: PathBuf) -> Result<(), String> {
            Ok(())
        }

        /// No-op: pcap-logging feature is disabled.
        pub fn disable(&self) {}

        /// Always returns false when pcap-logging feature is disabled.
        pub fn is_enabled(&self) -> bool {
            false
        }

        /// Always returns None when pcap-logging feature is disabled.
        pub fn get_file_path(&self) -> Option<PathBuf> {
            None
        }

        /// No-op: pcap-logging feature is disabled.
        #[allow(unused_variables)]
        pub fn log_packet(&self, data: &[u8]) {}
    }
}

#[cfg(feature = "pcap-logging")]
pub use enabled::PcapLogger;

#[cfg(not(feature = "pcap-logging"))]
pub use disabled::PcapLogger;
