<script setup lang="ts">
// 笔记新增 / 编辑共用弹框：editing 切换标题；
// 标题可选，内容必填（为空禁用保存）
const show = defineModel<boolean>({ required: true });
const title = defineModel<string>("title", { required: true });
const content = defineModel<string>("content", { required: true });
defineProps<{ editing: boolean }>();
defineEmits<{ (e: "submit"): void }>();
</script>

<template>
  <q-dialog v-model="show" persistent>
    <q-card class="wf-dialog">
      <q-card-section>
        <div class="wf-dialog-title">{{ editing ? "Edit Note" : "New Note" }}</div>
        <div class="wf-dialog-sub">Saved locally, use Sync to share with the server</div>
      </q-card-section>
      <q-card-section class="column q-gutter-sm">
        <q-input
          v-model="title"
          filled
          dense
          dark
          color="white"
          bg-color="black"
          label="title (optional)"
          class="wf-input"
        />
        <q-input
          v-model="content"
          type="textarea"
          filled
          dense
          dark
          color="white"
          bg-color="black"
          label="content"
          class="wf-input"
          :input-style="{ minHeight: '120px' }"
        />
      </q-card-section>
      <q-card-actions align="right">
        <q-btn
          label="Save"
          color="black"
          text-color="white"
          unelevated
          :disable="!content.trim()"
          @click="$emit('submit')"
        />
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>