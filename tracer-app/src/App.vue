<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { useQuasar } from "quasar";

import type {
  DownloadFailedPayload,
  DownloadFinishedPayload,
  DownloadProgressPayload,
  DownloadTask,
  FileNotice,
  RawPayload,
} from "./types";
import { formatSize } from "./utils/format";
import AppHeader from "./components/AppHeader.vue";
import DownloadTaskList from "./components/DownloadTaskList.vue";
import NoticeList from "./components/NoticeList.vue";
import TicketDialog from "./components/TicketDialog.vue";
import NameDialog from "./components/NameDialog.vue";
import DetailDialog from "./components/DetailDialog.vue";

const $q = useQuasar();

// ---- 页面状态 ----
const ticketDialog = ref(false);
const nameDialog = ref(false);
const detailDialog = ref(false);

const ticketInput = ref("");
const nameInput = ref("");
const connecting = ref(false);
// 已注册的姓名（set_name 成功后记录）与应用版本号，展示在头部
const registeredName = ref("");
const appVersion = ref("");
// 连接状态：未连接显示红点，连接后显示绿点 + Relay/P2P
const connected = ref(false);
const connType = ref<"relay" | "p2p" | "">("");
const connTypeLabel = computed(() => (connType.value === "relay" ? "Relay" : "P2P"));

const fileNotices = ref<FileNotice[]>([]);
const selected = ref<FileNotice | null>(null);
const preparingDownload = ref(false);
const downloadTasks = ref<DownloadTask[]>([]);

// 正在下载中的 ticker 列表，用于通知列表行内「下载中」徽标
const downloadingTickers = computed(() =>
  downloadTasks.value
    .filter((task) => task.status === "downloading")
    .map((task) => task.ticker),
);

const isSelectedDownloading = computed(
  () => !!selected.value && isTickerDownloading(selected.value.ticker),
);

// ---- 事件监听 ----
let unlisten: UnlistenFn | null = null;

async function loadRecords() {
  try {
    const records = await invoke<RawPayload[]>("query_records");
    // 通知按「从新到旧」展示：后端为追加顺序（旧→新），这里反转
    fileNotices.value = records
      .filter((r): r is { FileNotice: FileNotice } => "FileNotice" in r)
      .map((r) => r.FileNotice)
      .reverse();
  } catch (e) {
    console.error("query_records failed", e);
  }
}

onMounted(async () => {
  // 读取应用版本号展示在头部
  try {
    appVersion.value = await getVersion();
  } catch (e) {
    console.error("getVersion failed", e);
  }
  // 进入即弹出 ticket 输入框
  ticketDialog.value = true;
  unlisten = await listen("record-event", (event) => {
    const payload = event.payload as Record<string, unknown>;
    // 连接建立 / 路径切换：更新连接状态与中继·P2P 类型
    if ("Connection" in payload) {
      const data = payload.Connection as { conn_type: "relay" | "p2p" };
      connected.value = true;
      connType.value = data.conn_type;
      return;
    }
    // 连接断开：红点
    if ("DisConnection" in payload) {
      connected.value = false;
      connType.value = "";
      return;
    }
    // 后端在收到新消息时发出 MessageUpdate，此时拉取最新记录
    if ("MessageUpdate" in payload) {
      void loadRecords();
      return;
    }
    if ("DownloadProgress" in payload) {
      const data = payload.DownloadProgress as DownloadProgressPayload;
      updateDownloadProgress(data.task_id, data.downloaded);
      return;
    }
    if ("DownloadFinished" in payload) {
      const data = payload.DownloadFinished as DownloadFinishedPayload;
      finishDownloadTask(data.task_id, data.size);
      return;
    }
    if ("DownloadFailed" in payload) {
      const data = payload.DownloadFailed as DownloadFailedPayload;
      failDownloadTask(data.task_id, data.error);
    }
  });
});

onBeforeUnmount(() => {
  unlisten?.();
});

// ---- 流程动作 ----
async function submitTicket() {
  if (!ticketInput.value.trim()) return;
  connecting.value = true;
  try {
    await invoke("init_ticket", { ticket: ticketInput.value.trim() });
    ticketDialog.value = false;
    nameDialog.value = true;
  } catch (e) {
    $q.notify({ type: "negative", position: "top", message: String(e) });
  } finally {
    connecting.value = false;
  }
}

async function submitName() {
  if (!nameInput.value.trim()) return;
  try {
    await invoke("set_name", { name: nameInput.value.trim() });
    registeredName.value = nameInput.value.trim();
    nameDialog.value = false;
    await loadRecords();
  } catch (e) {
    $q.notify({ type: "negative", position: "top", message: String(e) });
  }
}

function openDetail(item: FileNotice) {
  selected.value = item;
  detailDialog.value = true;
}

// ---- 下载任务管理 ----
function createDownloadTaskId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `download-${Date.now()}-${Math.random().toString(36).slice(2, 11)}`;
}

function findDownloadTask(taskId: string): DownloadTask | undefined {
  return downloadTasks.value.find((task) => task.id === taskId);
}

function updateDownloadProgress(taskId: string, downloaded: number) {
  const task = findDownloadTask(taskId);
  if (!task || task.status !== "downloading") return;
  const next = Math.max(task.downloaded, downloaded);
  task.downloaded = task.size > 0 ? Math.min(next, task.size) : next;
}

function finishDownloadTask(taskId: string, size: number) {
  const task = findDownloadTask(taskId);
  if (!task || task.status !== "downloading") return;
  task.status = "finished";
  task.size = size > 0 ? size : task.size;
  task.downloaded = task.size;
  task.error = undefined;
  $q.notify({
    type: "positive",
    position: "top",
    timeout: 2500,
    message: `Download complete: ${task.filename}`,
    caption: formatSize(task.size),
  });
  removeDownloadTask(taskId);
}

function failDownloadTask(taskId: string, error: string) {
  const task = findDownloadTask(taskId);
  if (!task || task.status !== "downloading") return;
  task.status = "failed";
  task.error = error;
  $q.notify({
    type: "negative",
    position: "top",
    timeout: 6000,
    message: `Download failed: ${task.filename}`,
    caption: error || "Unknown error",
  });
  removeDownloadTask(taskId);
}

function removeDownloadTask(taskId: string) {
  const index = downloadTasks.value.findIndex((task) => task.id === taskId);
  if (index >= 0) downloadTasks.value.splice(index, 1);
}

function isTickerDownloading(ticker: string): boolean {
  return downloadTasks.value.some(
    (task) => task.ticker === ticker && task.status === "downloading",
  );
}

// 点击卡片后，用户确认「是」即调用后端 download：
// 先弹出系统保存对话框选择保存目录与文件名，把选出的完整路径传给后端。
// 每次下载都会生成一个只使用一次的 task_id，后端按这个 id 回推进度事件。
async function downloadFile(item: FileNotice) {
  if (isTickerDownloading(item.ticker)) {
    $q.notify({
      type: "warning",
      position: "top",
      message: `${item.filename} is already downloading`,
    });
    return;
  }

  preparingDownload.value = true;
  let outPath: string | null = null;
  try {
    outPath = await save({
      title: "Choose save location",
      defaultPath: item.filename,
    });
  } catch (e) {
    $q.notify({
      type: "negative",
      position: "top",
      message: `Failed to choose save location: ${String(e)}`,
    });
    return;
  } finally {
    preparingDownload.value = false;
  }

  if (!outPath) {
    $q.notify({ type: "info", position: "top", message: "Download canceled" });
    return;
  }

  const task: DownloadTask = {
    id: createDownloadTaskId(),
    filename: item.filename,
    ticker: item.ticker,
    size: item.size,
    downloaded: 0,
    status: "downloading",
  };
  downloadTasks.value.unshift(task);
  detailDialog.value = false;

  try {
    await invoke("download", {
      taskId: task.id,
      ticker: item.ticker,
      outFileName: outPath,
    });
    finishDownloadTask(task.id, task.size);
  } catch (e) {
    failDownloadTask(task.id, String(e));
  }
}
</script>

<template>
  <q-layout view="hHh lpR fFf" class="wf-bg">
    <AppHeader
      :count="fileNotices.length"
      :name="registeredName"
      :version="appVersion"
      :connected="connected"
      :conn-type-label="connTypeLabel"
    />

    <q-page-container>
      <q-page class="wf-page column">
        <!-- 下载进度：wireframe 描边风格面板 -->
        <DownloadTaskList :tasks="downloadTasks" />
        <!-- 文件通知列表：从新到旧 -->
        <NoticeList
          :notices="fileNotices"
          :downloading-tickers="downloadingTickers"
          @open="openDetail"
        />
      </q-page>
    </q-page-container>

    <!-- ① Ticket 输入弹框 -->
    <TicketDialog
      v-model="ticketDialog"
      v-model:ticket="ticketInput"
      :loading="connecting"
      @submit="submitTicket"
    />

    <!-- ② Name 输入弹框 -->
    <NameDialog v-model="nameDialog" v-model:name="nameInput" @submit="submitName" />

    <!-- ③ 详情 / 下载确认弹框 -->
    <DetailDialog
      v-model="detailDialog"
      :selected="selected"
      :downloading="isSelectedDownloading"
      :loading="preparingDownload"
      @download="selected && downloadFile(selected)"
    />
  </q-layout>
</template>
