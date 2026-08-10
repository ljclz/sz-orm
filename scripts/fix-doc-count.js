const fs = require('fs');
const path = require('path');

const files = [
    'docs/sz-orm-engineering-practices.md',
    'README.md',
    'README.en.md',
    'docs/sz-orm使用指南.md',
    'docs/sz-orm架构设计.md',
    'docs/sz-ormAPI参考.md',
    'docs/sz-orm与同类产品对比分析.md'
];

const replacements = [
    { from: /43 workspace 包/g, to: '46 workspace 包' },
    { from: /工作空间 43 个成员/g, to: '工作空间 46 个成员' },
    { from: /43 个工作空间成员/g, to: '46 个工作空间成员' },
    { from: /43 个 sz-orm-\* lib/g, to: '41 个 sz-orm-* lib' },
    { from: /41 个 lib \+ cli \+ examples/g, to: '44 个 lib + cli + examples' },
    { from: /覆盖 43 个 workspace 包/g, to: '覆盖 46 个 workspace 包' },
    { from: /同步至 43 包/g, to: '同步至 46 包' },
    { from: /\[Packages\]\(https:\/\/img\.shields\.io\/badge\/packages-43-purple\.svg\)/g, to: '[Packages](https://img.shields.io/badge/packages-46-purple.svg)' },
];

let totalChanges = 0;

for (const file of files) {
    const filePath = path.join(__dirname, '..', file);
    if (!fs.existsSync(filePath)) {
        console.log(`跳过不存在的文件: ${file}`);
        continue;
    }

    let content = fs.readFileSync(filePath, 'utf8');
    let original = content;

    for (const rep of replacements) {
        content = content.replace(rep.from, rep.to);
    }

    if (content !== original) {
        fs.writeFileSync(filePath, content, 'utf8');
        console.log(`已更新: ${file}`);
        totalChanges++;
    }
}

console.log(`\n共更新 ${totalChanges} 个文件`);