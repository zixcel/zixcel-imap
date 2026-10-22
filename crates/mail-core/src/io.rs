use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
/// Caps bytes before a protocol parser can accumulate an unbounded response.
pub struct Bounded<S> {
    inner: S,
    read_left: usize,
    write_left: usize,
}
impl<S> Bounded<S> {
    pub fn new(inner: S, maximum: usize) -> Self {
        Self {
            inner,
            read_left: maximum.min(crate::MAX_WIRE),
            write_left: maximum.min(crate::MAX_WIRE),
        }
    }
}
impl<S> std::fmt::Debug for Bounded<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BoundedMailStream")
    }
}
fn exhausted() -> io::Error {
    io::Error::other("mail_wire_limit")
}
impl<S: AsyncRead + Unpin> AsyncRead for Bounded<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if out.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if self.read_left == 0 {
            return Poll::Ready(Err(exhausted()));
        }
        let mut bytes = [0u8; 4096];
        let n = bytes.len().min(self.read_left).min(out.remaining());
        let mut b = ReadBuf::new(&mut bytes[..n]);
        match Pin::new(&mut self.inner).poll_read(cx, &mut b) {
            Poll::Ready(Ok(())) => {
                self.read_left -= b.filled().len();
                out.put_slice(b.filled());
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}
impl<S: AsyncWrite + Unpin> AsyncWrite for Bounded<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        b: &[u8],
    ) -> Poll<io::Result<usize>> {
        if b.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if self.write_left == 0 {
            return Poll::Ready(Err(exhausted()));
        }
        let n = b.len().min(self.write_left);
        match Pin::new(&mut self.inner).poll_write(cx, &b[..n]) {
            Poll::Ready(Ok(n)) => {
                self.write_left -= n;
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
