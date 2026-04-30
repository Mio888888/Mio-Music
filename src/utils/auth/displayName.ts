export default function displayName(user: { name?: string | null; username?: string | null; nickname?: string | null } | null): string {
  if (!user) return '未登录'
  return user.name || user.nickname || user.username || '未登录'
}
