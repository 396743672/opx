<template>
  <div class="dialog-overlay" @click.self="$emit('close')">
    <div class="dialog-panel max-w-md">
      <div class="dialog-header">
        <h2>{{ $t('groupConfig') }}</h2>
      </div>
      <div class="dialog-body space-y-2">
        <div v-for="g in localGroups" :key="g.id" class="flex items-center gap-2 p-2 rounded border border-border">
          <Icon icon="mdi:drag" class="text-muted-foreground cursor-move" />
          <input v-model="g.name" class="input flex-1" :placeholder="$t('group')" />
          <input v-model.number="g.order" type="number" class="input w-16" min="0" />
          <button class="btn text-red-500 p-1" @click="removeGroup(g.id)">
            <Icon icon="mdi:delete" />
          </button>
        </div>
        <button class="btn text-sm w-full" @click="addGroup">
          <Icon icon="mdi:plus" /> {{ $t('addGroup') }}
        </button>
      </div>
      <div class="dialog-footer">
        <button class="btn" @click="$emit('close')">{{ $t('cancel') }}</button>
        <button class="btn primary" @click="save">{{ $t('save') }}</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Icon } from '@iconify/vue'
import type { AppGroup } from '@/models/springboot'
import { useSpringBootStore } from '../stores/springboot'

const props = defineProps<{
  groups: AppGroup[]
}>()

const emit = defineEmits<{ close: [] }>()

const store = useSpringBootStore()
const localGroups = ref<AppGroup[]>(props.groups.map(g => ({ ...g })))

function addGroup() {
  localGroups.value.push({
    id: crypto.randomUUID(),
    name: '',
    order: localGroups.value.length,
    depends_on: [],
  })
}

function removeGroup(id: string) {
  localGroups.value = localGroups.value.filter(g => g.id !== id)
}

async function save() {
  await store.saveGroups(localGroups.value)
  emit('close')
}
</script>
