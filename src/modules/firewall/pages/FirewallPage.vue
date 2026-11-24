<template>
  <div class="animate-fade-in">
    <PageHeader
      icon="mdi:shield-lock-outline"
      :title="$t('firewallPorts')"
      :subtitle="$t('firewallDesc')"
    >
      <template #actions>
        <button class="btn ghost" :disabled="loading" @click="load">
          <Icon icon="mdi:refresh" /> {{ $t('refresh') }}
        </button>
        <button class="btn primary" :disabled="!canManage" @click="openAdd">
          <Icon icon="mdi:plus" /> {{ $t('firewallAddPort') }}
        </button>
      </template>
    </PageHeader>

    <!-- 生效状态：和「装没装」是两回事。系统没有拦截规则时，下面的开关不会改变可达性 -->
    <div
      v-if="overview"
      class="flex items-start gap-2.5 rounded-lg border p-3 mb-3"
      :class="enforceClass"
    >
      <Icon :icon="enforcementIcon" class="text-xl flex-shrink-0 mt-0.5" />
      <div class="min-w-0">
        <p class="font-semibold text-sm">{{ $t(`firewallEnforce_${enforcement}`) }}</p>
        <p class="text-xs mt-1 leading-relaxed opacity-90">
          {{ $t(`firewallEnforceDesc_${enforcement}`) }}
        </p>
      </div>
    </div>

    <!-- 后端状态：明确「opx 不改防火墙服务状态」这一边界，避免用户以为能在这里关防火墙 -->
    <div v-if="overview" class="panel">
      <div class="flex items-center gap-2 flex-wrap">
        <Icon :icon="backendIcon" class="text-xl" :class="backendHealthy ? 'text-emerald-500' : 'text-amber-500'" />
        <span class="font-semibold text-sm">{{ backendTitle }}</span>
        <span class="text-[11px] font-mono px-1.5 py-0.5 rounded bg-muted text-muted-foreground">
          {{ overview.backend.kind }}
        </span>
        <!-- 一眼可见「到底有没有在拦」 -->
        <span class="text-[11px] px-1.5 py-0.5 rounded" :class="enforceChipClass">
          {{ $t(`firewallEnforceChip_${enforcement}`) }}
        </span>
        <span class="text-[11px] text-muted-foreground">· {{ $t('firewallNoServiceHint') }}</span>
      </div>
      <p class="text-xs text-muted-foreground mt-2 leading-relaxed">{{ backendNote }}</p>
    </div>

    <!-- 规则读取失败（非 root 等）：必须显式展示，否则「全是未放行」会误导用户 -->
    <div v-if="overview?.error" class="error-bar">
      <Icon icon="mdi:alert-outline" class="flex-shrink-0" />
      <span class="min-w-0 break-all">{{ $t('firewallReadError') }}: {{ overview.error }}</span>
    </div>

    <div v-if="pageError" class="error-bar">
      <Icon icon="mdi:alert-outline" class="flex-shrink-0" />
      <span class="min-w-0 break-all">{{ pageError }}</span>
    </div>

    <div v-if="notice" class="notice-bar">
      <Icon icon="mdi:check-circle" class="flex-shrink-0" />
      <span class="min-w-0">{{ notice }}</span>
    </div>

    <EmptyState
      v-if="!loading && groups.length === 0"
      icon="mdi:shield-lock-outline"
      :title="$t('firewallEmpty')"
      :description="$t('firewallEmptyDesc')"
    />

    <section v-for="g in groups" :key="g.source" class="group">
      <div class="group-head">
        <Icon :icon="g.icon" class="text-base" />
        <span class="text-xs font-semibold uppercase tracking-wide">{{ $t(g.titleKey) }}</span>
        <span class="text-[11px] text-muted-foreground">({{ g.items.length }})</span>
      </div>

      <div class="rows">
        <div v-for="p in g.items" :key="portKey(p)" class="row">
          <div class="port-cell">
            <span class="port-num">{{ p.port }}</span>
            <span class="port-proto">{{ p.proto }}</span>
          </div>

          <div class="name-cell">
            <div class="flex items-center gap-1.5 min-w-0">
              <span class="truncate text-sm font-medium">{{ displayName(p) }}</span>
              <Icon
                v-if="isRiskyPort(p.port)"
                icon="mdi:alert-outline"
                class="text-amber-500 text-sm flex-shrink-0"
                :title="$t('firewallRiskyPort')"
              />
            </div>
            <!-- 来源限定：一眼看出这条规则不是全网放行（从防火墙规则读回，所有端口来源都可能有） -->
            <div v-if="p.sources?.length" class="flex items-center gap-1 flex-wrap mt-0.5">
              <span
                v-for="s in p.sources"
                :key="s"
                class="src-chip"
                :title="$t('firewallSourceOnly', { source: s })"
              >
                <Icon icon="mdi:account-key-outline" />
                {{ s }}
              </span>
            </div>
            <div class="text-[11px] text-muted-foreground truncate">
              {{ p.detail || p.bind || '—' }}
            </div>
          </div>

          <!-- 第二道门：只监听回环时，放行防火墙也白搭 -->
          <div v-if="p.bind_local_only" class="bind-cell">
            <span class="bind-warn">
              <Icon icon="mdi:laptop" />
              {{ bindWarnText(p) }}
            </span>
            <button
              v-if="p.instance_id && p.bind_fixable"
              class="btn ghost tiny"
              :disabled="pending.has(portKey(p))"
              @click="fixBind(p)"
            >
              {{ $t('firewallFixBind') }}
            </button>
            <!-- 监听地址写死在程序或私有配置里：不给假按钮，直接告诉用户去哪改 -->
            <span v-else-if="p.instance_id" class="bind-manual">
              {{ $t('firewallBindManualFix') }}
            </span>
          </div>

          <div class="action-cell">
            <button
              class="btn"
              :disabled="!canManage || pending.has(portKey(p))"
              :title="$t('firewallEditSource')"
              @click="openEditSource(p)"
            >
              <Icon icon="mdi:account-key-outline" />
              {{ $t('firewallEditSource') }}
            </button>
            <button
              class="btn tiny"
              :class="p.allowed ? 'primary' : ''"
              :disabled="!canManage || pending.has(portKey(p))"
              @click="toggle(p)"
            >
              <Icon :icon="p.allowed ? 'mdi:check' : 'mdi:lock-outline'" />
              {{ p.allowed ? $t('firewallAllowed') : $t('firewallDenied') }}
            </button>
            <button
              v-if="p.source === 'manual'"
              class="btn ghost tiny"
              :disabled="pending.has(portKey(p))"
              :title="$t('delete')"
              @click="removeManual(p)"
            >
              <Icon icon="mdi:delete" />
            </button>
          </div>
        </div>
      </div>
    </section>

    <p v-if="groups.length > 0" class="note">
      <Icon icon="mdi:information-outline" />
      {{ $t('firewallNodeHint') }}
    </p>

    <!-- 移除手动端口确认（用全局 .overlay / .confirm-box，与其它页面一致） -->
    <Teleport to="body">
      <div v-if="confirmTarget" class="overlay" @click.self="confirmTarget = null">
        <div class="confirm-box">
          <div class="confirm-title">
            <Icon icon="mdi:alert-circle-outline" /> {{ $t('confirmDelete') }}
          </div>
          <p class="confirm-msg">
            {{ $t('firewallRemoveManualConfirm', { port: confirmTarget.port }) }}
          </p>
          <div class="confirm-actions">
            <button class="btn" @click="confirmTarget = null">{{ $t('cancel') }}</button>
            <button class="btn danger" @click="doRemoveManual">{{ $t('delete') }}</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- 手动添加 -->
    <Teleport to="body">
      <div v-if="addOpen" class="overlay" @click.self="addOpen = false">
        <div class="confirm-box">
          <div class="confirm-title">
            <Icon icon="mdi:plus" /> {{ $t('firewallAddPort') }}
          </div>
          <p class="confirm-msg">{{ $t('firewallAddPortHint') }}</p>
          <div class="flex gap-2 mb-3">
            <input
              v-model.number="newPort"
              type="number"
              min="1"
              max="65535"
              class="field flex-1"
              :placeholder="$t('port')"
              @keyup.enter="submitAdd"
            />
            <select v-model="newProto" class="field w-24">
              <option value="tcp">TCP</option>
              <option value="udp">UDP</option>
            </select>
          </div>
          <input
            v-model="newSources"
            class="field w-full mb-1"
            :placeholder="$t('firewallAddPortSource')"
            @keyup.enter="submitAdd"
          />
          <p class="confirm-msg !mb-4">{{ $t('firewallAddPortSourceHint') }}</p>
          <div class="confirm-actions">
            <button class="btn" @click="addOpen = false">{{ $t('cancel') }}</button>
            <button class="btn primary" :disabled="addBusy || !validNewPort" @click="submitAdd">
              {{ $t('confirm') }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>
    <!-- 来源限定编辑（所有端口行可用：保存覆盖该端口全部放行规则） -->
    <Teleport to="body">
      <div v-if="editTarget" class="overlay" @click.self="editTarget = null">
        <div class="confirm-box">
          <div class="confirm-title">
            <Icon icon="mdi:account-key-outline" />
            {{ $t('firewallEditSourceTitle', { port: editTarget.port }) }}
          </div>
          <p class="confirm-msg">{{ $t('firewallEditSourceHint') }}</p>
          <input
            v-model="editSources"
            class="field w-full mb-1"
            :placeholder="$t('firewallAddPortSource')"
            @keyup.enter="submitEditSource"
          />
          <p class="confirm-msg !mb-4">{{ $t('firewallAddPortSourceHint') }}</p>
          <div class="confirm-actions">
            <button class="btn" @click="editTarget = null">{{ $t('cancel') }}</button>
            <button class="btn primary" :disabled="editBusy" @click="submitEditSource">
              {{ $t('confirm') }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@/utils/ipc'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import { portKey, type FirewallOverview, type FirewallPortEntry, type FirewallPortSource } from '@/models/firewall'

const { t } = useI18n()

const overview = ref<FirewallOverview | null>(null)
const loading = ref(false)
const pageError = ref('')
const pending = ref<Set<string>>(new Set())

const canManage = computed(() => overview.value?.backend.available === true)
/** 可管理 且 防火墙确实在生效（装了没启用时不该显示绿色「已保护」） */
const backendHealthy = computed(() => canManage.value && overview.value?.backend.active !== false)

const backendIcon = computed(() => (backendHealthy.value ? 'mdi:shield-check' : 'mdi:shield-off-outline'))
const backendTitle = computed(() => {
  const kind = overview.value?.backend.kind ?? 'none'
  return t(`firewallBackend_${kind}`)
})
/**
 * 说明按「判定依据」取词条（`ufw_active` / `ufw_inactive` / `nft_blocking` …），
 * 而不是按后端名：同一个 ufw 在「生效」和「装了没启用」两种状态下必须说不同的话，
 * 否则用户会以为端口已经放开。
 */
const backendNote = computed(() => {
  const reason = overview.value?.backend.reason ?? 'no_tool'
  const key = `firewallBackendNote_${reason}`
  const text = t(key)
  // 词条缺失时 vue-i18n 会把 key 原样返回，回落到后端给的兜底说明
  return text === key ? (overview.value?.backend.note ?? '') : text
})

/** 规则到底有没有在拦（后端判定；不是「装没装」） */
const enforcement = computed(() => overview.value?.backend.enforcement ?? 'unknown')

const enforcementIcon = computed(() => {
  switch (enforcement.value) {
    case 'enforced':
      return 'mdi:shield-check'
    case 'system_blocking':
      return 'mdi:alert-outline'
    case 'unknown':
      return 'mdi:alert-outline'
    default:
      return 'mdi:shield-off-outline'
  }
})

/** 已经是纯静态类名，避免 Tailwind 扫不到动态拼接的类 */
const enforceClass = computed(() => {
  switch (enforcement.value) {
    case 'enforced':
      return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
    case 'system_blocking':
      return 'border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300'
    case 'unknown':
      return 'border-border bg-muted/40 text-foreground'
    default:
      return 'border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300'
  }
})

const enforceChipClass = computed(() => {
  switch (enforcement.value) {
    case 'enforced':
      return 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'
    case 'system_blocking':
      return 'bg-sky-500/15 text-sky-600 dark:text-sky-400'
    default:
      return 'bg-amber-500/15 text-amber-600 dark:text-amber-400'
  }
})

/**
 * 「仅监听本机」的文案分两种：取自真实套接字（正在跑，可信）与取自配置
 * （没在跑，只是预期）。混为一谈会让用户以为服务已经在拦或已经在听。
 */
function bindWarnText(p: FirewallPortEntry): string {
  const addr = p.bind || '127.0.0.1'
  return p.bind_source === 'listen'
    ? t('firewallBindLocalOnly', { addr })
    : t('firewallBindLocalOnlyConfig', { addr })
}

const GROUP_META: Record<FirewallPortSource, { icon: string; titleKey: string }> = {
  opx: { icon: 'mdi:application-cog-outline', titleKey: 'firewallGroupOpx' },
  software: { icon: 'mdi:package-variant-closed', titleKey: 'firewallGroupSoftware' },
  jar: { icon: 'mdi:leaf', titleKey: 'firewallGroupJar' },
  site: { icon: 'mdi:web-box', titleKey: 'firewallGroupSite' },
  manual: { icon: 'mdi:hand-back-right-outline', titleKey: 'firewallGroupManual' },
}

const groups = computed(() => {
  const ports = overview.value?.ports ?? []
  const order: FirewallPortSource[] = ['opx', 'software', 'jar', 'site', 'manual']
  return order
    .map((source) => ({
      source,
      ...GROUP_META[source],
      items: ports.filter((p) => p.source === source),
    }))
    .filter((g) => g.items.length > 0)
})

/** 数据库类端口：放行到公网是勒索/挖矿的头号入口，给个视觉警示（不阻止，用户已明确要求默认放行） */
function isRiskyPort(port: number): boolean {
  return [1433, 1521, 27017, 3306, 5432, 6379, 9200, 9300, 2181, 9092].includes(port)
}

function displayName(p: FirewallPortEntry): string {
  if (p.source === 'opx') return t('firewallOpxEntry')
  return p.label || '—'
}

async function load() {
  loading.value = true
  try {
    overview.value = await invoke<FirewallOverview>('get_firewall_overview')
  } catch (e) {
    pageError.value = String(e)
  } finally {
    loading.value = false
  }
}

function withPending(key: string, fn: () => Promise<void>) {
  if (pending.value.has(key)) return
  pending.value.add(key)
  fn().finally(() => pending.value.delete(key))
}

async function toggle(p: FirewallPortEntry) {
  const key = portKey(p)
  const want = !p.allowed
  pageError.value = ''
  pending.value.add(key)
  try {
    // 来源限定规则（每来源一条）逐条同步开关；全网规则走单条调用。
    // sources 由清单从防火墙规则读回，所有端口来源（软件/JAR/站点/opx）都可能有。
    if (p.sources?.length) {
      if (want) {
        for (const s of p.sources) {
          await invoke('set_firewall_port', {
            port: p.port,
            proto: p.proto,
            allow: true,
            source: s,
          })
        }
      } else {
        // 撤销：全网规则 + 每条来源规则一起撤，避免留下半开状态
        await invoke('set_firewall_port', { port: p.port, proto: p.proto, allow: false })
        for (const s of p.sources) {
          await invoke('set_firewall_port', {
            port: p.port,
            proto: p.proto,
            allow: false,
            source: s,
          })
        }
      }
    } else {
      await invoke('set_firewall_port', { port: p.port, proto: p.proto, allow: want })
    }
    // 以系统为准重新拉取：同端口的其它来源行状态会一起变
    await load()
  } catch (e) {
    pageError.value = String(e)
  } finally {
    pending.value.delete(key)
  }
}

async function fixBind(p: FirewallPortEntry) {
  if (!p.instance_id) return
  const key = portKey(p)
  pageError.value = ''
  await withPending(key, async () => {
    try {
      await invoke('set_instance_bind_all', { installedId: p.instance_id })
      showNotice(t('firewallFixBindDone', { name: displayName(p) }))
      await load()
    } catch (e) {
      pageError.value = String(e)
    }
  })
}

// ===== 提示条（替代原生 alert：与全局对话框风格一致，且不阻塞渲染）=====
const notice = ref('')
let noticeTimer: ReturnType<typeof setTimeout> | null = null

function showNotice(text: string) {
  notice.value = text
  if (noticeTimer) clearTimeout(noticeTimer)
  noticeTimer = setTimeout(() => {
    notice.value = ''
  }, 5000)
}

// ===== 手动端口移除（确认框走模板里的 overlay，不用 window.confirm）=====
const confirmTarget = ref<FirewallPortEntry | null>(null)

function removeManual(p: FirewallPortEntry) {
  confirmTarget.value = p
}

async function doRemoveManual() {
  const p = confirmTarget.value
  confirmTarget.value = null
  if (!p) return
  const key = portKey(p)
  pageError.value = ''
  await withPending(key, async () => {
    try {
      await invoke('remove_manual_port', { port: p.port, proto: p.proto })
      await load()
    } catch (e) {
      pageError.value = String(e)
      // 失败时也刷新：规则可能已被撤销、只是设置写入失败，状态要反映真实情况
      await load()
    }
  })
}

// ===== 手动添加 =====
const addOpen = ref(false)
const addBusy = ref(false)
const newPort = ref<number | string>('')
const newProto = ref('tcp')
/** 来源限定：逗号/空格分隔的 IP 或 CIDR，留空 = 对任意来源放行 */
const newSources = ref('')
const validNewPort = computed(() => {
  const n = Number(newPort.value)
  return Number.isInteger(n) && n >= 1 && n <= 65535
})

/** 客户端粗校验：非空 token 都像 IP/CIDR（具体合法性由后端 authoritative 校验） */
function parseSources(raw: string): string[] | null {
  const list = raw
    .split(/[,，\s]+/)
    .map((s) => s.trim())
    .filter(Boolean)
  if (list.length === 0) return null // 无来源 = 全网放行
  const looksLikeAddr = (s: string) =>
    /^[0-9a-fA-F.:]+$/.test(s) && /\d/.test(s) // 只含地址字符且至少有个数字
  return list.every(looksLikeAddr) ? list : null
}

function openAdd() {
  newPort.value = ''
  newProto.value = 'tcp'
  newSources.value = ''
  addOpen.value = true
}

async function submitAdd() {
  if (!validNewPort.value || addBusy.value) return
  const sources = parseSources(newSources.value)
  if (sources === null) {
    pageError.value = t('firewallAddPortSourceInvalid')
    return
  }
  addBusy.value = true
  pageError.value = ''
  try {
    await invoke('add_manual_port', {
      port: Number(newPort.value),
      proto: newProto.value,
      sources,
    })
    addOpen.value = false
    await load()
  } catch (e) {
    pageError.value = String(e)
  } finally {
    addBusy.value = false
  }
}

// ===== 来源限定编辑（所有端口行可用；保存 = 整组覆盖该端口规则）=====
const editTarget = ref<FirewallPortEntry | null>(null)
const editBusy = ref(false)
/** 来源限定：逗号/空格分隔的 IP 或 CIDR，留空 = 对任意来源放行 */
const editSources = ref('')

function openEditSource(p: FirewallPortEntry) {
  pageError.value = ''
  editTarget.value = p
  editSources.value = (p.sources ?? []).join(', ')
}

async function submitEditSource() {
  const p = editTarget.value
  if (!p || editBusy.value) return
  // 留空 = 撤销来源限定（全网放行）；非空但格式不对才报错
  let sources: string[] = []
  if (editSources.value.trim()) {
    const parsed = parseSources(editSources.value)
    if (parsed === null) {
      pageError.value = t('firewallAddPortSourceInvalid')
      return
    }
    sources = parsed
  }
  editBusy.value = true
  pageError.value = ''
  try {
    await invoke('set_firewall_port_sources', {
      port: p.port,
      proto: p.proto,
      sources,
    })
    editTarget.value = null
    showNotice(t('firewallEditSourceDone', { port: p.port }))
    await load()
  } catch (e) {
    pageError.value = String(e)
  } finally {
    editBusy.value = false
  }
}

onMounted(load)
</script>

<style scoped>
/* 全屏遮罩：模板里两个 Teleport 弹窗（添加 / 移除确认）都靠它居中。
   此前本页漏了这个定义，弹窗没有定位样式直接顺流排在页面末尾 ——
   看起来像「跑到左下角」。与 DnsAccountsPage 等页的定义保持一致。 */
.overlay {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
}

/* 来源限定芯片（手动端口） */
.src-chip {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-size: 10px;
  font-family: var(--font-mono, monospace);
  padding: 1px 6px;
  border-radius: 4px;
  background: color-mix(in oklch, var(--color-info, #3b82f6) 12%, transparent);
  color: var(--color-info, #3b82f6);
}
.src-chip svg {
  width: 11px;
  height: 11px;
}

/* 局部按钮尺寸：全局 .btn 是 32px 高度，端口行里需要更紧凑 */
.btn.tiny {
  height: 26px;
  padding: 0 8px;
  font-size: 12px;
  gap: 4px;
}
.btn.tiny svg {
  width: 14px;
  height: 14px;
}

.panel {
  border: 1px solid var(--color-border);
  border-radius: 8px;
  background: var(--color-card);
  padding: 14px 16px;
  margin-bottom: 14px;
}

.error-bar {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 12px;
  margin-bottom: 14px;
  border-radius: 8px;
  border: 1px solid oklch(0.7 0.15 60 / 0.4);
  background: oklch(0.7 0.15 60 / 0.12);
  color: oklch(0.75 0.14 60);
  font-size: 12px;
  line-height: 1.5;
}

.notice-bar {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 12px;
  margin-bottom: 14px;
  border-radius: 8px;
  border: 1px solid oklch(0.7 0.15 150 / 0.4);
  background: oklch(0.7 0.15 150 / 0.12);
  color: oklch(0.75 0.14 150);
  font-size: 12px;
  line-height: 1.5;
}

.group {
  margin-bottom: 18px;
}

.group-head {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 8px;
  color: var(--color-muted-foreground);
}

.rows {
  border: 1px solid var(--color-border);
  border-radius: 8px;
  background: var(--color-card);
  overflow: hidden;
}

.row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--color-border);
}
.row:last-child {
  border-bottom: none;
}
.row:hover {
  background: var(--color-muted);
}

.port-cell {
  display: flex;
  align-items: baseline;
  gap: 4px;
  min-width: 84px;
}
.port-num {
  font-family: ui-monospace, monospace;
  font-size: 16px;
  font-weight: 600;
  color: var(--color-foreground);
}
.port-proto {
  font-size: 10px;
  text-transform: uppercase;
  color: var(--color-muted-foreground);
}

.name-cell {
  flex: 1;
  min-width: 0;
}

.bind-cell {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}
.bind-warn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: oklch(0.75 0.14 75);
  max-width: 340px;
}
.bind-manual {
  font-size: 11px;
  color: var(--color-muted-foreground);
  opacity: 0.85;
}

.action-cell {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.field {
  height: 32px;
  padding: 0 10px;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-background);
  color: var(--color-foreground);
  font-size: 13px;
}

.note {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin-top: 4px;
  font-size: 11px;
  line-height: 1.6;
  color: var(--color-muted-foreground);
}
</style>
