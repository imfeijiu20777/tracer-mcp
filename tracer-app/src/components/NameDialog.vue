<script setup lang="ts">
// ② Name 设置弹框
const show = defineModel<boolean>({ required: true });
const name = defineModel<string>("name", { required: true });
defineEmits<{ (e: "submit"): void }>();
</script>

<template>
  <q-dialog v-model="show" persistent>
    <q-card class="wf-dialog">
      <q-card-section>
        <div class="wf-dialog-title">Set Name</div>
        <div class="wf-dialog-sub">Enter your name and send it to the server</div>
      </q-card-section>
      <q-card-section>
        <q-input
          v-model="name"
          filled
          dense
          dark
          color="white"
          bg-color="black"
          label="name"
          class="wf-input"
        />
      </q-card-section>
      <q-card-actions align="right">
        <q-btn
          label="Send"
          color="black"
          text-color="white"
          unelevated
          @click="$emit('submit')"
        />
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>
