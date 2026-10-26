<script setup lang="ts">
// ③ 文件详情 / 下载确认弹框
import type { FileNotice } from "../types";
import { toMB } from "../utils/format";

const show = defineModel<boolean>({ required: true });
defineProps<{
  selected: FileNotice | null;
  downloading: boolean;
  loading: boolean;
}>();
defineEmits<{ (e: "download"): void }>();
</script>

<template>
  <q-dialog v-model="show">
    <q-card class="wf-dialog">
      <q-card-section>
        <div class="wf-dialog-title">File Details</div>
        <div class="wf-dialog-sub">Download this file?</div>
      </q-card-section>
      <q-card-section v-if="selected">
        <div class="wf-detail-row">
          <span class="wf-detail-key">Name</span>
          <span class="wf-detail-val">{{ selected.filename }}</span>
        </div>
        <div class="wf-detail-row">
          <span class="wf-detail-key">Size</span>
          <span class="wf-detail-val">{{ toMB(selected.size) }}</span>
        </div>
      </q-card-section>
      <q-card-actions align="right">
        <q-btn v-close-popup flat label="Cancel" color="black" />
        <q-btn
          label="Download"
          color="black"
          text-color="white"
          unelevated
          :loading="loading"
          :disable="!selected || downloading"
          @click="$emit('download')"
        />
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>
