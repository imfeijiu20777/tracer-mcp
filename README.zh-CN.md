# Tracer Tool

[English](README.md) | [中文](README.zh-CN.md)

基于 [iroh](https://github.com/n0-computer/iroh) 的 P2P 文件分发工具。
AI 助手通过 MCP 工具向桌面客户端推送文件通知，客户端再通过 P2P 直连（含中继兜底）从服务端拉取文件，并实时展示下载进度。

## 工作原理

```
┌─────────────┐   MCP (stdio)   ┌──────────────┐   file_notice (QUIC)   ┌────────────┐
│  AI 助手     │ ──────────────▶ │  tracer-mcp  │ ─────────────────────▶ │ tracer-app │
│ (MCP 宿主)   │                 │  (服务端)     │                        │  (客户端)   │
└─────────────┘                 └──────────────┘ ◀───────────────────── └────────────┘
                                       │            blob 下载 (iroh-blobs, P2P/中继)
```

1. `tracer-mcp` 以 stdio 方式作为 MCP 服务运行，维护一个 iroh endpoint 和 blob 存储。
2. `tracer-app`（Tauri 桌面客户端）使用 endpoint ticket 连接服务端，并注册用户名。
3. AI 调用 `send_file` 工具 → 服务端把文件导入 blob 存储，并向目标客户端推送 `file_notice`。
4. 客户端通过系统保存对话框选择保存位置，开始下载并实时展示进度。

## 界面截图

<img src="docs/screenshots/client-ticket.png" width="320" alt="tracer-app 接入 ticket 弹框">

## Workspace 结构

| Crate | 说明 |
| --- | --- |
| `tracer-common` | 共享协议：帧化双向流消息、`Payload` 消息格式、路径/密钥解析 |
| `tracer-mcp` | MCP 服务端（stdio，rmcp）：连接管理、blob provider、`send_file` / `get_ticker` / `user_all` 工具 |
| `tracer-app` | Tauri 2 桌面客户端（Vue 3 + Quasar）：ticket 连接、通知列表、带进度的 P2P 下载 |

## MCP 工具

| 工具 | 说明 |
| --- | --- |
| `get_ticker` | 获取服务端连接凭证（endpoint ticket） |
| `user_all` | 列出当前所有已注册用户 |
| `send_file` | 向指定用户发送文件（参数：`name`、`file_path`） |

## 快速开始

### 环境要求

- Rust（stable）
- Node.js + pnpm

### 服务端（tracer-mcp）

```bash
cargo run -p tracer-mcp
```

命令行参数：

```
-d, --data <PATH>    blobs 数据目录
                     （缺省回退：环境变量 TRACET_SERVER_DATA_PATH，其次 cwd/tracer_server_data）
-s, --secret <PATH>  私钥文件路径
                     （缺省回退：环境变量 TRACET_SERVER_SECRET_KEY，其次 cwd/tracer_server_secret_key.key）
-h, --help           打印帮助
```

在 MCP 宿主（如 Claude Desktop）中配置：

```json
{
  "mcpServers": {
    "tracer": {
      "command": "tracer-mcp"
    }
  }
}
```

> 日志输出到 stderr（stdout 保留给 JSON-RPC 协议）。用 `RUST_LOG` 控制日志级别。

### 客户端（tracer-app）

```bash
cd tracer-app
pnpm install
pnpm tauri dev      # 开发模式
pnpm tauri build    # 打包产物
```

启动后会先要求输入 **ticket**（可通过 AI 调用 `get_ticker` 获取，或向服务端运维索取），然后输入**用户名**。连接成功后，收到的文件通知会出现在列表中——点击卡片确认、选择保存位置即开始下载。

## 配置项

| 环境变量 | 角色 | 说明 |
| --- | --- | --- |
| `TRACET_SERVER_DATA_PATH` | 服务端 | blob 存储目录 |
| `TRACET_SERVER_SECRET_KEY` | 服务端 | 私钥文件（保持稳定，否则 ticket 的 node id 会变化） |
| `TRACET_CLIENT_DATA_PATH` | 客户端 | 客户端 blob 缓存目录 |
| `RUST_LOG` | 通用 | 日志级别过滤（默认 `info`） |

## 技术栈

- **网络 / 存储**：iroh 1.4（QUIC P2P、中继兜底、路径迁移）、iroh-blobs 0.103（内容寻址 blob 传输）、iroh-tickets
- **服务端**：rmcp（Model Context Protocol，stdio 传输）、clap
- **客户端**：Tauri 2、Vue 3、Quasar、Vite、TypeScript
- **序列化**：postcard（长度前缀帧消息）
