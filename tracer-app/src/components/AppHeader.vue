<script setup lang="ts">
// 顶部工具栏：标题 + 连接状态点 + 徽标（通知数 / 姓名 / 版本号）
defineProps<{
  count: number;
  name: string;
  version: string;
  connected: boolean;
  connTypeLabel: string;
}>();
</script>

<template>
  <q-header class="wf-header" bordered>
    <q-toolbar>
      <q-toolbar-title class="wf-title"> TRACER </q-toolbar-title>
      <div class="wf-conn">
        <span
          class="wf-dot"
          :class="connected ? 'wf-dot--on' : 'wf-dot--off'"
        ></span>
        <span v-if="connected" class="wf-conn-text">{{ connTypeLabel }}</span>
      </div>
      <div class="wf-badges">
        <q-badge outline color="black" class="wf-badge">{{ count }}</q-badge>
        <q-badge outline color="black" class="wf-badge">
          {{ name || "Unnamed" }}
        </q-badge>
        <q-badge outline color="black" class="wf-badge"> v{{ version }} </q-badge>
      </div>
    </q-toolbar>
  </q-header>
</template>
