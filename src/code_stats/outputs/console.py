"""控制台输出"""

from typing import Any, Dict, List

from code_stats.outputs.base import BaseOutput
from code_stats.utils.formatters import format_size


class ConsoleOutput(BaseOutput):
    """控制台输出器"""

    def output(
        self,
        all_repos: List[Dict[str, Any]],
        summary: Dict[str, Any]
    ) -> None:
        """输出到控制台"""
        if not self.summary_only:
            print(f"\n扫描目录: {self.current_dir}")
            print("=" * 80)

            for repo in sorted(all_repos, key=lambda x: x['name']):
                print(f"\n仓库: {repo['name']}")
                print(f"  主要语言: {repo['language']}")
                print(f"  文件数: {repo['files']:,}")
                print(f"  总行数: {repo['lines']:,}")

                # 计算并显示注释率和空行率
                total_lines = repo['lines']
                code_lines = repo.get('code_lines', 0)
                comment_lines = repo.get('comment_lines', 0)
                blank_lines = repo.get('blank_lines', 0)

                if total_lines > 0:
                    comment_rate = (comment_lines / total_lines) * 100
                    blank_rate = (blank_lines / total_lines) * 100
                    code_rate = (code_lines / total_lines) * 100
                    print(f"    - 纯代码行: {code_lines:,} ({code_rate:.1f}%)")
                    print(f"    - 注释行: {comment_lines:,} ({comment_rate:.1f}%)")
                    print(f"    - 空行: {blank_lines:,} ({blank_rate:.1f}%)")

                # 显示前5个文件类型
                sorted_exts = sorted(
                    repo['details'].items(),
                    key=lambda x: x[1]['lines'],
                    reverse=True
                )[:5]
                if sorted_exts:
                    print("  主要文件类型:")
                    for ext, data in sorted_exts:
                        print(f"    {ext}: {data['files']} 个文件, {data['lines']:,} 行")

                # 显示 Git 信息
                if 'git' in repo and repo['git']:
                    git = repo['git']
                    print("  Git 信息:")
                    if git.get('branch'):
                        print(f"    分支: {git['branch']}")
                    if git.get('last_commit_date'):
                        print(f"    最后提交: {git['last_commit_date']}")
                    if git.get('last_commit_author'):
                        print(f"    提交者: {git['last_commit_author']}")
                    if git.get('total_commits'):
                        print(f"    提交总数: {git['total_commits']:,}")
                    if git.get('contributors'):
                        print(f"    贡献者数: {git['contributors']}")

        # 输出总体统计
        print("\n" + "=" * 80)
        print("总体统计")
        print("=" * 80)
        print(f"仓库总数: {summary['total_repos']}")
        print(f"代码文件数: {summary['total_files']:,}")
        print(f"代码总行数: {summary['total_lines']:,}")

        # 显示代码类型分布
        code_details = summary.get('code_details', {})
        if code_details:
            sorted_codes = sorted(
                code_details.items(),
                key=lambda x: x[1]['lines'],
                reverse=True
            )
            print("  代码类型分布:")
            for ext, details in sorted_codes:
                print(f"    {ext}: {details['files']} 个文件, {details['lines']:,} 行")

        # 显示代码组成分析
        total_code_lines = summary.get('total_code_lines', 0)
        total_comment_lines = summary.get('total_comment_lines', 0)
        total_blank_lines = summary.get('total_blank_lines', 0)
        if summary['total_lines'] > 0:
            code_rate = (total_code_lines / summary['total_lines']) * 100
            comment_rate = (total_comment_lines / summary['total_lines']) * 100
            blank_rate = (total_blank_lines / summary['total_lines']) * 100
            print("代码组成:")
            print(f"  - 纯代码行: {total_code_lines:,} ({code_rate:.1f}%)")
            print(f"  - 注释行: {total_comment_lines:,} ({comment_rate:.1f}%)")
            print(f"  - 空行: {total_blank_lines:,} ({blank_rate:.1f}%)")

        print(f"文档文件数: {summary.get('total_doc_files', 0):,}")
        print(f"文档总行数: {summary.get('total_doc_lines', 0):,}")

        # 显示文档类型详情
        doc_details = summary.get('doc_details', {})
        if doc_details:
            sorted_docs = sorted(
                doc_details.items(),
                key=lambda x: x[1]['lines'],
                reverse=True
            )
            print("  文档类型分布:")
            for ext, details in sorted_docs:
                print(f"    {ext}: {details['files']} 个文件, {details['lines']:,} 行")

        print("\n大小统计:")
        print(f"  代码大小: {format_size(summary.get('total_size', 0))}")
        print(f"  文档大小: {format_size(summary.get('total_doc_size', 0))}")
        print(f"  仓库总大小: {format_size(summary.get('total_all_size', summary.get('total_size', 0)))}")

        # 按语言统计
        print("\n按语言统计:")
        sorted_languages = sorted(
            summary['by_language'].items(),
            key=lambda x: x[1]['lines'],
            reverse=True
        )
        for lang, data in sorted_languages:
            percentage = (data['lines'] / summary['total_lines'] * 100) if summary['total_lines'] > 0 else 0
            print(f"  {lang}:")
            print(f"    仓库数: {data['repos']}")
            print(f"    文件数: {data['files']:,}")
            print(f"    代码行数: {data['lines']:,} ({percentage:.1f}%)")

        # 按仓库排序
        print("\n仓库排行（按代码行数）:")
        sorted_repos = sorted(all_repos, key=lambda x: x['lines'], reverse=True)[:10]

        for i, repo in enumerate(sorted_repos, 1):
            print(f"  {i}. {repo['name']} ({repo['language']}): {repo['lines']:,} 行")
