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
          <div v-for="g in localGroups" :key="g.id" class="group-card">
            <div class="group-row">
              <Icon icon="mdi:drag" class="drag-icon" />
              <input v-model="g.name" class="input flex-1" :placeholder="$t('group')" />
              <input v-model.number="g.order" type="number" class="input order-input" min="0" />
              <button class="btn icon-btn" :class="{ active: g._showEnv }" @click="g._showEnv = !g._showEnv" :title="$t('envVars')">
                <Icon icon="mdi:code-braces" />
              </button>
              <button class="btn icon-btn" @click="removeGroup(g.id)">
                <Icon icon="mdi:delete" />
              </button>
            </div>
            <div v-if="g._showEnv" class="env-section">
              <div v-for="(env, i) in g.env_vars" :key="i" class="env-row">
                <input class="input input-mono env-key" v-model="env[0]" placeholder="KEY" />
                <input class="input input-mono env-val" v-model="env[1]" placeholder="VALUE" />
                <button class="btn icon-btn" @click="g.env_vars.splice(i, 1)">
                  <Icon icon="mdi:close" />
                </button>
              </div>
              <button class="btn add-env-btn" @click="g.env_vars.push(['', ''])">
                <Icon icon="mdi:plus" /> {{ $t('addVariable') }}
              </button>
            </div>
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
const localGroups = ref<(AppGroup & { _showEnv?: boolean })[]>(
  props.groups.map(g => ({ ...g, env_vars: (g.env_vars || []).map(e => [...e] as [string, string]) }))
)

function addGroup() {
  localGroups.value.push({
    id: crypto.randomUUID(),
    name: '',
    order: localGroups.value.length,
    depends_on: [],
    env_vars: [],
  })
}

function removeGroup(id: string) {
  localGroups.value = localGroups.value.filter(g => g.id !== id)
}

async function save() {
  const clean = localGroups.value.map(({ _showEnv, ...g }) => g)
  await store.saveGroups(clean)
  emit('close')
}
</script>

<style scoped>
.overlay {
  position: fixed; inset: 0; z-index: 50;
  display: flex; align-items: center; justify-content: center;
  background: oklch(0 0 0 / 0.5); backdrop-filter: blur(4px);
  animation: fade 0.15s ease-out;
}
@keyframes fade { from { opacity: 0; } to { opacity: 1; } }
.dialog {
  width: 520px; max-height: 80vh; border-radius: 10px;
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
.group-card {
  border: 1px solid var(--color-border); border-radius: 6px; overflow: hidden;
}
.group-row {
  display: flex; align-items: center; gap: 8px; padding: 8px 10px;
}
.env-section {
  padding: 8px 10px 10px 42px; border-top: 1px solid var(--color-border);
  display: flex; flex-direction: column; gap: 6px;
}
.env-row { display: flex; gap: 6px; align-items: center; }
.env-key { flex: 2; min-width: 0; }
.env-val { flex: 3; min-width: 0; }
.drag-icon { color: var(--color-muted-foreground); cursor: move; font-size: 18px; }
.input {
  height: 34px; padding: 0 10px; background: var(--color-muted);
  border: 1px solid transparent; border-radius: 6px; color: var(--color-foreground);
  font-size: 13px; outline: none; box-sizing: border-box;
}
.input:focus { border-color: var(--color-primary); background: var(--color-card); }
.flex-1 { flex: 1; }
.order-input { width: 64px; }
.input-mono { font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace; font-size: 12px; }
.btn {
  display: inline-flex; align-items: center; gap: 6px; height: 32px;
  padding: 0 12px; border-radius: 6px; border: 1px solid var(--color-border);
  background: var(--color-card); color: var(--color-foreground); font-size: 13px; cursor: pointer;
}
.btn:hover { background: var(--color-muted); }
.btn.active { background: var(--color-primary); color: var(--color-primary-foreground); border-color: var(--color-primary); }
.btn.primary { background: var(--color-primary); color: var(--color-primary-foreground); border-color: var(--color-primary); }
.icon-btn { width: 32px; padding: 0; justify-content: center; flex-shrink: 0; }
.add-btn { width: 100%; justify-content: center; border-style: dashed; color: var(--color-muted-foreground); font-size: 12px; }
.add-btn:hover { color: var(--color-foreground); border-color: var(--color-primary); }
.add-env-btn { width: 100%; justify-content: center; border-style: dashed; color: var(--color-muted-foreground); font-size: 11px; height: 28px; }
.add-env-btn:hover { color: var(--color-foreground); border-color: var(--color-primary); }
</style>
