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
    pub fn read_to_slice(&mut self, buf: &mut [u8]) -> usize {
        use std::cmp::min;

        #[allow(clippy::transmute_ptr_to_ref)]
        let ringbuf: &mut Box<[u8]> = unsafe { mem::transmute(self.buffer.buf.get()) };

        let ringbuf_capacity = ringbuf.len();
        let ringbuf_len = self.buffer.len.load(Ordering::SeqCst);

        // Max number of bytes we might read
        let max_read_size = min(buf.len(), ringbuf_len);
        let contents_until_end = ringbuf_capacity - self.start;
        let read_size = min(max_read_size, contents_until_end);

        buf[..read_size].copy_from_slice(&ringbuf[self.start..self.start + read_size]);
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
    pub fn write_from_slice(&mut self, buf: &[u8]) -> usize {
        use std::cmp::min;

        #[allow(clippy::transmute_ptr_to_ref)]
        let ringbuf: &mut Box<[u8]> = unsafe { mem::transmute(self.buffer.buf.get()) };

        let ringbuf_capacity = ringbuf.len();
        let ringbuf_len = self.buffer.len.load(Ordering::SeqCst);

        // Max number of bytes we might read
        let max_write_size = min(buf.len(), ringbuf_capacity - ringbuf_len);
        let space_until_end = ringbuf_capacity - self.end;
        let write_size = min(max_write_size, space_until_end);

        ringbuf[self.end..self.end + write_size].copy_from_slice(&buf[..write_size]);
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
}
