//! 自研 rz/sz 协议状态机（无外部 crate 依赖）。
//!
//! 实现 rz/sz 协议中 rz/sz 文件传输所需的子集：hex/binary 帧头、ZDLE 转义、
//! CRC16/CRC32 校验、ZRQINIT/ZRINIT/ZSINIT/ZFILE/ZDATA/ZEOF/ZFIN/ZACK/ZRPOS/
//! ZSKIP/ZABORT 帧类型，以及发送/接收两侧的调用方驱动状态机（poll/submit API）。
//!
//! 关键语义：
//! - 接收侧向对端声明非停等 I/O（buffer_len=0 + CANOVIO），SSH 可靠流上连续
//!   流式传输；CRC 损坏的数据子包可恢复：ZRPOS 回退 + 帧头 resync 容错扫描 +
//!   重试预算，元数据（ZFILE/ZSINIT）CRC 损坏保持致命（无偏移可回退）；
//! - 发送侧流式窗口（默认全非停等），子包默认 ZBIN32 编码；坏帧排队 NAK 并
//!   返回错误，由调用方决定丢弃对齐（对端重发恢复）；
//! - `timeout()` 驱动握手重试（SessionBegin/WaitReceiverInit 阶段重发首帧）。
//!
//! 两侧状态机均为纯同步实现，任务整体跑在 blocking 线程（见 rzsz.rs）。

use std::fmt;

// ============================================================
// 线路常量
// ============================================================

/// ZPAD（`*`），帧头起始标记
const ZPAD: u8 = 0x2a;
/// ZDLE（CAN 0x18），转义标记
const ZDLE: u8 = 0x18;
/// XON（hex 帧行尾补齐，防止被流控吞掉）
const XON: u8 = 0x11;

/// 帧头编码字节：binary + CRC16
const ENC_ZBIN: u8 = 0x41;
/// 帧头编码字节：hex + CRC16
const ENC_ZHEX: u8 = 0x42;
/// 帧头编码字节：binary + CRC32
const ENC_ZBIN32: u8 = 0x43;

/// 帧类型（rz/sz spec 0-19）
const ZRQINIT: u8 = 0;
const ZRINIT: u8 = 1;
const ZSINIT: u8 = 2;
const ZACK: u8 = 3;
const ZFILE: u8 = 4;
const ZSKIP: u8 = 5;
const ZABORT: u8 = 7;
const ZFIN: u8 = 8;
const ZRPOS: u8 = 9;
const ZDATA: u8 = 10;
const ZEOF: u8 = 11;
/// 对端以 CAN*5 放弃会话
const ZCAN: u8 = 16;

/// 子包结束符（ZDLE 后出现）
const ZCRCE: u8 = 0x68;
const ZCRCG: u8 = 0x69;
const ZCRCQ: u8 = 0x6a;
const ZCRCW: u8 = 0x6b;

/// 子包负载上限（rz/sz spec 建议值）
const SUBPACKET_MAX_SIZE: usize = 1024;

/// 帧头负载字节数（帧类型 + 4 标志）
const HEADER_PAYLOAD_SIZE: usize = 5;

/// ZRINIT 标志位：可使用 32 位帧校验
const CANFC32: u8 = 0x20;
/// ZRINIT 标志位：边收边写盘（存储可重叠 I/O）
const CANOVIO: u8 = 0x02;

/// 连续损坏数据子包（无进展）容忍上限：每个消耗一次 ZRPOS 重退预算。
/// 链路连一个干净子包都送不到时，如实上抛 CRC 错误比无限循环诚实。
/// lrzsz 的 rz 在相近的垃圾/重试阈值上放弃。
const MAX_ZRPOS_RETRIES: u8 = 10;

/// 帧级诊断日志（真实 lrzsz 互通调试）：与 rzsz.rs 的 diag 同写临时目录
/// fyshell-rzsz.log，行首带 P 前缀区分来源。纯 std 自包含实现，
/// 独立编译（rustc --test）可用。
fn diag(msg: &str) {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("fyshell-rzsz.log"))
    {
        let _ = writeln!(f, "P{n} {ts} {msg}");
    }
}

// ============================================================
// CRC
// ============================================================

/// CRC-16-XMODEM（CCITT，poly 0x1021，初值 0）
fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut calc = Crc16Calc::new();
    calc.update(data);
    calc.finalize()
}

/// CRC-32-ISO-HDLC（IEEE，poly 0xEDB88320，初值 0xFFFFFFFF，取反输出）
fn crc32_iso_hdlc(data: &[u8]) -> u32 {
    let mut calc = Crc32Calc::new();
    calc.update(data);
    calc.finalize()
}

/// 增量式 CRC-16 计算器（子包 CRC 流式校验用）
#[derive(Clone, Copy)]
struct Crc16Calc {
    crc: u16,
}

impl Crc16Calc {
    const fn new() -> Self {
        Self { crc: 0 }
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.crc ^= u16::from(byte) << 8;
            for _ in 0..8 {
                if self.crc & 0x8000 != 0 {
                    self.crc = (self.crc << 1) ^ 0x1021;
                } else {
                    self.crc <<= 1;
                }
            }
        }
    }

    const fn finalize(&self) -> u16 {
        self.crc
    }
}

/// 增量式 CRC-32 计算器（子包 CRC 流式校验用）
#[derive(Clone, Copy)]
struct Crc32Calc {
    crc: u32,
}

impl Crc32Calc {
    const fn new() -> Self {
        Self { crc: 0xFFFF_FFFF }
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.crc ^= u32::from(byte);
            for _ in 0..8 {
                if self.crc & 1 != 0 {
                    self.crc = (self.crc >> 1) ^ 0xEDB8_8320;
                } else {
                    self.crc >>= 1;
                }
            }
        }
    }

    const fn finalize(&self) -> u32 {
        !self.crc
    }
}

// ============================================================
// ZDLE 转义
// ============================================================

/// ZRUB0 序列（ZDLE 'l'）：数据位置表达 0x7f
const ZRUB0: u8 = 0x6c;
/// ZRUB1 序列（ZDLE 'm'）：数据位置表达 0xff
const ZRUB1: u8 = 0x6d;

/// 需要 ZDLE 转义的字节集（lrzsz 约定）：CR、DLE、XON、XOFF、ZDLE 本身，
/// 以及 0x7f/0xff（经 ZRUB0/ZRUB1 序列表达，其余字节原样上线）。
fn needs_escape(b: u8) -> bool {
    matches!(b, 0x0d | 0x10 | 0x11 | 0x13 | 0x18 | 0x7f | 0xff)
}

/// 将 data 按 ZDLE 转义规则追加到 out：转义字节前缀 ZDLE，随字节与 0x40
/// 异或（0x7f/0xff 走 ZRUB0/ZRUB1 序列）。
fn zdle_escape(out: &mut Vec<u8>, data: &[u8]) {
    for &b in data {
        if needs_escape(b) {
            out.push(ZDLE);
            out.push(match b {
                0x7f => ZRUB0,
                0xff => ZRUB1,
                other => other ^ 0x40,
            });
        } else {
            out.push(b);
        }
    }
}

/// ZDLE 后字节的还原：0x40..0x5f 区间 XOR 0x40 还原控制字符，
/// ZRUB0/ZRUB1 还原 0x7f/0xff，其余原样。
fn zdle_unescape(b: u8) -> u8 {
    match b {
        ZRUB0 => 0x7f,
        ZRUB1 => 0xff,
        0x40..=0x5f => b ^ 0x40,
        other => other,
    }
}

// ============================================================
// 错误类型
// ============================================================

/// rz/sz 协议错误。CRC 校验错误可恢复（NAK/对端重发），其余终止传输。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    UnexpectedCrc16,
    UnexpectedCrc32,
    MalformedFrame(u8),
    MalformedHeader,
    MalformedFileName,
    MalformedFileSize,
    InvalidState,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedCrc16 => write!(f, "CRC16 校验失败"),
            Self::UnexpectedCrc32 => write!(f, "CRC32 校验失败"),
            Self::MalformedFrame(t) => write!(f, "帧类型不合法: {t:#04x}"),
            Self::MalformedHeader => write!(f, "帧头格式错误"),
            Self::MalformedFileName => write!(f, "文件名格式错误"),
            Self::MalformedFileSize => write!(f, "文件大小格式错误"),
            Self::InvalidState => write!(f, "状态机状态不合法"),
        }
    }
}

impl std::error::Error for ProtocolError {}

// ============================================================
// 公共类型（poll/submit API）
// ============================================================

/// 文件传输中的字节偏移（rz/sz 位置为 32 位）
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position(u32);

impl Position {
    pub const fn new(offset: u32) -> Self {
        Self(offset)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl From<Position> for u32 {
    fn from(position: Position) -> Self {
        position.0
    }
}

/// 文件元数据（发送方向注册 / 接收方向事件携带）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileInfo<'a> {
    /// 线路原始文件名字节
    pub name: &'a [u8],
    /// 文件大小（已知时）
    pub size: Option<Position>,
}

impl<'a> FileInfo<'a> {
    pub const fn new(name: &'a [u8], size: Option<Position>) -> Self {
        Self { name, size }
    }
}

/// 状态机下一步动作：调用方必须执行的操作。
#[derive(Debug, PartialEq, Eq)]
pub enum Action<'a> {
    /// 写线路（协议帧），写完后以 `wire_written` 汇报
    WriteWire(&'a [u8]),
    /// 写文件数据（接收方向），写完后以 `file_written` 汇报
    WriteFile(&'a [u8]),
    /// 读文件数据（发送方向），读完后以 `submit_file` 提交
    ReadFile { offset: Position, max_len: usize },
    /// 协议事件
    Event(Event<'a>),
    /// 无待办：提交更多线路输入（`submit_wire`）或等 `timeout()`
    Idle,
}

/// 协议事件。
#[derive(Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// 新文件开始，携带对端通告的元数据
    FileStarted(FileInfo<'a>),
    /// 当前文件完成
    FileCompleted,
    /// 会话成功完成
    SessionCompleted,
    /// 会话被中止
    Aborted,
}

// ============================================================
// 帧头编解码
// ============================================================

/// 帧头编码
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Encoding {
    Zbin,
    ZHex,
    Zbin32,
}

impl Encoding {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            ENC_ZBIN => Some(Self::Zbin),
            ENC_ZHEX => Some(Self::ZHex),
            ENC_ZBIN32 => Some(Self::Zbin32),
            _ => None,
        }
    }

    /// 解转义后应读取的帧头字节数（负载 + CRC）
    fn read_size(self) -> usize {
        match self {
            Self::Zbin => HEADER_PAYLOAD_SIZE + 2,
            Self::Zbin32 => HEADER_PAYLOAD_SIZE + 4,
            // hex 负载为 ASCII，每字节两位字符
            Self::ZHex => (HEADER_PAYLOAD_SIZE + 2) * 2,
        }
    }

    fn crc_len(self) -> usize {
        if self == Self::Zbin32 { 4 } else { 2 }
    }
}

/// 解码后的 rz/sz 帧头
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Header {
    encoding: Encoding,
    frame: u8,
    /// 4 字节标志字段；对使用计数的帧类型（ZRPOS/ZACK 等）即小端 u32
    flags: [u8; 4],
}

impl Header {
    /// 使用计数的帧类型（ZRPOS/ZACK）取标志字段为小端 u32
    const fn count(&self) -> u32 {
        u32::from_le_bytes(self.flags)
    }

    /// 编码并写出帧头到 out（含起始 ZPAD/ZDLE 与 hex 帧行尾）。
    fn write(&self, out: &mut Vec<u8>) {
        // 帧头起始：hex 双 ZPAD，binary 单 ZPAD，随后 ZDLE + 编码字节
        out.push(ZPAD);
        if self.encoding == Encoding::ZHex {
            out.push(ZPAD);
        }
        out.push(ZDLE);
        out.push(match self.encoding {
            Encoding::Zbin => ENC_ZBIN,
            Encoding::ZHex => ENC_ZHEX,
            Encoding::Zbin32 => ENC_ZBIN32,
        });

        // 负载：帧类型 + flags，随后 CRC
        let mut payload = [0u8; HEADER_PAYLOAD_SIZE];
        payload[0] = self.frame;
        payload[1..].copy_from_slice(&self.flags);

        if self.encoding == Encoding::ZHex {
            // hex 帧：负载 + CRC16（大端）转 ASCII hex 后转义（hex 字符
            // 本身全部可打印，转义保持统一处理）
            let mut full = [0u8; HEADER_PAYLOAD_SIZE + 2];
            full[..HEADER_PAYLOAD_SIZE].copy_from_slice(&payload);
            full[HEADER_PAYLOAD_SIZE..].copy_from_slice(&crc16_xmodem(&payload).to_be_bytes());
            const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
            let mut hex_buf = Vec::with_capacity(full.len() * 2);
            for &b in &full {
                hex_buf.push(HEX_DIGITS[usize::from(b >> 4)]);
                hex_buf.push(HEX_DIGITS[usize::from(b & 0x0f)]);
            }
            zdle_escape(out, &hex_buf);
            // hex 帧行尾：CR LF（非 ZACK/ZFIN 补 XON）
            out.push(b'\r');
            out.push(b'\n');
            if self.frame != ZACK && self.frame != ZFIN {
                out.push(XON);
            }
        } else {
            // binary 帧：负载 + CRC 全部 ZDLE 转义
            zdle_escape(out, &payload);
            if self.encoding == Encoding::Zbin32 {
                let crc = crc32_iso_hdlc(&payload).to_le_bytes();
                zdle_escape(out, &crc);
            } else {
                let crc = crc16_xmodem(&payload).to_be_bytes();
                zdle_escape(out, &crc);
            }
        }
    }
}

/// 将子包（data + 结束符 kind）编码追加到 out：data 转义 + ZDLE + kind +
/// 转义后的 CRC（覆盖 data + kind）。
fn encode_subpacket(out: &mut Vec<u8>, crc32: bool, kind: u8, data: &[u8]) {
    zdle_escape(out, data);
    out.push(ZDLE);
    out.push(kind);
    if crc32 {
        let mut calc = Crc32Calc::new();
        calc.update(data);
        calc.update(&[kind]);
        zdle_escape(out, &calc.finalize().to_le_bytes());
    } else {
        let mut calc = Crc16Calc::new();
        calc.update(data);
        calc.update(&[kind]);
        zdle_escape(out, &calc.finalize().to_be_bytes());
    }
}

/// ASCII hex 字符 → 4 位值
fn hex_val(b: u8) -> Result<u8, ProtocolError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        _ => Err(ProtocolError::MalformedHeader),
    }
}

/// ASCII 十进制文件大小解析（checked 运算防溢出，空串 = 0）
fn parse_file_size(bytes: &[u8]) -> Result<u32, ProtocolError> {
    if bytes.is_empty() {
        return Ok(0);
    }
    bytes.iter().try_fold(0u32, |acc, &b| {
        if !b.is_ascii_digit() {
            return Err(ProtocolError::MalformedFileSize);
        }
        acc.checked_mul(10)
            .and_then(|v| v.checked_add(u32::from(b - b'0')))
            .ok_or(ProtocolError::MalformedFileSize)
    })
}

/// 解码帧头负载并校验 CRC，返回帧头。`data` 为解转义后的字节：hex 帧为
/// ASCII hex 字符，binary 帧为负载 + CRC 原样字节。
fn decode_header(encoding: Encoding, data: &[u8]) -> Result<Header, ProtocolError> {
    let crc_len = encoding.crc_len();
    let body: Vec<u8> = if encoding == Encoding::ZHex {
        if data.len() % 2 != 0 {
            return Err(ProtocolError::MalformedHeader);
        }
        let mut out = vec![0u8; data.len() / 2];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = (hex_val(data[i * 2])? << 4) | hex_val(data[i * 2 + 1])?;
        }
        out
    } else {
        data.to_vec()
    };
    if body.len() < HEADER_PAYLOAD_SIZE + crc_len {
        return Err(ProtocolError::MalformedHeader);
    }
    let (payload, crc_bytes) = body.split_at(HEADER_PAYLOAD_SIZE);
    if encoding == Encoding::Zbin32 {
        let expected = crc32_iso_hdlc(payload).to_le_bytes();
        if crc_bytes != &expected[..crc_len] {
            return Err(ProtocolError::UnexpectedCrc32);
        }
    } else {
        let expected = crc16_xmodem(payload).to_be_bytes();
        if crc_bytes != &expected[..crc_len] {
            return Err(ProtocolError::UnexpectedCrc16);
        }
    }
    let frame = frame_from_u8(payload[0])?;
    let mut flags = [0u8; 4];
    flags.copy_from_slice(&payload[1..=4]);
    Ok(Header {
        encoding,
        frame,
        flags,
    })
}

/// 帧类型字节 → 常量（0-19 全部合法，未知帧由状态机忽略）
fn frame_from_u8(value: u8) -> Result<u8, ProtocolError> {
    match value {
        0..=19 => Ok(value),
        _ => Err(ProtocolError::MalformedFrame(value)),
    }
}

// ============================================================
// 帧头增量读取（接收侧）
// ============================================================

#[derive(Clone, Copy, PartialEq)]
enum ReaderState {
    /// 扫描帧头起始（ZPAD ZPAD ZDLE 或 ZPAD ZDLE）
    SeekingZpad,
    /// 已识别帧头起始，读编码字节
    ReadingEncoding,
    /// 已识别编码，读帧头负载
    ReadingData,
}

/// 增量式帧头读取器：从字节流中扫描 ZPAD/ZDLE 序列、解码负载并校验 CRC。
/// 各 `read`/`read_byte` 返回 `None` 表示输入耗尽（跨批续读）。
struct HeaderReader {
    state: ReaderState,
    /// 已见的 ZPAD 前缀计数（0/1/2）
    zpad_state: u8,
    encoding: Encoding,
    /// 解转义后的帧头字节缓冲
    buf: Vec<u8>,
    expected_len: usize,
    /// ZDLE 已到、转义字节未到（跨批）
    escape_pending: bool,
    /// resync 容错模式：坏帧后的窗口尾部按垃圾跳过，直到下一个合法帧头
    resyncing: bool,
}

impl HeaderReader {
    const fn new() -> Self {
        Self {
            state: ReaderState::SeekingZpad,
            zpad_state: 0,
            encoding: Encoding::Zbin,
            buf: Vec::new(),
            expected_len: 0,
            escape_pending: false,
            resyncing: false,
        }
    }

    /// 进入 resync 模式：损坏数据子包后，发送方在响应 ZRPOS 前仍在传输
    /// 中止窗口的尾部（任意字节，部分形似帧头起始），这些按垃圾跳过而非
    /// 致命错误，直到下一个合法帧头解码完成。
    fn enter_resync(&mut self) {
        self.resyncing = true;
    }

    /// 重置帧扫描状态（保留 resync 容错：伪装帧头的垃圾不清除容忍度）
    fn reset(&mut self) {
        self.state = ReaderState::SeekingZpad;
        self.zpad_state = 0;
        self.buf.clear();
        self.escape_pending = false;
    }

    /// 推进 ZPAD 前缀状态，返回是否识别出帧头起始（ZPAD 后跟 ZDLE）。
    /// binary 帧头（ZBIN/ZBIN32）为单 ZPAD + ZDLE，hex 为双 ZPAD + ZDLE，
    /// 两者都以 ZPAD + ZDLE 序列起始，编码字节随后区分。
    fn advance_zpad_state(&mut self, byte: u8) -> bool {
        match self.zpad_state {
            0 => {
                if byte == ZPAD {
                    self.zpad_state = 1;
                }
                false
            }
            _ => {
                if byte == ZDLE {
                    self.zpad_state = 0;
                    return true;
                }
                self.zpad_state = if byte == ZPAD { 2 } else { 0 };
                false
            }
        }
    }

    /// 识别编码字节并进入读负载状态。
    /// 返回 `Err(MalformedHeader)` 表示编码字节不合法（已 reset）。
    fn start_reading_data(&mut self, byte: u8) -> Result<(), ProtocolError> {
        let Some(encoding) = Encoding::from_byte(byte) else {
            self.reset();
            return Err(ProtocolError::MalformedHeader);
        };
        self.expected_len = encoding.read_size();
        self.encoding = encoding;
        self.escape_pending = false;
        self.buf.clear();
        self.state = ReaderState::ReadingData;
        Ok(())
    }

    /// 从输入读取一个解转义字节（跨批保留 ZDLE 状态）
    fn read_byte(&mut self, input: &[u8], pos: &mut usize) -> Option<u8> {
        if self.escape_pending {
            let b = *input.get(*pos)?;
            *pos += 1;
            self.escape_pending = false;
            return Some(zdle_unescape(b));
        }
        let b = *input.get(*pos)?;
        *pos += 1;
        if b == ZDLE {
            // 批次边界恰落在 ZDLE 与转义字节之间：必须保留转义状态，
            // 否则下批首字节被当原始字节导致帧头/校验误判
            let Some(next) = input.get(*pos).copied() else {
                self.escape_pending = true;
                return None;
            };
            *pos += 1;
            return Some(zdle_unescape(next));
        }
        Some(b)
    }

    /// 尝试从输入读取一个完整帧头。返回 `Ok(None)` 表示输入耗尽。
    fn read(&mut self, input: &[u8], pos: &mut usize) -> Result<Option<Header>, ProtocolError> {
        loop {
            match self.state {
                ReaderState::SeekingZpad => {
                    let Some(byte) = input.get(*pos).copied() else {
                        return Ok(None);
                    };
                    *pos += 1;
                    if self.advance_zpad_state(byte) {
                        self.state = ReaderState::ReadingEncoding;
                    }
                }
                ReaderState::ReadingEncoding => {
                    let Some(byte) = input.get(*pos).copied() else {
                        return Ok(None);
                    };
                    *pos += 1;
                    match self.start_reading_data(byte) {
                        Ok(()) => {}
                        // `ZPAD ZDLE` 出现在窗口垃圾中间不是真帧头：resync
                        // 中继续扫描，否则上抛
                        Err(e) => {
                            if self.resyncing {
                                continue;
                            }
                            return Err(e);
                        }
                    }
                }
                ReaderState::ReadingData => {
                    while self.buf.len() < self.expected_len {
                        let Some(byte) = self.read_byte(input, pos) else {
                            return Ok(None);
                        };
                        self.buf.push(byte);
                    }
                    let (encoding, payload) = (self.encoding, std::mem::take(&mut self.buf));
                    let header = match decode_header(encoding, &payload) {
                        Ok(header) => header,
                        Err(e) => {
                            self.reset();
                            // 垃圾偶然通过编码字节仍会败于 CRC：resync 中
                            // 再跳过一个坏帧头，不是致命错误
                            if self.resyncing {
                                continue;
                            }
                            return Err(e);
                        }
                    };
                    // 合法帧头结束 resync：流已对齐，恢复致命语义
                    self.resyncing = false;
                    self.reset();
                    return Ok(Some(header));
                }
            }
        }
    }
}

// ============================================================
// 子包 CRC 增量校验（接收侧）
// ============================================================

/// 子包 CRC 流式校验器：随数据字节增量更新，结束符后读完整 CRC 并比对。
struct SubpacketCrc {
    calc16: Crc16Calc,
    calc32: Crc32Calc,
    buf: [u8; 4],
    bytes_read: u8,
    escape_pending: bool,
}

impl SubpacketCrc {
    const fn new() -> Self {
        Self {
            calc16: Crc16Calc::new(),
            calc32: Crc32Calc::new(),
            buf: [0; 4],
            bytes_read: 0,
            escape_pending: false,
        }
    }

    fn reset(&mut self) {
        self.calc16 = Crc16Calc::new();
        self.calc32 = Crc32Calc::new();
        self.bytes_read = 0;
        self.escape_pending = false;
    }

    fn update(&mut self, byte: u8, encoding: Encoding) {
        if encoding == Encoding::Zbin32 {
            self.calc32.update(&[byte]);
        } else {
            self.calc16.update(&[byte]);
        }
    }

    /// 结束符后读取 CRC 字节（解转义、跨批）并校验。
    /// 返回 `Ok(None)` 表示输入耗尽。
    fn process(
        &mut self,
        input: &[u8],
        pos: &mut usize,
        encoding: Encoding,
    ) -> Result<Option<()>, ProtocolError> {
        let crc_len = if encoding == Encoding::Zbin32 { 4 } else { 2 };
        let Some(byte) = self.read_byte(input, pos) else {
            return Ok(None);
        };
        self.buf[self.bytes_read as usize] = byte;
        self.bytes_read += 1;
        if self.bytes_read < crc_len {
            return Ok(None);
        }
        if encoding == Encoding::Zbin32 {
            let expected = self.calc32.finalize().to_le_bytes();
            if expected != self.buf {
                diag(&format!(
                    "recv: 子包CRC32判坏 期望={:02x?} 实际={:02x?}",
                    expected, self.buf
                ));
                return Err(ProtocolError::UnexpectedCrc32);
            }
        } else {
            let expected = self.calc16.finalize().to_be_bytes();
            if expected != [self.buf[0], self.buf[1]] {
                diag(&format!(
                    "recv: 子包CRC16判坏 期望={:02x?} 实际={:02x?}",
                    expected,
                    [self.buf[0], self.buf[1]]
                ));
                return Err(ProtocolError::UnexpectedCrc16);
            }
        }
        Ok(Some(()))
    }

    /// 从输入读取一个解转义字节（跨批保留 ZDLE 状态）
    fn read_byte(&mut self, input: &[u8], pos: &mut usize) -> Option<u8> {
        if self.escape_pending {
            let b = *input.get(*pos)?;
            *pos += 1;
            self.escape_pending = false;
            return Some(zdle_unescape(b));
        }
        let b = *input.get(*pos)?;
        *pos += 1;
        if b == ZDLE {
            // 批次边界恰落在 ZDLE 与转义字节之间：必须保留转义状态，
            // 否则下批首字节被当原始字节喂 CRC 导致校验误判
            let Some(next) = input.get(*pos).copied() else {
                self.escape_pending = true;
                return None;
            };
            *pos += 1;
            return Some(zdle_unescape(next));
        }
        Some(b)
    }
}

// ============================================================
// 接收器状态机
// ============================================================

/// 接收器子包阶段
#[derive(Clone, Copy, PartialEq)]
enum SubpacketPhase {
    /// 无子包进行中
    Idle,
    /// hex 帧头后的 CR LF XON 行尾待跳过（framing，非负载）
    SkipTrailer,
    /// 读子包数据字节
    Reading,
    /// 数据读毕，读 CRC 字节
    Crc(u8),
    /// CRC 校验通过，数据待调用方写盘
    Writing(u8),
}

/// 接收器事件（事件队列载荷）
#[derive(Clone, Copy, PartialEq)]
enum RecvEvent {
    FileComplete,
    SessionComplete,
    Aborted,
}

/// rz/sz 接收器状态机（对端 sz → 本地落盘）。
///
/// 通过 `submit_wire` 喂入线路字节、`poll` 取动作、`wire_written`/`file_written`
/// 汇报写盘进度、`timeout` 驱动握手重试。
pub struct Receiver {
    /// 会话/文件阶段主状态
    state: RecvPhase,
    /// 运行中的文件偏移（已确认的好偏移）
    count: u32,
    file_name: Vec<u8>,
    file_size: u32,
    /// 当前子包数据缓冲
    buf: Vec<u8>,
    /// buf 中已被调用方写盘（file_written）的字节数
    buf_write_offset: usize,
    data_encoding: Encoding,
    header_reader: HeaderReader,
    subpacket_phase: SubpacketPhase,
    /// 子包数据阶段 ZDLE 跨批状态
    subpacket_escape_pending: bool,
    crc: SubpacketCrc,
    /// 待写线路字节
    outgoing: Vec<u8>,
    outgoing_offset: usize,
    /// 待消费事件
    pending_event: Option<RecvEvent>,
    /// FileStarted 事件待输出标志（借用约束下以一次性标志输出）
    file_started_pending: bool,
    /// 构造时通告的能力（timeout 重发 ZRINIT 用同一声明）
    buffer_len: u16,
    overlapped_io: bool,
    /// 连续损坏数据子包重试预算
    zrpos_retries: u8,
    /// 当前有活跃文件（accept 后、完成前）
    file_active: bool,
}

/// 接收器主状态
#[derive(Clone, Copy, PartialEq)]
enum RecvPhase {
    /// 会话开始（等 ZRQINIT/第一个 ZFILE）
    SessionBegin,
    /// 文件已通告（元数据读毕），等 ZDATA
    FileBegin,
    /// 读文件数据子包
    FileReadingSubpacket,
    /// 数据子包结束（ZCRCW/ZCRCE），等 ZDATA/ZEOF/ZFIN
    FileWaitingSubpacket,
    /// ZSINIT 的 attn 子包读取中
    SinitReadingData,
    /// 会话结束
    SessionEnd,
}

impl Receiver {
    /// 创建接收器。`buffer_len` 为通告的接收缓冲字节数，0 表示非停等 I/O
    /// （发送方可连续流式传输）；`overlapped_io` 附加 CANOVIO（边收边写盘）。
    /// 本应用在 SSH 可靠流上使用 `(0, true)` 组合。
    pub fn with_flow_control(buffer_len: u16, overlapped_io: bool) -> Result<Self, ProtocolError> {
        let mut receiver = Self {
            state: RecvPhase::SessionBegin,
            count: 0,
            file_name: Vec::new(),
            file_size: 0,
            buf: Vec::with_capacity(SUBPACKET_MAX_SIZE),
            buf_write_offset: 0,
            data_encoding: Encoding::Zbin,
            header_reader: HeaderReader::new(),
            subpacket_phase: SubpacketPhase::Idle,
            subpacket_escape_pending: false,
            crc: SubpacketCrc::new(),
            outgoing: Vec::new(),
            outgoing_offset: 0,
            pending_event: None,
            file_started_pending: false,
            buffer_len,
            overlapped_io,
            zrpos_retries: 0,
            file_active: false,
        };
        receiver.queue_zrinit(buffer_len, overlapped_io)?;
        Ok(receiver)
    }

    /// 线路输入喂入状态机，返回消费的字节数。
    ///
    /// # 错误
    ///
    /// 帧头格式错误返回 `MalformedHeader`（已排队 NAK）；数据子包 CRC 损坏
    /// 返回 `UnexpectedCrc16/32`（已排队 ZRPOS 重退），可恢复。
    pub fn submit_wire(&mut self, input: &[u8]) -> Result<usize, ProtocolError> {
        let mut pos = 0usize;
        loop {
            if self.blocked() {
                break;
            }
            let before = pos;

            // 子包阶段（数据/元数据/attn）优先于帧头
            if matches!(
                self.subpacket_phase,
                SubpacketPhase::Reading
                    | SubpacketPhase::Crc(_)
                    | SubpacketPhase::SkipTrailer
            ) {
                let Some(()) = self.process_subpacket(input, &mut pos)? else {
                    break;
                };
                if pos == before {
                    break;
                }
                continue;
            }

            let Some(header) = self.header_reader.read(input, &mut pos)? else {
                break;
            };
            self.handle_header(header)?;
            if pos == before || pos == input.len() {
                break;
            }
        }
        Ok(pos)
    }

    /// 汇报 `n` 字节协议数据已写线路。
    pub fn wire_written(&mut self, n: usize) {
        let remaining = self.outgoing.len().saturating_sub(self.outgoing_offset);
        self.outgoing_offset += n.min(remaining);
        if self.outgoing_offset >= self.outgoing.len() {
            self.outgoing.clear();
            self.outgoing_offset = 0;
        }
    }

    /// 汇报 `n` 字节文件数据已落盘。缓冲写满时推进子包状态机。
    pub fn file_written(&mut self, n: usize) -> Result<(), ProtocolError> {
        let SubpacketPhase::Writing(kind) = self.subpacket_phase else {
            return Ok(());
        };
        let remaining = self.buf.len().saturating_sub(self.buf_write_offset);
        self.buf_write_offset += n.min(remaining);
        if self.buf_write_offset < self.buf.len() {
            return Ok(());
        }
        self.finish_subpacket(kind);
        Ok(())
    }

    /// 协议响应超时：会话/文件等待阶段重发 ZRINIT 握手。
    pub fn timeout(&mut self) {
        if matches!(self.state, RecvPhase::SessionBegin | RecvPhase::FileBegin)
            && !self.outgoing_pending()
        {
            let (buffer_len, overlapped_io) = (self.buffer_len, self.overlapped_io);
            let _ = self.queue_zrinit(buffer_len, overlapped_io);
        }
    }

    /// 取下一个动作：事件优先，其次线路字节，其次文件数据，最后 Idle。
    pub fn poll(&mut self) -> Action<'_> {
        if let Some(event) = self.pending_event.take() {
            return match event {
                RecvEvent::FileComplete => Action::Event(Event::FileCompleted),
                RecvEvent::SessionComplete => Action::Event(Event::SessionCompleted),
                RecvEvent::Aborted => Action::Event(Event::Aborted),
            };
        }
        if self.outgoing_pending() {
            return Action::WriteWire(&self.outgoing[self.outgoing_offset..]);
        }
        if self.file_started_pending {
            self.file_started_pending = false;
            return Action::Event(Event::FileStarted(FileInfo {
                name: &self.file_name,
                size: Some(Position(self.file_size)),
            }));
        }
        if let SubpacketPhase::Writing(_) = self.subpacket_phase {
            if self.buf_write_offset < self.buf.len() {
                return Action::WriteFile(&self.buf[self.buf_write_offset..]);
            }
        }
        Action::Idle
    }

    /// 当前状态名（诊断日志用，只读）
    pub fn state_name(&self) -> &'static str {
        match self.state {
            RecvPhase::SessionBegin => "SessionBegin",
            RecvPhase::FileBegin => "FileBegin",
            RecvPhase::FileReadingSubpacket => "FileReadingSubpacket",
            RecvPhase::FileWaitingSubpacket => "FileWaitingSubpacket",
            RecvPhase::SinitReadingData => "SinitReadingData",
            RecvPhase::SessionEnd => "SessionEnd",
        }
    }

    /// 线路字节待写
    fn outgoing_pending(&self) -> bool {
        self.outgoing_offset < self.outgoing.len()
    }

    /// 入队待消费事件（单槽：poll 未消费前 blocked 挡住新输入防覆盖）
    fn push_event(&mut self, event: RecvEvent) {
        self.pending_event = Some(event);
    }

    /// 无进一步线路输入可消费（需调用方先取事件、排空 outgoing 或写盘）
    fn blocked(&self) -> bool {
        self.outgoing_pending()
            || self.pending_event.is_some()
            || self.file_started_pending
            || matches!(self.subpacket_phase, SubpacketPhase::Writing(_))
    }

    fn queue_header(&mut self, header: Header) -> Result<(), ProtocolError> {
        if self.outgoing_pending() {
            // 理论不可达：submit_wire 在 outgoing 非空时被 blocked 挡住。
            // 防御性忽略：丢帧由对端重试机制恢复
            return Ok(());
        }
        header.write(&mut self.outgoing);
        Ok(())
    }

    fn queue_zrinit(&mut self, buffer_len: u16, overlapped_io: bool) -> Result<(), ProtocolError> {
        let mut flags = CANFC32;
        if overlapped_io {
            flags |= CANOVIO;
        }
        let size = buffer_len.to_le_bytes();
        self.queue_header(Header {
            encoding: Encoding::ZHex,
            frame: ZRINIT,
            flags: [size[0], size[1], 0, flags],
        })
    }

    fn queue_zrpos(&mut self, count: u32) -> Result<(), ProtocolError> {
        self.queue_header(Header {
            encoding: Encoding::ZHex,
            frame: ZRPOS,
            flags: count.to_le_bytes(),
        })
    }

    fn queue_zack(&mut self) -> Result<(), ProtocolError> {
        self.queue_header(Header {
            encoding: Encoding::ZHex,
            frame: ZACK,
            flags: self.count.to_le_bytes(),
        })
    }

    fn queue_zfin(&mut self) -> Result<(), ProtocolError> {
        self.queue_header(Header {
            encoding: Encoding::ZHex,
            frame: ZFIN,
            flags: [0; 4],
        })
    }

    fn handle_header(&mut self, header: Header) -> Result<(), ProtocolError> {
        diag(&format!(
            "recv: 收到帧 {} enc={:?} 远端count={}",
            header.frame, header.encoding, header.count()
        ));
        match header.frame {
            // 会话开始阶段：对端 ZRQINIT（或重发的 ZDATA）→ 重发 ZRINIT
            ZRQINIT | ZDATA if self.state == RecvPhase::SessionBegin => {
                let (buffer_len, overlapped_io) = (self.buffer_len, self.overlapped_io);
                self.queue_zrinit(buffer_len, overlapped_io)?;
            }
            // ZSINIT（如 sz -e）后跟 attn 数据子包，读取后 ACK
            ZSINIT if self.state == RecvPhase::SessionBegin => {
                // lrzsz 的 ZSINIT 帧头为 ZHEX，但子包为 binary CRC16：
                // hex 帧行尾在帧头与子包之间，需跳过
                self.data_encoding = match header.encoding {
                    Encoding::ZHex => Encoding::Zbin,
                    other => other,
                };
                self.state = RecvPhase::SinitReadingData;
                self.subpacket_phase = if header.encoding == Encoding::ZHex {
                    SubpacketPhase::SkipTrailer
                } else {
                    SubpacketPhase::Reading
                };
                self.subpacket_escape_pending = false;
                self.crc.reset();
                self.buf.clear();
                self.buf_write_offset = 0;
            }
            // ZFILE：读元数据子包（文件名\0大小）
            ZFILE if matches!(self.state, RecvPhase::SessionBegin | RecvPhase::FileBegin) => {
                self.data_encoding = header.encoding;
                self.state = RecvPhase::FileBegin;
                self.subpacket_phase = SubpacketPhase::Reading;
                self.subpacket_escape_pending = false;
                self.crc.reset();
                self.buf.clear();
                self.buf_write_offset = 0;
            }
            // ZDATA：count 不匹配（对端续传点错位）→ ZRPOS 纠正
            ZDATA if matches!(
                self.state,
                RecvPhase::FileBegin | RecvPhase::FileWaitingSubpacket
            ) =>
            {
                // 文件完成后（ZEOF 已确认、无活跃文件）的滞后 ZDATA 是在途
                // 重复帧：忽略不回退——已完成的对端收到 ZRPOS 会错乱中止
                //（真实 sz 打印 "sz: skipped" 并退出，会话无法干净收尾）
                if self.state == RecvPhase::FileBegin && !self.file_active {
                    return Ok(());
                }
                if header.count() != self.count {
                    diag(&format!(
                        "recv: ZDATA count 不匹配 远端={} 本地={} → ZRPOS",
                        header.count(),
                        self.count
                    ));
                    self.queue_zrpos(self.count)?;
                    return Ok(());
                }
                self.data_encoding = header.encoding;
                self.state = RecvPhase::FileReadingSubpacket;
                self.subpacket_phase = SubpacketPhase::Reading;
                self.subpacket_escape_pending = false;
                self.crc.reset();
                self.buf.clear();
                self.buf_write_offset = 0;
            }
            // ZEOF：文件完成（count 匹配）→ ZRINIT + FileComplete
            ZEOF
                if (self.state == RecvPhase::FileWaitingSubpacket
                    || (self.state == RecvPhase::FileBegin && self.file_active))
                    && header.count() == self.count =>
            {
                // FileBegin 同时覆盖「文件活跃」（accept 后、ZDATA 前的立即
                // ZEOF，如空文件/续传点在 EOF）与「文件间」两种时态；
                // file_active 防止文件间的散落/重发 ZEOF 重复触发完成事件
                let (buffer_len, overlapped_io) = (self.buffer_len, self.overlapped_io);
                self.queue_zrinit(buffer_len, overlapped_io)?;
                self.state = RecvPhase::FileBegin;
                self.file_active = false;
                self.push_event(RecvEvent::FileComplete);
            }
            // 中止
            ZABORT | ZCAN => {
                self.state = RecvPhase::SessionEnd;
                self.push_event(RecvEvent::Aborted);
            }
            // ZFIN：对端结束会话
            ZFIN if matches!(
                self.state,
                RecvPhase::FileWaitingSubpacket | RecvPhase::FileBegin
            ) =>
            {
                self.queue_zfin()?;
                self.state = RecvPhase::SessionEnd;
                self.push_event(RecvEvent::SessionComplete);
            }
            // 其余帧类型（ZFERR/ZCOMMAND 等）本应用不处理，忽略
            _ => {}
        }
        Ok(())
    }

    /// ZFILE 元数据解析：文件名 \0 大小（空格分隔取首段）
    fn parse_zfile_buf(&mut self) -> Result<(), ProtocolError> {
        let payload = &self.buf;
        let mut fields = payload.split(|&b| b == b'\0');

        let name_bytes = fields.next().ok_or(ProtocolError::MalformedFileName)?;
        if name_bytes.is_empty() {
            return Err(ProtocolError::MalformedFileName);
        }
        self.file_name = name_bytes.to_vec();

        self.file_size = if let Some(size_bytes) = fields.next() {
            let size_field = size_bytes.split(|&b| b == b' ').next().unwrap_or_default();
            parse_file_size(size_field)?
        } else {
            0
        };
        self.count = 0;
        Ok(())
    }

    /// 处理子包阶段字节。返回 `Ok(None)` 表示输入耗尽（跨批续读）。
    fn process_subpacket(
        &mut self,
        input: &[u8],
        pos: &mut usize,
    ) -> Result<Option<()>, ProtocolError> {
        match self.subpacket_phase {
            SubpacketPhase::SkipTrailer => {
                // hex 帧头行尾（CR LF XON）在帧头与子包之间：framing 字节，
                // 跳过后第一个负载字节就地翻到 Reading 处理
                loop {
                    let Some(byte) = input.get(*pos).copied() else {
                        return Ok(None);
                    };
                    *pos += 1;
                    if matches!(byte, 0x0d | 0x0a | 0x8d | 0x8a | 0x11 | 0x13 | 0x91 | 0x93) {
                        continue;
                    }
                    self.subpacket_phase = SubpacketPhase::Reading;
                    return self.subpacket_plain_byte(input, pos, byte);
                }
            }
            SubpacketPhase::Reading => {
                if self.subpacket_escape_pending {
                    let Some(byte) = input.get(*pos).copied() else {
                        return Ok(None);
                    };
                    *pos += 1;
                    self.subpacket_escape_pending = false;
                    return self.subpacket_followup_byte(byte);
                }
                let Some(byte) = input.get(*pos).copied() else {
                    return Ok(None);
                };
                *pos += 1;
                self.subpacket_plain_byte(input, pos, byte)
            }
            SubpacketPhase::Crc(kind) => {
                let crc_len = if self.data_encoding == Encoding::Zbin32 { 4 } else { 2 };
                // 循环读 CRC 字节：process 每次调用读 1 字节（跨批保留
                // bytes_read/escape 状态），读满即校验。Ok(None) 时必须
                // 继续循环消费剩余 CRC 字节——若当作输入耗尽 break，尾部
                // 字节会被调用方丢弃、CRC 永远读不满、传输卡死
                while (self.crc.bytes_read as usize) < crc_len {
                    if *pos >= input.len() {
                        return Ok(None);
                    }
                    match self.crc.process(input, pos, self.data_encoding) {
                        Ok(Some(())) => break,
                        Ok(None) => {}
                        // 数据子包 CRC 损坏可恢复（rz/sz 相对 X/YMODEM 的核心价值：
                        // 接收方请求从最后好偏移重传）。元数据（ZFILE/ZSINIT）CRC
                        // 损坏保持致命：无偏移可回退
                        Err(e @ (ProtocolError::UnexpectedCrc16 | ProtocolError::UnexpectedCrc32))
                            if self.state == RecvPhase::FileReadingSubpacket =>
                        {
                            return self.recover_corrupt_subpacket(e);
                        }
                        Err(e) => return Err(e),
                    }
                }

                if self.state == RecvPhase::FileBegin {
                    // 元数据子包完成：解析文件名/大小
                    self.parse_zfile_buf()?;
                    self.buf.clear();
                    self.buf_write_offset = 0;
                    self.crc.reset();
                    self.subpacket_escape_pending = false;
                    // 自动接受：立即从偏移 0 接收（目录已由前端对话框选定）
                    self.queue_zrpos(0)?;
                    self.state = RecvPhase::FileBegin;
                    // 空文件/续传点在 EOF 时发送方以立即 ZEOF(0) 应答且无数据
                    // 帧：file_active 让 ZEOF 处理器从 FileBegin 接受该完成
                    self.file_active = true;
                    self.subpacket_phase = SubpacketPhase::Idle;
                    self.file_started_pending = true;
                } else if self.state == RecvPhase::SinitReadingData {
                    // attn 字符串不使用，但发送方阻塞到帧被确认为止
                    self.buf.clear();
                    self.buf_write_offset = 0;
                    self.crc.reset();
                    self.subpacket_escape_pending = false;
                    self.queue_header(Header {
                        encoding: Encoding::ZHex,
                        frame: ZACK,
                        flags: [0; 4],
                    })?;
                    self.state = RecvPhase::SessionBegin;
                    self.subpacket_phase = SubpacketPhase::Idle;
                } else {
                    // 数据子包：数据交给调用方写盘（Writing 阶段）
                    self.subpacket_phase = SubpacketPhase::Writing(kind);
                    self.buf_write_offset = 0;
                    if self.buf.is_empty() {
                        self.finish_subpacket(kind);
                    }
                }
                Ok(Some(()))
            }
            SubpacketPhase::Writing(_) => Ok(Some(())),
            SubpacketPhase::Idle => Err(ProtocolError::InvalidState),
        }
    }

    /// 子包数据阶段：ZDLE 后的字节（结束符或转义数据字节）
    fn subpacket_followup_byte(&mut self, byte: u8) -> Result<Option<()>, ProtocolError> {
        if byte == ZCRCE || byte == ZCRCG || byte == ZCRCQ || byte == ZCRCW {
            self.crc.update(byte, self.data_encoding);
            self.subpacket_phase = SubpacketPhase::Crc(byte);
        } else {
            let unescaped = zdle_unescape(byte);
            self.buf.push(unescaped);
            self.crc.update(unescaped, self.data_encoding);
        }
        Ok(Some(()))
    }

    /// 子包数据阶段：普通（非转义延续）字节
    fn subpacket_plain_byte(
        &mut self,
        input: &[u8],
        pos: &mut usize,
        byte: u8,
    ) -> Result<Option<()>, ProtocolError> {
        if byte == ZDLE {
            let Some(next) = input.get(*pos).copied() else {
                self.subpacket_escape_pending = true;
                return Ok(None);
            };
            *pos += 1;
            return self.subpacket_followup_byte(next);
        }
        self.buf.push(byte);
        self.crc.update(byte, self.data_encoding);
        Ok(Some(()))
    }

    /// 子包完成：推进偏移、按结束符分发状态
    fn finish_subpacket(&mut self, kind: u8) {
        let len = u32::try_from(self.buf.len()).unwrap_or(u32::MAX);
        // 运行偏移为 32 位：超 4 GiB 的流式发送（无论 bug 还是恶意）不得
        // 回绕到低偏移导致失步，饱和即可
        self.count = self.count.saturating_add(len);
        diag(&format!(
            "recv: 子包落地 kind={kind:#04x} len={len} count={}",
            self.count
        ));
        self.buf.clear();
        self.buf_write_offset = 0;
        self.crc.reset();
        // 干净子包落地：偏移推进，损坏连击打断、重试预算补满
        self.zrpos_retries = 0;

        match kind {
            ZCRCW => {
                self.subpacket_phase = SubpacketPhase::Idle;
                let _ = self.queue_zack();
                self.state = RecvPhase::FileWaitingSubpacket;
            }
            ZCRCQ => {
                self.subpacket_phase = SubpacketPhase::Reading;
                let _ = self.queue_zack();
            }
            ZCRCG => {
                self.subpacket_phase = SubpacketPhase::Reading;
            }
            ZCRCE => {
                self.subpacket_phase = SubpacketPhase::Idle;
                self.state = RecvPhase::FileWaitingSubpacket;
            }
            _ => {}
        }
    }

    /// 数据子包 CRC 损坏恢复：ZRPOS(count) 请求重传，缓冲坏字节丢弃、count
    /// 保持在最后确认好偏移，保证不损坏落盘；帧头读取器进入 resync 容错
    /// （流式发送方在响应 ZRPOS 前仍会发出中止窗口尾部）。重试预算耗尽后
    /// 原样上抛错误。
    fn recover_corrupt_subpacket(
        &mut self,
        err: ProtocolError,
    ) -> Result<Option<()>, ProtocolError> {
        self.zrpos_retries = self.zrpos_retries.saturating_add(1);
        if self.zrpos_retries > MAX_ZRPOS_RETRIES {
            return Err(err);
        }
        diag(&format!(
            "recv: 子包CRC恢复 偏移={} 重试={}",
            self.count, self.zrpos_retries
        ));
        self.buf.clear();
        self.buf_write_offset = 0;
        self.crc.reset();
        self.subpacket_escape_pending = false;
        self.queue_zrpos(self.count)?;
        self.state = RecvPhase::FileWaitingSubpacket;
        self.subpacket_phase = SubpacketPhase::Idle;
        self.header_reader.enter_resync();
        Ok(Some(()))
    }
}

// ============================================================
// 发送器状态机
// ============================================================

/// 发送器事件
#[derive(Clone, Copy, PartialEq)]
enum SendEvent {
    FileComplete,
    SessionComplete,
    Aborted,
}

/// 发送器文件请求（读文件数据的参数）
#[derive(Clone, Copy)]
struct FileRequest {
    offset: u32,
    len: usize,
}

/// 发送器主状态
#[derive(Clone, Copy, PartialEq)]
enum SendPhase {
    /// 等接收方 ZRINIT（构造即排队 ZRQINIT）
    WaitReceiverInit,
    /// ZRINIT 已协商，可接受新文件
    ReadyForFile,
    /// ZFILE 已发，等 ZRPOS
    WaitFilePos,
    /// 等调用方提供文件数据（pending_request）
    NeedFileData,
    /// 数据子包已发，等 ACK
    WaitFileAck,
    /// ZEOF 已发，等 ZRINIT
    WaitFileDone,
    /// ZFIN 已发，等对端 ZFIN
    WaitFinish,
    /// 会话结束
    Done,
}

/// rz/sz 发送器状态机（本地文件 → 对端 rz）。
///
/// 通过 `start_file` 注册文件元数据、`submit_file` 提供文件数据、`submit_wire`
/// 喂入线路字节、`poll` 取动作、`wire_written` 汇报写线路进度、`timeout` 驱动
/// 握手重试。
pub struct Sender {
    state: SendPhase,
    file_name: Vec<u8>,
    file_size: u32,
    /// 已注册待发文件（start_file 后、完成前）
    has_file: bool,
    pending_request: Option<FileRequest>,
    /// 当前 ACK 窗口剩余子包数
    frame_remaining: usize,
    /// 下一个数据子包是否需要先发 ZDATA 帧头
    frame_needs_header: bool,
    /// 待写线路字节
    outgoing: Vec<u8>,
    outgoing_offset: usize,
    header_reader: HeaderReader,
    pending_event: Option<SendEvent>,
    /// 当前文件完成后结束会话
    finish_requested: bool,
    /// 非停等流式窗口（子包数）；usize::MAX 表示完全非停等
    streaming_window: usize,
    /// 接收方通告了非停等 I/O（streaming_window 生效的前提）
    rx_nonstop: bool,
}

/// 每个数据子包的负载上限
const SENDER_SUBPACKET_MAX: usize = SUBPACKET_MAX_SIZE;

impl Sender {
    /// 创建发送器（构造即排队 ZRQINIT 握手帧）。
    pub fn new() -> Result<Self, ProtocolError> {
        let mut sender = Self {
            state: SendPhase::WaitReceiverInit,
            file_name: Vec::new(),
            file_size: 0,
            has_file: false,
            pending_request: None,
            frame_remaining: 0,
            frame_needs_header: false,
            outgoing: Vec::new(),
            outgoing_offset: 0,
            header_reader: HeaderReader::new(),
            pending_event: None,
            finish_requested: false,
            streaming_window: 10,
            rx_nonstop: false,
        };
        sender.queue_zrqinit()?;
        Ok(sender)
    }

    /// 设置非停等流式窗口（子包数/ACK）。SSH 可靠流上每个 ACK 等待是一轮
    /// 完整往返，调大窗口或传 `usize::MAX`（完全非停等）消除大传输的主要
    /// 延迟成本。小于 1 的值收敛为 1。
    pub fn set_streaming_window(&mut self, subpackets: usize) {
        self.streaming_window = subpackets.max(1);
        // ZRINIT 可能已协商：接收方非停等时立即重算节奏
        if self.rx_nonstop {
            self.frame_remaining = self.streaming_window;
        }
    }

    /// 当前状态名（诊断日志用，只读）
    pub fn state_name(&self) -> &'static str {
        match self.state {
            SendPhase::WaitReceiverInit => "WaitReceiverInit",
            SendPhase::ReadyForFile => "ReadyForFile",
            SendPhase::WaitFilePos => "WaitFilePos",
            SendPhase::NeedFileData => "NeedFileData",
            SendPhase::WaitFileAck => "WaitFileAck",
            SendPhase::WaitFileDone => "WaitFileDone",
            SendPhase::WaitFinish => "WaitFinish",
            SendPhase::Done => "Done",
        }
    }

    /// 注册待发文件元数据。文件大小必须已知。
    ///
    /// 在 `WaitReceiverInit`/`ReadyForFile` 阶段调用：WaitReceiverInit 时记录
    /// 元数据（ZRINIT 到达后自动发 ZFILE）；ReadyForFile 时立即发 ZFILE。
    ///
    /// # 错误
    ///
    /// 传输已在进行中（有文件待发）返回 `InvalidState`。
    pub fn start_file(&mut self, info: FileInfo) -> Result<(), ProtocolError> {
        let Some(size) = info.size else {
            return Err(ProtocolError::InvalidState);
        };
        if !matches!(self.state, SendPhase::WaitReceiverInit | SendPhase::ReadyForFile) {
            return Err(ProtocolError::InvalidState);
        }
        self.file_name = info.name.to_vec();
        self.file_size = size.get();
        self.has_file = true;
        self.pending_request = None;
        self.frame_remaining = 0;
        self.frame_needs_header = false;

        if self.state == SendPhase::ReadyForFile {
            if self.outgoing_pending() {
                return Err(ProtocolError::InvalidState);
            }
            self.queue_zfile()?;
            self.state = SendPhase::WaitFilePos;
        }
        Ok(())
    }

    /// 请求当前文件完成后结束会话。
    pub fn finish(&mut self) -> Result<(), ProtocolError> {
        self.finish_requested = true;
        if self.state == SendPhase::ReadyForFile {
            if self.outgoing_pending() {
                return Err(ProtocolError::InvalidState);
            }
            self.queue_zfin()?;
            self.state = SendPhase::WaitFinish;
        }
        Ok(())
    }

    /// 提供当前 [`Action::ReadFile`] 请求的文件数据。
    ///
    /// # 错误
    ///
    /// 数据为空或超过请求长度/文件剩余返回 `InvalidState`。
    pub fn submit_file(&mut self, data: &[u8]) -> Result<(), ProtocolError> {
        if self.state != SendPhase::NeedFileData {
            return Err(ProtocolError::InvalidState);
        }
        let Some(request) = self.pending_request else {
            return Err(ProtocolError::InvalidState);
        };
        if data.is_empty() || data.len() > request.len {
            return Err(ProtocolError::InvalidState);
        }
        let remaining = self.file_size.saturating_sub(request.offset) as usize;
        if data.len() > remaining {
            return Err(ProtocolError::InvalidState);
        }
        if self.outgoing_pending() {
            return Err(ProtocolError::InvalidState);
        }

        let offset = request.offset;
        let next_offset = offset.saturating_add(data.len() as u32);
        let remaining_after = self.file_size.saturating_sub(next_offset);
        // 最后一个子包（窗口尽/请求尽/文件尽）用 ZCRCW（等 ACK），否则
        // ZCRCG（连续发送）
        let is_last_in_frame =
            self.frame_remaining <= 1 || data.len() < request.len || remaining_after == 0;
        let kind = if is_last_in_frame {
            ZCRCW
        } else {
            ZCRCG
        };

        self.queue_zdata(offset, data, kind, self.frame_needs_header)?;
        self.frame_needs_header = false;

        if self.frame_remaining > 0 {
            self.frame_remaining -= 1;
        }

        if is_last_in_frame {
            self.pending_request = None;
            self.state = SendPhase::WaitFileAck;
            self.frame_remaining = 0;
        } else {
            let max_len = SENDER_SUBPACKET_MAX.min(remaining_after as usize);
            self.pending_request = Some(FileRequest {
                offset: next_offset,
                len: max_len,
            });
        }
        Ok(())
    }

    /// 线路输入喂入状态机，返回消费的字节数。
    ///
    /// # 错误
    ///
    /// 帧头格式错误返回 `MalformedHeader`（已排队 NAK）。
    pub fn submit_wire(&mut self, input: &[u8]) -> Result<usize, ProtocolError> {
        let mut pos = 0usize;
        loop {
            if self.outgoing_pending()
                || self.state == SendPhase::Done
                || self.pending_request.is_some()
            {
                break;
            }
            let before = pos;
            let Some(header) = self.header_reader.read(input, &mut pos)? else {
                break;
            };
            self.handle_header(header)?;
            if pos == before || pos == input.len() {
                break;
            }
        }
        Ok(pos)
    }

    /// 汇报 `n` 字节协议数据已写线路。
    pub fn wire_written(&mut self, n: usize) {
        let remaining = self.outgoing.len().saturating_sub(self.outgoing_offset);
        self.outgoing_offset += n.min(remaining);
        if self.outgoing_offset >= self.outgoing.len() {
            self.outgoing.clear();
            self.outgoing_offset = 0;
        }
    }

    /// 协议响应超时：等接收方初始化时重发 ZRQINIT。
    pub fn timeout(&mut self) {
        if self.state == SendPhase::WaitReceiverInit && !self.outgoing_pending() {
            let _ = self.queue_zrqinit();
        }
    }

    /// 取下一个动作：事件优先，其次线路字节，其次文件读请求，最后 Idle。
    pub fn poll(&mut self) -> Action<'_> {
        if let Some(event) = self.pending_event.take() {
            return match event {
                SendEvent::FileComplete => Action::Event(Event::FileCompleted),
                SendEvent::SessionComplete => Action::Event(Event::SessionCompleted),
                SendEvent::Aborted => Action::Event(Event::Aborted),
            };
        }
        if self.outgoing_pending() {
            return Action::WriteWire(&self.outgoing[self.outgoing_offset..]);
        }
        if let Some(request) = &self.pending_request {
            return Action::ReadFile {
                offset: Position(request.offset),
                max_len: request.len,
            };
        }
        Action::Idle
    }

    fn outgoing_pending(&self) -> bool {
        self.outgoing_offset < self.outgoing.len()
    }

    /// 入队待消费事件
    fn push_event(&mut self, event: SendEvent) {
        self.pending_event = Some(event);
    }

    fn queue_header(&mut self, header: Header) -> Result<(), ProtocolError> {
        if self.outgoing_pending() {
            // 理论不可达：submit_wire 在 outgoing 非空时提前 break。
            // 防御性忽略：丢帧由对端重试机制恢复
            return Ok(());
        }
        header.write(&mut self.outgoing);
        Ok(())
    }

    fn queue_zrqinit(&mut self) -> Result<(), ProtocolError> {
        self.queue_header(Header {
            encoding: Encoding::ZHex,
            frame: ZRQINIT,
            flags: [0; 4],
        })
    }

    /// 排队 ZFILE 帧头 + 元数据子包（文件名\0大小\0，ZCRCW 结束等 ACK）
    fn queue_zfile(&mut self) -> Result<(), ProtocolError> {
        if self.outgoing_pending() {
            return Ok(());
        }
        let mut payload = Vec::new();
        payload.extend_from_slice(&self.file_name);
        payload.push(b'\0');
        payload.extend_from_slice(self.file_size.to_string().as_bytes());
        payload.push(b'\0');

        Header {
            encoding: Encoding::Zbin32,
            frame: ZFILE,
            flags: [0; 4],
        }
        .write(&mut self.outgoing);
        encode_subpacket(&mut self.outgoing, true, ZCRCW, &payload);
        Ok(())
    }

    /// 排队 ZDATA 帧头（可选）+ 数据子包
    fn queue_zdata(
        &mut self,
        offset: u32,
        data: &[u8],
        kind: u8,
        include_header: bool,
    ) -> Result<(), ProtocolError> {
        if self.outgoing_pending() {
            return Ok(());
        }
        if include_header {
            Header {
                encoding: Encoding::Zbin32,
                frame: ZDATA,
                flags: offset.to_le_bytes(),
            }
            .write(&mut self.outgoing);
        }
        encode_subpacket(&mut self.outgoing, true, kind, data);
        Ok(())
    }

    fn queue_zeof(&mut self, offset: u32) -> Result<(), ProtocolError> {
        self.queue_header(Header {
            encoding: Encoding::Zbin32,
            frame: ZEOF,
            flags: offset.to_le_bytes(),
        })
    }

    fn queue_zfin(&mut self) -> Result<(), ProtocolError> {
        self.queue_header(Header {
            encoding: Encoding::ZHex,
            frame: ZFIN,
            flags: [0; 4],
        })
    }

    /// 排队会话结束的 "OO"（ZFIN 应答）
    fn queue_oo(&mut self) -> Result<(), ProtocolError> {
        if self.outgoing_pending() {
            return Ok(());
        }
        self.outgoing.push(b'O');
        self.outgoing.push(b'O');
        Ok(())
    }

    fn handle_header(&mut self, header: Header) -> Result<(), ProtocolError> {
        match header.frame {
            ZRINIT => self.on_zrinit(header),
            ZRPOS | ZACK => self.on_zrpos(header.count()),
            ZSKIP => {
                self.on_zskip();
                Ok(())
            }
            ZABORT | ZCAN => {
                self.on_abort();
                Ok(())
            }
            ZFIN => {
                self.on_zfin();
                Ok(())
            }
            // 其余帧类型本应用不处理；WaitReceiverInit 阶段重发 ZRQINIT
            _ => {
                if self.state == SendPhase::WaitReceiverInit {
                    self.queue_zrqinit()?;
                }
                Ok(())
            }
        }
    }

    fn on_zrinit(&mut self, header: Header) -> Result<(), ProtocolError> {
        self.update_receiver_caps(header);
        match self.state {
            SendPhase::WaitReceiverInit => {
                if self.has_file {
                    self.queue_zfile()?;
                    self.state = SendPhase::WaitFilePos;
                } else {
                    self.state = SendPhase::ReadyForFile;
                    if self.finish_requested {
                        self.queue_zfin()?;
                        self.state = SendPhase::WaitFinish;
                    }
                }
            }
            SendPhase::WaitFileDone => {
                self.push_event(SendEvent::FileComplete);
                self.has_file = false;
                if self.finish_requested {
                    self.queue_zfin()?;
                    self.state = SendPhase::WaitFinish;
                } else {
                    self.state = SendPhase::ReadyForFile;
                }
            }
            SendPhase::WaitFinish => {
                self.queue_oo()?;
                self.state = SendPhase::Done;
                self.push_event(SendEvent::SessionComplete);
            }
            _ => {}
        }
        Ok(())
    }

    /// 从 ZRINIT 标志解析接收方能力：缓冲大小 + CANOVIO 决定 ACK 节奏。
    fn update_receiver_caps(&mut self, header: Header) {
        let rx_buf_size = u16::from_le_bytes([header.flags[0], header.flags[1]]) as usize;
        let can_ovio = (header.flags[3] & CANOVIO) != 0;
        // 记录非停等标记：后续 set_streaming_window 可重算节奏
        self.rx_nonstop = rx_buf_size == 0 && can_ovio;
        if rx_buf_size == 0 && self.rx_nonstop {
            // 非停等（缓冲 0 + CANOVIO）：按流式窗口节奏发送，每个窗口尽
            // 才等一次 ACK
            self.frame_remaining = self.streaming_window;
        } else {
            // 停等：每个子包等 ACK
            self.frame_remaining = 1;
        }
    }

    fn on_zrpos(&mut self, offset: u32) -> Result<(), ProtocolError> {
        match self.state {
            SendPhase::WaitReceiverInit => {
                self.queue_zrqinit()?;
            }
            SendPhase::WaitFilePos | SendPhase::WaitFileAck | SendPhase::NeedFileData => {
                if offset >= self.file_size {
                    self.queue_zeof(offset)?;
                    self.state = SendPhase::WaitFileDone;
                    self.pending_request = None;
                } else {
                    let remaining = (self.file_size - offset) as usize;
                    let len = remaining.min(SENDER_SUBPACKET_MAX);
                    // 新窗口开始：按接收方能力重算节奏（非停等时为流式窗口）
                    self.frame_remaining = if self.rx_nonstop {
                        self.streaming_window
                    } else {
                        1
                    };
                    self.frame_needs_header = true;
                    self.pending_request = Some(FileRequest { offset, len });
                    self.state = SendPhase::NeedFileData;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn on_zskip(&mut self) {
        if matches!(
            self.state,
            SendPhase::WaitFilePos
                | SendPhase::NeedFileData
                | SendPhase::WaitFileAck
                | SendPhase::WaitFileDone
        ) {
            self.has_file = false;
            self.pending_request = None;
            self.frame_remaining = 0;
            self.push_event(SendEvent::FileComplete);
            self.state = SendPhase::ReadyForFile;
        }
    }

    fn on_abort(&mut self) {
        self.state = SendPhase::Done;
        self.pending_request = None;
        self.pending_event = Some(SendEvent::Aborted);
    }

    fn on_zfin(&mut self) {
        if self.state == SendPhase::WaitFinish {
            let _ = self.queue_oo();
            self.state = SendPhase::Done;
            self.push_event(SendEvent::SessionComplete);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- CRC 已知测试向量 ----------

    #[test]
    fn crc16_xmodem_known_vector() {
        // CRC-16/XMODEM（poly 0x1021，初值 0）("123456789") = 0x31C3
        assert_eq!(crc16_xmodem(b"123456789"), 0x31C3);
    }

    #[test]
    fn crc32_iso_hdlc_known_vector() {
        // CRC-32 ("123456789") = 0xCBF43926
        assert_eq!(crc32_iso_hdlc(b"123456789"), 0xCBF4_3926);
    }

    // ---------- ZDLE 转义 roundtrip ----------

    #[test]
    fn zdle_escape_roundtrip_all_special_bytes() {
        // 全部需要转义的字节 + 普通字节混合
        let data: Vec<u8> = (0..=0xff).collect();
        let mut escaped = Vec::new();
        zdle_escape(&mut escaped, &data);
        // 模拟接收侧解转义：扫描 ZDLE 序列还原
        let mut unescaped = Vec::new();
        let mut i = 0;
        while i < escaped.len() {
            let b = escaped[i];
            i += 1;
            if b == ZDLE {
                assert!(i < escaped.len(), "ZDLE 后必须有转义字节");
                unescaped.push(zdle_unescape(escaped[i]));
                i += 1;
            } else {
                unescaped.push(b);
            }
        }
        assert_eq!(unescaped, data);
    }

    // ---------- 发送器 → 接收器 全链路对局 ----------

    /// 测试虚拟链路的驱动结果
    #[derive(Default)]
    struct RxLog {
        file_started: Option<(Vec<u8>, u32)>,
        file_completed: bool,
        session_completed: bool,
        aborted: bool,
        received: Vec<u8>,
    }

    /// 双向驱动：sender outgoing → receiver 输入，receiver wire/file 输出由
    /// 本函数接管（直接落进 received 缓冲），循环至双方 Idle 收敛。
    /// submit_wire 契约为「返回消费数、剩余由调用方决定重喂」——与集成层
    /// rzsz.rs 的 leftover 机制一致，pump 同样保留未消费残余优先重喂。
    fn pump(sender: &mut Sender, receiver: &mut Receiver, log: &mut RxLog) {
        let mut tx_leftover: Vec<u8> = Vec::new();
        let mut rx_leftover: Vec<u8> = Vec::new();
        for _ in 0..1000 {
            let mut progress = false;

            // sender 动作
            loop {
                match sender.poll() {
                    Action::WriteWire(bytes) => {
                        if !tx_leftover.is_empty() {
                            // leftover 非空时新线路字节先不喂：等末尾重喂按序
                            // 消费完，否则新字节排在 leftover 前面导致乱序
                            break;
                        }
                        let n = bytes.len();
                        let consumed = receiver.submit_wire(bytes).expect("rx submit_wire");
                        if consumed < n {
                            tx_leftover.extend_from_slice(&bytes[consumed..]);
                        }
                        sender.wire_written(n);
                        if consumed > 0 {
                            progress = true;
                        }
                        if consumed == 0 {
                            break;
                        }
                    }
                    Action::ReadFile { offset, max_len } => {
                        // 测试数据：确定性伪随机填充
                        let data: Vec<u8> = (0..max_len)
                            .map(|i| {
                                let v = 0xA5u8.wrapping_add(i as u8).wrapping_mul(31);
                                v ^ (offset.get() as u8)
                            })
                            .collect();
                        sender.submit_file(&data).expect("submit_file");
                    }
                    Action::Idle => break,
                    _ => {}
                }
            }

            // receiver 动作
            loop {
                match receiver.poll() {
                    Action::WriteWire(bytes) => {
                        let n = bytes.len();
                        let consumed = sender.submit_wire(bytes).expect("tx submit_wire");
                        if consumed < n {
                            rx_leftover.extend_from_slice(&bytes[consumed..]);
                        }
                        receiver.wire_written(n);
                        if consumed > 0 {
                            progress = true;
                        }
                        if consumed == 0 {
                            break;
                        }
                    }
                    Action::WriteFile(data) => {
                        let n = data.len();
                        let chunk = data.to_vec();
                        receiver.file_written(n).expect("file_written");
                        log.received.extend_from_slice(&chunk);
                        progress = true;
                    }
                    // 事件必须在动作循环内消费：poll 对 fsp/pending_event 是
                    // take 语义，若留到后面的事件循环，动作循环里的一次 poll
                    // 就已经把 FileStarted 消费掉了，信息丢失
                    Action::Event(Event::FileStarted(info)) => {
                        log.file_started =
                            Some((info.name.to_vec(), info.size.map(|p| p.get()).unwrap_or(0)));
                        progress = true;
                    }
                    Action::Event(Event::FileCompleted) => {
                        log.file_completed = true;
                        progress = true;
                    }
                    Action::Event(Event::SessionCompleted) => {
                        log.session_completed = true;
                    }
                    Action::Event(Event::Aborted) => {
                        log.aborted = true;
                    }
                    // receiver 不产生 ReadFile（读文件是 sender 侧动作），
                    // 为穷尽 match 补分支
                    Action::ReadFile { .. } => {}
                    Action::Idle => break,
                }
            }

            // receiver 事件
            loop {
                match receiver.poll() {
                    Action::Event(Event::FileStarted(info)) => {
                        log.file_started = Some((info.name.to_vec(), info.size.unwrap().get()));
                        progress = true;
                        continue;
                    }
                    Action::Event(Event::FileCompleted) => {
                        log.file_completed = true;
                        progress = true;
                        continue;
                    }
                    Action::Event(Event::SessionCompleted) => {
                        log.session_completed = true;
                        continue;
                    }
                    Action::Event(Event::Aborted) => {
                        log.aborted = true;
                        continue;
                    }
                    _ => {}
                }
                break;
            }

            // 未消费残余优先重喂（blocked 解除后 1 字节进展即继续）
            // 未消费残余优先重喂（blocked 解除后 1 字节进展即继续）。
            // drain 而非 clear：blocked 时 submit_wire 消费不完，剩余必须
            // 留待下一轮，clear 会把未消费字节静默丢弃
            if !tx_leftover.is_empty() {
                let consumed = receiver.submit_wire(&tx_leftover).expect("rx submit_wire");
                tx_leftover.drain(..consumed);
                if consumed > 0 {
                    progress = true;
                }
            }
            if !rx_leftover.is_empty() {
                let consumed = sender.submit_wire(&rx_leftover).expect("tx submit_wire");
                rx_leftover.drain(..consumed);
                if consumed > 0 {
                    progress = true;
                }
            }

            if !progress {
                break;
            }
        }
    }

    #[test]
    fn full_transfer_single_file() {
        let mut sender = Sender::new().expect("sender");
        let mut receiver = Receiver::with_flow_control(0, true).expect("receiver");
        let mut log = RxLog::default();

        sender
            .start_file(FileInfo::new(b"test.txt", Some(Position::new(4096))))
            .expect("start_file");
        sender.finish().expect("finish");
        pump(&mut sender, &mut receiver, &mut log);

        assert_eq!(
            log.file_started.as_ref().map(|(n, _)| n.as_slice()),
            Some(&b"test.txt"[..])
        );
        assert_eq!(log.file_started.as_ref().map(|(_, s)| *s), Some(4096));
        assert_eq!(log.received.len(), 4096, "应完整收到 4096 字节");
        assert!(log.file_completed, "应收到文件完成事件");
        assert!(log.session_completed, "应收到会话完成事件");
        assert!(!log.aborted);
    }

    #[test]
    fn full_transfer_empty_file() {
        let mut sender = Sender::new().expect("sender");
        let mut receiver = Receiver::with_flow_control(0, true).expect("receiver");
        let mut log = RxLog::default();

        sender
            .start_file(FileInfo::new(b"empty.bin", Some(Position::new(0))))
            .expect("start_file");
        sender.finish().expect("finish");
        pump(&mut sender, &mut receiver, &mut log);

        assert!(log.file_completed, "空文件应正常完成");
        assert!(log.session_completed, "空文件后会话应正常结束");
        assert_eq!(log.received.len(), 0);
    }

    #[test]
    fn full_transfer_large_multi_subpacket() {
        // 3.5 个子包的传输：覆盖 ZCRCG 连续帧与 ZCRCW 块尾交替
        let size = SUBPACKET_MAX_SIZE * 3 + 512;
        let mut sender = Sender::new().expect("sender");
        let mut receiver = Receiver::with_flow_control(0, true).expect("receiver");
        let mut log = RxLog::default();

        sender
            .start_file(FileInfo::new(b"big.dat", Some(Position::new(size as u32))))
            .expect("start_file");
        sender.finish().expect("finish");
        pump(&mut sender, &mut receiver, &mut log);

        assert_eq!(log.received.len(), size, "大文件应完整接收");
        assert!(log.session_completed);
    }
}


