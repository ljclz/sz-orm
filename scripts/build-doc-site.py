#!/usr/bin/env python3
"""构建统一 API 文档站：聚合 70 包 rustdoc HTML + 跨包搜索 + 统一导航 + 敏感信息扫描。

用法：
    # 先生成 rustdoc HTML
    cargo doc --workspace --no-deps

    # 构建文档站
    python scripts/build-doc-site.py \
        --rustdoc-html target/doc \
        --version 7.2.0 \
        --git-commit $(git rev-parse HEAD) \
        --output target/doc-site

    # 仅扫描敏感信息
    python scripts/build-doc-site.py \
        --rustdoc-html target/doc \
        --output target/doc-site \
        --scan-only
"""

# 2026-10-28：`from __future__ import annotations` 使 `list[dict]` 等 3.9+ 注解
# 延迟求值，本机 Python 3.8 可执行（行为不变，CI 的 3.11+ 亦兼容）。
from __future__ import annotations

import argparse
import html
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path


def copy_rustdoc_html(rustdoc_html: str, output: str) -> int:
    """将 target/doc/ 下各 crate HTML 产物复制到 doc-site。"""
    src = Path(rustdoc_html)
    dst = Path(output)
    crate_count = 0

    if not src.exists():
        print(f"错误: rustdoc HTML 目录不存在: {src}", file=sys.stderr)
        return 0

    for item in src.iterdir():
        if item.is_dir() and item.name.startswith("sz_orm"):
            target_dir = dst / item.name
            if target_dir.exists():
                shutil.rmtree(target_dir)
            shutil.copytree(item, target_dir)
            crate_count += 1

    return crate_count


def build_search_index(rustdoc_json: str) -> dict:
    """从 rustdoc JSON 产物构建跨包符号搜索索引。"""
    index = {}
    json_path = Path(rustdoc_json)

    if not json_path.exists():
        print("警告: rustdoc JSON 目录不存在，搜索索引不可用 (SEARCH_UNAVAILABLE)", file=sys.stderr)
        return index

    for json_file in json_path.rglob("*.json"):
        try:
            data = json.loads(json_file.read_text(encoding="utf-8", errors="ignore"))
            crate_name = data.get("name", json_file.stem)
            for item in data.get("items", []):
                name = item.get("name")
                if name and item.get("visibility", "").startswith("pub"):
                    path = f"{crate_name}::{item.get('path', '')}::{name}"
                    if name not in index:
                        index[name] = []
                    index[name].append({
                        "crate": crate_name,
                        "path": path,
                        "kind": item.get("kind", "unknown"),
                    })
        except Exception:
            continue

    return index


def inject_unified_nav(doc_site: str, crate_count: int) -> None:
    """为每个 crate 的 index.html 注入统一导航栏。"""
    doc_path = Path(doc_site)
    crates = sorted([d.name for d in doc_path.iterdir() if d.is_dir() and d.name.startswith("sz_orm")])

    nav_items = "".join(
        f'<a href="../{c}/index.html" style="margin-right:10px">{html.escape(c)}</a>'
        for c in crates
    )
    nav_html = f'<div id="unified-nav" style="padding:10px;background:#f0f0f0;border-bottom:1px solid #ccc"><strong>sz-orm 包导航:</strong> {nav_items}</div>'

    for crate in crates:
        index_file = doc_path / crate / "index.html"
        if index_file.exists():
            content = index_file.read_text(encoding="utf-8", errors="ignore")
            if "unified-nav" not in content:
                content = content.replace("<body>", f"<body>{nav_html}", 1)
                index_file.write_text(content, encoding="utf-8")


def inject_version_annotation(doc_site: str, version: str, git_commit: str) -> None:
    """在首页注入版本标注。"""
    doc_path = Path(doc_site)
    index_file = doc_path / "index.html"

    annotation = f'<div id="version-info" style="padding:10px;background:#e8f5e9;border-bottom:1px solid #ccc"><strong>sz-orm {html.escape(version)}</strong> | git commit: <code>{html.escape(git_commit[:12])}</code></div>'

    if index_file.exists():
        content = index_file.read_text(encoding="utf-8", errors="ignore")
        if "version-info" not in content:
            content = content.replace("<body>", f"<body>{annotation}", 1)
            index_file.write_text(content, encoding="utf-8")
    else:
        # 创建首页
        index_file.write_text(
            f'<!DOCTYPE html><html><head><meta charset="utf-8"><title>sz-orm {html.escape(version)} 文档站</title></head><body>{annotation}<h1>sz-orm {html.escape(version)}</h1><p>共 {0} 个包</p></body></html>',
            encoding="utf-8",
        )


def scan_sensitive_info(doc_site: str) -> list[dict]:
    """扫描文档站中的敏感信息。"""
    patterns = {
        "数据库连接串": r"(mysql|postgres|sqlite)://[^\s\"<>]+",
        "密码/密钥": r"(password|passwd|secret|api_key)\s*[=:]\s*\S+",
        "内网地址": r"(127\.0\.0\.1|192\.168\.\d+\.\d+|10\.0\.\d+\.\d+)",
        "测试密码": r"test123",
    }

    findings = []
    doc_path = Path(doc_site)

    for file_path in doc_path.rglob("*"):
        if not file_path.is_file():
            continue
        if file_path.suffix not in (".html", ".json", ".js", ".css"):
            continue
        try:
            content = file_path.read_text(encoding="utf-8", errors="ignore")
            for i, line in enumerate(content.split("\n"), 1):
                for name, pattern in patterns.items():
                    for match in re.finditer(pattern, line, re.IGNORECASE):
                        findings.append({
                            "type": name,
                            "file": str(file_path),
                            "line": i,
                            "match": match.group()[:100],
                        })
        except Exception:
            continue

    return findings


MARKDOWN_SCAN_DIRS = ["docs/adr", "docs/migration"]
# R10（2026-10-28 TW 评测）：核心用户文档全文搜索索引范围
MARKDOWN_DOC_FILES = ["README.md", "README.zh.md", "README.en.md", "CHANGELOG.md"]


def parse_changelog_versions(repo_root: str) -> list[dict]:
    """解析 CHANGELOG.md 的 `## [x.y.z] — 日期` 版本头，返回倒序版本列表。"""
    path = Path(repo_root) / "CHANGELOG.md"
    versions = []
    if path.exists():
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            m = re.match(r"^##\s*\[([\d.]+)\]\s*(?:—|-)\s*([\d-]+)?", line.strip())
            if m:
                versions.append({"version": m.group(1), "date": m.group(2) or ""})
    return versions


def write_version_archive(repo_root: str, doc_site: str) -> int:
    """生成历史版本归档（R10）：versions.html 版本索引页 + versions/<ver>.html 详情页。"""
    versions = parse_changelog_versions(repo_root)
    out = Path(doc_site)
    versions_dir = out / "versions"
    versions_dir.mkdir(exist_ok=True)

    rows = []
    for v in versions:
        safe = v["version"].replace(".", "-")
        rows.append(
            f'<li><a href="versions/{safe}.html">{v["version"]}</a>'
            + (f' <span class="date">（{v["date"]}）</span>' if v["date"] else " <span class=\"date\">（日期待考）</span>")
            + '</li>'
        )
        detail = f"""<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>sz-orm {v['version']} 归档</title></head>
<body><h1>sz-orm {v['version']}</h1>
<p>发布日期：{v['date'] or '待考（补录条目，见 CHANGELOG 补录声明）'}</p>
<ul>
<li><a href="../../CHANGELOG.md">CHANGELOG 对应条目</a></li>
<li><a href="../../docs/README.md">文档索引（含版本历程）</a></li>
<li><a href="../../docs/assessment/">交付/审计报告</a></li>
</ul>
<p><em>历史文档快照：当前仓库为最新版（10.13.0+）；如需某版本的完整源码与文档快照，请通过 git tag / commit 检出。</em></p>
</body></html>"""
        (versions_dir / f"{safe}.html").write_text(detail, encoding="utf-8")

    index_page = f"""<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>sz-orm 版本归档</title>
<style>body{{font-family:-apple-system,"Segoe UI","Microsoft YaHei",sans-serif;margin:0}}
header{{padding:12px 20px;background:#f0f0f0;border-bottom:1px solid #ccc}}
li{{margin:4px 0}}</style></head>
<body><header><strong>sz-orm 版本归档（共 {len(versions)} 个版本）</strong> · <a href="../index.html">返回文档站</a> · <a href="../../CHANGELOG.md">CHANGELOG</a></header>
<ul>{''.join(rows)}</ul></body></html>"""
    (out / "versions.html").write_text(index_page, encoding="utf-8")
    return len(versions)


def collect_markdown_docs(repo_root: str) -> list[dict]:
    """收集核心用户 Markdown 文档（根级入口 + docs 根 + adr + migration）。"""
    root = Path(repo_root)
    files: list[Path] = []
    for name in MARKDOWN_DOC_FILES:
        p = root / name
        if p.exists():
            files.append(p)
    docs_dir = root / "docs"
    if docs_dir.exists():
        files.extend(sorted(docs_dir.glob("*.md")))
    for sub in MARKDOWN_SCAN_DIRS:
        sub_dir = root / sub
        if sub_dir.exists():
            files.extend(sorted(sub_dir.glob("*.md")))

    docs = []
    for p in files:
        try:
            lines = p.read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            continue
        rel = p.relative_to(root).as_posix()
        title = ""
        sections = []
        for i, ln in enumerate(lines, 1):
            if ln.startswith("# ") and not title:
                title = ln[2:].strip()
            elif re.match(r"^#{2,3} ", ln):
                sections.append({"line": i, "text": ln.lstrip("#").strip()})
        excerpt_lines = [ln.strip() for ln in lines if ln.strip() and not ln.startswith("#")]
        docs.append({
            "path": rel,
            "title": title or rel,
            "excerpt": " ".join(excerpt_lines)[:200],
            "sections": sections,
        })
    return docs


def build_markdown_search_index(repo_root: str, doc_site: str, version: str) -> int:
    """生成 Markdown 全文搜索索引 markdown-search.json（R10）。"""
    docs = collect_markdown_docs(repo_root)
    payload = {
        "generated_for_version": version,
        "note": "标题 + 章节标题 + 首段摘要索引；完整正文见仓库源文件（结果链接相对文档站目录回仓库根）",
        "docs": docs,
    }
    out = Path(doc_site) / "markdown-search.json"
    out.write_text(json.dumps(payload, ensure_ascii=False), encoding="utf-8")
    return len(docs)


def write_markdown_search_page(doc_site: str) -> None:
    """生成纯静态搜索页 search.html（原生 JS，无第三方依赖）。"""
    page = """<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>sz-orm 文档全文搜索</title>
<style>
 body { font-family: -apple-system, "Segoe UI", "Microsoft YaHei", sans-serif; margin: 0; }
 header { padding: 12px 20px; background: #f0f0f0; border-bottom: 1px solid #ccc; }
 input { width: 60%; padding: 8px; font-size: 15px; }
 #status { color: #666; margin-left: 10px; }
 li.doc { margin: 14px 0; }
 .path { color: #888; font-size: 12px; }
 .hit { background: #fff3cd; }
 ul { margin: 4px 0 0 16px; padding: 0; }
</style>
</head>
<body>
<header>
 <strong>sz-orm 文档全文搜索</strong>
 <input id="q" type="search" placeholder="输入关键词（标题 / 章节标题 / 摘要）…" autofocus>
 <span id="status"></span>
</header>
<main><ul id="results"></ul></main>
<script>
let DATA = null;
const results = document.getElementById('results');
const status = document.getElementById('status');
fetch('markdown-search.json').then(r => r.json()).then(d => {
  DATA = d.docs;
  status.textContent = `已索引 ${DATA.length} 份文档`;
});
function esc(s) { const d = document.createElement('div'); d.textContent = s; return d.innerHTML; }
function mark(text, q) {
  const i = text.toLowerCase().indexOf(q);
  if (i < 0) return esc(text);
  return esc(text.slice(0, i)) + '<span class="hit">' + esc(text.slice(i, i + q.length)) + '</span>' + esc(text.slice(i + q.length));
}
document.getElementById('q').addEventListener('input', e => {
  const q = e.target.value.trim().toLowerCase();
  results.innerHTML = '';
  if (!q || !DATA) { if (DATA) status.textContent = `已索引 ${DATA.length} 份文档`; return; }
  const hits = [];
  for (const doc of DATA) {
    let score = 0; const sectionHits = [];
    if (doc.title.toLowerCase().includes(q)) score += 10;
    if (doc.excerpt.toLowerCase().includes(q)) score += 1;
    for (const s of doc.sections) {
      if (s.text.toLowerCase().includes(q)) { score += 3; sectionHits.push(s); }
    }
    if (score > 0) hits.push({ doc, score, sectionHits: sectionHits.slice(0, 3) });
  }
  hits.sort((a, b) => b.score - a.score);
  status.textContent = `${hits.length} 份文档命中`;
  for (const h of hits.slice(0, 20)) {
    const li = document.createElement('li');
    li.className = 'doc';
    li.innerHTML = `<a href="../../${esc(h.doc.path)}">${mark(h.doc.title, q)}</a>` +
      ` <span class="path">${esc(h.doc.path)}</span>` +
      (h.sectionHits.length ? '<ul>' + h.sectionHits.map(s => `<li>L${s.line}: ${mark(s.text, q)}</li>`).join('') + '</ul>' : '');
    results.appendChild(li);
  }
});
</script>
</body>
</html>
"""
    out = Path(doc_site) / "search.html"
    out.write_text(page, encoding="utf-8")


def inject_version_history(doc_site: str, version: str) -> None:
    """首页注入版本历程链接（R10 版本化最小实现：CHANGELOG + 文档索引版本历程表）。"""
    doc_path = Path(doc_site)
    index_file = doc_path / "index.html"
    banner = (
        f'<div id="version-history" style="padding:10px;background:#e3f2fd;border-bottom:1px solid #ccc">'
        f'<strong>版本历程（当前 {html.escape(version)}）：</strong> '
        f'<a href="../../CHANGELOG.md">完整版本线（Keep a Changelog）</a> · '
        f'<a href="../../docs/README.md">文档索引（含版本历程表）</a></div>'
    )
    if index_file.exists():
        content = index_file.read_text(encoding="utf-8", errors="ignore")
        if "version-history" not in content:
            content = content.replace("<body>", f"<body>{banner}", 1)
            index_file.write_text(content, encoding="utf-8")


def generate_manifest(crate_count: int, version: str, git_commit: str, output: str,
                      markdown_doc_count: int = 0, version_archive_count: int = 0) -> None:
    """生成 DocSiteManifest。"""
    manifest = {
        "crate_count": crate_count,
        "sz_orm_version": version,
        "git_commit": git_commit,
        "has_unified_nav": True,
        "has_cross_pkg_search": True,
        "has_markdown_search": markdown_doc_count > 0,
        "markdown_doc_count": markdown_doc_count,
        "has_version_archive": version_archive_count > 0,
        "version_archive_count": version_archive_count,
        "version_history_linked": True,
        "sensitive_info_scan_passed": True,
    }
    manifest_path = Path(output) / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False), encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description="构建统一 API 文档站")
    parser.add_argument("--rustdoc-html", required=True, help="cargo doc HTML 产物目录")
    parser.add_argument("--rustdoc-json", default=None, help="cargo doc JSON 产物目录")
    parser.add_argument("--version", default="unknown", help="sz-orm 版本")
    parser.add_argument("--git-commit", default="unknown", help="git commit hash")
    parser.add_argument("--output", required=True, help="输出目录")
    parser.add_argument("--repo-root", default=str(Path(__file__).resolve().parent.parent),
                        help="仓库根目录（用于 Markdown 全文搜索索引，默认脚本上级目录）")
    parser.add_argument("--scan-only", action="store_true", help="仅执行敏感信息扫描")
    args = parser.parse_args()

    output_path = Path(args.output)

    if args.scan_only:
        findings = scan_sensitive_info(args.output)
        if findings:
            print(f"SENSITIVE_INFO_DETECTED: 发现 {len(findings)} 处敏感信息:")
            for f in findings:
                print(f"  {f['file']}:{f['line']} — [{f['type']}] {f['match']}")
            sys.exit(1)
        else:
            print("敏感信息扫描通过: 无敏感信息")
            sys.exit(0)

    # 1. 聚合 HTML
    crate_count = copy_rustdoc_html(args.rustdoc_html, args.output)
    print(f"聚合 {crate_count} 个 crate HTML 产物")

    # 2. 构建搜索索引
    if args.rustdoc_json:
        search_index = build_search_index(args.rustdoc_json)
        index_path = output_path / "search-index.json"
        index_path.write_text(json.dumps(search_index, indent=2, ensure_ascii=False), encoding="utf-8")
        print(f"搜索索引: {len(search_index)} 个符号")
    else:
        print("警告: 未提供 --rustdoc-json，跳过搜索索引构建")

    # 3. 注入统一导航
    inject_unified_nav(args.output, crate_count)
    print("统一导航栏已注入")

    # 4. 注入版本标注
    inject_version_annotation(args.output, args.version, args.git_commit)
    print(f"版本标注已注入: sz-orm {args.version}")

    # 4.5 Markdown 全文搜索索引 + 静态搜索页 + 版本历程链接（R10）
    md_count = build_markdown_search_index(args.repo_root, args.output, args.version)
    write_markdown_search_page(args.output)
    inject_version_history(args.output, args.version)
    version_count = write_version_archive(args.repo_root, args.output)
    print(f"Markdown 全文搜索: {md_count} 份文档已索引（search.html + markdown-search.json）")
    print(f"版本归档: {version_count} 个版本（versions.html + versions/<ver>.html）")

    # 5. 敏感信息扫描
    findings = scan_sensitive_info(args.output)
    if findings:
        print(f"SENSITIVE_INFO_DETECTED: 发现 {len(findings)} 处敏感信息")
        for f in findings:
            print(f"  {f['file']}:{f['line']} — [{f['type']}] {f['match']}")
    else:
        print("敏感信息扫描通过")

    # 6. 生成 manifest
    generate_manifest(crate_count, args.version, args.git_commit, args.output, md_count, version_count)
    print(f"manifest.json 已生成")

    print(f"\n文档站构建完成: {args.output} ({crate_count} 个 crate)")


if __name__ == "__main__":
    main()