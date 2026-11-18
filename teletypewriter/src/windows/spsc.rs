// Single-producer single-consumer buffer for Rust

use std::cell::UnsafeCell;
use std::io::{self, Read, Write};
use std::mem;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct SpscBuffer {
    buf: UnsafeCell<Box<[u8]>>,
    len: AtomicUsize,
}

impl SpscBuffer {
    fn new(size: usize) -> Self {
        Self {
            buf: UnsafeCell::new(vec![0; size].into_boxed_slice()),
            len: AtomicUsize::new(0),
        }
    }

    fn len(&self) -> usize {
        self.len.load(Ordering::SeqCst)
    }

    fn capacity(&self) -> usize {
        unsafe { &*self.buf.get() }.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }
}

/// Consumer of the ringbuffer.
pub struct SpscBufferReader {
    start: usize,
    buffer: Rc<SpscBuffer>,
}

impl SpscBufferReader {
    /// Get length of contents currently in the buffer
    #[allow(unused)]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Get total capacity of the buffer
    #[allow(unused)]
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// Check whether the buffer is currently empty
    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Check whether the buffer is currently empty
    #[allow(unused)]
    pub fn is_full(&self) -> bool {
        self.buffer.is_full()
    }

    /// Read data from the buffer. Returns number of bytes read.
    ///
    /// Contents that wrap around the end of the ring are returned by the
    /// same call: a read that stopped there would hand the caller a
    /// message in two pieces, and the PTY input pipe then gets it in two
    /// writes, which the in-box ConPTY turns into a lost escape sequence
    /// head and a tail typed as text.
    pub fn read_to_slice(&mut self, buf: &mut [u8]) -> usize {
        use std::cmp::min;

        #[allow(clippy::transmute_ptr_to_ref)]
        let ringbuf: &mut Box<[u8]> = unsafe { mem::transmute(self.buffer.buf.get()) };

        let ringbuf_capacity = ringbuf.len();
        let ringbuf_len = self.buffer.len.load(Ordering::SeqCst);

        let read_size = min(buf.len(), ringbuf_len);
        let until_end = min(read_size, ringbuf_capacity - self.start);
        let wrapped = read_size - until_end;

        buf[..until_end].copy_from_slice(&ringbuf[self.start..self.start + until_end]);
        buf[until_end..read_size].copy_from_slice(&ringbuf[..wrapped]);
        self.start = (self.start + read_size) % ringbuf_capacity;
        self.buffer.len.fetch_sub(read_size, Ordering::SeqCst);

        read_size
    }
}

impl Read for SpscBufferReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Ok(self.read_to_slice(buf))
    }
}

unsafe impl Sync for SpscBufferReader {}
unsafe impl Send for SpscBufferReader {}

/// Producer for the ringbuffer
pub struct SpscBufferWriter {
    end: usize,
    buffer: Rc<SpscBuffer>,
}

impl SpscBufferWriter {
    /// Get length of contents currently in the buffer
    #[allow(unused)]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Get total capacity of the buffer
    #[allow(unused)]
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// Check whether the buffer is currently empty
    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Check whether the buffer is currently empty
    pub fn is_full(&self) -> bool {
        self.buffer.is_full()
    }

    /// Write data to the buffer. Returns number of bytes written.
    ///
    /// The write is short only when the ring is full. Space that wraps
    /// around the end of the ring is filled by the same call, and the
    /// length is published once, after both parts are in place, so the
    /// consumer never sees the part before the wrap on its own.
    pub fn write_from_slice(&mut self, buf: &[u8]) -> usize {
        use std::cmp::min;

        #[allow(clippy::transmute_ptr_to_ref)]
        let ringbuf: &mut Box<[u8]> = unsafe { mem::transmute(self.buffer.buf.get()) };

        let ringbuf_capacity = ringbuf.len();
        let ringbuf_len = self.buffer.len.load(Ordering::SeqCst);

        let write_size = min(buf.len(), ringbuf_capacity - ringbuf_len);
        let until_end = min(write_size, ringbuf_capacity - self.end);
        let wrapped = write_size - until_end;

        ringbuf[self.end..self.end + until_end].copy_from_slice(&buf[..until_end]);
        ringbuf[..wrapped].copy_from_slice(&buf[until_end..write_size]);
        self.end = (self.end + write_size) % ringbuf_capacity;
        self.buffer.len.fetch_add(write_size, Ordering::SeqCst);

        write_size
    }
}

unsafe impl Sync for SpscBufferWriter {}
unsafe impl Send for SpscBufferWriter {}

impl Write for SpscBufferWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(self.write_from_slice(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Create a new SPSC buffer pair.
///
/// The producer and consumer can safely be transferred between threads; the
/// expected use case is that one thread will be writing and one will be reading.
///
/// The underlying buffer's size is synchronised using an atomic. The producer
/// and consumer have methods to query the size and the capacity, which is
/// guaranteed to be consistent between threads but may not be sufficient to
/// prevent races depending on what you are trying to achieve.
///
/// See the mio-anonymous-pipes crate for example usage.
pub fn spsc_buffer(size: usize) -> (SpscBufferWriter, SpscBufferReader) {
    let buffer = Rc::new(SpscBuffer::new(size));

    let producer = SpscBufferWriter {
        end: 0,
        buffer: buffer.clone(),
    };
    let consumer = SpscBufferReader { start: 0, buffer };

    (producer, consumer)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_spsc_buffer() {
        let buf = [1u8; 100];

        let (mut producer, mut consumer) = spsc_buffer(60);

        assert!(producer.is_empty());
        assert!(consumer.is_empty());

        assert_eq!(producer.len(), 0);
        assert_eq!(consumer.len(), 0);

        assert_eq!(producer.capacity(), 60);
        assert_eq!(consumer.capacity(), 60);

        let mut out_buf = [0u8; 100];

        assert_eq!(producer.write_from_slice(&buf), 60);
        assert_eq!(producer.len(), 60);
        assert_eq!(consumer.len(), 60);

        assert_eq!(consumer.read_to_slice(&mut out_buf), 60);
        assert_eq!(producer.len(), 0);
        assert_eq!(consumer.len(), 0);

        assert_eq!(producer.write_from_slice(&buf[60..]), 40);
        assert_eq!(producer.len(), 40);
        assert_eq!(consumer.len(), 40);

        assert_eq!(consumer.read_to_slice(&mut out_buf[60..]), 40);
        assert_eq!(producer.len(), 0);
        assert_eq!(consumer.len(), 0);

        assert_eq!(&buf[..], &out_buf[..]);
    }

    /// A message that straddles the end of the ring is written by one
    /// call and read back by one call, in order.
    #[test]
    fn test_spsc_buffer_wrap_is_one_write_and_one_read() {
        let (mut producer, mut consumer) = spsc_buffer(16);
        let mut out_buf = [0u8; 16];

        // Move both ends to 12, four bytes short of the wrap.
        assert_eq!(producer.write_from_slice(&[0u8; 12]), 12);
        assert_eq!(consumer.read_to_slice(&mut out_buf), 12);
        assert!(consumer.is_empty());

        let report = b"\x1b[<35;4;6M";
        assert_eq!(producer.write_from_slice(report), report.len());
        assert_eq!(consumer.len(), report.len());

        assert_eq!(consumer.read_to_slice(&mut out_buf), report.len());
        assert_eq!(&out_buf[..report.len()], report);
        assert!(consumer.is_empty());
    }

    /// A full ring takes what fits, across the wrap, and no more.
    #[test]
    fn test_spsc_buffer_wrap_stops_when_full() {
        let (mut producer, mut consumer) = spsc_buffer(16);
        let mut out_buf = [0u8; 16];

        assert_eq!(producer.write_from_slice(&[0u8; 12]), 12);
        assert_eq!(consumer.read_to_slice(&mut out_buf[..10]), 10);

        // Two bytes are unread; 14 are free, four of them before the wrap.
        let data: Vec<u8> = (1..=20).collect();
        assert_eq!(producer.write_from_slice(&data), 14);
        assert!(producer.is_full());
        assert_eq!(producer.write_from_slice(&data[14..]), 0);

        assert_eq!(consumer.read_to_slice(&mut out_buf), 16);
        assert_eq!(&out_buf[..2], &[0, 0]);
        assert_eq!(&out_buf[2..], &data[..14]);
        assert!(consumer.is_empty());
    }
}
