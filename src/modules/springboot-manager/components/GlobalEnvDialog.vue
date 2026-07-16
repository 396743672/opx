<template>
  <Teleport to="body">
    <div class="overlay" @click.self="$emit('close')">
      <div class="dialog">
        <div class="dialog-head">
          <div class="dialog-title">
            <Icon icon="mdi:earth" />
            全局环境变量
          </div>
          <button class="dialog-close" @click="$emit('close')">
            <Icon icon="mdi:close" />
          </button>
        </div>
        <div class="dialog-body">
          <div class="hint">所有 SpringBoot 应用启动时注入，分组和应用级可同名覆盖</div>
          <div v-for="(env, i) in localVars" :key="i" class="env-row">
            <input class="input input-mono env-key" v-model="env[0]" placeholder="KEY" />
            <input class="input input-mono env-val" v-model="env[1]" placeholder="VALUE" />
            <button class="btn icon-btn" @click="remove(i)">
              <Icon icon="mdi:close" />
            </button>
          </div>
          <button class="btn add-btn" @click="add">
            <Icon icon="mdi:plus" /> 添加变量
          </button>
        </div>
        <div class="dialog-footer">
          <button class="btn" @click="$emit('close')">取消</button>
          <button class="btn primary" @click="save" :disabled="saving">
            {{ saving ? '保存中…' : '保存' }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { useSpringBootStore } from '../stores/springboot'

const emit = defineEmits<{ close: [] }>()
const store = useSpringBootStore()

const saving = ref(false)
const localVars = ref<[string, string][]>([])

onMounted(async () => {
  localVars.value = await store.getGlobalEnvVars()
})

function add() { localVars.value.push(['', '']) }
function remove(i: number) { localVars.value.splice(i, 1) }

async function save() {
  saving.value = true
  try {
    await store.setGlobalEnvVars(localVars.value)
    emit('close')
  } catch (e) {
    window.alert(String(e))
  } finally {
    saving.value = false
  }
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
  display: flex; flex-direction: column;
}
.dialog-head {
  display: flex; align-items: center; justify-content: space-between;
  padding: 16px 20px; border-bottom: 1px solid var(--color-border);
}
.dialog-title { font-size: 15px; font-weight: 600; display: flex; align-items: center; gap: 8px; }
.dialog-close {
  width: 28px; height: 28px; border: none; background: transparent;
  color: var(--color-muted-foreground); border-radius: 4px; cursor: pointer;
  display: flex; align-items: center; justify-content: center;
}
.dialog-close:hover { background: var(--color-muted); color: var(--color-foreground); }
.dialog-body { padding: 16px 20px; overflow-y: auto; flex: 1; }
.dialog-footer {
  display: flex; justify-content: flex-end; gap: 8px;
  padding: 12px 20px; border-top: 1px solid var(--color-border);
}
.hint { font-size: 12px; color: var(--color-muted-foreground); margin-bottom: 12px; }
.env-row { display: flex; gap: 6px; align-items: center; margin-bottom: 6px; }
.env-key { flex: 2; min-width: 0; }
.env-val { flex: 3; min-width: 0; }
.input {
  height: 34px; padding: 0 10px; background: var(--color-muted);
  border: 1px solid transparent; border-radius: 6px; color: var(--color-foreground);
  font-size: 13px; outline: none; box-sizing: border-box;
}
.input:focus { border-color: var(--color-primary); }
.input-mono { font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace; font-size: 12px; }
.btn {
  display: inline-flex; align-items: center; gap: 6px; height: 32px;
  padding: 0 12px; border-radius: 6px; border: 1px solid var(--color-border);
  background: var(--color-card); color: var(--color-foreground); font-size: 13px; cursor: pointer;
}
.btn:hover { background: var(--color-muted); }
.btn.primary { background: var(--color-primary); color: var(--color-primary-foreground); border-color: var(--color-primary); }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.icon-btn { width: 32px; padding: 0; justify-content: center; flex-shrink: 0; }
.add-btn { width: 100%; justify-content: center; border-style: dashed; color: var(--color-muted-foreground); font-size: 12px; margin-top: 4px; }
.add-btn:hover { color: var(--color-foreground); border-color: var(--color-primary); }
</style>
