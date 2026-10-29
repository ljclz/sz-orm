#!/usr/bin/env python3
"""验证统一 API 文档站的完整性与性能。

用法：
    python scripts/verify-doc-site.py \
        --doc-site target/doc-site \
        --expected-crates 70 \
        --expected-version 7.2.0
"""

import argparse
import json
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description="验证文档站完整性")
    parser.add_argument("--doc-site", required=True, help="文档站目录")
    parser.add_argument("--expected-crates", type=int, default=70, help="期望的 crate 数量")
    parser.add_argument("--expected-version", default=None, help="期望的版本号")
    parser.add_argument("--search-symbol", default=None, help="搜索符号验证")
    args = parser.parse_args()

    doc_path = Path(args.doc_site)
    if not doc_path.exists():
        print(f"错误: 文档站目录不存在: {doc_path}", file=sys.stderr)
        sys.exit(1)

    # 1. 检查 crate 数量
    crates = [d.name for d in doc_path.iterdir() if d.is_dir() and d.name.startswith("sz_orm")]
    actual_count = len(crates)
    if actual_count < args.expected_crates:
        print(f"失败: crate 数量 {actual_count} < 期望 {args.expected_crates}")
        sys.exit(1)
    print(f"✓ crate 数量: {actual_count} (期望 {args.expected_crates})")

    # 2. 检查 manifest
    manifest_path = doc_path / "manifest.json"
    if manifest_path.exists():
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        if args.expected_version:
            if manifest.get("sz_orm_version") != args.expected_version:
                print(f"失败: 版本不匹配 {manifest.get('sz_orm_version')} != {args.expected_version}")
                sys.exit(1)
            print(f"✓ 版本: {manifest.get('sz_orm_version')}")
        print(f"✓ 统一导航: {manifest.get('has_unified_nav')}")
        print(f"✓ 跨包搜索: {manifest.get('has_cross_pkg_search')}")
        print(f"✓ Markdown 全文搜索: {manifest.get('has_markdown_search')}（{manifest.get('markdown_doc_count', 0)} 份文档）")
        print(f"✓ 版本归档: {manifest.get('has_version_archive')}（{manifest.get('version_archive_count', 0)} 个版本）")
        print(f"✓ 版本历程链接: {manifest.get('version_history_linked')}")
        print(f"✓ 敏感信息扫描: {'通过' if manifest.get('sensitive_info_scan_passed') else '未通过'}")
    else:
        print("警告: manifest.json 不存在")

    # 3.5 检查 Markdown 全文搜索产物（R10）
    md_index_path = doc_path / "markdown-search.json"
    md_page_path = doc_path / "search.html"
    if md_index_path.exists() and md_page_path.exists():
        md_index = json.loads(md_index_path.read_text(encoding="utf-8"))
        md_docs = md_index.get("docs", [])
        if not md_docs:
            print("失败: markdown-search.json 为空")
            sys.exit(1)
        print(f"✓ Markdown 搜索索引: {len(md_docs)} 份文档, search.html 存在")
    else:
        print("警告: markdown-search.json / search.html 不存在（R10 之前的构建产物）")

    # 3.6 检查版本归档产物（R10）
    versions_page = doc_path / "versions.html"
    versions_dir = doc_path / "versions"
    if versions_page.exists() and versions_dir.is_dir():
        vcount = len(list(versions_dir.glob("*.html")))
        if vcount == 0:
            print("失败: versions/ 目录为空")
            sys.exit(1)
        print(f"✓ 版本归档: versions.html + {vcount} 个版本详情页")
    else:
        print("警告: versions.html / versions/ 不存在（R10 之前的构建产物）")

    # 3. 检查搜索索引
    search_index_path = doc_path / "search-index.json"
    if search_index_path.exists():
        search_index = json.loads(search_index_path.read_text(encoding="utf-8"))
        print(f"✓ 搜索索引: {len(search_index)} 个符号")

        if args.search_symbol:
            results = search_index.get(args.search_symbol, [])
            if results:
                print(f"✓ 搜索 '{args.search_symbol}': {len(results)} 个结果")
                for r in results[:5]:
                    print(f"    {r['crate']} — {r['path']}")
            else:
                print(f"失败: 搜索 '{args.search_symbol}' 无结果")
                sys.exit(1)
    else:
        print("警告: search-index.json 不存在")

    # 4. 检查统一导航
    for crate in crates[:3]:
        index_file = doc_path / crate / "index.html"
        if index_file.exists():
            content = index_file.read_text(encoding="utf-8", errors="ignore")
            if "unified-nav" in content:
                print(f"✓ {crate}/index.html 含统一导航栏")
            else:
                print(f"警告: {crate}/index.html 不含统一导航栏")

    print("\n文档站验证通过")
    sys.exit(0)


if __name__ == "__main__":
    main()