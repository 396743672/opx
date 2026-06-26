import zhCN from './src/locales/zh-CN.ts';
import enUS from './src/locales/en-US.ts';

const zhKeys = Object.keys(zhCN);
const enKeys = Object.keys(enUS);

const missingInEN = zhKeys.filter(key => !enKeys.includes(key));
const missingInZH = enKeys.filter(key => !zhKeys.includes(key));

console.log('=== 翻译键对比 ===');
console.log(`中文翻译键数量: ${zhKeys.length}`);
console.log(`英文翻译键数量: ${enKeys.length}`);

if (missingInEN.length > 0) {
  console.log('\n❌ 中文有但英文缺少的键:', missingInEN);
} else {
  console.log('\n✅ 所有中文键都在英文中存在');
}

if (missingInZH.length > 0) {
  console.log('\n❌ 英文有但中文缺少的键:', missingInZH);
} else {
  console.log('\n✅ 所有英文键都在中文中存在');
}

if (missingInEN.length === 0 && missingInZH.length === 0) {
  console.log('\n🎉 所有翻译键完全对应!');
}