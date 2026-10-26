<script setup lang="ts">
// 页内下载进度面板（wireframe 描边进度条）
import type { DownloadStatus, DownloadTask } from "../types";
import { formatSize } from "../utils/format";

defineProps<{ tasks: DownloadTask[] }>();

function progressValue(task: DownloadTask): number {
  if (task.status === "finished") return 1;
  if (task.size <= 0) return 0;
  return Math.min(1, Math.max(0, task.downloaded / task.size));
}

function statusText(status: DownloadStatus): string {
  if (status === "downloading") return "Downloading";
  if (status === "finished") return "Finished";
  return "Failed";
}
</script>

<template>
  <q-list v-if="tasks.length" separator class="wf-list wf-downloads">
    <q-item v-for="task in tasks" :key="task.id" class="wf-card">
      <q-item-section>
        <q-item-label class="wf-card-title">{{ task.filename }}</q-item-label>
        <div class="wf-progress">
          <div
            class="wf-progress-bar"
            :style="{ width: Math.round(progressValue(task) * 100) + '%' }"
          ></div>
        </div>
        <q-item-label caption class="wf-card-sub">
          {{ statusText(task.status) }} ·
          {{ formatSize(task.downloaded) }} / {{ formatSize(task.size) }}
        </q-item-label>
      </q-item-section>
      <q-item-section side>
        <q-badge outline color="black" class="wf-badge">
          {{ Math.round(progressValue(task) * 100) }}%
        </q-badge>
      </q-item-section>
    </q-item>
  </q-list>
</template>
