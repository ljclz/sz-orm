// 修复 SQLite URL 格式（Windows 兼容）
const fs = require('fs');
const path = require('path');

const filePath = path.join(__dirname, 'benches', 'cross_db_comparison.rs');
let src = fs.readFileSync(filePath, 'utf8');
src = src.replace(/\r\n/g, '\n');

const old = `    let sqlite_url = format!("sqlite://{}", tmp_path.display());`;
const newStr = `    // sqlx SQLite URL：Windows 上需要 file:// URI 形式（避免盘符被误认为 host）
    let tmp_path_str = tmp_path.display().to_string().replace('\\\\', '/');
    let sqlite_url = format!("sqlite://file:{}", tmp_path_str);`;

if (src.includes(old)) {
    src = src.replace(old, newStr);
    console.log('Replaced sqlite_url format');
} else {
    console.log('Old sqlite_url not found');
    process.exit(1);
}

fs.writeFileSync(filePath, src);
console.log('Done');
