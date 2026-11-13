use std::fmt;

#[derive(Debug, Clone, serde::Serialize)]
pub enum Payload {
    Message(String),
    File { filename: String, key: String },
    Unknown(String),
    Name(String),
    // 文件通知 ,size:字节
    FileNotice{filename:String,ticker:String,size:u64},
    // 笔记投递通知：id 为服务端笔记 id，title 空串代表无标题
    NoteNotice { id: i64, title: String }
}

impl Payload {
    /// TODO 不匹配协议的情况还没有考虑
    pub fn parse(msg: &str) -> Self {
        let parts: Vec<&str> = msg.split('|').collect();

        match parts.first() {
            Some(&"message") => {
                let content = parts[1..].join("|");
                Payload::Message(content)
            }
            Some(&"init") => {
                let content = parts.get(1).copied().unwrap();
                Payload::Name(content.to_string())
            }
            Some(&"file") => {
                let filename = parts.get(1).copied().unwrap_or("");
                let key = parts.get(2).copied().unwrap_or("");
                Payload::File {
                    filename: filename.to_string(),
                    key: key.to_string(),
                }
            }
            Some(&"file_notice") => {
                let filename = parts.get(1).copied().unwrap_or("");
                let ticker = parts.get(2).copied().unwrap_or("");
                let size = parts.get(3).copied().unwrap_or("");
                Payload::FileNotice { filename: filename.to_string(), ticker: ticker.to_string(),size:size.trim().parse::<u64>().unwrap_or(0) }
            }
            Some(&"note_notice") => {
                let id = parts.get(1).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
                // 标题可能含 |，取第 2 段之后全部拼回
                let title = if parts.len() > 2 { parts[2..].join("|") } else { String::new() };
                Payload::NoteNotice { id, title }
            }
            Some(_) => Payload::Unknown(msg.to_string()),
            None => Payload::Unknown(String::new()),
        }
    }
}
impl fmt::Display for Payload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Payload::Message(content) => write!(f, "message|{}", content),
            Payload::File { filename, key } => write!(f, "file|{}|{}", filename, key),
            Payload::Name(name) => write!(f, "init|{}", name),
            Payload::Unknown(raw) => write!(f, "{}", raw),
            Payload::FileNotice { filename, ticker ,size}=>write!(f,"file_notice|{}|{}|{}",filename,ticker,size),
            Payload::NoteNotice { id, title } => write!(f, "note_notice|{}|{}", id, title)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_notice_roundtrip() {
        // 普通标题 / 空标题 / 含 | 标题，Display -> parse 均须还原
        for title in ["hello", "", "a|b|c"] {
            let p = Payload::NoteNotice { id: 42, title: title.to_string() };
            match Payload::parse(&p.to_string()) {
                Payload::NoteNotice { id, title: t } => {
                    assert_eq!(id, 42);
                    assert_eq!(t, title);
                }
                other => panic!("unexpected payload: {other}"),
            }
        }
    }
}