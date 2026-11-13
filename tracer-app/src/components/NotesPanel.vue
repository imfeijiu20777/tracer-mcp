<script setup lang="ts">
// 笔记列表：工具行（条数 / 新增 / 同步）+ 卡片列表；
// 增删改与同步均 emit 给父组件处理，本组件只负责渲染
import type { ZNote } from "../types";
import { formatTime } from "../utils/format";

defineProps<{ notes: ZNote[]; syncing: boolean; connected: boolean }>();
defineEmits<{
  (e: "add"): void;
  (e: "edit", note: ZNote): void;
  (e: "delete", note: ZNote): void;
  (e: "sync"): void;
}>();

// 内容预览：压平空白并截断，避免长文本撑破卡片
function preview(content: string): string {
  const flat = content.replace(/\s+/g, " ").trim();
  return flat.length > 80 ? `${flat.slice(0, 80)}...` : flat;
}
</script>

<template>
  <div class="column">
    <!-- 工具行：条数徽标 + 新增 + 同步（未连接禁用同步） -->
    <div class="row items-center q-gutter-sm q-mb-sm">
      <q-badge outline color="black" class="wf-badge">{{ notes.length }}</q-badge>
      <q-space />
      <q-btn
        label="New Note"
        icon="add"
        color="black"
        text-color="white"
        unelevated
        dense
        @click="$emit('add')"
      />
      <q-btn
        label="Sync"
        icon="sync"
        color="black"
        text-color="white"
        unelevated
        dense
        :disable="!connected"
        :loading="syncing"
        @click="$emit('sync')"
      />
    </div>

    <!-- 空态 -->
    <div v-if="notes.length === 0" class="wf-empty flex flex-center column">
      <q-icon name="sticky_note_2" size="48px" class="wf-empty-icon" />
      <div class="wf-empty-text">No notes yet</div>
      <div class="wf-empty-sub">Create a note or sync from the server</div>
    </div>

    <!-- 单列卡片列表：点击卡片或编辑按钮打开编辑弹框 -->
    <q-list v-else separator class="wf-list">
      <q-item
        v-for="note in notes"
        :key="note.id"
        clickable
        class="wf-card"
        @click="$emit('edit', note)"
      >
        <q-item-section avatar>
          <q-icon name="sticky_note_2" size="28px" color="black" />
        </q-item-section>
        <q-item-section>
          <q-item-label class="wf-card-title">
            {{ note.title || "Untitled" }}
          </q-item-label>
          <q-item-label caption class="wf-card-sub">
            {{ preview(note.content) }}
          </q-item-label>
          <q-item-label caption class="wf-card-sub">
            {{ formatTime(note.updated_at) }}
          </q-item-label>
        </q-item-section>
        <q-item-section side>
          <div class="row q-gutter-xs">
            <q-btn
              flat
              dense
              round
              icon="edit"
              color="black"
              @click.stop="$emit('edit', note)"
            />
            <q-btn
              flat
              dense
              round
              icon="delete"
              color="black"
              @click.stop="$emit('delete', note)"
            />
          </div>
        </q-item-section>
      </q-item>
    </q-list>
  </div>
</template>