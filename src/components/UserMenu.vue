<template>
  <div class="user-menu">
    <button @click="toggleMenu" class="user-btn">
      👤
    </button>
    <div v-if="menuOpen" class="dropdown-menu">
      <div class="menu-item" @click="handleProfile">
        👤 个人资料
      </div>
      <div class="menu-item" @click="handleLogout">
        🚪 退出登录
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

const menuOpen = ref(false)

const toggleMenu = () => {
  menuOpen.value = !menuOpen.value
}

const handleProfile = () => {
  // TODO: 跳转到个人资料页面
  menuOpen.value = false
}

const handleLogout = () => {
  // TODO: 处理登出逻辑
  menuOpen.value = false
}
</script>

<style scoped>
.user-menu {
  position: relative;
}

.user-btn {
  background: none;
  border: none;
  font-size: 1.2rem;
  cursor: pointer;
  padding: 0.5rem;
  border-radius: 50%;
  transition: background 0.2s;
}

.user-btn:hover {
  background: var(--background);
}

.dropdown-menu {
  position: absolute;
  top: 100%;
  right: 0;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 4px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
  min-width: 150px;
  margin-top: 0.5rem;
  z-index: 100;
}

.menu-item {
  padding: 0.75rem 1rem;
  cursor: pointer;
  transition: background 0.2s;
  white-space: nowrap;
}

.menu-item:hover {
  background: var(--background);
}
</style>