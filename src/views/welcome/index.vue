<template>
  <div class="welcome-page">
    <div class="welcome-content">
      <img src="/default-cover.png" alt="logo" class="welcome-logo" />
      <h1>澜音 Music</h1>
      <p>一款简洁优雅的音乐播放器</p>
      <t-button theme="primary" size="large" @click="$router.push('/home')">
        开始使用
      </t-button>
    </div>
  </div>
</template>

<style scoped>
.welcome-page {
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
}
.welcome-content {
  text-align: center;
}
.welcome-logo {
  width: 120px;
  height: 120px;
  border-radius: 20px;
  margin-bottom: 24px;
}
.welcome-content h1 {
  font-size: 32px;
  margin-bottom: 8px;
}
.welcome-content p {
  color: var(--td-text-color-secondary);
  margin-bottom: 32px;
}
</style>