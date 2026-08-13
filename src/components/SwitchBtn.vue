<template>
  <button
    type="button"
    role="switch"
    :aria-checked="modelValue"
    class="relative h-5 w-9 flex-shrink-0 rounded-full transition-colors duration-200 cursor-pointer"
    :class="modelValue ? 'bg-primary' : 'bg-border'"
    @click="$emit('update:modelValue', !modelValue)"
  >
    <span
      class="absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white shadow-sm transition-all duration-200"
      :class="modelValue ? 'left-[18px]' : 'left-0.5'"
    />
  </button>
</template>

<script setup lang="ts">
defineProps<{ modelValue: boolean }>()
defineEmits<{ 'update:modelValue': [value: boolean] }>()
</script>
