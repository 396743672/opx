<template>
  <!-- 批次 4.7：标题/图标/副标题已并入顶部导航栏（图标与侧边栏同源 navGroups），
       页面纵向空间归还正文。本组件退化为「页面操作按钮行」：
       仅当调用方提供 #actions 时渲染（紧凑单行），否则不占任何空间。 -->
  <div
    v-if="$slots.actions"
    class="flex items-center justify-end gap-1.5 flex-wrap mb-4"
  >
    <slot name="actions" />
  </div>
</template>

<script setup lang="ts">
// props 仍声明以兼容既有调用点（各页面传了 title/subtitle/icon 但不再渲染）
defineProps<{
  title?: string
  subtitle?: string
  icon?: string
}>()
</script>
