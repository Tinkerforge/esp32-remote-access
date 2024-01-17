use std::{task::Poll, io::{Read, Write}, rc::Rc, cell::RefCell};

use crate::{stream::TcpStream, wg_device::WgTunDevice};

pub struct HyperStream {
    stream: Rc<RefCell<TcpStream<'static, WgTunDevice>>>,
}

impl HyperStream {
    pub fn new(stream: Rc<RefCell<TcpStream<'static, WgTunDevice>>>) -> Self {
        Self {
            stream,
        }
    }
}

impl hyper::rt::Read for HyperStream {
    fn poll_read(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            mut buf: hyper::rt::ReadBufCursor<'_>,
        ) -> std::task::Poll<Result<(), std::io::Error>> {
        let mut stream = self.stream.borrow_mut();
        stream.poll();
        if !stream.can_recv() {
            cx.waker().clone().wake();
            return Poll::Pending
        }
        let write_buf = unsafe { buf.as_mut() };
        let buf_len = write_buf.len();
        let buf_ptr = write_buf.as_mut_ptr() as *mut u8;
        let mut write_buf = vec![0u8; buf_len];
        let len = match stream.read(&mut write_buf) {
            Ok(len) => len,
            Err(e) => panic!("failed to read data: {:?}", e),
        };
        unsafe {
            std::ptr::copy_nonoverlapping(write_buf.as_ptr(), buf_ptr, len);
            buf.advance(len);
        }
        Poll::Ready(Ok(()))
    }
}

impl hyper::rt::Write for HyperStream {
    fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, std::io::Error>> {
        let mut stream = self.stream.borrow_mut();
        match stream.write(buf) {
            Ok(len) => Poll::Ready(Ok(len)),
            Err(_) => Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, "failed to write data"))),
        }
    }

    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Result<(), std::io::Error>> {
        let mut stream = self.stream.borrow_mut();
        stream.poll();
        if !stream.can_send() {
            cx.waker().wake_by_ref();
            return Poll::Pending
        }
        match stream.flush() {
            Ok(_) => Poll::Ready(Ok(())),
            Err(err) => Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, format!("failed to flush data: {:?}", err)))),
        }
    }

    fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }
}
