//! UART-based [`SplitTransport`] implementation.
//!
//! [`UartTransport`] serializes [`KeyEvent`]s into fixed-size 11-byte frames
//! and sends them over any async UART (or other byte stream) implementing
//! [`embedded_io_async::Read`] and [`embedded_io_async::Write`].
//!
//! # Frame format
//!
//! ```text
//! Byte 0      : row (u8)
//! Byte 1      : col (u8)
//! Byte 2      : pressed (0x00 = released, 0x01 = pressed)
//! Bytes 3–10  : timestamp_ms (u64, little-endian)
//! ```

use embedded_io_async::{Read, Write};

use crate::event::{KeyEvent, KeyPos};
use crate::transport::SplitTransport;

const FRAME_LEN: usize = 11;

/// A [`SplitTransport`] backed by an async UART (or any `Read` + `Write` byte stream).
///
/// Generic over the reader `R` and writer `W` so that the same type works with
/// any HAL's UART implementation.
pub struct UartTransport<R, W> {
    reader: R,
    writer: W,
}

impl<R, W> UartTransport<R, W> {
    /// Creates a new UART transport from separate read and write halves.
    pub const fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }
}

fn serialize(event: KeyEvent) -> [u8; FRAME_LEN] {
    let ts = event.timestamp_ms.to_le_bytes();
    [
        event.pos.row,
        event.pos.col,
        u8::from(event.pressed),
        ts[0],
        ts[1],
        ts[2],
        ts[3],
        ts[4],
        ts[5],
        ts[6],
        ts[7],
    ]
}

fn deserialize(buf: &[u8; FRAME_LEN]) -> KeyEvent {
    let pos = KeyPos::new(buf[0], buf[1]);
    let pressed = buf[2] != 0;
    let timestamp_ms = u64::from_le_bytes([
        buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9], buf[10],
    ]);
    KeyEvent {
        pos,
        pressed,
        timestamp_ms,
    }
}

/// Error type for [`UartTransport`].
///
/// Wraps the underlying read or write error from the byte-stream implementation.
#[derive(Debug)]
pub enum UartTransportError<RE, WE> {
    /// A read operation failed.
    Read(RE),
    /// A write operation failed.
    Write(WE),
    /// The reader returned EOF before a full frame was received.
    UnexpectedEof,
}

impl<R, W> SplitTransport for UartTransport<R, W>
where
    R: Read,
    W: Write,
{
    type Error = UartTransportError<R::Error, W::Error>;

    async fn send(&mut self, event: KeyEvent) -> Result<(), Self::Error> {
        let frame = serialize(event);
        self.writer
            .write_all(&frame)
            .await
            .map_err(UartTransportError::Write)
    }

    async fn recv(&mut self) -> Result<KeyEvent, Self::Error> {
        let mut buf = [0u8; FRAME_LEN];
        self.reader
            .read_exact(&mut buf)
            .await
            .map_err(|e| match e {
                embedded_io_async::ReadExactError::UnexpectedEof => {
                    UartTransportError::UnexpectedEof
                }
                embedded_io_async::ReadExactError::Other(e) => UartTransportError::Read(e),
            })?;
        Ok(deserialize(&buf))
    }
}

#[expect(clippy::tests_outside_test_module, reason = "dual cfg gate")]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(
    clippy::panic,
    reason = "test helper panics on unexpected Poll::Pending"
)]
#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use core::convert::Infallible;

    struct MockStream {
        data: Vec<u8>,
        pos: usize,
    }

    impl MockStream {
        fn new(data: Vec<u8>) -> Self {
            Self { data, pos: 0 }
        }

        fn empty() -> Self {
            Self::new(Vec::new())
        }

        fn into_data(self) -> Vec<u8> {
            self.data
        }
    }

    impl embedded_io_async::ErrorType for MockStream {
        type Error = Infallible;
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "test mock with known-valid indices"
    )]
    impl Read for MockStream {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
            let available = self.data.len() - self.pos;
            let n = buf.len().min(available);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }

        async fn read_exact(
            &mut self,
            buf: &mut [u8],
        ) -> Result<(), embedded_io_async::ReadExactError<Self::Error>> {
            let available = self.data.len() - self.pos;
            if available < buf.len() {
                return Err(embedded_io_async::ReadExactError::UnexpectedEof);
            }
            let n = buf.len();
            buf.copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(())
        }
    }

    impl Write for MockStream {
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            self.data.extend_from_slice(buf);
            Ok(buf.len())
        }

        async fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
            self.data.extend_from_slice(buf);
            Ok(())
        }
    }

    #[test]
    fn serialize_roundtrip_press() {
        let event = KeyEvent::press(KeyPos::new(2, 5), 12345);
        let frame = serialize(event);
        let decoded = deserialize(&frame);
        assert_eq!(decoded, event);
    }

    #[test]
    fn serialize_roundtrip_release() {
        let event = KeyEvent::release(KeyPos::new(0, 0), 0);
        let frame = serialize(event);
        let decoded = deserialize(&frame);
        assert_eq!(decoded, event);
    }

    #[test]
    fn serialize_max_timestamp() {
        let event = KeyEvent::press(KeyPos::new(255, 255), u64::MAX);
        let frame = serialize(event);
        let decoded = deserialize(&frame);
        assert_eq!(decoded, event);
    }

    #[test]
    fn frame_length_is_11() {
        let frame = serialize(KeyEvent::press(KeyPos::new(0, 0), 0));
        assert_eq!(frame.len(), 11);
    }

    #[test]
    fn pressed_byte_is_one_for_press() {
        let frame = serialize(KeyEvent::press(KeyPos::new(0, 0), 0));
        assert_eq!(frame[2], 1);
    }

    #[test]
    fn pressed_byte_is_zero_for_release() {
        let frame = serialize(KeyEvent::release(KeyPos::new(0, 0), 0));
        assert_eq!(frame[2], 0);
    }

    #[test]
    fn nonzero_pressed_byte_deserializes_as_pressed() {
        let mut frame = serialize(KeyEvent::press(KeyPos::new(0, 0), 0));
        frame[2] = 0xFF;
        let decoded = deserialize(&frame);
        assert!(decoded.pressed);
    }

    fn run_async<F: core::future::Future>(f: F) -> F::Output {
        struct Waker;
        impl std::task::Wake for Waker {
            fn wake(self: std::sync::Arc<Self>) {}
            fn wake_by_ref(self: &std::sync::Arc<Self>) {}
        }
        let waker = std::sync::Arc::new(Waker).into();
        let mut cx = core::task::Context::from_waker(&waker);
        let mut f = core::pin::pin!(f);
        match f.as_mut().poll(&mut cx) {
            core::task::Poll::Ready(v) => v,
            core::task::Poll::Pending => panic!("future was not ready"),
        }
    }

    #[test]
    fn transport_send_writes_11_bytes() {
        let reader = MockStream::empty();
        let writer = MockStream::empty();
        let mut transport = UartTransport::new(reader, writer);
        let event = KeyEvent::press(KeyPos::new(1, 2), 42);

        run_async(transport.send(event)).unwrap();

        assert_eq!(transport.writer.into_data().len(), FRAME_LEN);
    }

    #[test]
    fn transport_roundtrip() {
        let event = KeyEvent::press(KeyPos::new(3, 7), 999);
        let frame = serialize(event);

        let reader = MockStream::new(frame.to_vec());
        let writer = MockStream::empty();
        let mut transport = UartTransport::new(reader, writer);

        let received = run_async(transport.recv()).unwrap();
        assert_eq!(received, event);
    }

    #[test]
    fn transport_send_then_recv() {
        let event = KeyEvent::release(KeyPos::new(2, 4), 7777);

        let writer = MockStream::empty();
        let reader = MockStream::empty();
        let mut tx = UartTransport::new(MockStream::empty(), writer);
        run_async(tx.send(event)).unwrap();

        let written = tx.writer.into_data();
        let mut rx = UartTransport::new(MockStream::new(written), reader);
        let received = run_async(rx.recv()).unwrap();
        assert_eq!(received, event);
    }

    #[test]
    fn multiple_events_roundtrip() {
        let events = [
            KeyEvent::press(KeyPos::new(0, 0), 100),
            KeyEvent::release(KeyPos::new(0, 0), 200),
            KeyEvent::press(KeyPos::new(1, 5), 300),
        ];

        let mut writer = MockStream::empty();
        let reader_stub = MockStream::empty();
        let mut tx = UartTransport::new(reader_stub, writer);
        for &e in &events {
            run_async(tx.send(e)).unwrap();
        }

        writer = tx.writer;
        let data = writer.into_data();
        assert_eq!(data.len(), FRAME_LEN * 3);

        let mut rx = UartTransport::new(MockStream::new(data), MockStream::empty());
        for &expected in &events {
            let received = run_async(rx.recv()).unwrap();
            assert_eq!(received, expected);
        }
    }
}
