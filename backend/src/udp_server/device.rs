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

use boringtun::noise::{Tunn, TunnResult};
use smoltcp::phy::{self, DeviceCapabilities, Medium};
use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::Arc,
};
use tokio::net::UdpSocket;

use super::multiplex::send_data;
use super::pcap_logger::PcapLogger;

pub struct ManagementDevice {
    rx_buf: VecDeque<Vec<u8>>,
    socket: Arc<UdpSocket>,
    tunn: Tunn,
    remote_addr: SocketAddr,
    pcap_logger: PcapLogger,
}

impl ManagementDevice {
    pub fn new(socket: Arc<UdpSocket>, tunn: Tunn, remote_addr: SocketAddr, pcap_logger: PcapLogger) -> Self {
        let rx_buf = VecDeque::new();
        Self {
            rx_buf,
            socket,
            tunn,
            remote_addr,
            pcap_logger,
        }
    }

    pub fn reset(&mut self) {
        self.tunn.clear_sessions();
    }

    pub fn push_packet(&mut self, data: Vec<u8>) {
        self.rx_buf.push_back(data)
    }

    pub fn decapsulate<'a>(&mut self, src: &'a [u8], dst: &'a mut [u8]) -> TunnResult<'a> {
        self.tunn.decapsulate(None, src, dst)
    }
}

impl phy::Device for ManagementDevice {
    type RxToken<'b> = ManagementRxToken
    where
        Self: 'b;
    type TxToken<'b> = ManagementTxToken<'b>
    where
        Self: 'b;

    fn capabilities(&self) -> phy::DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_burst_size = None;
        caps.max_transmission_unit = 1500;
        caps.medium = Medium::Ip;
        caps
    }

    fn receive(
        &mut self,
        _timestamp: smoltcp::time::Instant,
    ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        if let Some(buf) = self.rx_buf.pop_front() {
            let rx = ManagementRxToken {
                buf,
                pcap_logger: self.pcap_logger.clone(),
            };
            let tx = ManagementTxToken {
                socket: &self.socket,
                tunn: &mut self.tunn,
                remote_addr: self.remote_addr,
                pcap_logger: self.pcap_logger.clone(),
            };
            Some((rx, tx))
        } else {
            None
        }
    }

    fn transmit(&mut self, _timestamp: smoltcp::time::Instant) -> Option<Self::TxToken<'_>> {
        Some(ManagementTxToken {
            socket: &self.socket,
            tunn: &mut self.tunn,
            remote_addr: self.remote_addr,
            pcap_logger: self.pcap_logger.clone(),
        })
    }
}

pub struct ManagementRxToken {
    buf: Vec<u8>,
    pcap_logger: PcapLogger,
}

impl phy::RxToken for ManagementRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        self.pcap_logger.log_packet(&self.buf);
        f(&self.buf)
    }
}

pub struct ManagementTxToken<'a> {
    socket: &'a UdpSocket,
    tunn: &'a mut Tunn,
    remote_addr: SocketAddr,
    pcap_logger: PcapLogger,
}

impl<'a> phy::TxToken for ManagementTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = vec![0u8; len];
        let r = f(&mut buf);

        self.pcap_logger.log_packet(&buf);

        let mut dst_buf = vec![0u8; len + 148];
        let res = self.tunn.encapsulate(&buf, &mut dst_buf);
        if let TunnResult::WriteToNetwork(data) = res {
            send_data(self.socket, self.remote_addr, data);
        }

        r
    }
}
