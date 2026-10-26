// 引入 iroh 单向流类型：RecvStream(接收流) 与 SendStream(发送流)
use iroh::endpoint::{RecvStream, SendStream};
// 引入 tokio-util 的编解码工具：FramedRead/FramedWrite 负责把字节流“帧化”，
// LengthDelimitedCodec 是一种“长度前缀”编解码器
use tokio_util::{bytes::BytesMut, codec::{FramedRead, FramedWrite, LengthDelimitedCodec}};
// 引入 futures-util 的扩展 trait：`SinkExt` 提供 `send`，`TryStreamExt` 提供 `try_next`。
// 这些方法定义在扩展 trait 上，必须显式导入到作用域才能调用。
use futures_util::{sink::SinkExt, stream::TryStreamExt};
use anyhow::{Result};

// 单条消息(帧)的最大字节长度限制，防止对端发送超大帧导致内存被耗尽
const MAX_MESSAGE_SIZE: usize = 1000;

/// This is just a convenience wrapper around a [`FramedRead`] and [`FramedWrite`]
/// with a [`LengthDelimitedCodec`].
///
/// 中文说明：这是把 [`FramedRead`] 与 [`FramedWrite`] 组合在一起的便捷封装，
/// 底层统一使用 [`LengthDelimitedCodec`] 编解码器。
///
/// 背景知识（为什么需要“帧化”）：
/// iroh 的双向流底层是基于 QUIC 的“字节流”，它本身没有“消息边界”的概念——
/// 你写入的两条消息，接收方可能一次读到一半、另一半拼在下一条里。
/// 因此需要在字节流之上自行定义“一条完整消息”的边界，这就是“帧化(framing)”。
///
/// The [`LengthDelimitedCodec`] essentially means we'll prefix messages with a
/// big-endian encoded u32 indicating the length of the following message.
///
/// 中文说明：[`LengthDelimitedCodec`] 的做法是：在每条消息前面加上一个
/// 大端编码的 u32 长度前缀，表示“后面这条消息有多少字节”。
/// 接收端先读 4 字节拿到长度，再据此读出完整的一条消息。
///
/// The [`FramedRead`] and [`FramedWrite`] structs then turn the
/// [`SendStream`] and [`RecvStream`] that implement the individual,
/// bytes-based [`AsyncRead`] and [`AsyncWrite`] implementations into
/// variable-(byte-)sized message [`Stream`]s and [`Sink`]s, also called "Frames"
/// (because you can determine the beginning and end borders on the underlying byte
/// streams).
///
/// [`FramedRead`] does this by implementing a state machine that keeps fetching
/// more and more bytes until it's assembled a full frame, and
/// [`FramedWrite`] does this by implementing a state machine that keeps pushing
/// more and more bytes into the underlying stream until it the whole frame has
/// been written, at which point it accepts more frames to be queued.
///
/// 中文说明：[`FramedRead`] 内部是一个状态机，会不断从字节流里取数据，
/// 直到攒够“长度前缀 + 完整消息体”才算一帧；
/// [`FramedWrite`] 同样是状态机，会把一帧不断写入底层流，写完后才接受下一帧排队。
/// 这样就把“面向字节”的流转换成了“面向消息(帧)”的读写接口。
///
/// Because these will keep the frames in memory until they're complete, and you
/// probably don't want to an arbitrary amount of memory depending on the bytes
/// that were sent on the wire, you can configure a maximum frame length.
/// We've set it to 1000 bytes in this case ([`MAX_MESSAGE_SIZE`]), but that's
/// (like many other things) configurable in [`LengthDelimitedCodec`].
///
/// 中文说明：由于帧在收完整之前会一直驻留在内存中，如果不设上限，
/// 恶意或对端的异常数据可能占用任意多的内存。因此这里通过
/// [`MAX_MESSAGE_SIZE`] 把最大帧长度限制为 1000 字节（该值可自由配置）。
///
/// [`AsyncRead`]: tokio::io::AsyncRead
/// [`AsyncWrite`]: tokio::io::AsyncWrite
/// [`Stream`]: futures_util::Stream
/// [`Sink`]: futures_util::Sink
#[derive(Debug)]
pub struct FramedBiStream {
    // 写端：把“帧”编码后写入 iroh 的 SendStream（实现 Sink 接口）
    pub write: FramedWrite<SendStream, LengthDelimitedCodec>,
    // 读端：从 iroh 的 RecvStream 中解码出完整的“帧”（实现 Stream 接口）
    pub read: FramedRead<RecvStream, LengthDelimitedCodec>,
}

impl FramedBiStream {
    /// 由 iroh 的双向流 (SendStream, RecvStream) 构造一个帧化流。
    /// 参数 `(send, recv)` 是对元解构：来自 `Connection::open_bi()`
    /// 或 `Connection::accept_bi()` 返回的 (发送流, 接收流)。
    pub fn new((send, recv): (SendStream, RecvStream)) -> Self {
        // 使用构建器模式配置长度前缀编解码器
        let mut codec = LengthDelimitedCodec::builder();
        // 设置最大帧长度，超过该值会报错，避免内存被无限占用
        codec.max_frame_length(MAX_MESSAGE_SIZE);
        Self {
            // 分别把发送流、接收流包装成“帧化写”和“帧化读”
            write: codec.new_write(send),
            read: codec.new_read(recv),
        }
    }
}
pub type WriteHalf = FramedWrite<SendStream, LengthDelimitedCodec>;
pub type ReadHalf = FramedRead<iroh::endpoint::RecvStream, LengthDelimitedCodec>;

pub async fn send_str(write: &mut WriteHalf, msg: &str) -> Result<()> {
    let encoded = postcard::to_extend(&msg, BytesMut::new())?.freeze();
    write.send(encoded).await?;
    Ok(())
}

pub async fn recv_str(read: &mut ReadHalf) -> Result<String> {
    // FramedRead 实现了 Stream，try_next 返回 Option<Result<BytesMut, io::Error>>
    let Some(frame) = read.try_next().await? else {
        anyhow::bail!("stream closed");
    };
    Ok(postcard::from_bytes(&frame)?)
}