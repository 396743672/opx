<template>
  <Teleport to="body">
    <div class="overlay" @click.self="$emit('close')">
      <div class="dialog">
        <div class="dialog-head">
          <h2>{{ $t('groupConfig') }}</h2>
          <button class="dialog-close" @click="$emit('close')">
            <Icon icon="mdi:close" />
          </button>
        </div>
        <div class="dialog-body">
          <div v-for="g in localGroups" :key="g.id" class="group-row">
            <Icon icon="mdi:drag" class="drag-icon" />
            <input v-model="g.name" class="input flex-1" :placeholder="$t('group')" />
            <input v-model.number="g.order" type="number" class="input order-input" min="0" />
            <button class="btn icon-btn" @click="removeGroup(g.id)">
              <Icon icon="mdi:delete" />
            </button>
          </div>
          <button class="btn add-btn" @click="addGroup">
            <Icon icon="mdi:plus" /> {{ $t('addGroup') }}
          </button>
        </div>
        <div class="dialog-footer">
          <button class="btn" @click="$emit('close')">{{ $t('cancel') }}</button>
          <button class="btn primary" @click="save">{{ $t('save') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
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

<style scoped>
.overlay {
  position: fixed; inset: 0; z-index: 50;
  display: flex; align-items: center; justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
  animation: fade 0.15s ease-out;
}
@keyframes fade { from { opacity: 0; } to { opacity: 1; } }
.dialog {
  width: 460px; max-height: 80vh; border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card); box-shadow: var(--shadow-popover);
  display: flex; flex-direction: column; overflow: hidden;
}
.dialog-head {
  display: flex; align-items: center; justify-content: space-between;
  padding: 16px 20px; border-bottom: 1px solid var(--color-border);
}
.dialog-head h2 { font-size: 15px; font-weight: 600; margin: 0; }
.dialog-close {
  width: 28px; height: 28px; border: none; background: transparent;
  color: var(--color-muted-foreground); border-radius: 4px; cursor: pointer;
  display: flex; align-items: center; justify-content: center;
}
.dialog-close:hover { background: var(--color-muted); color: var(--color-foreground); }
.dialog-body {
  padding: 14px 20px; overflow-y: auto; flex: 1;
  display: flex; flex-direction: column; gap: 8px;
}
.dialog-footer {
  display: flex; justify-content: flex-end; gap: 8px;
  padding: 12px 20px; border-top: 1px solid var(--color-border);
}
.group-row {
  display: flex; align-items: center; gap: 8px; padding: 8px 10px;
  border-radius: 6px; border: 1px solid var(--color-border);
}
.drag-icon { color: var(--color-muted-foreground); cursor: move; font-size: 18px; }
.input {
  height: 34px; padding: 0 10px; background: var(--color-muted);
  border: 1px solid transparent; border-radius: 6px; color: var(--color-foreground);
  font-size: 13px; outline: none; box-sizing: border-box;
}
.input:focus { border-color: var(--color-primary); background: var(--color-card); }
.flex-1 { flex: 1; }
.order-input { width: 64px; }
.btn {
  display: inline-flex; align-items: center; gap: 6px; height: 32px;
  padding: 0 12px; border-radius: 6px; border: 1px solid var(--color-border);
  background: var(--color-card); color: var(--color-foreground); font-size: 13px; cursor: pointer;
}
.btn:hover { background: var(--color-muted); }
.btn.primary { background: var(--color-primary); color: var(--color-primary-foreground); border-color: var(--color-primary); }
.icon-btn { width: 32px; padding: 0; justify-content: center; flex-shrink: 0; }
.add-btn { width: 100%; justify-content: center; border-style: dashed; color: var(--color-muted-foreground); font-size: 12px; }
.add-btn:hover { color: var(--color-foreground); border-color: var(--color-primary); }
</style>
