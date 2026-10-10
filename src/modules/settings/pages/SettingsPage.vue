<template>
  <div class="animate-fade-in max-w-2xl">
    <PageHeader
      icon="mdi:cog"
      :title="$t('settings')"
      :subtitle="$t('appearance')"
    />

    <!-- Tab 分区：切换只改 v-show，字段与保存逻辑不变 -->
    <div class="mb-4">
      <CategoryTabs v-model="activeTab" :tabs="settingsTabs" />
    </div>

    <div class="rounded-xl border border-border bg-card shadow-card overflow-hidden">
      <!-- ===== 通用 ===== -->
      <div v-show="activeTab === 'general'">
        <!-- 外观 -->
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('appearance') }}</h3>
        </div>
        <div class="px-5 pb-1 divide-y divide-border">
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('themeLabel') }}</span>
            <select
              v-model="themeValue"
              class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
            >
              <option value="auto">{{ $t('auto') }}</option>
              <option value="light">{{ $t('light') }}</option>
              <option value="warm">{{ $t('warm') }}</option>
              <option value="dark">{{ $t('dark') }}</option>
            </select>
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('languageLabel') }}</span>
            <select
              v-model="languageValue"
              class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
            >
              <option value="zh-CN">中文</option>
              <option value="en-US">English</option>
            </select>
          </div>
        </div>

        <!-- 关闭行为 -->
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('closeWindowAction') }}</h3>
        </div>
        <div class="px-5 pb-1 divide-y divide-border">
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('closeBehavior') }}</span>
            <select
              v-model="closeActionValue"
              class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
            >
              <option value="CloseToTray">{{ $t('closeToTray') }}</option>
              <option value="Exit">{{ $t('exitProgram') }}</option>
            </select>
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('askOnClose') }}</span>
            <SwitchBtn v-model="askOnCloseValue" />
          </div>
        </div>

        <!-- 开机自启 -->
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('autoStartOnBoot') }}</h3>
        </div>
        <div class="px-5 pb-1 divide-y divide-border">
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('autoStartOnBootDesc') }}</span>
            <SwitchBtn v-model="autostartValue" @update:modelValue="onToggleAutostart" />
          </div>
        </div>

        <!-- 下载代理 -->
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('proxySettings') }}</h3>
        </div>
        <div class="px-5 pb-4 divide-y divide-border">
          <div class="flex items-start justify-between gap-4 py-3">
            <div class="flex flex-col gap-0.5">
              <span class="text-sm">{{ $t('githubProxy') }}</span>
              <span class="text-xs text-muted-foreground">{{ $t('githubProxyMultiHint') }}</span>
            </div>
            <div class="flex gap-2 items-start">
              <textarea
                v-model="githubProxyValue"
                rows="2"
                class="px-2 py-1 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono resize-y"
                :placeholder="$t('proxyDefaultHint')"
              />
              <button
                class="btn text-xs h-7 px-2"
                @click="githubProxyValue = 'https://ghfast.top'"
                :title="$t('resetDefault')"
              >↺</button>
            </div>
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('globalProxy') }}</span>
            <input
              v-model="proxyValue"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
              :placeholder="$t('proxyEmptyDirect')"
            />
          </div>
        </div>

        <!-- 应用更新 -->
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('updateSection') }}</h3>
        </div>
        <div class="px-5 pb-4 divide-y divide-border">
          <div class="flex items-center justify-between gap-4 py-3">
            <div>
              <span class="text-sm">{{ $t('autoCheckUpdate') }}</span>
              <div class="text-xs text-muted-foreground">{{ $t('autoCheckUpdateDesc') }}</div>
            </div>
            <SwitchBtn v-model="autoCheckUpdateValue" />
          </div>
          <div class="py-3">
            <span class="text-xs text-muted-foreground">{{ $t('updateCheckInAbout') }}</span>
          </div>
        </div>
      </div>

      <!-- ===== 监控与告警 ===== -->
      <div v-show="activeTab === 'monitor'">
        <!-- 告警阈值 -->
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('alertThresholds') }}</h3>
        </div>
        <div class="px-5 pb-4 divide-y divide-border">
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('alertSystemCpu') }}</span>
            <input
              v-model.number="alertSystemCpuValue"
              type="number"
              min="1"
              max="100"
              class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
            />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('alertSystemMem') }}</span>
            <input
              v-model.number="alertSystemMemValue"
              type="number"
              min="1"
              max="100"
              class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
            />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('alertProcessCpu') }}</span>
            <input
              v-model.number="alertProcessCpuValue"
              type="number"
              min="1"
              max="100"
              class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
            />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('alertProcessMem') }}</span>
            <input
              v-model.number="alertProcessMemValue"
              type="number"
              min="1"
              max="100"
              class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
            />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('metricsRetainDays') }}</span>
            <div class="flex items-center gap-2">
              <input
                v-model.number="metricsRetainDaysValue"
                type="number"
                min="1"
                max="90"
                class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
              />
              <span class="text-xs text-muted-foreground">{{ $t('daysUnit') }}</span>
            </div>
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('snapshotKeep') }}</span>
            <div class="flex items-center gap-2">
              <input
                v-model.number="snapshotKeepValue"
                type="number"
                min="1"
                max="50"
                class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
              />
            </div>
          </div>
        </div>

        <!-- 告警通知 -->
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('alertNotify') }}</h3>
        </div>
        <div class="px-5 pb-4 divide-y divide-border">
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('webhookUrl') }}</span>
            <input
              v-model="webhookUrlValue"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
              :placeholder="$t('webhookUrlPlaceholder')"
            />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('webhookFormat') }}</span>
            <select
              v-model="webhookFormatValue"
              class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
            >
              <option value="json">{{ $t('webhookFormatJson') }}</option>
              <option value="dingtalk">{{ $t('webhookFormatDingtalk') }}</option>
              <option value="wecom">{{ $t('webhookFormatWecom') }}</option>
              <option value="feishu">{{ $t('webhookFormatFeishu') }}</option>
            </select>
          </div>
          <div v-if="webhookFormatValue === 'dingtalk'" class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('dingtalkSecret') }}</span>
            <input
              v-model="webhookSecretValue"
              type="password"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
            />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('smtpEnabled') }}</span>
            <SwitchBtn v-model="smtpEnabledValue" />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('smtpHost') }}</span>
            <input
              v-model="smtpHostValue"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
              placeholder="smtp.qq.com"
            />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('smtpPort') }}</span>
            <input
              v-model.number="smtpPortValue"
              type="number"
              min="1"
              max="65535"
              class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
            />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('smtpUser') }}</span>
            <input
              v-model="smtpUserValue"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
            />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('smtpPass') }}</span>
            <input
              v-model="smtpPassValue"
              type="password"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
            />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('smtpTo') }}</span>
            <input
              v-model="smtpToValue"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
              placeholder="a@x.com, b@y.com"
            />
          </div>
          <div v-if="smtpEnabledValue" class="flex items-start justify-between gap-4 py-3">
            <span class="text-xs text-warning max-w-72">{{ $t('smtpPassWarning') }}</span>
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('sendTestNotify') }}</span>
            <div class="flex flex-col gap-1 items-end min-w-0">
              <button class="btn text-xs h-7 px-2" :disabled="notifyTesting" @click="testNotify">
                {{ notifyTesting ? $t('testNotifySending') : $t('sendTestNotify') }}
              </button>
              <span
                v-if="notifyTestResult"
                class="text-xs max-w-72 text-right"
                :class="notifyTestOk ? 'text-success' : 'text-destructive'"
                style="overflow-wrap: anywhere; word-break: break-word"
              >{{ notifyTestResult }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- ===== 域名与 DNS ===== -->
      <div v-show="activeTab === 'dns'">
        <!-- 证书签发 -->
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('certSection') }}</h3>
        </div>
        <div class="px-5 pb-4 border-b border-border">
          <div class="flex items-center justify-between gap-4 py-3">
            <div>
              <span class="text-sm" :class="{ 'text-warning': acmeStagingValue }">{{ $t('acmeStaging') }}</span>
              <div class="text-xs" :class="acmeStagingValue ? 'text-warning' : 'text-muted-foreground'">
                {{ acmeStagingValue ? $t('acmeStagingOn') : $t('acmeStagingDesc') }}
              </div>
            </div>
            <SwitchBtn v-model="acmeStagingValue" />
          </div>
        </div>

        <!-- DDNS 动态域名 -->
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('ddnsSection') }}</h3>
        </div>
        <div class="px-5 pb-4 divide-y divide-border">
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('ddnsEnabled') }}</span>
            <SwitchBtn v-model="ddnsEnabledValue" />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('ddnsProvider') }}</span>
            <select v-model="ddnsProviderValue" class="h-8 px-2 w-56 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer">
              <option value="cloudflare">Cloudflare</option>
              <option value="aliyun">阿里云</option>
              <option value="dnspod">DNSPod</option>
              <option value="huawei">华为云</option>
            </select>
          </div>
          <div v-if="ddnsProviderValue === 'cloudflare'" class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('ddnsCloudflareToken') }}</span>
            <input v-model="ddnsCloudflareTokenValue" type="password"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
          </div>
          <template v-else-if="ddnsProviderValue === 'aliyun'">
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsAliyunKey') }}</span>
              <input v-model="ddnsAliyunKeyValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsAliyunSecret') }}</span>
              <input v-model="ddnsAliyunSecretValue" type="password"
                class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
          </template>
          <template v-else-if="ddnsProviderValue === 'dnspod'">
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsDnspodId') }}</span>
              <input v-model="ddnsDnspodIdValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsDnspodKey') }}</span>
              <input v-model="ddnsDnspodKeyValue" type="password"
                class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
          </template>
          <template v-else>
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsHuaweiKey') }}</span>
              <input v-model="ddnsHuaweiKeyValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsHuaweiSecret') }}</span>
              <input v-model="ddnsHuaweiSecretValue" type="password"
                class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
          </template>
          <div class="flex items-start justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('ddnsDomains') }}</span>
            <textarea v-model="ddnsDomainsText" rows="4"
              :placeholder="$t('ddnsDomainsPlaceholder')"
              class="px-2 py-1.5 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">
              {{ $t('ddnsIpv6') }}
              <span class="block text-xs text-muted-foreground">{{ $t('ddnsIpv6Hint') }}</span>
            </span>
            <SwitchBtn v-model="ddnsIpv6Value" />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('syncDdnsNow') }}</span>
            <div class="flex flex-col gap-1 items-end min-w-0">
              <button class="btn text-xs h-7 px-2" :disabled="ddnsSyncing" @click="syncDdns">
                {{ ddnsSyncing ? $t('ddnsSyncing') : $t('syncDdnsNow') }}
              </button>
              <span v-if="ddnsSyncResult" class="text-xs max-w-72 text-right"
                :class="ddnsSyncOk ? 'text-success' : 'text-destructive'"
                style="overflow-wrap: anywhere; word-break: break-word">{{ ddnsSyncResult }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- ===== 安全 ===== -->
      <div v-show="activeTab === 'security'">
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('lockScreen') }}</h3>
        </div>
        <div class="px-5 pb-4 divide-y divide-border">
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('lockStatus') }}</span>
            <span class="text-sm" :class="lockStore.hasPassword ? 'text-success' : 'text-muted-foreground'">
              {{ lockStore.hasPassword ? $t('lockOn') : $t('lockOff') }}
            </span>
          </div>
          <!-- 未设锁：设置密码即开启 -->
          <div v-if="!lockStore.hasPassword" class="py-3 flex flex-col gap-3">
            <span class="text-xs text-muted-foreground">{{ $t('lockSetPasswordHint') }}</span>
            <input
              v-model="lockPasswordInput"
              type="password"
              :placeholder="$t('lockNewPassword')"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
            />
            <div class="flex items-center gap-2">
              <button class="btn text-xs h-7 px-2" :disabled="lockSetting || !lockPasswordInput" @click="enableLock">
                {{ lockSetting ? $t('loading') : $t('lockSetPassword') }}
              </button>
              <span v-if="lockSetError" class="text-xs text-destructive">{{ lockSetError }}</span>
            </div>
          </div>
          <!-- 已设锁：提示 + 立即锁定 + 清空（关闭） -->
          <div v-else class="py-3 flex flex-col gap-3">
            <span class="text-xs text-muted-foreground">{{ $t('lockIdleHint', { n: lockStore.IDLE_MINUTES }) }}</span>
            <div class="flex items-center gap-2 flex-wrap">
              <button class="btn text-xs h-7 px-2" :disabled="!lockStore.unlocked" @click="lockStore.lock()">
                {{ $t('lockLockNow') }}
              </button>
              <button v-if="!lockClearConfirming" class="btn text-xs h-7 px-2" @click="lockClearConfirming = true">
                {{ $t('lockClearPassword') }}
              </button>
              <template v-else>
                <span class="text-xs text-warning">{{ $t('lockClearConfirm') }}</span>
                <button class="btn text-xs h-7 px-2" :disabled="lockClearing" @click="disableLock">
                  {{ $t('lockClearConfirmYes') }}
                </button>
                <button class="btn text-xs h-7 px-2" @click="lockClearConfirming = false">
                  {{ $t('lockCancel') }}
                </button>
              </template>
            </div>
          </div>
        </div>

        <!-- Web 管理入口（批次 4.5：server 生命周期由后端 supervisor 热生效，
             本组补 token 查看/重置 + 打开浏览器 + LAN 强提示 + 启动失败横幅） -->
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('webGroup') }}</h3>
        </div>
        <div
          v-if="webServerErrorMsg"
          class="mx-5 mt-2 rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive flex items-start gap-2"
        >
          <Icon icon="mdi:alert-circle-outline" class="text-base flex-shrink-0" />
          <span>{{ $t('webServerError', { msg: webServerErrorMsg }) }}</span>
        </div>
        <div class="px-5 pb-4 divide-y divide-border">
          <div class="flex items-center justify-between gap-4 py-3">
            <div class="flex flex-col gap-1">
              <span class="text-sm">{{ $t('webEnabled') }}</span>
              <div class="text-xs text-muted-foreground">{{ $t('webEnabledHint') }}</div>
            </div>
            <SwitchBtn v-model="webEnabledValue" />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('webPort') }}</span>
            <input
              v-model.number="webPortValue"
              type="number"
              min="1"
              max="65535"
              class="h-8 px-2 w-24 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
            />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <div class="flex flex-col gap-1">
              <span class="text-sm">{{ $t('webLanAccess') }}</span>
              <div class="text-xs text-muted-foreground">{{ $t('webLanAccessHint') }}</div>
            </div>
            <SwitchBtn v-model="webLanAccessValue" />
          </div>
          <!-- LAN 强提示（D6/风险对策：HTTP 明文，同网段可嗅探 token） -->
          <div v-if="webLanAccessValue" class="flex items-start gap-2 py-3 text-xs text-destructive">
            <Icon icon="mdi:alert-outline" class="text-base flex-shrink-0" />
            <span>{{ $t('webLanWarning') }}</span>
          </div>
          <!-- token 查看/重置 + 打开浏览器：桌面专属（token 明文不经 HTTP 回传，
               Web 端重置后新令牌无法送回浏览器） -->
          <div v-if="isDesktop" class="flex items-center justify-between gap-4 py-3">
            <div class="flex flex-col gap-1 min-w-0">
              <span class="text-sm">{{ $t('webTokenLabel') }}</span>
              <div class="text-xs text-muted-foreground font-mono break-all">{{ webToken || '—' }}</div>
            </div>
            <div class="flex items-center gap-2 flex-shrink-0">
              <button class="btn text-xs h-7 px-2" :disabled="!webToken" @click="openWebUi">
                <Icon icon="mdi:open-in-new" />
                {{ $t('webOpenBrowser') }}
              </button>
              <template v-if="!webResetConfirming">
                <button class="btn text-xs h-7 px-2" @click="webResetConfirming = true">
                  {{ $t('webResetToken') }}
                </button>
              </template>
              <template v-else>
                <button class="btn text-xs h-7 px-2" :disabled="webResetting" @click="resetWebToken">
                  {{ webResetting ? $t('webResetting') : $t('webResetTokenOk') }}
                </button>
                <button class="btn text-xs h-7 px-2" @click="webResetConfirming = false">
                  {{ $t('lockCancel') }}
                </button>
              </template>
            </div>
          </div>
          <!-- 扫码访问（批次 4.6）：手机相机扫码直达（#token= 自动登录）。
               二维码内容为默认路由出口 IP 的 LAN URL（get_web_access_urls）；
               无出口 IP（离线/单机）时隐藏 -->
          <div v-if="webQrDataUrl" class="flex items-center justify-between gap-4 py-3">
            <div class="flex flex-col gap-1.5 min-w-0">
              <span class="text-sm">{{ $t('webScanTitle') }}</span>
              <div class="text-xs text-muted-foreground break-all font-mono">{{ webLanUrl }}</div>
              <button class="btn text-xs h-7 px-2 self-start" @click="copyWebUrl">
                <Icon icon="mdi:content-copy" />
                {{ $t('webCopyAddress') }}
              </button>
            </div>
            <img
              :src="webQrDataUrl"
              :alt="$t('webScanTitle')"
              class="w-[110px] h-[110px] rounded-lg border border-border flex-shrink-0"
            />
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import QRCode from 'qrcode'
import { invoke } from '@/utils/ipc'
import { listen, isDesktop, type UnlistenFn } from '@/utils/transport'
import { openUrl } from '@tauri-apps/plugin-opener'
import { toast } from '@/composables/useToast'
import { useSettingsStore } from '@/stores/settings'
import { useLockStore } from '@/stores/lock'
import { CloseWindowAction, type ThemeMode, type Language } from '@/models/settings'
import PageHeader from '@/components/PageHeader.vue'
import SwitchBtn from '@/components/SwitchBtn.vue'
import CategoryTabs, { type CategoryTab } from '@/modules/software-manager/components/CategoryTabs.vue'

const { t } = useI18n()
const settingsStore = useSettingsStore()
const lockStore = useLockStore()
const route = useRoute()
const router = useRouter()

// Tab 分区：选中状态存 URL query —— 刷新保持、可从别处直达；非法值回落首 Tab
const TAB_KEYS = ['general', 'monitor', 'dns', 'security'] as const
const activeTab = ref<string>(
  TAB_KEYS.includes(route.query.tab as (typeof TAB_KEYS)[number])
    ? (route.query.tab as string)
    : 'general'
)
watch(activeTab, (tab) => {
  // replace：切 Tab 不该污染浏览器历史
  router.replace({ query: { ...route.query, tab } })
})

const settingsTabs = computed<CategoryTab[]>(() => [
  { key: 'general', label: t('settingsTabGeneral'), icon: 'mdi:tune' },
  { key: 'monitor', label: t('settingsTabMonitor'), icon: 'mdi:bell-outline' },
  { key: 'dns', label: t('settingsTabDdns'), icon: 'mdi:dns-outline' },
  { key: 'security', label: t('settingsTabSecurity'), icon: 'mdi:shield-lock-outline' },
])

const themeValue = ref<ThemeMode>('auto')
const languageValue = ref<Language>('zh-CN')
const closeActionValue = ref<CloseWindowAction>(CloseWindowAction.CloseToTray)
const askOnCloseValue = ref(true)
const githubProxyValue = ref('')
const proxyValue = ref('')
const autoCheckUpdateValue = ref(false)
const autostartValue = ref(false)
const acmeStagingValue = ref(false)
const alertSystemCpuValue = ref(90)
const alertSystemMemValue = ref(90)
const alertProcessCpuValue = ref(90)
const alertProcessMemValue = ref(90)
const metricsRetainDaysValue = ref(7)
const snapshotKeepValue = ref(5)
const webhookUrlValue = ref('')
const webhookFormatValue = ref('json')
const webhookSecretValue = ref('')
const smtpEnabledValue = ref(false)
const smtpHostValue = ref('')
const smtpPortValue = ref(465)
const smtpUserValue = ref('')
const smtpPassValue = ref('')
const smtpToValue = ref('')
const notifyTesting = ref(false)
const notifyTestResult = ref('')
const notifyTestOk = ref(false)
const ddnsEnabledValue = ref(false)
const ddnsProviderValue = ref('cloudflare')
const ddnsCloudflareTokenValue = ref('')
const ddnsAliyunKeyValue = ref('')
const ddnsAliyunSecretValue = ref('')
const ddnsDnspodIdValue = ref('')
const ddnsDnspodKeyValue = ref('')
const ddnsHuaweiKeyValue = ref('')
const ddnsHuaweiSecretValue = ref('')
const ddnsDomainsText = ref('')
const ddnsIpv6Value = ref(false)
const ddnsSyncing = ref(false)
const ddnsSyncResult = ref('')
const ddnsSyncOk = ref(false)
// Web 管理入口（批次 4.2：仅存储字段；批次 4.5：token/打开浏览器/错误横幅）
const webEnabledValue = ref(false)
const webPortValue = ref(17580)
const webLanAccessValue = ref(false)
const webToken = ref('')
const webServerErrorMsg = ref('')
const webResetConfirming = ref(false)
const webResetting = ref(false)
// 扫码访问（批次 4.6）
const webLanUrl = ref('')
const webQrDataUrl = ref('')
let unlistenServerError: UnlistenFn | null = null

async function openWebUi() {
  if (!webToken.value) return
  const port = webPortValue.value || 17580
  // fragment 传 token：不随请求上送、不入访问日志（设计 D3 / ADR §3.4）
  const url = `http://127.0.0.1:${port}/#token=${webToken.value}`
  openUrl(url).catch(() => {})
}

async function resetWebToken() {
  webResetting.value = true
  try {
    await invoke('reset_web_token')
    webToken.value = await invoke<string>('get_web_token')
    webResetConfirming.value = false
    toast(t('webResetDone'), 'ok')
  } catch (e) {
    toast(typeof e === 'string' ? e : String((e as Error)?.message ?? e), 'err')
  } finally {
    webResetting.value = false
  }
}

/** 扫码地址一键复制（批次 4.6） */
async function copyWebUrl() {
  try {
    await navigator.clipboard.writeText(webLanUrl.value)
    toast(t('webCopyDone'), 'ok')
  } catch {
    toast(t('webPasteFailed'), 'err')
  }
}

/** 取访问 URL 列表并渲染 LAN 二维码（批次 4.6；桌面专属命令，Web 模式静默） */
async function loadAccessUrls() {
  try {
    const urls = await invoke<string[]>('get_web_access_urls')
    const lan = urls.find((u) => !u.includes('//127.0.0.1'))
    if (lan) {
      webLanUrl.value = lan
      webQrDataUrl.value = await QRCode.toDataURL(lan, { width: 220, margin: 1 })
    }
  } catch {
    // 令牌读取失败 / Web 模式 409：扫码区保持隐藏
  }
}

onMounted(async () => {
  try {
    autostartValue.value = await invoke<boolean>('get_autostart')
  } catch {
    autostartValue.value = false
  }
  if (isDesktop) {
    try {
      webToken.value = await invoke<string>('get_web_token')
    } catch {
      webToken.value = ''
    }
    await loadAccessUrls()
  }
  // 端口冲突等启动失败 → 后端 emit web-server-error，横幅显示（D6 明确报错）
  unlistenServerError = await listen<string>('web-server-error', (e) => {
    webServerErrorMsg.value = e.payload ?? ''
  })
})

onUnmounted(() => {
  unlistenServerError?.()
  unlistenServerError = null
})

async function onToggleAutostart(v: boolean) {
  try {
    await invoke('set_autostart', { enabled: v })
  } catch (e) {
    console.error('set autostart failed:', e)
    autostartValue.value = !v
  }
}

// 锁屏：设置 / 清空密码（锁即开关）
const lockPasswordInput = ref('')
const lockSetting = ref(false)
const lockSetError = ref('')
const lockClearConfirming = ref(false)
const lockClearing = ref(false)

async function enableLock() {
  if (!lockPasswordInput.value) return
  lockSetting.value = true
  lockSetError.value = ''
  try {
    await lockStore.setPassword(lockPasswordInput.value)
    lockPasswordInput.value = ''
  } catch (e) {
    lockSetError.value =
      typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
  } finally {
    lockSetting.value = false
  }
}

async function disableLock() {
  lockClearing.value = true
  try {
    await lockStore.clearPassword()
    lockClearConfirming.value = false
  } finally {
    lockClearing.value = false
  }
}


watch(
  () => settingsStore.settings,
  (s) => {
    if (s) {
      themeValue.value = (s.theme as ThemeMode) || 'auto'
      languageValue.value = (s.language as Language) || 'zh-CN'
      closeActionValue.value = s.close_window_action
      askOnCloseValue.value = s.ask_on_close
      githubProxyValue.value = s.github_proxy_url || ''
      proxyValue.value = s.proxy_url || ''
      autoCheckUpdateValue.value = s.auto_check_update
      acmeStagingValue.value = s.acme_use_staging
      alertSystemCpuValue.value = s.alert_system_cpu ?? 90
      alertSystemMemValue.value = s.alert_system_mem ?? 90
      alertProcessCpuValue.value = s.alert_process_cpu ?? 90
      alertProcessMemValue.value = s.alert_process_mem ?? 90
      metricsRetainDaysValue.value = s.metrics_retain_days ?? 7
      snapshotKeepValue.value = s.snapshot_keep ?? 5
      webhookUrlValue.value = s.alert_webhook_url || ''
      webhookFormatValue.value = s.alert_webhook_format || 'json'
      webhookSecretValue.value = s.alert_webhook_secret || ''
      smtpEnabledValue.value = s.smtp_enabled
      smtpHostValue.value = s.smtp_host || ''
      smtpPortValue.value = s.smtp_port ?? 465
      smtpUserValue.value = s.smtp_user || ''
      smtpPassValue.value = s.smtp_pass || ''
      smtpToValue.value = s.smtp_to || ''
      ddnsEnabledValue.value = s.ddns_enabled
      ddnsProviderValue.value = s.ddns_provider || 'cloudflare'
      ddnsCloudflareTokenValue.value = s.ddns_cloudflare_token || ''
      ddnsAliyunKeyValue.value = s.ddns_aliyun_access_key_id || ''
      ddnsAliyunSecretValue.value = s.ddns_aliyun_access_key_secret || ''
      ddnsDnspodIdValue.value = s.ddns_dnspod_secret_id || ''
      ddnsDnspodKeyValue.value = s.ddns_dnspod_secret_key || ''
      ddnsHuaweiKeyValue.value = s.ddns_huawei_access_key || ''
      ddnsHuaweiSecretValue.value = s.ddns_huawei_secret_key || ''
      ddnsDomainsText.value = (s.ddns_domains || []).join('\n')
      ddnsIpv6Value.value = s.ddns_enable_ipv6
      webEnabledValue.value = s.web_enabled ?? false
      webPortValue.value = s.web_port ?? 17580
      webLanAccessValue.value = s.web_lan_access ?? false
    }
  },
  { immediate: true }
)

watch(languageValue, (lang) => {
  settingsStore.setLanguage(lang)
})

watch(themeValue, (mode) => {
  settingsStore.setTheme(mode)
})

let saveTimer: ReturnType<typeof setTimeout> | undefined

watch(
  [closeActionValue, askOnCloseValue, githubProxyValue, proxyValue, autoCheckUpdateValue, acmeStagingValue, alertSystemCpuValue, alertSystemMemValue, alertProcessCpuValue, alertProcessMemValue, metricsRetainDaysValue, snapshotKeepValue, webhookUrlValue, webhookFormatValue, webhookSecretValue, smtpEnabledValue, smtpHostValue, smtpPortValue, smtpUserValue, smtpPassValue, smtpToValue, ddnsEnabledValue, ddnsProviderValue, ddnsCloudflareTokenValue, ddnsAliyunKeyValue, ddnsAliyunSecretValue, ddnsDnspodIdValue, ddnsDnspodKeyValue, ddnsHuaweiKeyValue, ddnsHuaweiSecretValue, ddnsDomainsText, ddnsIpv6Value, webEnabledValue, webPortValue, webLanAccessValue],
  () => {
    clearTimeout(saveTimer)
    saveTimer = setTimeout(save, 400)
  }
)

async function save() {
  if (!settingsStore.settings) return
  settingsStore.settings.close_window_action = closeActionValue.value
  settingsStore.settings.ask_on_close = askOnCloseValue.value
  settingsStore.settings.github_proxy_url = githubProxyValue.value
  settingsStore.settings.proxy_url = proxyValue.value
  settingsStore.settings.auto_check_update = autoCheckUpdateValue.value
  settingsStore.settings.acme_use_staging = acmeStagingValue.value
  // 数值输入被清空时 v-model.number 会给出 ''，直接写进 store 会让 save_settings 反序列化失败
  // 并污染后续所有保存 —— 这里归一到 [1,100]，非法值回落默认 90
  const pct = (v: number) => (Number(v) >= 1 && Number(v) <= 100 ? Number(v) : 90)
  alertSystemCpuValue.value = pct(alertSystemCpuValue.value)
  alertSystemMemValue.value = pct(alertSystemMemValue.value)
  alertProcessCpuValue.value = pct(alertProcessCpuValue.value)
  alertProcessMemValue.value = pct(alertProcessMemValue.value)
  settingsStore.settings.alert_system_cpu = alertSystemCpuValue.value
  settingsStore.settings.alert_system_mem = alertSystemMemValue.value
  settingsStore.settings.alert_process_cpu = alertProcessCpuValue.value
  settingsStore.settings.alert_process_mem = alertProcessMemValue.value
  // 指标保留天数归一 [1,90]，非法值回落 7（清空输入同理，防止 '' 写坏 settings.json）
  metricsRetainDaysValue.value =
    Number(metricsRetainDaysValue.value) >= 1 && Number(metricsRetainDaysValue.value) <= 90
      ? Number(metricsRetainDaysValue.value)
      : 7
  settingsStore.settings.metrics_retain_days = metricsRetainDaysValue.value
  // 快照保留数量归一 [1,50]，非法值回落 5（防止 '' 写坏 settings.json）
  snapshotKeepValue.value =
    Number(snapshotKeepValue.value) >= 1 && Number(snapshotKeepValue.value) <= 50
      ? Number(snapshotKeepValue.value)
      : 5
  settingsStore.settings.snapshot_keep = snapshotKeepValue.value
  // 端口输入清空时 v-model.number 给 ''，归一回落 465（同告警阈值的 pct 兜底逻辑）
  smtpPortValue.value = Number(smtpPortValue.value) >= 1 && Number(smtpPortValue.value) <= 65535 ? Number(smtpPortValue.value) : 465
  settingsStore.settings.alert_webhook_url = webhookUrlValue.value
  settingsStore.settings.alert_webhook_format = webhookFormatValue.value
  settingsStore.settings.alert_webhook_secret = webhookSecretValue.value
  settingsStore.settings.smtp_enabled = smtpEnabledValue.value
  settingsStore.settings.smtp_host = smtpHostValue.value
  settingsStore.settings.smtp_port = smtpPortValue.value
  settingsStore.settings.smtp_user = smtpUserValue.value
  settingsStore.settings.smtp_pass = smtpPassValue.value
  settingsStore.settings.smtp_to = smtpToValue.value
  settingsStore.settings.ddns_enabled = ddnsEnabledValue.value
  settingsStore.settings.ddns_provider = ddnsProviderValue.value
  settingsStore.settings.ddns_cloudflare_token = ddnsCloudflareTokenValue.value
  settingsStore.settings.ddns_aliyun_access_key_id = ddnsAliyunKeyValue.value
  settingsStore.settings.ddns_aliyun_access_key_secret = ddnsAliyunSecretValue.value
  settingsStore.settings.ddns_dnspod_secret_id = ddnsDnspodIdValue.value
  settingsStore.settings.ddns_dnspod_secret_key = ddnsDnspodKeyValue.value
  settingsStore.settings.ddns_huawei_access_key = ddnsHuaweiKeyValue.value
  settingsStore.settings.ddns_huawei_secret_key = ddnsHuaweiSecretValue.value
  settingsStore.settings.ddns_domains = ddnsDomainsText.value.split('\n')
    .map((x) => x.trim()).filter(Boolean)
  settingsStore.settings.ddns_enable_ipv6 = ddnsIpv6Value.value
  // Web 端口输入清空时 v-model.number 给 ''，归一回落 17580（同 smtp_port 兜底逻辑）
  webPortValue.value = Number(webPortValue.value) >= 1 && Number(webPortValue.value) <= 65535 ? Number(webPortValue.value) : 17580
  settingsStore.settings.web_enabled = webEnabledValue.value
  settingsStore.settings.web_port = webPortValue.value
  settingsStore.settings.web_lan_access = webLanAccessValue.value
  await settingsStore.saveSettings()
}

async function testNotify() {
  if (!webhookUrlValue.value.trim() && !smtpEnabledValue.value) {
    notifyTestOk.value = false
    notifyTestResult.value = t('testNotifyNoChannel')
    return
  }
  notifyTesting.value = true
  notifyTestResult.value = t('testNotifySending')
  try {
    await save() // 先持久化当前输入，后端读的是 settings.json
    await invoke('test_alert_webhook')
    notifyTestOk.value = true
    notifyTestResult.value = t('testNotifyOk')
  } catch (e) {
    notifyTestOk.value = false
    notifyTestResult.value = String(e)
  } finally {
    notifyTesting.value = false
  }
}

async function syncDdns() {
  ddnsSyncing.value = true
  ddnsSyncResult.value = t('ddnsSyncing')
  try {
    await save() // 先持久化当前输入，后端读的是 settings.json
    ddnsSyncResult.value = await invoke<string>('sync_ddns_now')
    ddnsSyncOk.value = true
  } catch (e) {
    ddnsSyncOk.value = false
    ddnsSyncResult.value = String(e)
  } finally {
    ddnsSyncing.value = false
  }
}
</script>
