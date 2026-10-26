use std::fmt;

#[derive(Debug, Clone, serde::Serialize)]
pub enum Payload {
    Message(String),
    File { filename: String, key: String },
    Unknown(String),
    Name(String),
    // 文件通知 ,size:字节
    FileNotice{filename:String,ticker:String,size:u64}
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
            Payload::FileNotice { filename, ticker ,size}=>write!(f,"file_notice|{}|{}|{}",filename,ticker,size)
        }
    }
}