<script setup lang="ts">
// 文件通知列表：单列卡片，点击打开详情；含空态
import type { FileNotice } from "../types";
import { formatSize } from "../utils/format";

defineProps<{ notices: FileNotice[]; downloadingTickers: string[] }>();
defineEmits<{ (e: "open", item: FileNotice): void }>();
</script>

<template>
  <!-- 空态 -->
  <div v-if="notices.length === 0" class="wf-empty flex flex-center column">
    <q-icon name="description" size="48px" class="wf-empty-icon" />
    <div class="wf-empty-text">No file notices yet</div>
    <div class="wf-empty-sub">Waiting for server notifications ...</div>
  </div>

  <!-- 单列卡片列表 -->
  <q-list v-else separator class="wf-list">
    <q-item
      v-for="(item, idx) in notices"
      :key="idx"
      clickable
      class="wf-card"
      @click="$emit('open', item)"
    >
      <q-item-section avatar>
        <q-icon name="insert_drive_file" size="28px" color="black" />
      </q-item-section>
      <q-item-section>
        <q-item-label class="wf-card-title">{{ item.filename }}</q-item-label>
        <q-item-label caption class="wf-card-sub">
          {{ formatSize(item.size) }}
        </q-item-label>
      </q-item-section>
      <q-item-section side>
        <q-badge
          v-if="downloadingTickers.includes(item.ticker)"
          outline
          color="black"
        >
          Downloading
        </q-badge>
        <q-icon v-else name="chevron_right" color="black" />
      </q-item-section>
    </q-item>
  </q-list>
</template>
