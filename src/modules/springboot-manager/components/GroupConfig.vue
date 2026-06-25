<template>
  <card>
    <card-header class="flex items-center justify-between">
      <h3 class="font-semibold">{{ $t('groupConfig') }}</h3>
      <button @click="openAddDialog" class="px-3 py-1 rounded-md bg-primary text-primary-foreground">
        {{ $t('addGroup') }}
      </button>
    </card-header>
    <card-content>
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b">
            <th class="text-left py-2 px-2">{{ $t('name') }}</th>
            <th class="text-left py-2 px-2">{{ $t('order') }}</th>
            <th class="text-left py-2 px-2">{{ $t('dependencies') }}</th>
            <th class="text-right py-2 px-2">{{ $t('action') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="group in groups" :key="group.id" class="border-b">
            <td class="py-2 px-2 font-medium">{{ group.name }}</td>
            <td class="py-2 px-2">{{ group.order }}</td>
            <td class="py-2 px-2">{{ group.depends_on.join(', ') }}</td>
            <td class="py-2 px-2 text-right">
              <button @click="handleDelete(group.id)" class="px-2 py-1 text-xs rounded-md bg-destructive/10 text-destructive">
                {{ $t('delete') }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <div v-if="groups.length === 0" class="text-center py-8 text-muted-foreground text-sm">
        {{ $t('noGroups') }}
      </div>
    </card-content>
  </card>
</template>

<script setup lang="ts">
import type { AppGroup } from '@/models/springboot'
import { useI18n } from 'vue-i18n'

withDefaults(defineProps<{
  groups: AppGroup[]
  onDelete: (id: string) => void
}>(), {})

const { t } = useI18n()

const emit = defineEmits<{
  addGroup: []
}>()

const openAddDialog = () => {
  emit('addGroup')
}

const handleDelete = (id: string) => {
  onDelete(id)
}
</script>