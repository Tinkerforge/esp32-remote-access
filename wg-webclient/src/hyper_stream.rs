/* esp32-remote-access
 * Copyright (C) 2024 Frederic Henrichs <frederic@tinkerforge.com>
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

use std::{
    cell::RefCell,
    io::{self, Read},
    rc::Rc,
    task::Poll,
};

use futures::task::AtomicWaker;

use crate::{stream::TcpStream, wg_device::WgTunDevice};

/**
   A wrapper around a reference counted TcpStream struct that implements the hyper::rt::Read and
   hyper::rt::Write traits to be able to reuse one TcpStream when passing the ownership of this to hyper.
*/
pub struct HyperStream<'a> {
    stream: Rc<RefCell<TcpStream<'static, WgTunDevice>>>,
    network_waker: &'a AtomicWaker,
}

impl<'a> HyperStream<'a> {
    pub fn new(
        stream: Rc<RefCell<TcpStream<'static, WgTunDevice>>>,
        network_waker: &'a AtomicWaker,
    ) -> Self {
        Self {
            stream,
            network_waker,
        }
    }
}

impl hyper::rt::Read for HyperStream<'_> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        mut buf: hyper::rt::ReadBufCursor<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let mut stream = self.stream.borrow_mut();
        stream.poll();
        if !stream.can_recv() {
            if !stream.may_recv() {
                return Poll::Ready(Ok(()));
            }
            self.network_waker.register(cx.waker());
            return Poll::Pending;
        }
        let mut data = vec![0; buf.remaining()];
        let len = stream.read(&mut data)?;
        buf.put_slice(&data[..len]);
        Poll::Ready(Ok(()))
    }
}

impl hyper::rt::Write for HyperStream<'_> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let stream = self.stream.borrow();
        if !stream.can_send() {
            if !stream.may_send() {
                return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
            }
            self.network_waker.register(cx.waker());
            return Poll::Pending;
        }
        Poll::Ready(stream.send_slice(buf).map_err(io::Error::other))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> Poll<io::Result<()>> {
        // poll_write queues bytes directly in the TCP socket.
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }
}
