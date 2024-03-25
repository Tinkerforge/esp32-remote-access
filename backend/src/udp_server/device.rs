use boringtun::noise::{Tunn, TunnResult};
use smoltcp::phy::{self, DeviceCapabilities, Medium};
use std::{
    collections::VecDeque,
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
};

use super::multiplex::send_data;

pub struct ManagementDevice {
    rx_buf: VecDeque<Vec<u8>>,
    socket: Arc<UdpSocket>,
    tunn: Arc<Mutex<Tunn>>,
    remote_addr: SocketAddr,
}

impl ManagementDevice {
    pub fn new(socket: Arc<UdpSocket>, tunn: Arc<Mutex<Tunn>>, remote_addr: SocketAddr) -> Self {
        let rx_buf = VecDeque::new();
        Self {
            rx_buf,
            socket,
            tunn,
            remote_addr,
        }
    }

    pub fn push_packet(&mut self, data: Vec<u8>) {
        self.rx_buf.push_back(data)
    }
}

impl phy::Device for ManagementDevice {
    type RxToken<'a> = ManagementRxToken;
    type TxToken<'a> = ManagementTxToken<'a>;

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
            let rx = ManagementRxToken { buf };
            let tx = ManagementTxToken {
                socket: &self.socket,
                tunn: self.tunn.clone(),
                remote_addr: self.remote_addr,
            };
            Some((rx, tx))
        } else {
            None
        }
    }

    fn transmit(&mut self, _timestamp: smoltcp::time::Instant) -> Option<Self::TxToken<'_>> {
        Some(ManagementTxToken {
            socket: &self.socket,
            tunn: self.tunn.clone(),
            remote_addr: self.remote_addr,
        })
    }
}

pub struct ManagementRxToken {
    buf: Vec<u8>,
}

impl phy::RxToken for ManagementRxToken {
    fn consume<R, F>(mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        f(&mut self.buf)
    }
}

pub struct ManagementTxToken<'a> {
    socket: &'a UdpSocket,
    tunn: Arc<Mutex<Tunn>>,
    remote_addr: SocketAddr,
}

impl<'a> phy::TxToken for ManagementTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = vec![0u8; len];
        let r = f(&mut buf);

        let mut tunn = self.tunn.lock().unwrap();
        let mut dst_buf = vec![0u8; len + 32];
        if let TunnResult::WriteToNetwork(data) = tunn.encapsulate(&buf, &mut dst_buf) {
            send_data(self.socket, self.remote_addr, data);
        }

        r
    }
}
