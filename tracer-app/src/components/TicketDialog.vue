<script setup lang="ts">
// ① Ticket 接入弹框
const show = defineModel<boolean>({ required: true });
const ticket = defineModel<string>("ticket", { required: true });
defineProps<{ loading: boolean }>();
defineEmits<{ (e: "submit"): void }>();
</script>

<template>
  <q-dialog v-model="show" persistent>
    <q-card class="wf-dialog">
      <q-card-section>
        <div class="wf-dialog-title">Connect with Ticket</div>
        <div class="wf-dialog-sub">Paste the server ticket to establish a connection</div>
      </q-card-section>
      <q-card-section>
        <q-input
          v-model="ticket"
          filled
          bg-color="grey-4"
          label="ticket"
          type="textarea"
          class="wf-input wf-ticket-input"
        />
      </q-card-section>
      <q-card-actions align="right">
        <q-btn
          label="Connect"
          color="black"
          text-color="white"
          unelevated
          :loading="loading"
          @click="$emit('submit')"
        />
      </q-card-actions>
    </q-card>
  </q-dialog>
</template>
