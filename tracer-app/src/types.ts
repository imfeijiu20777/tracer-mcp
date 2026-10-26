// 前后端共享的数据结构定义

// ---- 后端返回的数据结构（serde 外部标签序列化）----
// Payload::FileNotice { filename, ticker, size } => { "FileNotice": { ... } }
export interface FileNotice {
  filename: string;
  ticker: string;
  size: number;
}

export type RawPayload =
  | { FileNotice: FileNotice }
  | { Message: [string] }
  | { Name: [string] }
  | { File: { filename: string; key: string } }
  | { Unknown: [string] };

export type DownloadStatus = "downloading" | "finished" | "failed";

export interface DownloadTask {
  id: string;
  filename: string;
  ticker: string;
  size: number;
  downloaded: number;
  status: DownloadStatus;
  error?: string;
}

// ---- 后端下载事件载荷 ----
export interface DownloadProgressPayload {
  task_id: string;
  downloaded: number;
}

export interface DownloadFinishedPayload {
  task_id: string;
  size: number;
}

export interface DownloadFailedPayload {
  task_id: string;
  error: string;
}
