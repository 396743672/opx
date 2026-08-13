// 离线注册应用使用的 MDI 图标子集，避免运行时依赖 api.iconify.design。
// 图标子集由 scripts/gen-icons.mjs 生成，新增图标后需重新生成。
import { addCollection } from '@iconify/vue'
import mdiIcons from '@/assets/mdi-icons.json'

addCollection(mdiIcons as unknown as Parameters<typeof addCollection>[0])
