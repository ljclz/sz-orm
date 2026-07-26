const fs = require('fs');
const f = 'e:/vue/test/鲜视达/rust/sz-orm/bench-comparison/BENCHMARK_REPORT.md';
let c = fs.readFileSync(f, 'utf8');

// 在 "参数绑定将 SZ-ORM 与 sqlx 在 SELECT BY ID" 之前插入 SELECT ALL 差距数据
const marker = '参数绑定将 SZ-ORM 与 sqlx 在 SELECT BY ID';
const idx = c.indexOf(marker);
if (idx < 0) {
    console.log('NOT FOUND marker');
    process.exit(1);
}

const insertion = [
    'v0.6 SQLite SELECT ALL 100k：sz-orm 192.60 ms vs sqlx 128.28 ms（慢 50.2%）',
    'v1.1.0 SQLite SELECT ALL 10k：sz-orm-params 20.91 ms（注：数据量不同无法直接对比绝对值；按每行归一化：sz-orm-format 2.50 µs/行 → sz-orm-params 2.09 µs/行，提升 16.5%）',
    ''
].join('\n');

c = c.slice(0, idx) + insertion + c.slice(idx);

// 更新差距描述
const oldDesc = '参数绑定将 SZ-ORM 与 sqlx 在 SELECT BY ID 场景的差距从 49.4% 缩小到 21.3%，剩余差距';
const newDesc = '参数绑定将 SZ-ORM 与 sqlx 在 SELECT BY ID 场景的差距从 49.4% 缩小到 21.3%，在 SELECT ALL 场景提升 16.5%。剩余差距';
c = c.replace(oldDesc, newDesc);

fs.writeFileSync(f, c, 'utf8');
console.log('OK: 10.4 updated with SELECT ALL gap data');
