"""Markdown 输出"""

from datetime import datetime
from typing import Any, Dict, List, Optional

from code_stats.outputs.base import BaseOutput
from code_stats.utils.formatters import format_size, get_language_emoji


class MarkdownOutput(BaseOutput):
    """Markdown 输出器"""

    def __init__(
        self,
        output_file: Optional[str] = None,
        current_dir: str = "",
        summary_only: bool = False,
        quiet: bool = False,
        sort_by: str = "lines",
        top: Optional[int] = None,
    ):
        super().__init__(output_file, current_dir, summary_only, quiet)
        self.sort_by = sort_by
        self.top = top

    def output(
        self,
        all_repos: List[Dict[str, Any]],
        summary: Dict[str, Any]
    ) -> None:
        """输出 Markdown 格式"""
        output_file = self.output_file or 'code_statistics.md'

        with open(output_file, 'w', encoding='utf-8') as f:
            f.write("# 代码统计报告\n\n")
            f.write(f"> 生成时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n")
            f.write(f"> 扫描目录: `{self.current_dir}`\n\n")

            # 执行摘要
            f.write("## 执行摘要\n\n")
            f.write("### 统计概览\n\n")
            f.write("| 指标 | 数值 |\n")
            f.write("|------|------|\n")
            f.write(f"| **仓库总数** | {summary['total_repos']} |\n")
            f.write(f"| **代码文件总数** | {summary['total_files']:,} |\n")
            f.write(f"| **代码总行数** | {summary['total_lines']:,} |\n")
            f.write(f"| **文档文件总数** | {summary.get('total_doc_files', 0):,} |\n")
            f.write(f"| **文档总行数** | {summary.get('total_doc_lines', 0):,} |\n")
            f.write(f"| **代码文件大小** | {format_size(summary.get('total_size', 0))} |\n")
            f.write(f"| **仓库总大小** | {format_size(summary.get('total_all_size', 0))} |\n")

            avg_lines_per_repo = (
                summary['total_lines'] // summary['total_repos']
                if summary['total_repos'] > 0 else 0
            )
            avg_lines_per_file = (
                summary['total_lines'] // summary['total_files']
                if summary['total_files'] > 0 else 0
            )
            f.write(f"| **平均每仓库代码行数** | {avg_lines_per_repo:,} |\n")
            f.write(f"| **平均每文件代码行数** | {avg_lines_per_file} |\n\n")

            # 语言分布
            f.write("### 语言分布\n\n")
            sorted_languages = sorted(
                summary['by_language'].items(),
                key=lambda x: x[1]['lines'],
                reverse=True
            )

            # 使用进度条可视化
            max_lines = sorted_languages[0][1]['lines'] if sorted_languages else 0
            for lang, data in sorted_languages[:10]:
                percentage = (
                    (data['lines'] / summary['total_lines'] * 100)
                    if summary['total_lines'] > 0 else 0
                )
                bar_length = int((data['lines'] / max_lines) * 40) if max_lines > 0 else 0
                bar = '█' * bar_length + '░' * (40 - bar_length)
                f.write(f"**{lang:>15}** |{bar}| {percentage:>5.1f}% ({data['lines']:,} 行)\n")

            if len(sorted_languages) > 10:
                f.write(f"\n*... 以及其他 {len(sorted_languages) - 10} 种语言*\n")

            # 按语言详细统计表
            f.write("\n## 按语言统计\n\n")
            f.write("| # | 语言 | 仓库数 | 文件数 | 代码行数 | 占比 | 平均行/文件 |\n")
            f.write("|---|------|--------|--------|----------|------|-------------|\n")

            for idx, (lang, data) in enumerate(sorted_languages, 1):
                percentage = (
                    (data['lines'] / summary['total_lines'] * 100)
                    if summary['total_lines'] > 0 else 0
                )
                avg_lines = data['lines'] // data['files'] if data['files'] > 0 else 0
                lang_emoji = get_language_emoji(lang)
                f.write(
                    f"| {idx} | {lang_emoji} {lang} | {data['repos']} | "
                    f"{data['files']:,} | {data['lines']:,} | {percentage:.1f}% | {avg_lines} |\n"
                )

            # 仓库排行榜
            f.write("\n## 仓库排行榜\n\n")
            sorted_by_lines = sorted(all_repos, key=lambda x: x['lines'], reverse=True)
            f.write("### 按代码行数 TOP 10\n\n")
            f.write("| 排名 | 仓库名 | 主要语言 | 代码文件 | 代码行数 | 文档文件 | 文档行数 | 占比 |\n")
            f.write("|------|--------|----------|--------|----------|--------|----------|------|\n")

            for i, repo in enumerate(sorted_by_lines[:10], 1):
                percentage = (
                    (repo['lines'] / summary['total_lines'] * 100)
                    if summary['total_lines'] > 0 else 0
                )
                medal = "🥇" if i == 1 else "🥈" if i == 2 else "🥉" if i == 3 else f"{i}"
                lang_emoji = get_language_emoji(repo['language'])
                f.write(
                    f"| {medal} | **{repo['name']}** | {lang_emoji} {repo['language']} | "
                    f"{repo['files']:,} | {repo['lines']:,} | {repo.get('doc_files', 0):,} | "
                    f"{repo.get('doc_lines', 0):,} | {percentage:.1f}% |\n"
                )

            # 仓库详细列表
            f.write("\n## 仓库详细信息\n\n")

            # 排序
            if self.sort_by == 'lines':
                sorted_repos = sorted(all_repos, key=lambda x: x['lines'], reverse=True)
            elif self.sort_by == 'files':
                sorted_repos = sorted(all_repos, key=lambda x: x['files'], reverse=True)
            else:
                sorted_repos = sorted(all_repos, key=lambda x: x['name'])

            # 应用 top 限制
            if self.top:
                sorted_repos = sorted_repos[:self.top]

            for repo in sorted_repos:
                lang_emoji = get_language_emoji(repo['language'])
                f.write(f"### {repo['name']}\n\n")
                f.write(f"- **主要语言**: {lang_emoji} {repo['language']}\n")
                f.write(f"- **代码文件数**: {repo['files']:,}\n")
                f.write(f"- **代码行数**: {repo['lines']:,}\n")

                # 添加代码组成分析
                total_lines = repo['lines']
                code_lines = repo.get('code_lines', 0)
                comment_lines = repo.get('comment_lines', 0)
                blank_lines = repo.get('blank_lines', 0)

                if total_lines > 0:
                    comment_rate = (comment_lines / total_lines) * 100
                    blank_rate = (blank_lines / total_lines) * 100
                    code_rate = (code_lines / total_lines) * 100
                    f.write(f"  - 纯代码行: {code_lines:,} ({code_rate:.1f}%)\n")
                    f.write(f"  - 注释行: {comment_lines:,} ({comment_rate:.1f}%)\n")
                    f.write(f"  - 空行: {blank_lines:,} ({blank_rate:.1f}%)\n")

                f.write(f"- **文档文件数**: {repo.get('doc_files', 0):,}\n")
                f.write(f"- **文档行数**: {repo.get('doc_lines', 0):,}\n")
                f.write(f"- **代码文件大小**: {format_size(repo.get('size', 0))}\n")
                f.write(f"- **仓库总大小**: {format_size(repo.get('total_size', 0))}\n")

                # 显示文件类型分布
                if repo.get('details'):
                    f.write("- **文件类型分布**:\n")
                    sorted_exts = sorted(
                        repo['details'].items(),
                        key=lambda x: x[1]['lines'],
                        reverse=True
                    )[:5]
                    for ext, data in sorted_exts:
                        f.write(f"  - `{ext}`: {data['files']} 个文件, {data['lines']:,} 行\n")
                f.write("\n")

            # 统计信息页脚
            f.write("---\n\n")
            f.write("*使用 code-stats 生成*\n")

        self._print_saved_message(output_file)
