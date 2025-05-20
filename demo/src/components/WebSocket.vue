<template>
  <div class="p-4">
    <h2 class="text-xl font-bold">WebSocket Client</h2>
    <div class="mt-2">
      <input v-model="message" placeholder="Type message" class="border p-2 mr-2" />
      <button @click="sendMessage" class="bg-blue-500 text-white px-4 py-2 rounded">Send</button>
    </div>
    <div class="mt-4">
      <h3 class="font-semibold">Messages:</h3>
      <ul>
        <li v-for="(msg, index) in messages" :key="index">{{ msg }}</li>
      </ul>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'

const socketUrl = 'ws://127.0.0.1:8080' // Adjust as needed

const ws = ref<WebSocket | null>(null)
const messages = ref<string[]>([])
const message = ref<string>('')

onMounted(() => {
  ws.value = new WebSocket(socketUrl)

  ws.value.onopen = () => {
    console.log('WebSocket connected')
    messages.value.push('[Connected]')
  }

  ws.value.onmessage = (event: MessageEvent) => {
    console.log('Received:', event.data)
    messages.value.push(`Server: ${event.data}`)
  }

  ws.value.onerror = (error: Event) => {
    console.error('WebSocket error', error)
  }

  ws.value.onclose = () => {
    messages.value.push('[Disconnected]')
  }
})

onUnmounted(() => {
  if (ws.value && ws.value.readyState === WebSocket.OPEN) {
    ws.value.close()
  }
})

function sendMessage(): void {
  if (ws.value && ws.value.readyState === WebSocket.OPEN) {
    ws.value.send(message.value)
    messages.value.push(`You: ${message.value}`)
    message.value = ''
  } else {
    alert('WebSocket not connected')
  }
}
</script>

<style scoped>
input {
  width: 300px;
}
</style>
