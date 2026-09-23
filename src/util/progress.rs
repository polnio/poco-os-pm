use std::io::Read;
use std::sync::mpsc::Sender;

pub struct ProgressReader<R> {
    inner: R,
    sender: Sender<usize>,
}

impl<R> ProgressReader<R> {
    pub fn new(inner: R, sender: Sender<usize>) -> Self {
        Self { inner, sender }
    }
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        let _ = self.sender.send(n);
        Ok(n)
    }
}
