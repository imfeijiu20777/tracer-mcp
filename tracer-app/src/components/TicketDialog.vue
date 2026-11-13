<script setup lang="ts">
// ① 服务端公钥接入弹框
const show = defineModel<boolean>({ required: true });
const serverKey = defineModel<string>("serverKey", { required: true });
defineProps<{ loading: boolean }>();
defineEmits<{ (e: "submit"): void }>();
</script>

<template>
  <q-dialog v-model="show" persistent>
    <q-card class="wf-dialog">
      <q-card-section>
        <div class="wf-dialog-title">Connect with Server Key</div>
        <div class="wf-dialog-sub">Paste the server node public key to establish a connection</div>
      </q-card-section>
      <q-card-section>
        <q-input
          v-model="serverKey"
          filled
          bg-color="grey-4"
          label="server public key"
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
